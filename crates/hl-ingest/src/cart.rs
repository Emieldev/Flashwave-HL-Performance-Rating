//! The cart in a numbers advantage, for a match page (Q11), read off the
//! kept timelines of the match's STV demos -- like spychecks, nothing is
//! stored, and it works after the demo file is gone.

use anyhow::Result;
use hl_db::Db;
use hl_demos::timeline::{Stored, Timeline};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CartView {
    /// How many seconds up counts as "up": the threshold used.
    pub up: i32,
    pub after_s: u32,
    pub rounds: Vec<CartRoundView>,
    pub stalls: Vec<StallView>,
    pub fights: Vec<FightView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CartRoundView {
    pub demo_id: i64,
    pub seconds: u32,
    pub moving_s: u32,
    pub up_s: u32,
    pub up_still_s: u32,
    /// Of the still seconds, those with no attacker near the cart; the rest
    /// had one there and a defender blocking.
    pub up_still_empty_s: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StallView {
    pub demo_id: i64,
    /// Index into `rounds`.
    pub round: usize,
    pub at_s: u32,
    pub seconds: u32,
    pub most_up: i32,
    pub empty_s: u32,
    pub jump_tick: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FightView {
    pub demo_id: i64,
    pub round: usize,
    pub nth: u32,
    pub at_s: u32,
    pub lost_attackers: u32,
    pub lost_defenders: u32,
    pub window_s: u32,
    pub moving_s: u32,
    pub jump_tick: u32,
}

/// `None` when no STV timeline of the match has a cart in it.
pub async fn for_log(db: &Db, log_id: i64) -> Result<Option<CartView>> {
    let mut view = CartView { up: hl_demos::cart::UP, after_s: hl_demos::cart::AFTER_S, rounds: Vec::new(), stalls: Vec::new(), fights: Vec::new() };
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
        let found = tokio::task::spawn_blocking(move || -> Result<_> { Ok(hl_demos::cart::cart(&Timeline::decode(&stored)?)) }).await??;
        let Some(r) = found else { continue };
        let base = view.rounds.len();
        let id = demo.demo_id;
        view.rounds.extend(r.rounds.iter().map(|x| CartRoundView {
            demo_id: id,
            seconds: x.seconds,
            moving_s: x.moving_s,
            up_s: x.up_s,
            up_still_s: x.up_still_s,
            up_still_empty_s: x.up_still_empty_s,
        }));
        view.stalls.extend(r.stalls.iter().map(|s| StallView {
            demo_id: id,
            round: base + s.round,
            at_s: s.from_s,
            seconds: s.seconds,
            most_up: s.most_up,
            empty_s: s.empty_s,
            jump_tick: s.jump_tick,
        }));
        view.fights.extend(r.conversions.iter().map(|c| FightView {
            demo_id: id,
            round: base + c.round,
            nth: c.nth,
            at_s: c.at_s,
            lost_attackers: c.lost.0,
            lost_defenders: c.lost.1,
            window_s: c.window_s,
            moving_s: c.moving_s,
            jump_tick: c.jump_tick,
        }));
    }
    Ok((!view.rounds.is_empty()).then_some(view))
}
