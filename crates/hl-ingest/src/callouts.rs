//! Callouts (Q28, Flashy): named zones on each map, and where players spent
//! their time in them.
//!
//! Callouts are the community's words, not data: no file in the game says
//! where "cliff" is, and teams disagree at the edges. So they are stored as
//! translations are (Q13): a seed set ships with the app (`callouts/*.json`
//! in the repo, drafts drawn from what the TF2 wiki describes), and a copy
//! the owner edits is kept in `<data>/callouts/` and always wins. Zones are
//! polygons in game units, so they survive a new overview render.

use anyhow::{Context, Result};
use hl_db::Db;
use hl_demos::map_base;
use hl_demos::timeline::{Stored, Timeline};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Zone {
    pub name: String,
    /// Game units, `[x, y]`, in order around the edge.
    pub points: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CalloutFile {
    pub map: String,
    /// Not yet checked by someone who plays the map.
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub source: String,
    /// Most specific first: a position counts for the first zone holding it.
    #[serde(default)]
    pub zones: Vec<Zone>,
    /// Callouts known by name and not yet drawn.
    #[serde(default)]
    pub names: Vec<String>,
    /// Where this copy came from: "yours", "built in" or "none". Not saved.
    #[serde(default, skip_deserializing)]
    pub origin: String,
}

const BUILT_IN: &[(&str, &str)] = &[
    ("product", include_str!("../../../callouts/product.json")),
    ("upward", include_str!("../../../callouts/upward.json")),
    ("steel", include_str!("../../../callouts/steel.json")),
    ("swiftwater", include_str!("../../../callouts/swiftwater.json")),
    ("proot", include_str!("../../../callouts/proot.json")),
    ("ashville", include_str!("../../../callouts/ashville.json")),
    ("vigil", include_str!("../../../callouts/vigil.json")),
];

fn user_path(data: &Path, base: &str) -> PathBuf {
    data.join("callouts").join(format!("{base}.json"))
}

/// Maps with a built-in seed.
pub fn built_in_bases() -> impl Iterator<Item = &'static str> {
    BUILT_IN.iter().map(|(b, _)| *b)
}

/// Maps the player has their own callouts for.
pub fn user_bases(data: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(data.join("callouts")) else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok()?.strip_suffix(".json").map(str::to_string))
        .collect()
}

/// The callouts for `map`: the owner's copy, else the built-in seed, else
/// an empty file to start drawing on.
pub fn load(data: &Path, map: &str) -> Result<CalloutFile> {
    let base = map_base(map);
    let path = user_path(data, &base);
    if path.exists() {
        let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let mut f: CalloutFile = serde_json::from_str(&text).with_context(|| format!("{} is not a callout file", path.display()))?;
        f.origin = "yours".into();
        return Ok(f);
    }
    if let Some((_, text)) = BUILT_IN.iter().find(|(b, _)| *b == base) {
        let mut f: CalloutFile = serde_json::from_str(text).context("a built-in callout file")?;
        f.origin = "built in".into();
        return Ok(f);
    }
    Ok(CalloutFile { map: base, origin: "none".into(), ..Default::default() })
}

/// Save the owner's copy. Written whole, through a temporary file, so a crash
/// mid-write cannot leave half a map.
pub fn save(data: &Path, map: &str, file: &CalloutFile) -> Result<CalloutFile> {
    let base = map_base(map);
    let path = user_path(data, &base);
    std::fs::create_dir_all(path.parent().context("no folder")?)?;
    let mut f = file.clone();
    f.map = base.clone();
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&f)?)?;
    std::fs::rename(&tmp, &path)?;
    load(data, &base)
}

/// Drop the owner's copy and go back to the built-in one.
pub fn reset(data: &Path, map: &str) -> Result<CalloutFile> {
    let path = user_path(data, &map_base(map));
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    load(data, map)
}

fn inside(x: f64, y: f64, poly: &[[f64; 2]]) -> bool {
    let mut c = false;
    let n = poly.len();
    for i in 0..n {
        let ([x1, y1], [x2, y2]) = (poly[i], poly[(i + 1) % n]);
        if (y1 > y) != (y2 > y) && x < (x2 - x1) * (y - y1) / (y2 - y1) + x1 {
            c = !c;
        }
    }
    c
}

