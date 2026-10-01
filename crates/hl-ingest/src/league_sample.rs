//! The league sample (Flashy, September 2026): ETF2L Highlander officials
//! from every division, so ratings can be read against the league instead
//! of only the owner's matches, and a player can be told which division the
//! people they play against come from.
//!
//! **Finding them is cheap.** ETF2L's results give every official's division
//! and tier ([`crate::leagues::fetch_seasons`]); trends.tf lists every ETF2L
//! Highlander log with the ETF2L match it belongs to. Joined, every log has
//! a division without asking logs.tf anything.
//!
//! **Choosing them** ([`select`]): per tier, [`MATCHES_PER_TIER`] matches,
//! from the last [`PREFER_YEARS`] years unless that cannot fill it, taken
//! round-robin across maps so every map in the pool is in there.
//!
//! **Downloading them is slow on purpose** ([`step`]): one unit of work a
//! call, and the caller waits between calls. logs.tf's JSON first, then its
//! raw server logs; while logs.tf is refusing us, more.tf's copy stands in
//! for the JSON. ETF2L's match pages (who played) come alongside, from a
//! different host. Everything is state in the database, so it picks up
//! wherever it was after a restart.
//!
//! Nothing here touches the owner's matches: the sample lives in its own
//! tables (migration 0033) until it is decided how it should count.

use crate::sources::Sources;
use anyhow::Result;
use hl_db::{Db, LeagueLogRow};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

/// Matches wanted from each division. It began at 300; Flashy wants every
/// official, and no division has played this many in six years, so it is
/// "all of them" with a guard.
pub const MATCHES_PER_TIER: usize = 5_000;
/// Taken from the last three years first...
pub const PREFER_YEARS: i64 = 3;
/// ...and from as far back as six when that does not fill a division. The
/// player catalogue (every official's roster) covers the same six years.
pub const MAX_YEARS: i64 = 6;
/// ETF2L seasons read for their results: two a year, six years.
pub const SEASONS_BACK: i64 = 12;
/// Shorter logs are restarts and forfeits, not played maps.
const MIN_DURATION_S: i64 = 300;
/// How often the lists are read again for new officials.
const REDISCOVER_S: i64 = 24 * 3600;
/// A log neither source could give after this many tries is left alone.
const MAX_JSON_ATTEMPTS: i64 = 3;

const YEAR_S: i64 = 365 * 24 * 3600;
const KEY_ON: &str = "league_sample_on";
const KEY_DISCOVERED: &str = "league_sample_discovered_at";
/// The window the last discovery used: a wider one lists again from scratch.
const KEY_WINDOW: &str = "league_sample_window";

/// Changes when discovery must run again at once: a wider window, or a
/// name parser that reads competitions it skipped before (2: the unnumbered
/// seasons 28-31 and the preseason cups; 3: AFA 2025, between 34 and 35;
/// 4: the other Highlander tournaments; 5: every Grand Final, finals played
/// as a round of the season and divisions named only in their playoffs).
fn window() -> String {
    format!("{SEASONS_BACK}/{MAX_YEARS}/5")
}

pub async fn enabled(db: &Db) -> Result<bool> {
    Ok(db.get_setting(KEY_ON).await?.as_deref() == Some("1"))
}

