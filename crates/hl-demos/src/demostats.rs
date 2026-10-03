//! Ping (Q44) and Pyro reflects (Q45, ivg and ImABush), read off a timeline.
//!
//! **Ping** is the scoreboard's: what the server measured, smoothed, about
//! once a second. A player's line is time-weighted -- a minute at 90 ms
//! counts a minute -- and a spike is a stretch at least [`SPIKE_MS`] above
//! their own usual (the median), so a player on 80 ms all match has none.
//!
//! **Reflects** come from the projectiles themselves (see `deep`): each one
//! is classified by how it ended --
//!
//! - **hit**: the reflector damaged an enemy within [`HIT_TICKS`] of the
//!   projectile exploding, or got a reflect kill;
//! - **miss**: it exploded and hurt nobody;
//! - **sent back**: the other side reflected it again before it landed;
//! - **unknown**: it left a POV demo's view, or the demo ended.
//!
//! -- and, where the demo showed it flying long enough, by whether it was a
//! **threat**: carried on along the way it was going, it would have passed
//! within [`THREAT_UNITS`] of the reflector or a living teammate inside
//! [`THREAT_SECONDS`]. Rockets fly straight; pipes are given gravity and no
//! bounces. Walls are not known, so this says "headed at someone", not "was
//! certain to hit": an estimate, and called one wherever it is shown.

use crate::timeline::{GameEvent, Timeline};
use serde::Serialize;

/// A stretch this far above a player's median ping is a spike.
pub const SPIKE_MS: u16 = 40;
/// A hurt this close to a projectile's end is its doing.
pub const HIT_TICKS: u32 = 3;
/// Passing this close counts as headed at someone: a rocket's splash reaches
/// 146 units, and this keeps to well inside it.
pub const THREAT_UNITS: f32 = 110.0;
/// How far ahead the path is followed.
pub const THREAT_SECONDS: f32 = 1.2;
/// Gravity on a pipe, units a second squared.
const GRAVITY: f32 = 800.0;

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PingLine {
    pub slot: usize,
    /// Time-weighted mean, ms.
    pub avg: f64,
    pub median: u16,
    pub min: u16,
    pub max: u16,
    /// Seconds the line covers: what it is weighted by beside another demo's.
    pub seconds: f64,
    /// `(from s, to s, peak ms)`.
    pub spikes: Vec<(f64, f64, u16)>,
    /// `(s, ms)` at every change, for a sparkline.
    pub points: Vec<(f64, u16)>,
}

/// Every player's ping over the demo; empty for a timeline from before v2.
pub fn pings(tl: &Timeline) -> Vec<PingLine> {
    let end = tl.end();
    let mut out = Vec::new();
    for (slot, track) in tl.tracks.iter().enumerate() {
        let changes: Vec<(u32, u16)> = track
            .changes
            .iter()
            .filter_map(|c| match c.field {
                crate::timeline::Field::Ping(v) if v > 0 => Some((c.t, v)),
                _ => None,
            })
            .collect();
        if changes.is_empty() {
            continue;
        }
        // (ms, ticks held)
        let held: Vec<(u16, u32)> = changes
            .iter()
            .enumerate()
            .map(|(i, &(t, v))| (v, changes.get(i + 1).map_or(end, |n| n.0).saturating_sub(t).max(1)))
            .collect();
        let total: u64 = held.iter().map(|h| u64::from(h.1)).sum();
        let avg = held.iter().map(|h| f64::from(h.0) * f64::from(h.1)).sum::<f64>() / total.max(1) as f64;
        // Time-weighted median.
        let mut sorted = held.clone();
        sorted.sort_by_key(|h| h.0);
        let mut acc = 0u64;
        let median = sorted.iter().find(|h| {
            acc += u64::from(h.1);
            acc * 2 >= total
        });
        let median = median.map_or(0, |h| h.0);
        let mut spikes: Vec<(f64, f64, u16)> = Vec::new();
        let mut open: Option<(u32, u16)> = None;
        for (i, &(t, v)) in changes.iter().enumerate() {
            let high = v >= median.saturating_add(SPIKE_MS);
            match (&mut open, high) {
                (None, true) => open = Some((t, v)),
                (Some((_, peak)), true) => *peak = (*peak).max(v),
                (Some((from, peak)), false) => {
                    spikes.push((tl.seconds(*from), tl.seconds(t), *peak));
                    open = None;
                }
                (None, false) => {}
            }
            if i + 1 == changes.len() {
                if let Some((from, peak)) = open.take() {
                    spikes.push((tl.seconds(from), tl.seconds(end), peak));
                }
            }
        }
        out.push(PingLine {
            slot,
            avg,
            median,
            seconds: tl.seconds(total as u32),
            min: changes.iter().map(|c| c.1).min().unwrap_or(0),
            max: changes.iter().map(|c| c.1).max().unwrap_or(0),
            spikes,
            points: changes.iter().map(|&(t, v)| (tl.seconds(t), v)).collect(),
        });
    }
    out
}

