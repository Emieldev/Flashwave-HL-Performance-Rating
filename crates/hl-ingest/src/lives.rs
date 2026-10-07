//! A player's lives on one map, across every match a demo followed them
//! through (Q57, Emiel): every Spy cross they made on Product, split by
//! playing RED and BLU.
//!
//! The routes are stored per life already (`demo_path`); what they lack is
//! the class and the side, which the demo's timeline has. Those are read
//! from the timeline when asked, so nothing has to be read from the demo
//! files again. A life whose demo has no timeline keeps its route with the
//! class and side unknown.

use crate::aim::account_of;
use anyhow::Result;
use hl_core::maps::map_base;
use hl_db::Db;
use hl_demos::timeline::{Field, Stored, Timeline};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

/// A map the player has lives on, with how many matches hold them.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapWithLives {
    /// Without its version: `product`.
    pub map: String,
    /// The full name of the newest version played, for the map's picture.
    pub name: String,
    pub matches: usize,
    pub lives: usize,
}

/// The maps `account_id` has routes on, most matches first.
pub async fn maps(db: &Db, account_id: u32) -> Result<Vec<MapWithLives>> {
    let mut by: HashMap<String, (String, Option<i64>, HashSet<i64>, usize)> = HashMap::new();
    for r in db.lives_index(account_id).await? {
        let Some(map) = r.map.filter(|m| !m.is_empty()) else { continue };
        let e = by.entry(map_base(&map)).or_insert_with(|| (map.clone(), r.played_at, HashSet::new(), 0));
        if r.played_at > e.1 {
            e.0 = map.clone();
            e.1 = r.played_at;
        }
        e.2.insert(r.log_id);
        e.3 += 1;
    }
    let mut out: Vec<MapWithLives> = by.into_iter().map(|(map, (name, _, logs, lives))| MapWithLives { map, name, matches: logs.len(), lives }).collect();
    out.sort_by(|a, b| b.matches.cmp(&a.matches).then_with(|| a.map.cmp(&b.map)));
    Ok(out)
}

/// One life on the map.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LifeOnMap {
    pub log_id: i64,
    pub played_at: Option<i64>,
    pub demo_id: i64,
    /// The recording was SourceTV's rather than the player's own.
    pub stv: bool,
    /// `scout` ... `spy`, as it was when the life began; `None` where the
    /// demo has no timeline.
    pub class: Option<String>,
    /// `Red` or `Blue`, likewise.
    pub team: Option<String>,
    pub round_num: Option<i64>,
    pub died: bool,
    pub seconds: f64,
    /// `(x, y)` in map units, about four a second.
    pub points: Vec<(i32, i32)>,
}

/// Everything the view draws, and the counts it states.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LivesOnMap {
    pub map: String,
    /// Matches on this map with a demo that followed the player.
    pub matches: usize,
    /// Of those, with a SourceTV demo; the rest are their own recordings.
    pub stv_matches: usize,
    pub lives: Vec<LifeOnMap>,
}

/// TF2's class numbers, as the timeline stores them.
fn class_name(n: u8) -> Option<&'static str> {
    Some(match n {
        1 => "scout",
        2 => "sniper",
        3 => "soldier",
        4 => "demoman",
        5 => "medic",
        6 => "heavy",
        7 => "pyro",
        8 => "spy",
        9 => "engineer",
        _ => return None,
    })
}

fn team_name(n: u8) -> Option<&'static str> {
    match n {
        2 => Some("Red"),
        3 => Some("Blue"),
        _ => None,
    }
}

/// `account_id`'s lives on `map` (any version of it).
pub async fn on_map(db: &Db, account_id: u32, map: &str) -> Result<LivesOnMap> {
    let want = map_base(map);
    // Which lives, by match and demo.
    let mut picked: BTreeMap<i64, (Option<i64>, HashMap<i64, (bool, HashSet<i64>)>)> = BTreeMap::new();
    for r in db.lives_index(account_id).await? {
        if r.map.as_deref().map(map_base) != Some(want.clone()) {
            continue;
        }
        let log = picked.entry(r.log_id).or_insert_with(|| (r.played_at, HashMap::new()));
        log.1.entry(r.demo_id).or_insert_with(|| (r.kind == "stv", HashSet::new())).1.insert(r.seq);
    }

    let mut out = LivesOnMap { map: want, ..Default::default() };
    for (log_id, (played_at, demos)) in picked {
        out.matches += 1;
        if demos.values().any(|(stv, _)| *stv) {
            out.stv_matches += 1;
        }
        let paths = db.paths_for_log(log_id).await?;
        for (demo_id, (stv, seqs)) in demos {
            let lives: Vec<&hl_db::PathRow> = paths.iter().filter(|p| p.demo_id == demo_id && p.account_id == account_id && seqs.contains(&p.seq)).collect();
            if lives.is_empty() {
                continue;
            }
            let rate = 66.67;
            // Class and side from the timeline, at each life's middle.
            let who = match db.timeline(demo_id).await? {
                Some(row) => {
                    let s = stored(row);
                    let ticks: Vec<u32> = lives.iter().map(|l| ((l.from_tick + l.to_tick) / 2).max(0) as u32).collect();
                    tokio::task::spawn_blocking(move || -> Result<Vec<(Option<u8>, Option<u8>)>> {
                        let tl = Timeline::decode(&s)?;
                        let slot = tl.people.iter().position(|p| account_of(&p.steamid) == Some(account_id));
                        Ok(ticks
                            .iter()
                            .map(|&tick| {
                                let (Some(slot), Some(t)) = (slot, tl.t_of(tick)) else { return (None, None) };
                                let class = match tl.at(slot, t, |f| matches!(f, Field::Class(_))) {
                                    Some(Field::Class(c)) => Some(c),
                                    _ => None,
                                };
                                let team = match tl.at(slot, t, |f| matches!(f, Field::Team(_))) {
                                    Some(Field::Team(c)) => Some(c),
                                    _ => None,
                                };
                                (class, team)
                            })
                            .collect())
                    })
                    .await??
                }
                None => vec![(None, None); lives.len()],
            };
            for (l, (class, team)) in lives.into_iter().zip(who) {
                out.lives.push(LifeOnMap {
                    log_id,
                    played_at,
                    demo_id,
                    stv,
                    class: class.and_then(class_name).map(str::to_string),
                    team: team.and_then(team_name).map(str::to_string),
                    round_num: l.round_num,
                    died: l.died,
                    seconds: ((l.to_tick - l.from_tick).max(0) as f64 / rate * 10.0).round() / 10.0,
                    points: l.points.iter().map(|&(_, x, y, _)| (x, y)).collect(),
                });
            }
        }
    }
    Ok(out)
}

fn stored(row: hl_db::TimelineRow) -> Stored {
    Stored {
        version: row.version,
        tick_rate: row.tick_rate,
        stride: row.stride as u32,
        head: row.head,
        samples: row.samples,
        changes: row.changes,
        objects: row.objects,
        events: row.events,
        raw_bytes: row.raw_bytes as usize,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_and_team_numbers_read_as_names() {
        assert_eq!(class_name(8), Some("spy"));
        assert_eq!(class_name(6), Some("heavy"));
        assert_eq!(class_name(0), None);
        assert_eq!(team_name(2), Some("Red"));
        assert_eq!(team_name(3), Some("Blue"));
        assert_eq!(team_name(1), None);
    }
}