fn area(poly: &[[f64; 2]]) -> f64 {
    let n = poly.len();
    let twice: f64 = (0..n).map(|i| {
        let ([x1, y1], [x2, y2]) = (poly[i], poly[(i + 1) % n]);
        x1 * y2 - x2 * y1
    }).sum();
    (twice / 2.0).abs()
}

impl CalloutFile {
    /// The zone holding `(x, y)`, if any. Where zones overlap the smallest
    /// wins -- Shack inside Flank is Shack -- whatever order they are in, so
    /// an edited file cannot shadow a zone by where it put it.
    pub fn zone_at(&self, x: f64, y: f64) -> Option<usize> {
        self.zones
            .iter()
            .enumerate()
            .filter(|(_, z)| z.points.len() >= 3 && inside(x, y, &z.points))
            .min_by(|(_, a), (_, b)| area(&a.points).total_cmp(&area(&b.points)))
            .map(|(i, _)| i)
    }
}

// ---- time in each zone, from STV timelines ----------------------------

/// One player's seconds, summed over a match's demos: name, seconds on each
/// class, team, seconds alive, seconds in each zone.
type Tally = (String, [u32; 10], u8, u32, HashMap<usize, u32>);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneTime {
    pub zone: String,
    pub seconds: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerPositions {
    pub account_id: u32,
    pub name: String,
    /// 1 Scout ... 9 Engineer (`tf_demo_parser`'s numbering): their most played.
    pub class: u8,
    /// 2 RED, 3 BLU: the player's team for the whole match, as the log has
    /// it -- not the colour worn in a demo, which swaps between the halves
    /// and maps of a combined log.
    pub team: u8,
    pub alive_s: u32,
    /// Most time first; time in no zone is left out.
    pub zones: Vec<ZoneTime>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionsView {
    pub map: String,
    pub zones: usize,
    pub draft: bool,
    pub players: Vec<PlayerPositions>,
}

/// Where each player spent their live time, zone by zone, from the match's
/// STV timelines: one position a second. `None` without an STV timeline or
/// without any drawn zone for the map.
pub async fn positions(db: &Db, data: &Path, log_id: i64, map: &str) -> Result<Option<PositionsView>> {
    let callouts = load(data, map)?;
    if callouts.zones.is_empty() {
        return Ok(None);
    }
    let mut acc: HashMap<u32, Tally> = HashMap::new();
    let mut any = false;
    for demo in db.demos_for_log(log_id).await?.into_iter().filter(|d| d.kind == "stv") {
        let Some(row) = db.timeline(demo.demo_id).await? else { continue };
        let stored = Stored {
            version: row.version,
            tick_rate: row.tick_rate,
            stride: row.stride as u32,
            head: row.head,
            samples: row.samples,
            changes: row.changes,
            objects: row.objects,
            events: row.events,
            raw_bytes: row.raw_bytes as usize,
        };
        let zones = callouts.clone();
        let per_slot = tokio::task::spawn_blocking(move || -> Result<_> {
            let tl = Timeline::decode(&stored)?;
            let step = tl.tick_rate.round().max(1.0) as u32;
            let mut out = Vec::new();
            for (slot, track) in tl.tracks.iter().enumerate() {
                let Some(account) = crate::aim::account_of(&tl.people[slot].steamid) else { continue };
                let stretches = tl.stretches(slot);
                let mut classes = [0u32; 10];
                let mut team_ticks: HashMap<u8, u32> = HashMap::new();
                let mut alive = 0;
                let mut in_zone: HashMap<usize, u32> = HashMap::new();
                let mut next = 0u32;
                let mut j = 0;
                for s in &track.samples {
                    if s.t < next {
                        continue;
                    }
                    next = s.t + step;
                    while j + 1 < stretches.len() && stretches[j].1 <= s.t {
                        j += 1;
                    }
                    let Some((from, to, n)) = stretches.get(j) else { continue };
                    if !(*from <= s.t && s.t < *to && n.live()) {
                        continue;
                    }
                    alive += 1;
                    classes[usize::from(n.class.min(9))] += 1;
                    *team_ticks.entry(n.team).or_default() += 1;
                    if let Some(z) = zones.zone_at(f64::from(s.pos[0]), f64::from(s.pos[1])) {
                        *in_zone.entry(z).or_default() += 1;
                    }
                }
                if alive > 0 {
                    let team = team_ticks.into_iter().max_by_key(|(_, n)| *n).map_or(0, |(t, _)| t);
                    out.push((account, tl.people[slot].name.clone(), classes, team, alive, in_zone));
                }
            }
            Ok(out)
        })
        .await??;
        any = true;
        for (account, name, classes, team, alive, zones) in per_slot {
            let e = acc.entry(account).or_insert_with(|| (name, [0; 10], team, 0, HashMap::new()));
            for (i, c) in classes.iter().enumerate() {
                e.1[i] += c;
            }
            e.3 += alive;
            for (z, s) in zones {
                *e.4.entry(z).or_default() += s;
            }
        }
    }
    if !any {
        return Ok(None);
    }
    // Sides from the log (Flashy): a combined log's demos can have the teams
    // in each other's colours, and a player's colour in whichever demo came
    // first put half of one team with the other. The log's team is one per
    // player for the whole match. Anyone the log does not have keeps the
    // colour they wore most.
    let sides = log_sides(db, log_id).await?;
    let mut players: Vec<PlayerPositions> = acc
        .into_iter()
        .map(|(account_id, (name, classes, worn, alive_s, zones))| {
            let team = sides.get(&account_id).copied().unwrap_or(worn);
            let class = (1..=9u8).max_by_key(|c| classes[usize::from(*c)]).unwrap_or(0);
            let mut zones: Vec<ZoneTime> = zones.into_iter().map(|(z, seconds)| ZoneTime { zone: callouts.zones[z].name.clone(), seconds }).collect();
            zones.sort_by_key(|z| std::cmp::Reverse(z.seconds));
            PlayerPositions { account_id, name, class, team, alive_s, zones }
        })
        .collect();
    players.sort_by_key(|p| (p.team, p.class));
    Ok(Some(PositionsView { map: callouts.map.clone(), zones: callouts.zones.len(), draft: callouts.draft, players }))
}

/// Each player's team for the whole match, from the stored log: 2 RED, 3 BLU.
async fn log_sides(db: &Db, log_id: i64) -> Result<HashMap<u32, u8>> {
    let Some(json) = db.raw_log(log_id).await? else { return Ok(HashMap::new()) };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) else { return Ok(HashMap::new()) };
    let Ok(log) = crate::normalize::normalize(log_id, &value) else { return Ok(HashMap::new()) };
    Ok(log
        .players
        .iter()
        .map(|p| (p.id.account_id(), if p.team == hl_core::matchdata::Team::Red { 2 } else { 3 }))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn where_zones_overlap_the_smaller_one_wins_whatever_the_order() {
        let f = CalloutFile {
            zones: vec![
                Zone { name: "flank".into(), points: vec![[-50.0, -50.0], [50.0, -50.0], [50.0, 50.0], [-50.0, 50.0]] },
                Zone { name: "shack".into(), points: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]] },
            ],
            ..Default::default()
        };
        assert_eq!(f.zone_at(5.0, 5.0), Some(1), "inside the shack is the shack, though flank comes first");
        assert_eq!(f.zone_at(-20.0, 30.0), Some(0));
        assert_eq!(f.zone_at(99.0, 0.0), None);
    }

    #[test]
    fn every_built_in_file_reads() {
        for (base, text) in BUILT_IN {
            let f: CalloutFile = serde_json::from_str(text).unwrap_or_else(|e| panic!("{base}: {e}"));
            assert_eq!(&f.map, base);
            assert!(f.zones.iter().all(|z| z.points.len() >= 3), "{base} has a zone with under three corners");
        }
    }

    #[test]
    fn a_saved_copy_wins_and_a_reset_brings_the_seed_back() {
        let dir = std::env::temp_dir().join(format!("hl-callouts-{}", std::process::id()));
        let seed = load(&dir, "koth_product_final").unwrap();
        assert_eq!(seed.origin, "built in");
        let mut mine = seed.clone();
        mine.zones.truncate(1);
        mine.draft = false;
        let saved = save(&dir, "koth_product_final", &mine).unwrap();
        assert_eq!((saved.origin.as_str(), saved.zones.len(), saved.draft), ("yours", 1, false));
        assert_eq!(reset(&dir, "koth_product_final").unwrap().zones.len(), seed.zones.len());
        let _ = std::fs::remove_dir_all(dir);
    }
}
