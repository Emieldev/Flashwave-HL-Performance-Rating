//! A match from a demo alone (Q18, beowulf): the server had no logs.tf
//! config, so no log exists, and the demo is all there is.
//!
//! `hl_demos::synth` turns the demo into a server log and a logs.tf summary;
//! this stores them under a log id of its own, so the match is an ordinary
//! one to every page and pass. The id is negative -- logs.tf's are positive,
//! so the two can never meet -- and made from the file's name, so importing
//! the same demo twice replaces the first rather than adding a second.
//!
//! The demo has to live where the app looks for demos: a folder scan drops
//! any demo row whose file it did not find. One picked from elsewhere is
//! copied into `tf/demos`.

use crate::sync::{store_log_json, Imported};
use anyhow::{bail, Context, Result};
use hl_core::SteamId;
use hl_db::{Db, TimelineRow};
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// How imported logs are linked to their demo: kept through every rescan.
pub const METHOD: &str = "import";

/// The log id an imported demo is stored under.
pub fn log_id_for(file_name: &str) -> i64 {
    let h = file_name.to_ascii_lowercase().bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3));
    -((h % 1_000_000_000) as i64) - 1
}

/// When the demo started, as a server writes its local time into a log:
/// from the name an STV server or Demo Support gives the file, else from
/// the file's own time less its length.
pub fn wall_start(file_name: &str, mtime_unix: i64, duration_s: i64) -> i64 {
    // `match-20260823-1956-pl_upward_f12.dem`
    let stv = file_name.split('-').collect::<Vec<_>>();
    if stv.len() >= 3 && stv[0].eq_ignore_ascii_case("match") && stv[1].len() == 8 && stv[2].len() == 4 {
        let s = format!("{} {}", stv[1], stv[2]);
        if let Ok(t) = chrono::NaiveDateTime::parse_from_str(&format!("{s}00"), "%Y%m%d %H%M%S") {
            return t.and_utc().timestamp();
        }
    }
    if let Some(s) = hl_demos::scan::filename_time(file_name) {
        if let Ok(t) = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S") {
            return t.and_utc().timestamp();
        }
    }
    mtime_unix - duration_s
}

/// Where the demo is kept: as it is inside the TF folder's demo folders,
/// else copied into `tf/demos`.
fn place(tf: &Path, file: &Path) -> Result<PathBuf> {
    let dirs = [tf.to_path_buf(), tf.join("demos"), tf.join("demos").join("stv")];
    let parent = file.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let same = |a: &Path, b: &Path| a.canonicalize().ok().zip(b.canonicalize().ok()).is_some_and(|(x, y)| x == y);
    if dirs.iter().any(|d| same(d, &parent)) {
        return Ok(file.to_path_buf());
    }
    let name = file.file_name().context("no file name")?;
    let dest = tf.join("demos").join(name);
    let size = std::fs::metadata(file)?.len();
    if std::fs::metadata(&dest).map(|m| m.len()) .ok() != Some(size) {
        std::fs::create_dir_all(tf.join("demos"))?;
        std::fs::copy(file, &dest).with_context(|| format!("copying the demo to {}", dest.display()))?;
    }
    Ok(dest)
}

fn zip_text(text: &str) -> Result<Vec<u8>> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        z.start_file("log.log", zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated))?;
        z.write_all(text.as_bytes())?;
        z.finish()?;
    }
    Ok(buf.into_inner())
}

/// The demo already belongs to a log that exists: open that match instead.
#[derive(Debug)]
pub struct AlreadyLogged {
    pub log_id: i64,
    pub share: f64,
}

impl std::fmt::Display for AlreadyLogged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "this demo already belongs to log {} ({:.0}% of it), which is in your matches", self.log_id, self.share * 100.0)
    }
}

impl std::error::Error for AlreadyLogged {}

/// What an import did.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoImported {
    #[serde(flatten)]
    pub log: Imported,
    pub rounds: usize,
    pub kills: usize,
    /// Where the demo is kept now.
    pub path: String,
}