pub async fn set_enabled(db: &Db, on: bool) -> Result<()> {
    db.set_setting(KEY_ON, if on { "1" } else { "0" }).await
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// A real map name, not a combined log's title ("Vigil") or a mod map.
fn played_map(map: &str) -> bool {
    let m = map.to_ascii_lowercase();
    ["koth_", "pl_", "cp_", "ctf_", "plr_"].iter().any(|p| m.starts_with(p))
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Discovered {
    pub competitions: usize,
    pub results: usize,
    pub logs_listed: usize,
    pub picked_matches: usize,
    pub picked_logs: usize,
}

/// Read ETF2L's results and trends.tf's list of ETF2L logs, then choose.
pub async fn discover(db: &Db, sources: &Sources, mut progress: impl FnMut(&str)) -> Result<Discovered> {
    let mut out = Discovered::default();
    progress("Reading ETF2L's results");
    let etf2l = crate::leagues::fetch_seasons(db, sources, SEASONS_BACK, 0, |_, _| {}).await?;
    out.competitions = etf2l.competitions;
    out.results = etf2l.results;

    // trends.tf, newest first, down to the window's start -- or, once the
    // back catalogue is held that far, to a week before the newest log held.
    let (known, oldest_known, newest_known) = db.league_logs_known().await?;
    let floor = now() - MAX_YEARS * YEAR_S;
    let back_catalogue_held = known > 0 && oldest_known.is_some_and(|o| o <= floor + 60 * 24 * 3600);
    let stop_at = if back_catalogue_held { newest_known.map_or(floor, |n| (n - 7 * 24 * 3600).max(floor)) } else { floor };
    let mut path: Option<String> = None;
    for page in 0..2000 {
        progress(&format!("Listing ETF2L logs on trends.tf ({} so far)", out.logs_listed));
        let (rows, next) = sources.trends_league_page(path.as_deref()).await?;
        if rows.is_empty() {
            break;
        }
        let keep: Vec<LeagueLogRow> = rows
            .iter()
            .filter(|r| r.duplicate_of.as_ref().is_none_or(|d| d.is_empty()))
            .filter_map(|r| {
                Some(LeagueLogRow {
                    log_id: r.logid,
                    etf2l_match_id: r.matchid?,
                    map: r.map.as_deref()?,
                    played_at: r.time?,
                    duration_s: r.duration,
                    title: r.title.as_deref(),
                })
            })
            .collect();
        out.logs_listed += keep.len();
        db.upsert_league_logs(&keep).await?;
        let oldest = rows.iter().filter_map(|r| r.time).min().unwrap_or(0);
        if oldest < stop_at || page == 1999 {
            break;
        }
        match next {
            Some(n) => path = Some(n),
            None => break,
        }
    }

    progress("Choosing the matches");
    let (matches, logs) = select(db).await?;
    out.picked_matches = matches;
    out.picked_logs = logs;
    db.set_setting(KEY_DISCOVERED, &now().to_string()).await?;
    db.set_setting(KEY_WINDOW, &window()).await?;
    Ok(out)
}

/// Choose the sample: per tier, [`MATCHES_PER_TIER`] matches, newest first,
/// the last [`PREFER_YEARS`] years before anything older, round-robin across
/// maps. Returns `(matches, logs)` picked.
pub async fn select(db: &Db) -> Result<(usize, usize)> {
    let t = now();
    let candidates = db.league_candidates().await?;
    // tier -> match -> (played_at, logs, maps)
    let mut by_tier: BTreeMap<i64, HashMap<i64, (i64, Vec<i64>, HashSet<String>)>> = BTreeMap::new();
    // Grand Finals always, whatever the caps below: they are what the gold
    // medals and the MVPs are read from (Flashy: "check all the grand
    // finals and download any that are missing").
    let mut finals: Vec<i64> = Vec::new();
    for c in candidates.iter().filter(|c| played_map(&c.map) && c.duration_s >= MIN_DURATION_S && c.played_at >= t - MAX_YEARS * YEAR_S) {
        // Today's ladder by the division's name first: "Division 2" is
        // High, "Freshest" Fresh Meat, though no regular division of that
        // season shares the name to borrow a tier from.
        let Some(tier) = crate::leagues::canonical_tier(&c.division).or(c.tier) else { continue };
        if c.is_final {
            finals.push(c.log_id);
        }
        let e = by_tier.entry(tier).or_default().entry(c.match_id).or_insert_with(|| (c.played_at, Vec::new(), HashSet::new()));
        e.0 = e.0.max(c.played_at);
        e.1.push(c.log_id);
        e.2.insert(hl_core::maps::map_base(&c.map));
    }

    let mut picked_logs: Vec<i64> = Vec::new();
    let mut picked_matches = 0;
    for matches in by_tier.values() {
        let recent_from = t - PREFER_YEARS * YEAR_S;
        let mut chosen: HashSet<i64> = HashSet::new();
        // The recent years first; older only if a division cannot fill.
        for window in [recent_from, t - MAX_YEARS * YEAR_S] {
            // map -> its matches in this window, newest first
            let mut per_map: BTreeMap<&str, Vec<(i64, i64)>> = BTreeMap::new();
            for (id, (at, _, maps)) in matches.iter().filter(|(_, (at, ..))| *at >= window) {
                for m in maps {
                    per_map.entry(m.as_str()).or_default().push((*at, *id));
                }
            }
            for list in per_map.values_mut() {
                list.sort_by(|a, b| b.cmp(a));
            }
            let mut cursors: Vec<(&str, usize)> = per_map.keys().map(|m| (*m, 0)).collect();
            // Round-robin: each map in turn gives its newest match not yet
            // taken, so a map played less often is still in the sample.
            while chosen.len() < MATCHES_PER_TIER {
                let mut took = false;
                for (map, at) in cursors.iter_mut() {
                    let list = &per_map[*map];
                    while *at < list.len() && chosen.contains(&list[*at].1) {
                        *at += 1;
                    }
                    if *at < list.len() {
                        chosen.insert(list[*at].1);
                        took = true;
                        if chosen.len() == MATCHES_PER_TIER {
                            break;
                        }
                    }
                }
                if !took {
                    break;
                }
            }
            if chosen.len() >= MATCHES_PER_TIER {
                break;
            }
        }
        picked_matches += chosen.len();
        for id in &chosen {
            picked_logs.extend(&matches[id].1);
        }
    }
    for id in finals {
        if !picked_logs.contains(&id) {
            picked_logs.push(id);
        }
    }
    db.set_league_picked(&picked_logs).await?;
    Ok((picked_matches, picked_logs.len()))
}

/// What one [`step`] did.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Step {
    Discovered(Discovered),
    Json { log_id: i64, source: &'static str },
    Raw { log_id: i64, found: bool },
    Roster { match_id: i64 },
    /// logs.tf is refusing us and there is nothing more.tf can do meanwhile.
    Waiting,
    /// Everything picked is here; nothing to do until the next discovery.
    Done,
    /// A request failed in a way worth backing off from.
    Failed { what: String },
}

/// Do one unit of work. The caller waits between calls: this is the pace.
/// `doing` is told what each request is before it is made, so a window can
/// say what the job is doing rather than only what it has done.
pub async fn step(db: &Db, sources: &Sources, mut doing: impl FnMut(&str) + Send) -> Result<Step> {
    let due = db
        .get_setting(KEY_DISCOVERED)
        .await?
        .and_then(|s| s.parse::<i64>().ok())
        .is_none_or(|at| now() - at > REDISCOVER_S)
        || db.get_setting(KEY_WINDOW).await?.as_deref() != Some(window().as_str());
    if due {
        return Ok(Step::Discovered(discover(db, sources, &mut doing).await?));
    }

    // ETF2L is another host: one match page each step, beside the logs.
    let roster = db.league_matches_without_roster(1, now() - MAX_YEARS * YEAR_S).await?.first().copied();
    if let Some(match_id) = roster {
        // A page ETF2L will not give is marked read by nothing: tried again
        // next discovery. Its failure does not stop the log work.
        doing(&format!("Reading who played ETF2L match {match_id}"));
        let _ = crate::leagues::fetch_match_detail(db, sources, match_id).await;
    }

    if !sources.logstf_resting() {
        // logs.tf: JSON first, then more.tf's stand-ins again, then raw logs.
        let json = db.league_json_todo(1, true, MAX_JSON_ATTEMPTS).await?.first().copied();
        if let Some(log_id) = json {
            doing(&format!("Downloading log {log_id} from logs.tf"));
            return Ok(match sources.logstf_log(log_id).await {
                Ok(body) => {
                    db.put_league_json(log_id, &body, "logs.tf").await?;
                    Step::Json { log_id, source: "logs.tf" }
                }
                Err(e) if crate::http::unreachable(&e) => Step::Failed { what: format!("logs.tf: {e:#}") },
                Err(e) => {
                    db.league_json_failed(log_id).await?;
                    Step::Failed { what: format!("log {log_id}: {e:#}") }
                }
            });
        }
        if let Some(log_id) = db.league_raw_todo(1).await?.first().copied() {
            doing(&format!("Downloading the server log of {log_id} from logs.tf"));
            return Ok(match sources.logstf_rawlog(log_id).await {
                Ok(zip) => {
                    let found = zip.is_some();
                    db.put_league_raw(log_id, zip.as_deref()).await?;
                    Step::Raw { log_id, found }
                }
                Err(e) => Step::Failed { what: format!("logs.tf raw log {log_id}: {e:#}") },
            });
        }
    } else if let Some(log_id) = db.league_json_todo(1, false, MAX_JSON_ATTEMPTS).await?.first().copied() {
        // Resting from logs.tf: more.tf's copy of the JSON meanwhile.
        doing(&format!("logs.tf is resting: downloading log {log_id} from more.tf"));
        return Ok(match sources.moretf_log(log_id).await {
            Ok(Some(body)) => {
                let v: serde_json::Value = serde_json::from_str(&body)?;
                match crate::moretf::stand_in(&v) {
                    Ok(s) => {
                        db.put_league_json(log_id, &serde_json::to_string(&s.json)?, "more.tf").await?;
                        Step::Json { log_id, source: "more.tf" }
                    }
                    Err(e) => {
                        db.league_json_failed(log_id).await?;
                        Step::Failed { what: format!("more.tf log {log_id}: {e:#}") }
                    }
                }
            }
            Ok(None) => {
                db.league_json_failed(log_id).await?;
                Step::Failed { what: format!("more.tf has no log {log_id}") }
            }
            Err(e) => Step::Failed { what: format!("more.tf: {e:#}") },
        });
    } else {
        return Ok(if roster.is_some() { Step::Roster { match_id: roster.unwrap_or(0) } } else { Step::Waiting });
    }
    Ok(match roster {
        Some(match_id) => Step::Roster { match_id },
        None => Step::Done,
    })
}

/// Where the sample stands, for Settings.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub enabled: bool,
    /// The player catalogue: officials in the window, rosters read, and
    /// players named in them.
    pub officials: i64,
    pub rosters_read: i64,
    pub players: i64,
    pub discovered_at: Option<i64>,
    pub logs_listed: i64,
    pub tiers: Vec<hl_db::TierProgress>,
    pub bytes: i64,
    pub logstf_resting: bool,
    pub target_per_tier: usize,
}

/// Progress rows are counted by ETF2L's tier and name; this puts them on
/// today's ladder ("Division 5A" with Low). Maps are summed, so a division
/// merged from two eras may count a map twice.
fn progress_by_ladder(rows: Vec<hl_db::TierProgress>) -> Vec<hl_db::TierProgress> {
    let mut out: BTreeMap<i64, hl_db::TierProgress> = BTreeMap::new();
    for r in rows {
        let tier = crate::leagues::canonical_tier(&r.division).unwrap_or(r.tier);
        match out.get_mut(&tier) {
            None => {
                let name = crate::leagues::TIER_NAMES.get(tier as usize).map_or(r.division.clone(), |n| n.to_string());
                out.insert(tier, hl_db::TierProgress { tier, division: name, ..r });
            }
            Some(e) => {
                e.matches += r.matches;
                e.logs += r.logs;
                e.json_logstf += r.json_logstf;
                e.json_moretf += r.json_moretf;
                e.raw += r.raw;
                e.raw_missing += r.raw_missing;
                e.maps = e.maps.max(r.maps);
                e.rosters += r.rosters;
                e.oldest = e.oldest.min(r.oldest).or(e.oldest.or(r.oldest));
                e.newest = e.newest.max(r.newest);
            }
        }
    }
    out.into_values().collect()
}

/// The sample's size on disk. Summing thousands of stored logs takes over a
/// second and Settings asks every ten, so the answer is kept five minutes.
async fn bytes_held(db: &Db) -> Result<i64> {
    static HELD: std::sync::Mutex<Option<(std::time::Instant, i64)>> = std::sync::Mutex::new(None);
    if let Some((at, bytes)) = *HELD.lock().unwrap() {
        if at.elapsed() < std::time::Duration::from_secs(300) {
            return Ok(bytes);
        }
    }
    let bytes = db.league_bytes().await?;
    *HELD.lock().unwrap() = Some((std::time::Instant::now(), bytes));
    Ok(bytes)
}

pub async fn status(db: &Db, sources: &Sources) -> Result<Status> {
    let (officials, rosters_read, players) = db.league_catalogue(now() - MAX_YEARS * YEAR_S).await?;
    Ok(Status {
        officials,
        rosters_read,
        players,
        enabled: enabled(db).await?,
        discovered_at: db.get_setting(KEY_DISCOVERED).await?.and_then(|s| s.parse().ok()),
        logs_listed: db.league_logs_known().await?.0,
        tiers: progress_by_ladder(db.league_progress().await?),
        bytes: bytes_held(db).await?,
        logstf_resting: sources.logstf_resting(),
        target_per_tier: MATCHES_PER_TIER,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_real_maps_count() {
        assert!(played_map("pl_vigil_rc10") && played_map("koth_product_final") && played_map("cp_steel_f12"));
        assert!(!played_map("Vigil"), "a combined log's title is not a map");
    }
}
