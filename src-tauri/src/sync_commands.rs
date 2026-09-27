//! Sync, reprocess and the match list.
//!
//! Sync and reprocess run for minutes, so the commands return immediately and
//! the work streams its progress to the UI as events:
//!
//! ```text
//! sync://progress   Progress        many times
//! sync://done       SyncSummary     once, on success
//! sync://error      CmdError        once, on failure
//! ```

use crate::error::{CmdError, CmdResult};
use crate::AppState;
use hl_db::{IndexStats, MatchFilter, MatchPage};
use hl_ingest::{Progress, SyncOptions, SyncSummary};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

const EV_PROGRESS: &str = "sync://progress";
const EV_DONE: &str = "sync://done";
const EV_ERROR: &str = "sync://error";

/// Clears the busy flag however the task ends — success, error or panic.
struct BusyGuard(Arc<AtomicBool>);

impl BusyGuard {
    fn acquire(flag: &Arc<AtomicBool>) -> Option<Self> {
        flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| BusyGuard(flag.clone()))
    }
}

impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// What `sync://done` carries. Reprocess reuses it with `fetched = 0`.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Done {
    kind: &'static str,
    fetched: usize,
    failed: usize,
    stats: IndexStats,
}

#[tauri::command]
pub async fn sync_start(app: AppHandle, state: State<'_, AppState>, full: bool) -> CmdResult<()> {
    let guard = BusyGuard::acquire(&state.busy)
        .ok_or_else(|| CmdError::new("busy", "A sync is already running."))?;
    let me = state
        .db
        .get_me()
        .await?
        .ok_or_else(|| CmdError::new("missing_config", "Set your SteamID before syncing."))?;

    let db = state.db.clone();
    let sources = state.sources.clone();
    let db_path = state.db_path.clone();
    let weights_path = state.db_path.with_file_name("weights.toml");

    tauri::async_runtime::spawn(async move {
        let _guard = guard;
        let emitter = app.clone();
        let opts = SyncOptions { full, max_fetch: None };
        let result = async {
            // A copy first: a sync rewrites derived tables, and the file is
            // the only thing here that cannot be fetched again.
            if let Err(e) = hl_ingest::backup::run(&db, &db_path, false).await {
                tracing::warn!(error = %format!("{e:#}"), "database backup failed");
            }
            // Loaded once: the fetch loop rates each log as it lands, and
            // the pass at the end re-rates everything.
            let (weights, _) = hl_rating::Weights::load(&weights_path);
            let summary = hl_ingest::sync(&db, &sources, me, &opts, &weights, |p: Progress| {
                let _ = emitter.emit(EV_PROGRESS, p);
            })
            .await?;
            // Parts of combined logs: best effort, like ETF2L. When logs.tf is
            // not answering the round maps come from the other sources.
            match hl_ingest::maps::fetch_parts(&db, &sources, |p: Progress| {
                let _ = emitter.emit(EV_PROGRESS, p);
            })
            .await
            {
                Ok(p) if p.gave_up => tracing::warn!(failed = p.failed, "logs.tf not answering; parts wait for the next sync"),
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %format!("{e:#}"), "part fetch failed"),
            }
            // Raw logs: every kill with time, classes and positions. Newest
            // first; the first sync fetches the whole history (~15 min).
            let raw = hl_ingest::kills::fetch(&db, &sources, Some(hl_ingest::BULK_PER_SYNC), |p: Progress| {
                let _ = emitter.emit(EV_PROGRESS, p);
            })
            .await?;
            if raw.failed > 0 {
                tracing::warn!(failed = raw.failed, "some raw logs could not be fetched; next sync retries");
            }
            // ETF2L itself was fetched inside the sync, before the queue:
            // what it says decides which logs are worth downloading. Sorting
            // the downloaded ones into officials, scrims and pugs needs their
            // player lists, so that part happens here.
            hl_ingest::etf2l::derive_context(&db, me).await?;
            // Your name and picture for the top bar; a failure keeps the old ones.
            let stage = |what| {
                let _ = emitter.emit(EV_PROGRESS, Progress::Stage { what });
            };
            stage("Refreshing your profile");
            if let Err(e) = hl_ingest::owner::refresh(&db, &sources, me).await {
                tracing::warn!(error = %format!("{e:#}"), "owner profile refresh failed");
            }
            // Which demos.tf demo each log is, for the ones trends.tf never
            // linked -- 30% of them here. Best effort: demos.tf being down
            // costs those links, not the sync.
            stage("Matching demos.tf");
            match hl_ingest::demostf::index(&db, &sources, me).await {
                Ok(f) if f.matched > 0 => tracing::info!(listed = f.listed, matched = f.matched, "demos.tf demos matched to logs"),
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %format!("{e:#}"), "demos.tf lookup failed"),
            }

            // Q29: every team's season, from ETF2L. Best effort, and a few
            // dozen match pages a sync, so a year fills in over a few syncs.
            stage("Reading ETF2L seasons");
            match hl_ingest::leagues::fetch(&db, &sources, |_, _| {}).await {
                Ok(s) => tracing::info!(competitions = s.competitions, results = s.results, details = s.details, "ETF2L seasons read"),
                Err(e) => tracing::warn!(error = %format!("{e:#}"), "ETF2L seasons could not be read"),
            }

            // New logs can link to demos already on disk. This also places
            // every log on the real clock, which the round maps use.
            if let Some(tf) = db.get_config().await?.tf_path {
                stage("Scanning your demos folder");
                let s = hl_ingest::index_demos(&db, std::path::Path::new(&tf)).await?;
                let _ = emitter.emit(EV_DEMOS_INDEXED, &s);
            }
            // Every round's map, then the rating: kills are valued on their map.
            stage("Resolving each round's map");
            hl_ingest::maps::resolve_all(&db).await?;
            hl_ingest::fights::derive_all(&db, false, |done, total| {
                let _ = emitter.emit(EV_PROGRESS, Progress::Fights { done, total });
            })
            .await?;
            // Aim from any newly linked demo. Seconds a demo, and a demo the
            // parser cannot read must not fail the whole sync.
            match hl_ingest::aim::derive_all(&db, me, false, |done, total, log_id| {
                let _ = emitter.emit(EV_PROGRESS, Progress::ReadingDemos { done, total, log_id });
            })
            .await
            {
                Ok(s) if s.read > 0 => tracing::info!(read = s.read, kills = s.kills, "aim read from demos"),
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %format!("{e:#}"), "reading aim from demos failed"),
            }
            // Asked for in Settings: a demo that has been read is 80 MB
            // doing nothing. It is swept here, right after the pass, and
            // nowhere else -- this is the only point where "the app has
            // finished with it" is certainly true.
            if db.auto_delete_demos().await.unwrap_or(false) {
                match sweep_read_demos(&db).await {
                    Ok(n) if n > 0 => tracing::info!(deleted = n, "downloaded demos cleaned up"),
                    Ok(_) => {}
                    Err(e) => tracing::warn!(error = %format!("{e:#}"), "demo cleanup failed"),
                }
            }
            // Every sync ends by re-rating: new matches shift the baselines.
            hl_ingest::rate_all(&db, Some(me), &weights, |p: Progress| {
                let _ = emitter.emit(EV_PROGRESS, p);
            })
            .await?;
            anyhow::Ok(summary)
        }
        .await;

        match result {
            Ok(SyncSummary { fetched, failed, stats }) => {
                let _ = app.emit(EV_DONE, Done { kind: "sync", fetched, failed, stats });
            }
            Err(e) => {
                let _ = app.emit(EV_ERROR, CmdError::from(e));
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn reprocess_start(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let guard = BusyGuard::acquire(&state.busy)
        .ok_or_else(|| CmdError::new("busy", "A sync is already running."))?;
    let db = state.db.clone();
    let db_path = state.db_path.clone();
    let weights_path = state.db_path.with_file_name("weights.toml");

    tauri::async_runtime::spawn(async move {
        let _guard = guard;
        let emitter = app.clone();
        let result = async {
            // A rebuild rewrites every derived table; keep a copy of what was.
            if let Err(e) = hl_ingest::backup::run(&db, &db_path, false).await {
                tracing::warn!(error = %format!("{e:#}"), "database backup failed");
            }
            let stats = hl_ingest::reprocess(&db, |p: Progress| {
                let _ = emitter.emit(EV_PROGRESS, p);
            })
            .await?;
            hl_ingest::kills::rederive_all(&db, |p: Progress| {
                let _ = emitter.emit(EV_PROGRESS, p);
            })
            .await?;
            let me = db.get_me().await?;
            if let Some(me) = me {
                let _ = emitter.emit(EV_PROGRESS, Progress::Stage { what: "Reading ETF2L" });
                hl_ingest::etf2l::derive_context(&db, me).await?;
            }
            let _ = emitter.emit(EV_PROGRESS, Progress::Stage { what: "Resolving each round's map" });
            hl_ingest::maps::resolve_all(&db).await?;
            hl_ingest::fights::derive_all(&db, true, |done, total| {
                let _ = emitter.emit(EV_PROGRESS, Progress::Fights { done, total });
            })
            .await?;
            let (weights, _) = hl_rating::Weights::load(&weights_path);
            hl_ingest::rate_all(&db, me, &weights, |p: Progress| {
                let _ = emitter.emit(EV_PROGRESS, p);
            })
            .await?;
            anyhow::Ok(stats)
        }
        .await;

        match result {
            Ok(stats) => {
                let _ = app.emit(EV_DONE, Done { kind: "reprocess", fetched: 0, failed: 0, stats });
            }
            Err(e) => {
                let _ = app.emit(EV_ERROR, CmdError::from(e));
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn sync_busy(state: State<'_, AppState>) -> CmdResult<bool> {
    Ok(state.busy.load(Ordering::Acquire))
}

#[tauri::command]
pub async fn index_stats(state: State<'_, AppState>) -> CmdResult<IndexStats> {
    Ok(state.db.index_stats().await?)
}

// A Tauri command takes its arguments flat, one per field the UI sends.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn list_matches(
    state: State<'_, AppState>,
    format: Option<String>,
    kind: Option<String>,
    from: Option<i64>,
    to: Option<i64>,
    limit: i64,
    offset: i64,
    sort: Option<String>,
    ascending: Option<bool>,
    class: Option<String>,
    map: Option<String>,
) -> CmdResult<MatchPage> {
    let me = state.db.get_me().await?;
    let filter = MatchFilter {
        format,
        kind,
        from,
        to,
        sort,
        class,
        map,
        ascending: ascending.unwrap_or(false),
        model_version: hl_rating::MODEL_VERSION.to_string(),
        // Bound the page size so a bad argument cannot pull the whole table.
        limit: limit.clamp(1, 500),
        offset: offset.max(0),
    };
    Ok(state.db.list_matches(me.map(|m| m.account_id()), &filter).await?)
}

/// Everything the match page shows. `None` when the log is not stored yet.
///
/// Weights are re-read on every call, so edits to `weights.toml` show up the
/// next time a match is opened.
#[tauri::command]
pub async fn get_match(state: State<'_, AppState>, log_id: i64) -> CmdResult<Option<hl_ingest::MatchView>> {
    let me = state.db.get_me().await?;
    let (weights, warning) = hl_rating::Weights::load(&state.db_path.with_file_name("weights.toml"));
    let Some(mut detail) = hl_ingest::match_detail(&state.db, log_id, me, &weights).await? else {
        return Ok(None);
    };
    detail.weights_warning = warning;
    Ok(Some(hl_ingest::view_of(&state.db, log_id, detail).await?))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileResponse {
    /// Classes with rated games, most played first: `(class, games)`.
    pub classes: Vec<(String, i64)>,
    pub profile: Option<hl_rating::Profile>,
    /// Kills in context against the players you face, under the same filters.
    pub fights: Option<hl_ingest::seasons::FightsCard>,
    /// What your demos say about your aim under the same filters (PLAN §14),
    /// and over everything read, to compare against.
    pub aim: Option<hl_db::AimTotals>,
    pub life: Option<hl_db::LifeTotals>,
    pub aim_all: Option<hl_db::AimTotals>,
    pub life_all: Option<hl_db::LifeTotals>,
}

/// The owner's profile on one class, defaulting to their most-rated class.
#[tauri::command]
pub async fn get_profile(
    state: State<'_, AppState>,
    class: Option<String>,
    kind: Option<String>,
    from: Option<i64>,
    to: Option<i64>,
) -> CmdResult<ProfileResponse> {
    let me = state
        .db
        .get_me()
        .await?
        .ok_or_else(|| CmdError::new("missing_config", "Set your SteamID first."))?;
    let classes = hl_ingest::rated_classes(&state.db, me).await?;
    let chosen = match class.or_else(|| classes.first().map(|(c, _)| c.clone())) {
        Some(c) => Some(hl_core::TfClass::parse(&c)?),
        None => None,
    };
    let period = match (from, to) {
        (None, None) => None,
        (f, t) => Some((f.unwrap_or(i64::MIN), t.unwrap_or(i64::MAX))),
    };
    let (profile, fights) = match chosen {
        Some(c) => (
            hl_ingest::load_profile(&state.db, me, c, kind.as_deref(), period).await?,
            hl_ingest::seasons::fights_card(&state.db, me, c, kind.as_deref(), from, to).await?,
        ),
        None => (None, None),
    };
    // The same filters, over what the demos say (PLAN §14).
    let class_name = chosen.map(|c| c.as_str());
    let scope = hl_db::AimFilter {
        me: me.account_id(),
        log_id: None,
        class: class_name,
        kind: kind.as_deref(),
        from,
        to,
    };
    let everything = hl_db::AimFilter { me: me.account_id(), class: class_name, ..Default::default() };
    Ok(ProfileResponse {
        classes,
        profile,
        fights,
        aim: state.db.aim_totals(&scope).await?,
        life: state.db.life_totals(&scope).await?,
        aim_all: state.db.aim_totals(&everything).await?,
        life_all: state.db.life_totals(&everything).await?,
    })
}

/// The demos the app downloaded and still holds (PLAN Q23).
#[tauri::command]
pub async fn downloaded_demos(state: State<'_, AppState>) -> CmdResult<Vec<hl_db::DownloadedDemo>> {
    Ok(state.db.downloaded_demos(hl_ingest::aim::VERSION, hl_ingest::aim::TIMELINE_VERSION).await?)
}

/// What one round of cleanup did.
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Cleaned {
    pub deleted: usize,
    pub bytes: i64,
    /// Demos left alone because the app has not finished reading them.
    pub skipped: usize,
}

/// Delete downloaded demos to reclaim space (PLAN Q23).
///
/// `only` names one demo; without it every downloaded demo goes. Deleting
/// is safe in the one sense that matters: the row stays, with the demos.tf
/// id on it, so anything removed here can be fetched again. What does not
/// come back on its own is a future pass version's chance to re-read it --
/// so a demo the current pass has *not* finished with is left alone unless
/// `force` says otherwise.
///
/// A POV demo is never touched. Those are the player's own recordings, TF2
/// wrote them, and `downloaded_demos` does not list them.
#[tauri::command]
pub async fn delete_downloaded_demos(
    state: State<'_, AppState>,
    only: Option<i64>,
    force: bool,
) -> CmdResult<Cleaned> {
    let all = state.db.downloaded_demos(hl_ingest::aim::VERSION, hl_ingest::aim::TIMELINE_VERSION).await?;
    let mut out = Cleaned::default();
    for d in all.iter().filter(|d| only.is_none_or(|id| id == d.demo_id)) {
        if !d.read && !force {
            out.skipped += 1;
            continue;
        }
        let Some((path, already)) = state.db.demo_path(d.demo_id).await? else { continue };
        if already {
            continue;
        }
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            // Already gone from disk is the state we wanted; record it.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::warn!(path = %path, error = %e, "could not delete demo");
                continue;
            }
        }
        state.db.mark_demo_deleted(d.demo_id).await?;
        out.deleted += 1;
        out.bytes += d.size_bytes;
    }
    Ok(out)
}

/// Copies of the database, newest first, with where they live.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Backups {
    pub dir: String,
    pub items: Vec<hl_ingest::backup::Backup>,
}

#[tauri::command]
pub async fn list_backups(state: State<'_, AppState>) -> CmdResult<Backups> {
    Ok(Backups {
        dir: hl_ingest::backup::dir(&state.db_path).to_string_lossy().into_owned(),
        items: hl_ingest::backup::list(&state.db_path),
    })
}

/// Whether every log ever played is downloaded, rather than the recent years
/// plus every official.
#[tauri::command]
pub async fn all_history(state: State<'_, AppState>) -> CmdResult<bool> {
    Ok(state.db.all_history().await?)
}

/// Turn the full history on or off. Turning it on does not fetch anything by
/// itself: the next sync sees a longer queue.
#[tauri::command]
pub async fn set_all_history(state: State<'_, AppState>, on: bool) -> CmdResult<bool> {
    state.db.set_all_history(on).await?;
    tracing::info!(on, "history policy changed");
    Ok(on)
}

/// Whether a downloaded demo is deleted once it has been read (PLAN Q23).
#[tauri::command]
pub async fn auto_delete_demos(state: State<'_, AppState>) -> CmdResult<bool> {
    Ok(state.db.auto_delete_demos().await?)
}

#[tauri::command]
pub async fn set_auto_delete_demos(state: State<'_, AppState>, on: bool) -> CmdResult<bool> {
    state.db.set_auto_delete_demos(on).await?;
    tracing::info!(on, "demo cleanup policy changed");
    Ok(on)
}

/// Delete every downloaded demo the pass has finished with.
///
/// Shared by the sweep after a sync and by the Settings button, so "has
/// been read" means the same thing in both. A demo still needed is left,
/// and so is every POV demo -- `downloaded_demos` only lists files the app
/// fetched and can fetch again.
async fn sweep_read_demos(db: &hl_db::Db) -> anyhow::Result<usize> {
    let mut n = 0;
    for d in db.downloaded_demos(hl_ingest::aim::VERSION, hl_ingest::aim::TIMELINE_VERSION).await? {
        if !d.read {
            continue;
        }
        let Some((path, already)) = db.demo_path(d.demo_id).await? else { continue };
        if already {
            continue;
        }
        // Already gone from disk is the state we were aiming for.
        if let Err(e) = std::fs::remove_file(&path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(path = %path, error = %e, "could not delete demo");
                continue;
            }
        }
        db.mark_demo_deleted(d.demo_id).await?;
        n += 1;
    }
    Ok(n)
}

/// Copy the database now, whatever the last copy's age.
#[tauri::command]
pub async fn backup_now(state: State<'_, AppState>) -> CmdResult<Option<hl_ingest::backup::Backup>> {
    Ok(hl_ingest::backup::run(&state.db, &state.db_path, true).await?)
}

/// Write a copy of the database wherever the person chose.
///
/// The automatic copies live beside the database and the uninstaller can
/// take them with it, so the advice has always been to keep one elsewhere.
/// This is how. The path comes from the file dialog in the window.
#[tauri::command]
pub async fn save_backup_as(state: State<'_, AppState>, path: String) -> CmdResult<hl_ingest::backup::Backup> {
    Ok(hl_ingest::backup::save_as(&state.db, std::path::Path::new(&path)).await?)
}

/// Your name and profile picture, as stored.
#[tauri::command]
pub async fn get_owner(state: State<'_, AppState>) -> CmdResult<Option<hl_ingest::owner::Owner>> {
    match state.db.get_me().await? {
        Some(me) => Ok(Some(hl_ingest::owner::load(&state.db, me).await?)),
        None => Ok(None),
    }
}

/// Seasons from your officials, newest first.
#[tauri::command]
pub async fn list_seasons(state: State<'_, AppState>) -> CmdResult<Vec<hl_ingest::seasons::Season>> {
    Ok(hl_ingest::seasons::list(&state.db).await?)
}

/// One class, season by season.
#[tauri::command]
pub async fn get_seasons(state: State<'_, AppState>, class: String) -> CmdResult<hl_ingest::seasons::SeasonsView> {
    let me = state
        .db
        .get_me()
        .await?
        .ok_or_else(|| CmdError::new("missing_config", "Set your SteamID first."))?;
    Ok(hl_ingest::seasons::by_season(&state.db, me, hl_core::TfClass::parse(&class)?).await?)
}

// ---- teammates and context ---------------------------------------------------

/// Your ETF2L teams and regular teammates. `all` includes pugs.
#[tauri::command]
pub async fn get_teammates(state: State<'_, AppState>, all: bool) -> CmdResult<hl_ingest::teammates::Teammates> {
    let me = state
        .db
        .get_me()
        .await?
        .ok_or_else(|| CmdError::new("missing_config", "Set your SteamID first."))?;
    let scope = if all { hl_ingest::teammates::Scope::All } else { hl_ingest::teammates::Scope::Team };
    Ok(hl_ingest::teammates::load(&state.db, me, scope).await?)
}

/// The analysis views for one match, built from its raw log. `None` without one.
#[tauri::command]
pub async fn get_match_analysis(
    state: State<'_, AppState>,
    log_id: i64,
) -> CmdResult<Option<hl_ingest::analysis::Analysis>> {
    let me = state.db.get_me().await?;
    Ok(hl_ingest::analysis::load(&state.db, log_id, me).await?)
}

/// What the demo says about one player's aim in one match (PLAN §14): every
/// kill it could answer for, and the averages over them. Empty without a demo.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AimResponse {
    /// Whose aim this is. The owner's unless the caller asked for someone
    /// else, so the cards can never be captioned with the wrong name.
    pub player: u32,
    /// The match has an STV demo. Only an STV carries all eighteen players'
    /// angles, so it is the difference between "nothing for this player" and
    /// "nothing for anyone but you" — and the UI has to say which.
    pub stv: bool,
    pub kills: Vec<hl_db::AimRow>,
    pub deaths: Vec<hl_db::DeathRow>,
    pub totals: Option<hl_db::AimTotals>,
    pub life: Option<hl_db::LifeTotals>,
    /// The same averages over every match with a demo, to compare against.
    pub career: Option<hl_db::AimTotals>,
    pub career_life: Option<hl_db::LifeTotals>,
}

/// Q27: hits on fully cloaked Spies, from the match's kept STV timelines.
/// `None` when it has none to read.
#[tauri::command]
pub async fn get_spychecks(state: State<'_, AppState>, log_id: i64) -> CmdResult<Option<hl_ingest::spy::SpyReport>> {
    Ok(hl_ingest::spy::for_log(&state.db, log_id).await?)
}

/// Q11: the cart in a numbers advantage, from the match's kept STV
/// timelines. `None` for a match with no cart to read.
#[tauri::command]
pub async fn get_cart(state: State<'_, AppState>, log_id: i64) -> CmdResult<Option<hl_ingest::cart::CartView>> {
    Ok(hl_ingest::cart::for_log(&state.db, log_id).await?)
}

#[tauri::command]
pub async fn get_aim(
    state: State<'_, AppState>,
    log_id: i64,
    player: Option<u32>,
) -> CmdResult<AimResponse> {
    let me = state.db.get_me().await?.map_or(0, |m| m.account_id());
    // No player named, or a zero from a page that has not resolved one yet,
    // means the owner: the same answer this command always gave.
    let who = player.filter(|p| *p != 0).unwrap_or(me);
    let this = hl_db::AimFilter { me: who, log_id: Some(log_id), ..Default::default() };
    let all = hl_db::AimFilter { me: who, ..Default::default() };
    Ok(AimResponse {
        player: who,
        stv: state.db.demos_for_log(log_id).await?.iter().any(|d| d.kind == "stv"),
        kills: state.db.aim_for_log(log_id, who).await?,
        deaths: state.db.deaths_for_log(log_id, who).await?,
        totals: state.db.aim_totals(&this).await?,
        life: state.db.life_totals(&this).await?,
        career: state.db.aim_totals(&all).await?,
        career_life: state.db.life_totals(&all).await?,
    })
}

/// What the match list's filters can offer: the classes and maps you have
/// actually played, most played first.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayedFilters {
    pub classes: Vec<(String, i64)>,
    pub maps: Vec<(String, i64)>,
}

#[tauri::command]
pub async fn played_filters(state: State<'_, AppState>) -> CmdResult<PlayedFilters> {
    let Some(me) = state.db.get_me().await? else {
        return Ok(PlayedFilters { classes: Vec::new(), maps: Vec::new() });
    };
    let (classes, maps) = state.db.played_classes_and_maps(me.account_id()).await?;
    Ok(PlayedFilters { classes, maps })
}

/// The scoreboards of the logs a combined log was built from. A part with no
/// stored data comes back without one; `fetch_part` gets it.
#[tauri::command]
pub async fn get_parts(state: State<'_, AppState>, log_id: i64) -> CmdResult<Vec<hl_ingest::parts::PartScore>> {
    let me = state.db.get_me().await?;
    let (weights, _) = hl_rating::Weights::load(&state.db_path.with_file_name("weights.toml"));
    Ok(hl_ingest::parts::scores(&state.db, log_id, me, &weights).await?)
}

/// Fetch one part's log from logs.tf and score it.
#[tauri::command]
pub async fn fetch_part(state: State<'_, AppState>, part_id: i64) -> CmdResult<Option<hl_ingest::MatchView>> {
    let me = state.db.get_me().await?;
    let (weights, _) = hl_rating::Weights::load(&state.db_path.with_file_name("weights.toml"));
    Ok(hl_ingest::parts::fetch(&state.db, &state.sources, part_id, me, &weights).await?)
}

/// Where you walked in one match, one route per life (PLAN §14). Empty
/// without a demo for it.
#[tauri::command]
pub async fn get_paths(state: State<'_, AppState>, log_id: i64) -> CmdResult<Vec<hl_db::PathRow>> {
    Ok(state.db.paths_for_log(log_id).await?)
}

/// A map's outline from every stored kill on it, plus your own kill and death
/// spots across all your matches there. `None` with too few kills.
#[tauri::command]
pub async fn get_map_view(state: State<'_, AppState>, map: String) -> CmdResult<Option<hl_ingest::mapview::MapView>> {
    let me = state.db.get_me().await?;
    Ok(hl_ingest::mapview::load(&state.db, &map, me).await?)
}

/// The overview image for a map, when one is saved in the app's `overviews`
/// folder and its placement is known. `None` otherwise: the kill map then
/// draws the outline from kills.
#[tauri::command]
pub async fn get_map_overview(state: State<'_, AppState>, map: String) -> CmdResult<Option<hl_ingest::overview::Overview>> {
    let dir = state.db_path.with_file_name("overviews");
    Ok(hl_ingest::overview::load(&dir, &map)?)
}

#[tauri::command]
pub async fn rawlog_stats(state: State<'_, AppState>) -> CmdResult<hl_db::RawlogStats> {
    Ok(state.db.rawlog_stats().await?)
}

#[tauri::command]
pub async fn context_counts(state: State<'_, AppState>) -> CmdResult<hl_db::ContextCounts> {
    Ok(state.db.context_counts(hl_ingest::etf2l::PLAYER_KEY).await?)
}

// ---- demos -----------------------------------------------------------------

const EV_DEMOS_INDEXED: &str = "demos://indexed";
const EV_STV_PROGRESS: &str = "stv://progress";
const EV_STV_DONE: &str = "stv://done";
const EV_STV_ERROR: &str = "stv://error";
const EV_STV_QUEUED: &str = "stv://queued";
/// What happens between the last byte and "ready": linking, reading, keeping.
const EV_STV_STAGE: &str = "stv://stage";

async fn tf_path(state: &AppState) -> CmdResult<std::path::PathBuf> {
    let tf = state
        .db
        .get_config()
        .await?
        .tf_path
        .ok_or_else(|| CmdError::new("missing_config", "Set your TF2 folder first."))?;
    Ok(std::path::PathBuf::from(tf))
}

/// Rescan the TF2 folder for demos and relink them. Header reads only, so
/// this is quick (~0.6 s for 100 demos) and safe to run on every sync.
#[tauri::command]
pub async fn scan_demos(app: AppHandle, state: State<'_, AppState>) -> CmdResult<hl_ingest::DemoIndexSummary> {
    let tf = tf_path(&state).await?;
    let summary = hl_ingest::index_demos(&state.db, &tf).await?;
    let _ = app.emit(EV_DEMOS_INDEXED, &summary);
    Ok(summary)
}

#[tauri::command]
pub async fn demo_stats(state: State<'_, AppState>) -> CmdResult<hl_db::DemoStats> {
    Ok(state.db.demo_stats().await?)
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct StvProgress {
    log_id: i64,
    bytes: u64,
    total: Option<u64>,
}

/// One step after the download, for the card's step list. `step` is
/// "linking", "reading", "keeping" or "saving"; the rest only where they
/// mean something.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct StvStage {
    log_id: i64,
    step: &'static str,
    demo: Option<usize>,
    of: Option<usize>,
    kind: Option<String>,
    pct: Option<u8>,
}

impl StvStage {
    fn linking(log_id: i64) -> Self {
        StvStage { log_id, step: "linking", demo: None, of: None, kind: None, pct: None }
    }

    fn from_read(log_id: i64, s: hl_ingest::aim::ReadStep) -> Self {
        use hl_ingest::aim::ReadStep;
        match s {
            ReadStep::Reading { demo, of, kind, pct } => {
                StvStage { log_id, step: "reading", demo: Some(demo), of: Some(of), kind: Some(kind), pct: Some(pct) }
            }
            ReadStep::Keeping { demo, of } => {
                StvStage { log_id, step: "keeping", demo: Some(demo), of: Some(of), kind: None, pct: None }
            }
            ReadStep::Saving => StvStage { log_id, step: "saving", demo: None, of: None, kind: None, pct: None },
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct StvError {
    log_id: i64,
    #[serde(flatten)]
    error: CmdError,
}

/// Where a download is in the queue. 0 is "running now".
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct StvQueued {
    log_id: i64,
    position: usize,
}

/// Demo downloads waiting their turn.
///
/// One at a time is deliberate: a SourceTV demo is a hundred megabytes and
/// the download is followed by parsing it, so two at once would be slower
/// than two in a row and much heavier on demos.tf. What was wrong was the
/// *refusal* -- a second request was answered with "a demo download is
/// already running" and dropped on the floor, while the window had already
/// drawn a card for it. It sat at "0 MB so far" until the user cancelled and
/// started it again, by which time the first had finished. Reported by
/// Gilaric, September 2026.
///
/// Now the second request waits. The queue is the order they were asked for,
/// the head is the one downloading, and everyone is told where they are.
#[derive(Default)]
pub struct DemoQueue {
    waiting: std::sync::Mutex<Vec<i64>>,
}

impl DemoQueue {
    /// Join the queue. `None` if this match is already in it -- clicking
    /// twice is not two downloads.
    fn join(&self, log_id: i64) -> Option<usize> {
        let mut q = self.waiting.lock().ok()?;
        if q.contains(&log_id) {
            return None;
        }
        q.push(log_id);
        Some(q.len() - 1)
    }

    /// Leave, wherever in the queue it was. `true` if it was still there.
    fn leave(&self, log_id: i64) -> bool {
        let Ok(mut q) = self.waiting.lock() else { return false };
        let before = q.len();
        q.retain(|x| *x != log_id);
        q.len() != before
    }

    fn contains(&self, log_id: i64) -> bool {
        self.waiting.lock().map(|q| q.contains(&log_id)).unwrap_or(false)
    }

    /// The queue as it stands, to tell everyone their new position.
    fn positions(&self) -> Vec<i64> {
        self.waiting.lock().map(|q| q.clone()).unwrap_or_default()
    }
}

/// Tell every waiting download where it now is.
fn announce(app: &AppHandle, queue: &DemoQueue) {
    for (position, log_id) in queue.positions().into_iter().enumerate() {
        let _ = app.emit(EV_STV_QUEUED, StvQueued { log_id, position });
    }
}

/// Download the demos.tf STV demo for a match in the background, then index
/// and link it. Progress streams on `stv://progress`.
/// Link a freshly downloaded demo to its match and read it, saying which
/// step it is on: this is the half-minute after the bar reaches 100%, and it
/// used to pass in silence.
async fn index_and_read(app: &AppHandle, db: &hl_db::Db, tf: &std::path::Path, log_id: i64) -> anyhow::Result<()> {
    let _ = app.emit(EV_STV_STAGE, StvStage::linking(log_id));
    hl_ingest::index_demos(db, tf).await?;
    if let Some(me) = db.get_me().await? {
        let emitter = app.clone();
        let routes = hl_ingest::aim::derive_log_with(db, me, log_id, &mut |s| {
            let _ = emitter.emit(EV_STV_STAGE, StvStage::from_read(log_id, s));
        })
        .await?;
        tracing::info!(log_id, routes, "STV demo read");
    }
    Ok(())
}

#[tauri::command]
pub async fn fetch_stv(app: AppHandle, state: State<'_, AppState>, log_id: i64) -> CmdResult<()> {
    // Joining the queue always succeeds; the wait happens in the task. The
    // one thing refused is asking twice for the same match.
    let Some(position) = state.demo_queue.join(log_id) else {
        return Ok(());
    };
    let tf = match tf_path(&state).await {
        Ok(tf) => tf,
        Err(e) => {
            state.demo_queue.leave(log_id);
            return Err(e);
        }
    };
    let db = state.db.clone();
    let sources = state.sources.clone();
    let queue = state.demo_queue.clone();
    let turn = state.demo_turn.clone();
    let _ = app.emit(EV_STV_QUEUED, StvQueued { log_id, position });

    tauri::async_runtime::spawn(async move {
        // Wait for the one in front, however it ends: the permit comes back
        // when the guard is dropped, panic or not.
        let Ok(_permit) = turn.acquire().await else { return };
        // Cancelled while it waited: nothing to do, and no event -- the card
        // is already gone.
        if !queue.contains(log_id) {
            return;
        }
        let emitter = app.clone();
        // Progress events at most every ~1%, not per network chunk.
        let mut last_pct = u64::MAX;
        let result = hl_ingest::fetch_stv(&db, &sources, &tf, log_id, |bytes, total| {
            let pct = total.map(|t| bytes * 100 / t.max(1)).unwrap_or(bytes / 1_000_000);
            if pct != last_pct {
                last_pct = pct;
                let _ = emitter.emit(EV_STV_PROGRESS, StvProgress { log_id, bytes, total });
            }
        })
        .await;
        match result {
            Ok(done) => {
                // The file is on disk; link it, then read it, so the match
                // page has everyone's movement by the time the event lands.
                if let Err(e) = index_and_read(&app, &db, &tf, log_id).await {
                    tracing::warn!(log_id, error = %format!("{e:#}"), "reading the new STV demo failed");
                }
                queue.leave(log_id);
                let _ = app.emit(EV_STV_DONE, done);
            }
            Err(e) => {
                queue.leave(log_id);
                let _ = app.emit(EV_STV_ERROR, StvError { log_id, error: CmdError::from(e) });
            }
        }
        announce(&app, &queue);
    });
    Ok(())
}

/// Drop a download that has not started yet.
///
/// Dismissing a queued card used to hide it and leave the download queued,
/// so it started later for no reason anyone could see. One already running
/// is left alone: the file is half on disk, and stopping it cleanly is a
/// bigger change than this.
#[tauri::command]
pub async fn cancel_stv(app: AppHandle, state: State<'_, AppState>, log_id: i64) -> CmdResult<bool> {
    if state.demo_queue.positions().first() == Some(&log_id) {
        return Ok(false);
    }
    let dropped = state.demo_queue.leave(log_id);
    if dropped {
        announce(&app, &state.demo_queue);
    }
    Ok(dropped)
}

// ---- logs that would not import ---------------------------------------------

/// Every log the sync gave up on, with why.
///
/// The sync has always counted them ("2 failed; next sync retries them") and
/// never said which, so the only way to find out was to read the log file.
#[tauri::command]
pub async fn failed_logs(state: State<'_, AppState>) -> CmdResult<Vec<hl_db::FailedLog>> {
    Ok(state.db.failed_logs().await?)
}

/// Forget a log's failures so the next sync tries it again. With no `log_id`,
/// forget all of them.
#[tauri::command]
pub async fn retry_failed(state: State<'_, AppState>, log_id: Option<i64>) -> CmdResult<u64> {
    match log_id {
        Some(id) => {
            state.db.clear_fetch_error(id).await?;
            Ok(1)
        }
        None => Ok(state.db.clear_all_fetch_errors().await?),
    }
}

/// Fetch one log now, by id or by a logs.tf link.
///
/// This ignores the queue and the attempt count on purpose: a log that has
/// failed three times, or that no index ever listed, is the whole reason this
/// exists. Runs in the foreground — it is one log, and the person is watching.
#[tauri::command]
pub async fn import_log(state: State<'_, AppState>, text: String) -> CmdResult<hl_ingest::Imported> {
    let log_id = hl_ingest::parse_log_id(&text).ok_or_else(|| {
        CmdError::new("bad_input", "That is not a log id or a logs.tf link.")
    })?;
    let (weights, _) = hl_rating::Weights::load(&state.db_path.with_file_name("weights.toml"));
    Ok(hl_ingest::import_log(&state.db, &state.sources, &weights, log_id).await?)
}

/// Q29: one ETF2L Highlander season's division tables; the newest when
/// `season` is not given.
#[tauri::command]
pub async fn get_leagues(state: State<'_, AppState>, season: Option<i64>) -> CmdResult<hl_ingest::leagues::SeasonView> {
    Ok(hl_ingest::leagues::season(&state.db, season).await?)
}

/// Q29: one team's page.
#[tauri::command]
pub async fn get_team(state: State<'_, AppState>, team_id: i64) -> CmdResult<Option<hl_ingest::leagues::TeamView>> {
    Ok(hl_ingest::leagues::team(&state.db, team_id).await?)
}

/// Q18: a match from a demo alone, for a server that wrote no log. Runs the
/// passes a sync would for it, so it holds the sync's turn while it does.
#[tauri::command]
pub async fn import_demo(state: State<'_, AppState>, path: String) -> CmdResult<hl_ingest::demo_import::DemoImported> {
    let _guard = BusyGuard::acquire(&state.busy).ok_or_else(|| CmdError::new("busy", "A sync is running; import the demo when it has finished."))?;
    let tf = state
        .db
        .get_config()
        .await?
        .tf_path
        .ok_or_else(|| CmdError::new("missing_config", "Set your TF2 folder in Settings first: the demo is kept in tf/demos."))?;
    let (weights, _) = hl_rating::Weights::load(&state.db_path.with_file_name("weights.toml"));
    let me = state.db.get_me().await?;
    let tf = std::path::Path::new(&tf);
    let got = match hl_ingest::demo_import::import(&state.db, &weights, tf, std::path::Path::new(&path), me, |_| {}).await {
        Ok(g) => g,
        Err(e) => {
            if let Some(a) = e.downcast_ref::<hl_ingest::demo_import::AlreadyLogged>() {
                return Err(CmdError::new("already_logged", a.to_string()));
            }
            return Err(e.into());
        }
    };
    hl_ingest::demo_import::derive(&state.db, &weights, me, |_| {}).await?;
    Ok(got)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_queue_keeps_the_order_it_was_asked_in() {
        let q = DemoQueue::default();
        assert_eq!(q.join(1), Some(0), "the first one runs straight away");
        assert_eq!(q.join(2), Some(1));
        assert_eq!(q.join(3), Some(2));
        assert_eq!(q.positions(), vec![1, 2, 3]);
    }

    #[test]
    fn asking_twice_for_the_same_match_is_not_two_downloads() {
        let q = DemoQueue::default();
        assert_eq!(q.join(7), Some(0));
        assert_eq!(q.join(7), None, "a second click changes nothing");
        assert_eq!(q.positions(), vec![7]);
    }

    #[test]
    fn leaving_moves_everyone_behind_up() {
        let q = DemoQueue::default();
        for id in [1, 2, 3] {
            q.join(id);
        }
        assert!(q.leave(2), "it was in the queue");
        assert_eq!(q.positions(), vec![1, 3], "3 is now next, not third");
        assert!(!q.leave(2), "and leaving twice is not an error, just nothing");
        assert!(q.contains(1));
        assert!(!q.contains(2));
    }

    #[test]
    fn a_finished_download_lets_the_next_one_join_again() {
        // The head leaves when it finishes; a match can then be asked for
        // again, which is what re-downloading a demo is.
        let q = DemoQueue::default();
        q.join(5);
        assert_eq!(q.join(5), None);
        q.leave(5);
        assert_eq!(q.join(5), Some(0));
    }
}

// ---- looking other people up (Q14) -----------------------------------------

/// What a player's page shows.
///
/// Everything here is *in your matches*. Their record against people you
/// have never played is not in this database and is not claimed.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerResponse {
    pub summary: hl_db::PlayerSummary,
    /// Their profile on the chosen class, built exactly the way yours is —
    /// same model, same pool, same breakdown.
    pub profile: Option<hl_rating::Profile>,
    /// The class this profile is for; their most played, unless asked.
    pub class: Option<String>,
}

/// Find a player by name or by any form of Steam ID.
#[tauri::command]
pub async fn search_players(state: State<'_, AppState>, query: String) -> CmdResult<Vec<hl_db::PlayerHit>> {
    Ok(state.db.search_players(&query, 25).await?)
}

/// One player's page.
///
/// The profile is `load_profile` with their account instead of the owner's,
/// which is the whole reason this feature is small: they are rated by the
/// same model against the same pool, because that pool is *built* from
/// these players. One asymmetry is worth knowing and is shown in the UI —
/// the owner is held out of the baseline so they are never compared with
/// themselves, and everybody else is in it.
#[tauri::command]
pub async fn get_player(
    state: State<'_, AppState>,
    account_id: u32,
    class: Option<String>,
) -> CmdResult<PlayerResponse> {
    let owner = state
        .db
        .get_me()
        .await?
        .ok_or_else(|| CmdError::new("missing_config", "Set your SteamID first."))?;
    let summary = state
        .db
        .player_summary(account_id, owner.account_id(), hl_rating::MODEL_VERSION)
        .await?
        .ok_or_else(|| CmdError::new("not_found", "Nobody by that id has played in your matches."))?;

    let chosen = match class.or_else(|| summary.classes.first().map(|c| c.class.clone())) {
        Some(c) => Some(hl_core::TfClass::parse(&c)?),
        None => None,
    };
    let them = hl_core::SteamId::from_account_id(account_id);
    let profile = match chosen {
        Some(c) => hl_ingest::load_profile(&state.db, them, c, None, None).await?,
        None => None,
    };
    Ok(PlayerResponse { summary, profile, class: chosen.map(|c| c.as_str().to_string()) })
}
