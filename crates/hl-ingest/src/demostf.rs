//! Finding the SourceTV demo of a match demos.tf never got linked to.
//!
//! trends.tf attaches a demos.tf id to 70% of this account's logs and nothing
//! to the other 30%, so a third of matches offered no way to reach their demo
//! — the download button had nothing to download. demos.tf does know about
//! them; a demo carries its map and the second it started, and that is enough
//! to say which log it belongs to.

use crate::sources::{DemosTfMeta, Sources};
use anyhow::Result;
use hl_core::maps::map_base;
use hl_core::SteamId;
use hl_db::Db;
use serde::Serialize;

/// How far apart a demo's time and a log's may be and still be the same match.
///
/// MEASURED, and not what it looked like: demos.tf stamps a demo when the
/// recording *started*, which is when the server loaded the map — before the
/// log, which starts when the match goes live. Against this account's logs
/// the demo runs 5 to 25 minutes early. So the window leans backwards.
///
/// It stays wide on the other side because a combined log is stamped at its
/// first round and its later maps' demos come long after. The map has to
/// match as well, and where two logs could claim one demo the nearer takes
/// it, so width costs little.
const EARLY_S: i64 = 45 * 60;
const LATE_S: i64 = 3 * 3600;

/// For a log with no map to match on: how far after the demo's time the log
/// may begin, beyond the demo's own length. The demo must have been
/// recording when the log went live, which another match's demo was not.
const UNMAPPED_SLACK_S: i64 = 60;

/// Pages to walk before giving up. 100 demos a page, so this is deep history.
const MAX_PAGES: usize = 20;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    /// Demos demos.tf listed for this player.
    pub listed: usize,
    /// Logs that gained a demos.tf id they did not have.
    pub matched: usize,
}

/// A log with no demo id yet: `(log_id, played_at, maps)`.
///
/// `maps` is a list because a combined log has several, and its own map field
/// is free text the uploader typed — "upward + steel", or an emoji. The maps
/// resolved per round are the ones to match on.
pub type Unlinked = (i64, i64, Vec<String>);

/// Which log each demo belongs to: `(log_id, demo_id, the demo's map)`.
///
/// By map and time first. Then a log whose map nobody knows (Q30) may take a
/// demo on time alone, from what the logs with a map left over, but only a
/// demo that was recording when the log began: one that started before it and
/// ran past its start. Where two logs could claim one demo, the closer in time
/// takes it, and no log takes two.
pub fn match_demos(logs: &[Unlinked], demos: &[DemosTfMeta]) -> Vec<(i64, i64, Option<String>)> {
    let mut mapped: Vec<(i64, i64, i64)> = Vec::new(); // (gap, log_id, demo_id)
    let mut unmapped: Vec<(i64, i64, i64)> = Vec::new();
    for d in demos {
        let (Some(at), Some(map)) = (d.time, d.map.as_deref()) else { continue };
        let map = map_base(map);
        for (log_id, played_at, log_maps) in logs {
            let gap = at - played_at;
            if log_maps.is_empty() {
                let Some(len) = d.duration else { continue };
                if at <= *played_at && *played_at <= at + len + UNMAPPED_SLACK_S {
                    unmapped.push((gap.abs(), *log_id, d.id));
                }
                continue;
            }
            if !log_maps.iter().any(|m| map_base(m) == map) {
                continue;
            }
            if (-EARLY_S..=LATE_S).contains(&gap) {
                mapped.push((gap.abs(), *log_id, d.id));
            }
        }
    }
    let mut used_logs = std::collections::HashSet::new();
    let mut used_demos = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (by_time_alone, mut pairs) in [(false, mapped), (true, unmapped)] {
        // Closest first, then each log and each demo is spoken for once.
        pairs.sort_unstable();
        for (_, log_id, demo_id) in pairs {
            if used_logs.contains(&log_id) || used_demos.contains(&demo_id) {
                continue;
            }
            used_logs.insert(log_id);
            used_demos.insert(demo_id);
            // The demo's map is kept only where the log had none: a log
            // with maps already knows them better than one demo's listing.
            let map = by_time_alone.then(|| demos.iter().find(|d| d.id == demo_id).and_then(|d| d.map.clone())).flatten();
            out.push((log_id, demo_id, map));
        }
    }
    out
}