/// How a reflect ended.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Hit,
    Miss,
    SentBack,
    Unknown,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReflectLine {
    pub t: u32,
    pub at_s: f64,
    /// The reflector's slot, when the demo said who it was.
    pub slot: Option<usize>,
    pub what: String,
    pub pos: [i32; 3],
    pub outcome: Outcome,
    /// Players the reflector damaged as it landed, and how much.
    pub victims: Vec<usize>,
    pub damage: u32,
    pub killed: bool,
    /// Headed at the reflector or a teammate before it was sent back;
    /// `None` when the demo had not shown it flying long enough to tell.
    pub threat: Option<bool>,
}

/// Every reflect in the demo, oldest first; empty before v2.
pub fn reflects(tl: &Timeline) -> Vec<ReflectLine> {
    let reflect_rows: Vec<&crate::timeline::ObjectRow> = tl.objects.iter().filter(|o| o.kind == "reflect").collect();
    let mut out = Vec::new();
    for (i, r) in reflect_rows.iter().enumerate() {
        // The same projectile sent back again before it landed.
        let again = reflect_rows[i + 1..].iter().find(|n| n.entity == r.entity && n.t >= r.t);
        let end = tl
            .objects
            .iter()
            .find(|o| o.entity == r.entity && o.t >= r.t && (o.kind == "reflect_end" || o.kind == "reflect_lost"));
        let by = r.by.map(usize::from);
        let mut line = ReflectLine {
            t: r.t,
            at_s: tl.seconds(r.t),
            slot: by,
            what: r.what.as_deref().unwrap_or("projectile").to_string(),
            pos: r.pos,
            outcome: Outcome::Unknown,
            victims: Vec::new(),
            damage: 0,
            killed: false,
            threat: by.and_then(|s| threat(tl, s, r)),
        };
        match (again, end) {
            (Some(a), Some(e)) if a.t <= e.t => line.outcome = Outcome::SentBack,
            (Some(_), None) => line.outcome = Outcome::SentBack,
            (_, Some(e)) if e.kind == "reflect_end" => {
                if let Some(slot) = by {
                    let enemy = |victim: usize| tl.now(victim, e.t).team != tl.now(slot, e.t).team;
                    for (t, ev) in &tl.events {
                        if t.abs_diff(e.t) > HIT_TICKS {
                            continue;
                        }
                        match ev {
                            GameEvent::PlayerHurt(h) => {
                                let (Some(a), Some(v)) = (tl.slot_of_user(h.attacker), tl.slot_of_user(h.user_id)) else { continue };
                                if a == slot && v != slot && enemy(v) {
                                    if !line.victims.contains(&v) {
                                        line.victims.push(v);
                                    }
                                    line.damage += u32::from(h.damage_amount);
                                }
                            }
                            GameEvent::PlayerDeath(d)
                                if tl.slot_of_user(d.attacker) == Some(slot) && d.weapon.to_string().starts_with("deflect") =>
                            {
                                line.killed = true;
                            }
                            _ => {}
                        }
                    }
                }
                line.outcome = if line.victims.is_empty() && !line.killed { Outcome::Miss } else { Outcome::Hit };
            }
            _ => {}
        }
        out.push(line);
    }
    out
}

