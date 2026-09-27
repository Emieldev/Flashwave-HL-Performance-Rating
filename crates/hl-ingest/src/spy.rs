//! Spychecks for a match page (Q27), read off the timelines kept of its STV
//! demos. Nothing is stored: a timeline decodes in a few hundredths of a
//! second, and keeping it is what lets a question like this one be asked of
//! a demo long after its file is gone.

use crate::aim::account_of;
use anyhow::Result;
use hl_db::Db;
use hl_demos::timeline::{Stored, Timeline};
use serde::Serialize;

/// Jumps land this long before the moment, as the round timeline's do.
const LEAD_SECONDS: f64 = 5.0;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpyReport {
    /// STV demos with a timeline: the ones this could be read from.
    pub demos: usize,
    pub players: Vec<SpyPlayer>,
    /// Oldest first.
    pub checks: Vec<SpyCheck>,
    /// Hits on a cloaked Spy that did not count, and why.
    pub fading: u32,
    pub blinking: u32,
    pub marked: u32,
    pub cooldown: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpyPlayer {
    pub account_id: u32,
    /// The name the demo carried, for anyone the log does not.
    pub name: String,
    /// Spychecks this player made.
    pub checks: u32,
    /// Times this player was found while fully cloaked.
    pub found: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpyCheck {
    pub demo_id: i64,
    /// Seconds into the demo.
    pub at_s: f64,
    /// The demo tick to jump to: a few seconds before the hit.
    pub jump_tick: u32,
    pub attacker: u32,
    pub spy: u32,
    pub damage: u16,
    pub killed: bool,
}

/// `None` when none of the match's STV demos has a timeline.
pub async fn for_log(db: &Db, log_id: i64) -> Result<Option<SpyReport>> {
    let mut report = SpyReport { demos: 0, players: Vec::new(), checks: Vec::new(), fading: 0, blinking: 0, marked: 0, cooldown: 0 };
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
        // Decoding inflates a few megabytes: off the async threads.
        let (tl, found) = tokio::task::spawn_blocking(move || -> Result<_> {
            let tl = Timeline::decode(&stored)?;
            let found = hl_demos::spy::spychecks(&tl);
            Ok((tl, found))
        })
        .await??;
        report.demos += 1;
        report.fading += found.skipped.fading;
        report.blinking += found.skipped.blinking;
        report.marked += found.skipped.marked;
        report.cooldown += found.skipped.cooldown;
        let lead = (LEAD_SECONDS * tl.tick_rate).round() as u32;
        let account = |slot: usize| account_of(&tl.people[slot].steamid);
        for c in &found.checks {
            let (Some(attacker), Some(spy)) = (account(c.attacker), account(c.spy)) else { continue };
            report.checks.push(SpyCheck {
                demo_id: demo.demo_id,
                at_s: tl.seconds(c.t),
                jump_tick: tl.tick_of(c.t.saturating_sub(lead)),
                attacker,
                spy,
                damage: c.damage,
                killed: c.killed,
            });
            for (who, slot, made) in [(attacker, c.attacker, true), (spy, c.spy, false)] {
                let i = match report.players.iter().position(|p| p.account_id == who) {
                    Some(i) => i,
                    None => {
                        report.players.push(SpyPlayer { account_id: who, name: tl.people[slot].name.clone(), checks: 0, found: 0 });
                        report.players.len() - 1
                    }
                };
                let p = &mut report.players[i];
                if made { p.checks += 1 } else { p.found += 1 }
            }
        }
    }
    report.players.sort_by_key(|p| std::cmp::Reverse((p.checks, p.found)));
    Ok((report.demos > 0).then_some(report))
}