/// Ask demos.tf what it has for this player, and fill in the ids trends.tf
/// never gave us. Stops as soon as a page is older than the oldest log that
/// still needs one.
pub async fn index(db: &Db, sources: &Sources, me: SteamId) -> Result<Found> {
    let logs = db.logs_without_demo_id().await?;
    if logs.is_empty() {
        return Ok(Found::default());
    }
    let oldest = logs.iter().map(|(_, at, _)| *at).min().unwrap_or(0);

    let steamid64 = me.to_steamid64();
    let mut all: Vec<DemosTfMeta> = Vec::new();
    let mut before = None;
    for _ in 0..MAX_PAGES {
        let page = sources.demostf_for_player(&steamid64, before).await?;
        if page.is_empty() {
            break;
        }
        let earliest = page.iter().filter_map(|d| d.time).min().unwrap_or(0);
        all.extend(page);
        // Nothing older than the oldest log we care about is worth asking for.
        if earliest <= oldest {
            break;
        }
        before = Some(earliest);
    }

    let pairs = match_demos(&logs, &all);
    db.set_demos_tf_ids(&pairs).await?;
    Ok(Found { listed: all.len(), matched: pairs.len() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo(id: i64, map: &str, time: i64) -> DemosTfMeta {
        DemosTfMeta {
            id,
            url: String::new(),
            name: String::new(),
            map: Some(map.into()),
            duration: None,
            time: Some(time),
        }
    }

    /// `(log, demo)` pairs, without the map, for the older tests.
    fn ids(v: Vec<(i64, i64, Option<String>)>) -> Vec<(i64, i64)> {
        v.into_iter().map(|(l, d, _)| (l, d)).collect()
    }

    fn long(id: i64, map: &str, time: i64, len: i64) -> DemosTfMeta {
        DemosTfMeta { duration: Some(len), ..demo(id, map, time) }
    }

    #[test]
    fn a_log_with_no_map_takes_the_demo_that_was_recording_when_it_began() {
        let hour = 3600;
        let logs = vec![(1, 10 * hour, vec![]), (2, 12 * hour, vec!["pl_upward_f12".into()])];
        let demos = vec![
            // Started ten minutes before log 1, ran 40: recording when it began.
            long(100, "pl_vigil_rc10", 10 * hour - 600, 2400),
            // Ended before log 1 began: another match.
            long(101, "koth_product_final", 10 * hour - 3000, 1800),
            long(200, "pl_upward_f12", 12 * hour - 600, 2400),
        ];
        let mut got = match_demos(&logs, &demos);
        got.sort_unstable();
        assert_eq!(got, vec![(1, 100, Some("pl_vigil_rc10".into())), (2, 200, None)], "only the log with no map keeps the demo's");
    }

    #[test]
    fn a_log_with_a_map_keeps_its_demo_from_an_unmapped_neighbour() {
        let hour = 3600;
        // Both logs began while demo 100 was recording; the one whose map
        // matches takes it, and the unmapped one gets nothing.
        let logs = vec![(1, 10 * hour, vec![]), (2, 10 * hour + 300, vec!["pl_vigil_rc10".into()])];
        let demos = vec![long(100, "pl_vigil_rc10", 10 * hour - 60, 3000)];
        assert_eq!(ids(match_demos(&logs, &demos)), vec![(2, 100)]);
    }

    #[test]
    fn with_no_length_a_demo_is_never_given_to_a_log_with_no_map() {
        let logs = vec![(1, 1000, vec![])];
        assert!(match_demos(&logs, &[demo(100, "pl_vigil_rc10", 900)]).is_empty());
    }

    #[test]
    fn a_demo_belongs_to_the_log_it_followed_on_the_same_map() {
        let hour = 3600;
        let logs = vec![
            (1, 10 * hour, vec!["pl_upward_f12".to_string()]),
            (2, 12 * hour, vec!["pl_vigil_rc10".to_string()]),
        ];
        let demos = vec![
            // Recording started ten minutes before the log went live.
            demo(100, "pl_upward_f12", 10 * hour - 600),
            // A different version of the same map still counts.
            demo(200, "pl_vigil_rc7", 12 * hour + 2400),
            // An hour and a half early: somebody else's game.
            demo(300, "pl_upward_f12", 8 * hour - 1800),
            // Right map, days later.
            demo(400, "pl_upward_f12", 40 * hour),
        ];
        let mut got = ids(match_demos(&logs, &demos));
        got.sort_unstable();
        assert_eq!(got, vec![(1, 100), (2, 200)]);
    }

    #[test]
    fn the_nearer_log_takes_the_demo_and_no_log_takes_two() {
        let hour = 3600;
        // Two Upward games the same evening, one demo between them.
        let logs = vec![(1, 10 * hour, vec!["pl_upward_f12".into()]), (2, 11 * hour, vec!["pl_upward_f12".into()])];
        let demos = vec![demo(100, "pl_upward_f12", 11 * hour - 300)];
        assert_eq!(ids(match_demos(&logs, &demos)), vec![(2, 100)], "the one it actually followed");
    }

    #[test]
    fn a_combined_log_matches_on_any_of_its_maps() {
        let hour = 3600;
        let logs = vec![(1, 10 * hour, vec!["pl_upward_f12".into(), "cp_steel_f12".into()])];
        // Its own map field would read "upward + steel" and match nothing.
        let demos = vec![demo(100, "cp_steel_f12", 10 * hour - 480)];
        assert_eq!(ids(match_demos(&logs, &demos)), vec![(1, 100)]);
    }

    #[test]
    fn a_map_that_does_not_match_is_never_claimed() {
        let logs = vec![(1, 0, vec!["koth_product_final".into()])];
        assert!(match_demos(&logs, &[demo(100, "pl_upward_f12", 600)]).is_empty());
    }
}