/// Whether the projectile was headed at the reflector or a living teammate.
fn threat(tl: &Timeline, by: usize, r: &crate::timeline::ObjectRow) -> Option<bool> {
    let v = r.vel?.map(|x| x as f32);
    if v.iter().all(|x| x.abs() < 1.0) {
        return None;
    }
    let p = r.pos.map(|x| x as f32);
    let gravity = if matches!(r.what.as_deref(), Some("pipe" | "sticky" | "jar")) { GRAVITY } else { 0.0 };
    let team = tl.now(by, r.t).team;
    let within = (tl.tick_rate * 0.25).round() as u32;
    let targets: Vec<(usize, [f32; 3])> = (0..tl.people.len())
        .filter(|&s| {
            let now = tl.now(s, r.t);
            now.live() && now.team == team
        })
        .filter_map(|s| tl.sample_near(s, r.t, within).map(|x| (s, x.pos)))
        .collect();
    // Points along the path, every 1/66 s; the reflector is right beside it
    // when it is sent back, so for them only a later approach counts.
    let steps = (THREAT_SECONDS * 66.0) as usize;
    for (slot, at) in targets {
        // Aim at the body's middle, not the feet.
        let body = [at[0], at[1], at[2] + 40.0];
        let mut nearest = f32::MAX;
        let mut when = 0.0;
        for i in 0..=steps {
            let s = i as f32 / 66.0;
            let q = [p[0] + v[0] * s, p[1] + v[1] * s, p[2] + v[2] * s - 0.5 * gravity * s * s];
            let d = ((q[0] - body[0]).powi(2) + (q[1] - body[1]).powi(2) + (q[2] - body[2]).powi(2)).sqrt();
            if d < nearest {
                nearest = d;
                when = s;
            }
        }
        let reach = if slot == by { 70.0 } else { THREAT_UNITS };
        if nearest <= reach && (slot != by || when > 0.03) {
            return Some(true);
        }
    }
    Some(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::{Change, Field, ObjectRow, Person, Sample, Track};
    use std::borrow::Cow;
    use tf_demo_parser::demo::gameevent_gen::PlayerHurtEvent;

    fn person(n: &str) -> Person {
        Person { steamid: format!("[U:1:{n}]"), name: n.into() }
    }

    fn row(t: u32, entity: u32, kind: &'static str) -> ObjectRow {
        ObjectRow {
            t,
            entity,
            kind: Cow::Borrowed(kind),
            pos: [0, 0, 0],
            builder: None,
            team: None,
            level: None,
            health: None,
            building: false,
            sapped: false,
            by: None,
            what: None,
            vel: None,
        }
    }

    fn live(team: u8, at: [f32; 3]) -> Track {
        Track {
            samples: vec![Sample { t: 100, pos: at, yaw: 0.0, pitch: 0.0 }],
            changes: vec![
                Change { t: 0, field: Field::Team(team) },
                Change { t: 0, field: Field::Alive(true) },
                Change { t: 0, field: Field::Here(true) },
            ],
        }
    }

    fn hurt(attacker: u16, victim: u16, damage: u16) -> GameEvent {
        GameEvent::PlayerHurt(PlayerHurtEvent {
            user_id: victim,
            health: 0,
            attacker,
            damage_amount: damage,
            custom: 0,
            show_disguised_crit: false,
            crit: false,
            mini_crit: false,
            all_see_crit: false,
            weapon_id: 0,
            bonus_effect: 0,
        })
    }

    /// Pyro (slot 0, RED, user 10) beside a teammate (slot 1, RED) and an
    /// enemy Soldier (slot 2, BLU, user 12).
    fn base() -> Timeline {
        Timeline {
            version: 2,
            tick_rate: 66.0,
            stride: 1,
            seams: vec![(0, 0)],
            people: vec![person("pyro"), person("mate"), person("soldier")],
            tracks: vec![live(2, [0.0, 0.0, 0.0]), live(2, [600.0, 0.0, 0.0]), live(3, [-800.0, 0.0, 0.0])],
            user_ids: vec![(10, 0), (11, 1), (12, 2)],
            ..Default::default()
        }
    }

    fn reflect(t: u32, entity: u32, vel: [i32; 3]) -> ObjectRow {
        ObjectRow { by: Some(0), team: Some(2), what: Some(Cow::Borrowed("rocket")), vel: Some(vel), pos: [50, 0, 40], ..row(t, entity, "reflect") }
    }

    #[test]
    fn a_reflect_that_lands_on_the_soldier_is_a_hit() {
        let mut tl = base();
        // Flying +x, towards the Pyro's teammate at x = 600: a threat.
        tl.objects = vec![reflect(100, 500, [1100, 0, 0]), row(140, 500, "reflect_end")];
        tl.events = vec![(141, hurt(10, 12, 90))];
        let r = &reflects(&tl)[0];
        assert_eq!((r.outcome, r.damage, r.victims.clone()), (Outcome::Hit, 90, vec![2]));
        assert_eq!(r.threat, Some(true));
    }

    #[test]
    fn one_that_lands_on_nobody_is_a_miss_and_one_flying_away_is_no_threat() {
        let mut tl = base();
        // Flying +y, away from everyone.
        tl.objects = vec![reflect(100, 501, [0, 1100, 0]), row(160, 501, "reflect_end")];
        let r = &reflects(&tl)[0];
        assert_eq!((r.outcome, r.threat), (Outcome::Miss, Some(false)));
    }

    #[test]
    fn sent_back_again_and_out_of_view() {
        let mut tl = base();
        tl.objects = vec![
            reflect(100, 502, [1100, 0, 0]),
            // The Soldier... no, an enemy Pyro sends it back again.
            ObjectRow { by: Some(2), team: Some(3), ..reflect(120, 502, [-1100, 0, 0]) },
            row(150, 502, "reflect_end"),
            reflect(200, 503, [1100, 0, 0]),
            row(230, 503, "reflect_lost"),
        ];
        let r = reflects(&tl);
        assert_eq!(r[0].outcome, Outcome::SentBack);
        assert_eq!(r[2].outcome, Outcome::Unknown);
        assert_eq!(r[0].threat, Some(true));
    }

    #[test]
    fn ping_is_time_weighted_and_spikes_are_above_the_players_own_usual() {
        let mut tl = base();
        tl.tick_rate = 1.0; // a tick a second, to read easily
        tl.tracks[0].changes.extend([
            Change { t: 0, field: Field::Ping(30) },
            Change { t: 100, field: Field::Ping(95) },
            Change { t: 110, field: Field::Ping(32) },
        ]);
        // The match runs to t = 200.
        tl.objects = vec![row(200, 1, "cart")];
        let p = pings(&tl).into_iter().find(|p| p.slot == 0).unwrap();
        assert_eq!((p.min, p.max, p.median), (30, 95, 30));
        assert!((p.avg - (30.0 * 100.0 + 95.0 * 10.0 + 32.0 * 90.0) / 200.0).abs() < 0.01, "{}", p.avg);
        assert_eq!(p.spikes, vec![(100.0, 110.0, 95)]);
        assert!(pings(&tl).iter().all(|p| p.slot == 0), "no ping, no line");
    }
}
