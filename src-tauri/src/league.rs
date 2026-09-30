//! The league sample running in the background (Flashy): one step at a
//! time, for as long as the app is open and the sample is switched on.
//!
//! The pace is the point. logs.tf has banned this address for asking too
//! much, so a step is one request and the loop waits between steps; while
//! logs.tf is resting, more.tf gives the JSON instead; and while the owner's
//! own sync runs the sample waits, so their games come first. Everything it
//! has done is in the database: closing the app loses nothing but the wait.

use hl_db::Db;
use hl_ingest::league_sample::{self, Step};
use hl_ingest::sources::Sources;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Between two requests to logs.tf: with its own 2 s throttle, one about
/// every 6 s, ~600 an hour.
const STEP_GAP: Duration = Duration::from_secs(4);
/// Nothing to do but wait for logs.tf's rest to end.
const WAITING_GAP: Duration = Duration::from_secs(60);
/// A request that failed: back off before the next.
const FAILED_GAP: Duration = Duration::from_secs(30);
/// Everything picked is here: look again in a while for new officials.
const DONE_GAP: Duration = Duration::from_secs(30 * 60);
/// Switched off, or the owner's sync is running.
const IDLE_GAP: Duration = Duration::from_secs(20);

pub fn spawn(db: Db, sources: Arc<Sources>, busy: Arc<AtomicBool>) {
    tauri::async_runtime::spawn(async move {
        // Let the startup passes have the database first.
        tokio::time::sleep(Duration::from_secs(30)).await;
        loop {
            let gap = if !league_sample::enabled(&db).await.unwrap_or(false) || busy.load(Ordering::Acquire) {
                IDLE_GAP
            } else {
                match league_sample::step(&db, &sources).await {
                    Ok(Step::Discovered(d)) => {
                        tracing::info!(matches = d.picked_matches, logs = d.picked_logs, listed = d.logs_listed, "league sample chosen");
                        STEP_GAP
                    }
                    Ok(Step::Json { log_id, source }) => {
                        tracing::debug!(log_id, source, "league sample log");
                        STEP_GAP
                    }
                    Ok(Step::Raw { .. } | Step::Roster { .. }) => STEP_GAP,
                    Ok(Step::Waiting) => WAITING_GAP,
                    Ok(Step::Done) => DONE_GAP,
                    Ok(Step::Failed { what }) => {
                        tracing::warn!(%what, "league sample step failed");
                        FAILED_GAP
                    }
                    Err(e) => {
                        tracing::warn!(error = %format!("{e:#}"), "league sample step failed");
                        FAILED_GAP * 4
                    }
                }
            };
            tokio::time::sleep(gap).await;
        }
    });
}
