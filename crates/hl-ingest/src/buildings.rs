//! Engineer buildings on the map (Emiel's ticket, October 2026): every
//! sentry, dispenser and teleporter a match's demos saw, where it stood and
//! when, so the Paths layer can show where a straight jump in a route went
//! -- through a teleporter -- and what a life walked past.
//!
//! Read from the demo timeline's object rows, which record each building
//! whenever it changes and a "gone" row when it disappears. A teleporter's
//! end (entrance or exit) is in demos read since this was added; for older
//! ones it is `None`, and the map works it out from the routes' jumps.

use crate::aim::account_of;
use anyhow::Result;
use hl_db::Db;
use hl_demos::timeline::{ObjectRow, Stored, Timeline};
use serde::Serialize;
use std::collections::HashMap;

/// One building, from when it was put down to when it went.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Building {
    pub demo_id: i64,
    /// "sentry", "dispenser" or "teleporter".
    pub kind: String,
    /// A teleporter's end: "entrance" or "exit", when the demo was read
    /// with it recorded.
    pub end: Option<String>,
    /// 2 RED, 3 BLU.
    pub team: Option<u8>,
    pub builder: Option<u32>,
    pub builder_name: Option<String>,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    /// Demo ticks, the frame the routes' points are in.
    pub from_tick: i64,
    pub to_tick: i64,
    /// The highest level it reached.
    pub level: u8,
}

/// An entity moving more than this between rows was picked up and put
/// down elsewhere: a new building, not the same one sliding.
const MOVED: i32 = 64;

/// The buildings in one demo's object rows, in the order they were put down.
/// `tick_of` turns a row's time into a demo tick; `who` a builder's user id
/// into an account and name.
pub fn from_rows(demo_id: i64, rows: &[ObjectRow], tick_of: impl Fn(u32) -> i64, who: impl Fn(u16) -> (Option<u32>, Option<String>)) -> Vec<Building> {
    let mut open: HashMap<u32, Building> = HashMap::new();
    let mut out = Vec::new();
    let is_building = |k: &str| matches!(k, "sentry" | "dispenser" | "teleporter");
    for r in rows {
        let at = tick_of(r.t);
        if r.kind == "gone" {
            if let Some(mut b) = open.remove(&r.entity) {
                b.to_tick = at;
                out.push(b);
            }
            continue;
        }
        if !is_building(&r.kind) {
            continue;
        }
        let moved = open.get(&r.entity).is_some_and(|b| {
            b.kind != r.kind || (b.x - r.pos[0]).abs() > MOVED || (b.y - r.pos[1]).abs() > MOVED || (b.z - r.pos[2]).abs() > MOVED
        });
        if moved {
            if let Some(mut b) = open.remove(&r.entity) {
                b.to_tick = at;
                out.push(b);
            }
        }
        match open.get_mut(&r.entity) {
            Some(b) => {
                b.to_tick = at;
                b.level = b.level.max(r.level.unwrap_or(1));
                if b.end.is_none() {
                    b.end = r.what.as_deref().map(str::to_string);
                }
            }
            None => {
                let (builder, builder_name) = r.builder.map(&who).unwrap_or((None, None));
                open.insert(
                    r.entity,
                    Building {
                        demo_id,
                        kind: r.kind.to_string(),
                        end: if r.kind == "teleporter" { r.what.as_deref().map(str::to_string) } else { None },
                        team: r.team,
                        builder,
                        builder_name,
                        x: r.pos[0],
                        y: r.pos[1],
                        z: r.pos[2],
                        from_tick: at,
                        to_tick: at,
                        level: r.level.unwrap_or(1),
                    },
                );
            }
        }
    }
    // Still standing when the demo ended.
    out.extend(open.into_values());
    out.sort_by_key(|b| (b.from_tick, b.x, b.y));
    out
}

/// Every building in a match's demos. SourceTV demos when there are any
/// (they see the whole map); otherwise your own.
pub async fn for_log(db: &Db, log_id: i64) -> Result<Vec<Building>> {
    let demos = db.demos_for_log(log_id).await?;
    let stv = demos.iter().any(|d| d.kind == "stv");
    let mut out = Vec::new();
    for demo in demos.into_iter().filter(|d| (d.kind == "stv") == stv) {
        let Some(row) = db.timeline(demo.demo_id).await? else { continue };
        let s = stored(row);
        let demo_id = demo.demo_id;
        // Decoding inflates a few megabytes: off the async threads.
        let found = tokio::task::spawn_blocking(move || -> Result<Vec<Building>> {
            let tl = Timeline::decode(&s)?;
            // The builder is the game's handle on the player's entity: its
            // low 11 bits are the entity (24587 = 0x600B: entity 11).
            let who = |handle: u16| match tl.slots_of_entity(u32::from(handle & 0x7FF)).next() {
                Some(slot) => (account_of(&tl.people[slot].steamid), Some(tl.people[slot].name.clone())),
                None => (None, None),
            };
            Ok(from_rows(demo_id, &tl.objects, |t| i64::from(tl.tick_of(t)), who))
        })
        .await??;
        out.extend(found);
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
    use std::borrow::Cow;

    fn row(t: u32, entity: u32, kind: &'static str, pos: [i32; 3], what: Option<&'static str>) -> ObjectRow {
        ObjectRow {
            t,
            entity,
            kind: Cow::Borrowed(kind),
            pos,
            builder: Some(7),
            team: Some(3),
            level: Some(1),
            health: Some(150),
            building: false,
            sapped: false,
            by: None,
            what: what.map(Cow::Borrowed),
            vel: None,
        }
    }

    #[test]
    fn a_building_lasts_from_its_first_row_to_its_gone() {
        let rows = vec![
            row(10, 1, "teleporter", [100, 200, 0], Some("entrance")),
            row(12, 2, "teleporter", [2000, 900, 0], Some("exit")),
            row(15, 3, "sentry", [500, 500, 0], None),
            row(40, 3, "gone", [500, 500, 0], None),
            // The cart is not a building.
            row(41, 9, "cart", [0, 0, 0], None),
        ];
        let b = from_rows(5, &rows, |t| i64::from(t) * 2, |_| (Some(42), Some("engie".into())));
        assert_eq!(b.len(), 3);
        let sentry = b.iter().find(|x| x.kind == "sentry").unwrap();
        assert_eq!((sentry.from_tick, sentry.to_tick), (30, 80));
        let ends: Vec<Option<&str>> = b.iter().filter(|x| x.kind == "teleporter").map(|x| x.end.as_deref()).collect();
        assert_eq!(ends, vec![Some("entrance"), Some("exit")]);
        assert_eq!(b[0].builder, Some(42));
    }

    #[test]
    fn a_building_carried_somewhere_else_is_a_new_one() {
        let rows = vec![row(10, 1, "sentry", [100, 100, 0], None), row(20, 1, "sentry", [900, 100, 0], None), row(30, 1, "gone", [900, 100, 0], None)];
        let b = from_rows(5, &rows, i64::from, |_| (None, None));
        assert_eq!(b.len(), 2);
        assert_eq!((b[0].x, b[0].to_tick), (100, 20));
        assert_eq!((b[1].x, b[1].from_tick, b[1].to_tick), (900, 20, 30));
    }
}