/// Import `file` as a match of its own. `progress` names each step.
pub async fn import(db: &Db, w: &hl_rating::Weights, tf: &Path, file: &Path, me: Option<SteamId>, mut progress: impl FnMut(&'static str)) -> Result<DemoImported> {
    progress("Copying the demo");
    let path = place(tf, file)?;
    let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let bytes = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let header = hl_demos::DemoHeader::parse(&bytes)?;
    drop(bytes);
    if header.map.is_empty() {
        bail!("{file_name} does not look like a TF2 demo");
    }
    let rate = header.tick_rate().unwrap_or(66.67);
    let is_stv = header.server.is_empty() || header.recorder.eq_ignore_ascii_case("SourceTV") || header.recorder.to_ascii_lowercase().contains("sourcetv");
    let owner = if is_stv { String::new() } else { me.map(|m| m.to_steamid3()).unwrap_or_default() };

    // The demo's row comes from the folder scan like any other's. If the
    // scan links it to a log that exists, this is not a match without one.
    crate::index_demos(db, tf).await?;
    let here = path.canonicalize()?;
    let demo_id = db
        .demos_named(&file_name)
        .await?
        .into_iter()
        .find(|(_, p)| Path::new(p).canonicalize().ok().as_ref() == Some(&here))
        .map(|(id, _)| id)
        .context("the demo was not found by the folder scan")?;
    let log_id = log_id_for(&file_name);
    // Any link to a logs.tf log (positive ids) means one exists: a demo
    // that is half of a combined log covers a small share of it, and is no
    // less that log's.
    if let Some((real, share)) = db.links_of_demo(demo_id).await?.into_iter().find(|(l, _)| *l > 0) {
        return Err(AlreadyLogged { log_id: real, share }.into());
    }

    progress("Reading the demo");
    let read_path = path.clone();
    let (stored, tl) = tokio::task::spawn_blocking(move || -> Result<_> {
        let (_, stored) = hl_demos::aim::pass_recording(&read_path, &owner, rate, Some(hl_demos::timeline::DEFAULT_STRIDE), &mut |_| {})?;
        let stored = stored.context("the demo gave no timeline")?;
        let tl = hl_demos::timeline::Timeline::decode(&stored)?;
        Ok((stored, tl))
    })
    .await??;
    if tl.people.len() < 4 {
        bail!("{file_name} has {} players in it: not a match", tl.people.len());
    }

    progress("Building the match from it");
    let mtime = std::fs::metadata(&path)?.modified()?.duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64;
    let start = wall_start(&file_name, mtime, tl.seconds(tl.end()) as i64);
    let title = format!("{} (from the demo)", file_name.trim_end_matches(".dem"));
    let synth = hl_demos::synth::synthesize(&tl, &header.map, start, &title);
    if synth.kills == 0 {
        bail!("{file_name} has no kills in a live round: nothing to rate");
    }

    let json = serde_json::to_string(&synth.json)?;
    let log = store_log_json(db, w, log_id, &json).await?;
    db.store_rawlog(log_id, &zip_text(&synth.text)?).await?;
    crate::kills::derive_log(db, log_id).await?;

    progress("Linking the demo");
    // Made by hand: an imported log has no upload time to be placed by.
    let path_s = path.to_string_lossy().into_owned();
    db.add_demo_link(demo_id, log_id, METHOD, 1.0).await?;
    // The log was written on the demo's own clock, so the demo starts
    // exactly where the log does: what the aim pass needs to line the two up.
    db.set_demo_start(demo_id, start as f64).await?;
    // And the log is placed on that clock now, not at the next scan.
    crate::index_demos(db, tf).await?;
    db.put_timeline(
        demo_id,
        &TimelineRow {
            version: stored.version,
            tick_rate: stored.tick_rate,
            stride: i64::from(stored.stride),
            head: stored.head.clone(),
            samples: stored.samples.clone(),
            changes: stored.changes.clone(),
            objects: stored.objects.clone(),
            events: stored.events.clone(),
            raw_bytes: stored.raw_bytes as i64,
            stored_bytes: stored.stored_bytes() as i64,
        },
    )
    .await?;

    Ok(DemoImported { log, rounds: synth.rounds, kills: synth.kills, path: path_s })
}

/// The passes a sync would run for a new log, for the one just imported:
/// each round's map, the fights pass, aim from the demo, then the rating
/// (which every sync ends with, since new matches move the baselines).
pub async fn derive(db: &Db, w: &hl_rating::Weights, me: Option<SteamId>, mut progress: impl FnMut(&'static str)) -> Result<()> {
    progress("Working out each round's map");
    crate::maps::resolve_all(db).await?;
    progress("Reading the fights");
    crate::fights::derive_all(db, false, |_, _| {}).await?;
    if let Some(me) = me {
        progress("Reading aim from the demo");
        if let Err(e) = crate::aim::derive_all(db, me, false, |_, _, _| {}).await {
            tracing::warn!(error = %format!("{e:#}"), "reading aim from an imported demo failed");
        }
    }
    progress("Rating");
    crate::rate_all(db, me, w, |_| {}).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_file_is_the_same_log() {
        let a = log_id_for("match-20260823-1956-pl_upward_f12.dem");
        assert!(a < 0);
        assert_eq!(a, log_id_for("MATCH-20260823-1956-pl_upward_f12.dem"));
        assert_ne!(a, log_id_for("match-20260823-1957-pl_upward_f12.dem"));
    }

    #[test]
    fn the_start_comes_from_the_name_when_it_has_one() {
        // 2026-08-23 19:56 as the server's clock, written as if UTC.
        assert_eq!(wall_start("match-20260823-1956-pl_upward_f12.dem", 0, 0), 1_787_514_960);
        assert_eq!(wall_start("flashwav2026-09-17_21-00-24.dem", 0, 0), 1_789_678_824);
        assert_eq!(wall_start("my_demo.dem", 10_000, 600), 9_400);
    }
}
