//! Ping (Q44) and Pyro reflects (Q45, ivg) for a match page, read off the
//! timelines kept of its demos. Nothing is stored, as with spychecks.
//!
//! STV demos are read when the match has any: they carry every player and
//! every projectile. Otherwise the owner's own recordings, which carry every
//! player's ping too but only the reflects near the person recording -- the
//! view says which (`stv`).

use crate::aim::account_of;
use anyhow::Result;
use hl_db::Db;
use hl_demos::demostats::{Outcome, PingLine};
use hl_demos::timeline::{Stored, Timeline};
use serde::Serialize;
use std::collections::HashMap;

/// Jumps land this long before the reflect, as spychecks' do.
const LEAD_SECONDS: f64 = 3.0;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoStats {
    /// Demos read, and whether they were STVs (everyone, every projectile).
    pub demos: usize,
    pub stv: bool,
    /// Timelines recorded before ping and reflects were (v1): their demo
    /// needs reading again, and may be gone.
    pub too_old: usize,
    pub pings: Vec<PlayerPing>,
    pub pyros: Vec<PyroLine>,
    pub reflects: Vec<ReflectRow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerPing {
    pub account_id: u32,
    pub name: String,
    pub avg: f64,
    pub median: u16,
    pub min: u16,
    pub max: u16,
    /// `(from s, to s, peak ms)`, seconds into the demo.
    pub spikes: Vec<(f64, f64, u16)>,
    /// `(s, ms)`, for a sparkline.
    pub points: Vec<(f64, u16)>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PyroLine {
    pub account_id: u32,
    pub name: String,
    pub reflects: u32,
    pub hits: u32,
    pub misses: u32,
    pub sent_back: u32,
    pub unknown: u32,
    pub kills: u32,
    pub damage: u32,
    /// Reflects of something headed at them or a teammate (an estimate),
    /// out of those the demo showed long enough to tell.
    pub threats: u32,
    pub judged: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReflectRow {
    pub demo_id: i64,
    pub at_s: f64,
    pub jump_tick: u32,
    pub by: Option<u32>,
    pub what: String,
    pub outcome: Outcome,
    pub victims: Vec<u32>,
    pub damage: u32,
    pub killed: bool,
    pub threat: Option<bool>,
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

/// `None` when none of the match's demos has a timeline.
pub async fn for_log(db: &Db, log_id: i64) -> Result<Option<DemoStats>> {
    let demos = db.demos_for_log(log_id).await?;
    let stv = demos.iter().any(|d| d.kind == "stv");
    let mut out = DemoStats { demos: 0, stv, too_old: 0, pings: Vec::new(), pyros: Vec::new(), reflects: Vec::new() };
    // account -> (each demo's ping line, name)
    let mut lines: HashMap<u32, (Vec<PingLine>, String)> = HashMap::new();
    let mut pyros: HashMap<u32, PyroLine> = HashMap::new();
    for demo in demos.into_iter().filter(|d| (d.kind == "stv") == stv) {
        let Some(row) = db.timeline(demo.demo_id).await? else { continue };
        let s = stored(row);
        // Decoding inflates a few megabytes: off the async threads.
        let (tl, pings, reflects) = tokio::task::spawn_blocking(move || -> Result<_> {
            let tl = Timeline::decode(&s)?;
            let pings = hl_demos::demostats::pings(&tl);
            let reflects = hl_demos::demostats::reflects(&tl);
            Ok((tl, pings, reflects))
        })
        .await??;
        out.demos += 1;
        if tl.version < 2 {
            out.too_old += 1;
            continue;
        }
        let account = |slot: usize| account_of(&tl.people[slot].steamid);
        for p in pings {
            let Some(a) = account(p.slot) else { continue };
            let e = lines.entry(a).or_insert_with(|| (Vec::new(), tl.people[p.slot].name.clone()));
            e.0.push(p);
        }
        let lead = (LEAD_SECONDS * tl.tick_rate).round() as u32;
        for r in reflects {
            let by = r.slot.and_then(account);
            out.reflects.push(ReflectRow {
                demo_id: demo.demo_id,
                at_s: r.at_s,
                jump_tick: tl.tick_of(r.t.saturating_sub(lead)),
                by,
                what: r.what.clone(),
                outcome: r.outcome,
                victims: r.victims.iter().filter_map(|v| account(*v)).collect(),
                damage: r.damage,
                killed: r.killed,
                threat: r.threat,
            });
            let Some(by) = by else { continue };
            let name = r.slot.map(|s| tl.people[s].name.clone()).unwrap_or_default();
            let p = pyros.entry(by).or_insert_with(|| PyroLine { account_id: by, name, ..Default::default() });
            p.reflects += 1;
            match r.outcome {
                Outcome::Hit => p.hits += 1,
                Outcome::Miss => p.misses += 1,
                Outcome::SentBack => p.sent_back += 1,
                Outcome::Unknown => p.unknown += 1,
            }
            p.kills += u32::from(r.killed);
            p.damage += r.damage;
            if let Some(t) = r.threat {
                p.judged += 1;
                p.threats += u32::from(t);
            }
        }
    }
    if out.demos == 0 {
        return Ok(None);
    }
    // One line per player across demos, weighted by how long each covers.
    for (account_id, (ls, name)) in lines {
        let total: f64 = ls.iter().map(|l| l.seconds).sum::<f64>().max(1e-9);
        let longest = ls.iter().max_by(|a, b| a.seconds.total_cmp(&b.seconds)).cloned().unwrap_or_default();
        out.pings.push(PlayerPing {
            account_id,
            name,
            avg: ls.iter().map(|l| l.avg * l.seconds).sum::<f64>() / total,
            median: longest.median,
            min: ls.iter().map(|l| l.min).min().unwrap_or(0),
            max: ls.iter().map(|l| l.max).max().unwrap_or(0),
            spikes: ls.iter().flat_map(|l| l.spikes.clone()).collect(),
            points: longest.points,
        });
    }
    out.pings.sort_by(|a, b| b.avg.total_cmp(&a.avg));
    out.pyros = pyros.into_values().collect();
    out.pyros.sort_by_key(|p| std::cmp::Reverse(p.reflects));
    out.reflects.sort_by(|a, b| a.demo_id.cmp(&b.demo_id).then(a.at_s.total_cmp(&b.at_s)));
    Ok(Some(out))
}
