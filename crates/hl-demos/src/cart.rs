//! The cart in a numbers advantage (Q11, Flashy): time the attackers were
//! up players and the payload sat still, and how much it moved after each
//! fight they won.
//!
//! "Measure and detect when the cart is staying still in a 9v5 or less to
//! see how much time you are wasting not pushing... Measure the average cart
//! time 1/2/3x after teamfights." A still cart with five defenders dead is
//! nobody walking to it, or a defender alive on it that nobody has killed;
//! both are the attacking team's to fix. Only a demo knows where the cart
//! is, so this is a pure function over a [`Timeline`] (STV: every player's
//! state, and the cart as an object).

use crate::timeline::{GameEvent, Now, ObjectRow, Timeline};

/// The team that pushes on payload. Stopwatch swaps the teams between
/// halves, not the colours, so BLU pushes in every round.
const ATTACKERS: u8 = 3;
const DEFENDERS: u8 = 2;
/// Moved further than this in a second: the cart was moving. A pushed cart
/// covers 40-100 units a second; rounding and the odd bump stay under it.
const MOVING_UNITS: f64 = 12.0;
/// Up this many players or more is a numbers advantage (a 9v6 or better).
pub const UP: i32 = 3;
/// A still cart shorter than this is a breath, not a stall.
const MIN_STALL_S: u32 = 3;
/// Kills further apart than this are two fights, as in the fights pass.
const FIGHT_GAP_S: f64 = 10.0;
/// How long after a won fight the push is measured over.
pub const AFTER_S: u32 = 30;
/// Jumps land this long before the moment, as the round timeline's do.
const LEAD_S: u32 = 5;
/// A still cart this long, with a fight in it, is a hold (Q12): the
/// defence stopping the push, rather than a breath between two.
pub const HOLD_S: u32 = 10;
/// An attacker this close to the cart, across the ground, is pushing it or
/// could be: pushers were measured standing up to ~180 units from the
/// cart's origin on upward, and the push trigger is wider than the model.
const NEAR_UNITS: f64 = 250.0;

/// One live payload round, setup over.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CartRound {
    /// Seconds into the demo, and the demo tick, where setup ended.
    pub from_s: u32,
    pub to_s: u32,
    pub from_tick: u32,
    pub seconds: u32,
    /// Seconds the cart moved.
    pub moving_s: u32,
    /// Seconds with the attackers up [`UP`] or more.
    pub up_s: u32,
    /// Of those, seconds the cart stood still: the waste.
    pub up_still_s: u32,
    /// Of those still seconds, how many had no attacker near the cart --
    /// nobody walking to it, rather than a defender blocking.
    pub up_still_empty_s: u32,
    /// Q12: how far the cart had come, second by second from setup's end,
    /// in units along the ground: pushed forward adds, rolled back with
    /// nobody near it takes away.
    pub progress: Vec<u32>,
}

/// A fight in a round, whoever won it (Q12).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fight {
    pub round: usize,
    /// Seconds into the round, first kill to last.
    pub from_s: u32,
    pub to_s: u32,
    /// Kills each side lost: `(attackers, defenders)`.
    pub lost: (u32, u32),
}

impl Fight {
    /// The attackers came out ahead.
    pub fn attackers_won(&self) -> bool {
        self.lost.1 > self.lost.0
    }
}

/// A hold (Q12, Flashy: "when the cart gets stuck and you start killing or
/// dying a lot at one spot"): the cart still for [`HOLD_S`] or more with a
/// fight in it. How many pushes the defence turned back, and whether it
/// broke or held until the round ended.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hold {
    pub round: usize,
    /// Seconds into the round.
    pub from_s: u32,
    pub seconds: u32,
    /// Fights in it the attackers did not win: pushes turned back.
    pub pushes_failed: u32,
    pub fights: u32,
    /// Kills each side lost in it: `(attackers, defenders)`.
    pub lost: (u32, u32),
    /// The cart rolled on before the round ended.
    pub broke: bool,
    /// Where the cart stood, across the ground.
    pub pos: [f64; 2],
    pub jump_tick: u32,
}

/// A stretch of seconds the attackers were up [`UP`] or more and the cart
/// did not move.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stall {
    pub round: usize,
    pub from_s: u32,
    pub seconds: u32,
    /// The biggest advantage in it: 4 is a 9v5.
    pub most_up: i32,
    /// Seconds in it with no attacker near the cart.
    pub empty_s: u32,
    pub jump_tick: u32,
}

/// A fight the attackers won, and what the cart did next.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Conversion {
    pub round: usize,
    /// The attackers' first, second, third... won fight of the round.
    pub nth: u32,
    pub at_s: u32,
    /// Kills each side lost in it: `(attackers, defenders)`.
    pub lost: (u32, u32),
    /// The seconds measured: [`AFTER_S`], or less where the round ended.
    pub window_s: u32,
    pub moving_s: u32,
    pub jump_tick: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CartReport {
    pub rounds: Vec<CartRound>,
    pub stalls: Vec<Stall>,
    pub conversions: Vec<Conversion>,
    /// Q12: every fight of every round, and the holds.
    pub fights: Vec<Fight>,
    pub holds: Vec<Hold>,
}

impl CartReport {
    /// Seconds up [`UP`] or more with the cart still, over every round.
    pub fn wasted_s(&self) -> u32 {
        self.rounds.iter().map(|r| r.up_still_s).sum()
    }

    /// Average seconds of movement after the attackers' `nth` won fight of a
    /// round (1, 2, 3; `None` for every fight): `(fights, mean seconds,
    /// mean window)`.
    pub fn after(&self, nth: Option<u32>) -> (usize, f64, f64) {
        let c: Vec<&Conversion> = self.conversions.iter().filter(|c| nth.is_none_or(|n| c.nth == n)).collect();
        if c.is_empty() {
            return (0, 0.0, 0.0);
        }
        let n = c.len() as f64;
        (c.len(), c.iter().map(|c| f64::from(c.moving_s)).sum::<f64>() / n, c.iter().map(|c| f64::from(c.window_s)).sum::<f64>() / n)
    }
}

/// Where the cart was at `t`: its last row at or before, if that was a cart.
fn cart_at(carts: &[&ObjectRow], t: u32) -> Option<[f64; 3]> {
    let i = carts.partition_point(|o| o.t <= t).checked_sub(1)?;
    let o = carts[i];
    (o.kind == "cart").then(|| o.pos.map(f64::from))
}

/// Across the ground only: a pusher on a slope stands above or below the
/// cart's origin.
fn flat(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// A round's holds: runs of [`HOLD_S`] or more still seconds with a fight
/// in them. `moved` and `wheres` are per second of the round; `jump` turns a
/// second into the tick to jump to.
fn holds(round: usize, moved: &[bool], wheres: &[[f64; 3]], fights: &[Fight], jump: impl Fn(u32) -> u32) -> Vec<Hold> {
    let mut out = Vec::new();
    let mut s = 0;
    while s < moved.len() {
        if moved[s] {
            s += 1;
            continue;
        }
        let start = s;
        while s < moved.len() && !moved[s] {
            s += 1;
        }
        let (from, to) = (start as u32, s as u32);
        if to - from < HOLD_S {
            continue;
        }
        // A fight belongs to the hold if it ended in it, or within a few
        // seconds of its end: the fight that breaks a hold ends as the
        // cart starts to roll.
        let inside: Vec<&Fight> = fights.iter().filter(|f| f.to_s >= from && f.to_s <= to + 3).collect();
        if inside.is_empty() {
            continue;
        }
        let at = wheres.get(start).copied().unwrap_or([0.0; 3]);
        out.push(Hold {
            round,
            from_s: from,
            seconds: to - from,
            pushes_failed: inside.iter().filter(|f| !f.attackers_won()).count() as u32,
            fights: inside.len() as u32,
            lost: inside.iter().fold((0, 0), |a, f| (a.0 + f.lost.0, a.1 + f.lost.1)),
            broke: s < moved.len(),
            pos: [at[0], at[1]],
            jump_tick: jump(from),
        });
    }
    out
}

/// `None` for a demo with no cart in it: not payload, or not an STV.
pub fn cart(tl: &Timeline) -> Option<CartReport> {
    let carts: Vec<&ObjectRow> = tl.objects.iter().filter(|o| o.kind == "cart" || o.kind == "gone").collect();
    // "gone" rows belong to every kind of object; keep only those for an
    // entity that was a cart, so a destroyed sentry does not hide the cart.
    let cart_ids: std::collections::HashSet<u32> = carts.iter().filter(|o| o.kind == "cart").map(|o| o.entity).collect();
    if cart_ids.is_empty() {
        return None;
    }
    let carts: Vec<&ObjectRow> = carts.into_iter().filter(|o| cart_ids.contains(&o.entity)).collect();
    let second = tl.tick_rate.max(1.0);
    let ticks = |s: u32| (f64::from(s) * second).round() as u32;
    let secs = |t: u32| (f64::from(t) / second) as u32;

    // Rounds: setup over to the round's end (or the next start, or the demo's).
    let mut spans: Vec<(u32, u32)> = Vec::new();
    let mut open: Option<u32> = None;
    for (t, e) in &tl.events {
        match e {
            GameEvent::TeamPlaySetupFinished(_) => open = Some(*t),
            GameEvent::TeamPlayRoundWin(_) | GameEvent::TeamPlayRoundStalemate(_) | GameEvent::TeamPlayRoundStart(_) => {
                if let Some(from) = open.take() {
                    spans.push((from, *t));
                }
            }
            _ => {}
        }
    }
    if let Some(from) = open {
        spans.push((from, tl.end()));
    }

    let stretches: Vec<Vec<(u32, u32, Now)>> = (0..tl.people.len()).map(|s| tl.stretches(s)).collect();
    let state = |slot: usize, t: u32| -> Option<Now> {
        let s = &stretches[slot];
        let i = s.partition_point(|(from, _, _)| *from <= t).checked_sub(1)?;
        (s[i].1 > t).then_some(s[i].2)
    };
    // Attackers alive minus defenders alive at `t`.
    let up_at = |t: u32| -> i32 {
        let (mut a, mut d) = (0, 0);
        for slot in 0..stretches.len() {
            match state(slot, t) {
                Some(n) if n.live() && n.team == ATTACKERS => a += 1,
                Some(n) if n.live() && n.team == DEFENDERS => d += 1,
                _ => {}
            }
        }
        a - d
    };
    let attacker_near = |t: u32, cart: [f64; 3]| {
        (0..stretches.len()).any(|slot| {
            state(slot, t).is_some_and(|n| n.live() && n.team == ATTACKERS)
                && tl.sample_near(slot, t, ticks(1)).is_some_and(|s| flat(s.pos.map(f64::from), cart) <= NEAR_UNITS)
        })
    };

    let mut out = CartReport::default();
    for (round, &(from, to)) in spans.iter().enumerate() {
        // A map can have more than one cart entity (Swiftwater's STV has
        // three): this round's is the one with the most seconds of cart-like
        // movement in it -- over MOVING_UNITS and under a teleport's 400 in a
        // second. Mixing their rows read a jump from one to another as the
        // cart moving; their total travel picks a prop that jiggles, and
        // their reach one that teleports.
        let ids: std::collections::BTreeSet<u32> = carts.iter().filter(|o| o.kind == "cart" && o.t >= from && o.t <= to).map(|o| o.entity).collect();
        let Some(this) = ids
            .into_iter()
            .max_by_key(|id| {
                let rows: Vec<&ObjectRow> = carts.iter().copied().filter(|o| o.entity == *id).collect();
                let mut n = 0u32;
                let mut s = 0;
                while from + ticks(s + 1) <= to {
                    if let (Some(a), Some(b)) = (cart_at(&rows, from + ticks(s)), cart_at(&rows, from + ticks(s + 1))) {
                        let d = dist(a, b);
                        n += u32::from(d > MOVING_UNITS && d < 400.0);
                    }
                    s += 1;
                }
                n
            })
        else {
            continue;
        };
        let carts: Vec<&ObjectRow> = carts.iter().copied().filter(|o| o.entity == this).collect();
        let mut r = CartRound { from_s: secs(from), to_s: secs(to), from_tick: tl.tick_of(from), ..Default::default() };
        // Per second: moving, and the advantage, kept for the fights below.
        let mut moved: Vec<bool> = Vec::new();
        // Per counted second: where the cart was, and whether it was pushed
        // forward (Q12's holds and progress).
        let mut wheres: Vec<[f64; 3]> = Vec::new();
        let mut pushed: Vec<bool> = Vec::new();
        let mut stall: Option<Stall> = None;
        let mut s = 0;
        loop {
            let (a, b) = (from + ticks(s), from + ticks(s + 1));
            if b > to {
                break;
            }
            let (Some(pa), Some(pb)) = (cart_at(&carts, a), cart_at(&carts, b)) else {
                moved.push(false);
                pushed.push(false);
                wheres.push(wheres.last().copied().unwrap_or([0.0; 3]));
                r.progress.push(r.progress.last().copied().unwrap_or(0));
                s += 1;
                continue;
            };
            // A reset between rounds teleports the cart: not movement.
            let d = dist(pa, pb);
            let moving = d > MOVING_UNITS && d < 400.0;
            moved.push(moving);
            // Q12: pushed, not rolled back. A cart moving with an attacker
            // near it is being pushed; one moving with nobody near it is
            // rolling back, and that takes ground away.
            let pushing = moving && attacker_near(a, pa);
            pushed.push(pushing);
            let before = r.progress.last().copied().unwrap_or(0);
            let step = flat(pa, pb).round() as u32;
            r.progress.push(if pushing { before + step } else if moving { before.saturating_sub(step) } else { before });
            wheres.push(pa);
            r.seconds += 1;
            r.moving_s += u32::from(moving);
            let up = up_at(a);
            if up >= UP {
                r.up_s += 1;
                if !moving {
                    r.up_still_s += 1;
                    let empty = !attacker_near(a, pa);
                    r.up_still_empty_s += u32::from(empty);
                    let st = stall.get_or_insert(Stall {
                        round,
                        from_s: secs(a),
                        seconds: 0,
                        most_up: up,
                        empty_s: 0,
                        jump_tick: tl.tick_of(a.saturating_sub(ticks(LEAD_S))),
                    });
                    st.seconds += 1;
                    st.most_up = st.most_up.max(up);
                    st.empty_s += u32::from(empty);
                    s += 1;
                    continue;
                }
            }
            if let Some(st) = stall.take().filter(|st| st.seconds >= MIN_STALL_S) {
                out.stalls.push(st);
            }
            s += 1;
        }
        if let Some(st) = stall.take().filter(|st| st.seconds >= MIN_STALL_S) {
            out.stalls.push(st);
        }

        // Fights: the round's kills, split where two are over 10 s apart.
        let mut kills: Vec<(u32, u8)> = Vec::new();
        for (t, e) in &tl.events {
            if *t < from || *t >= to {
                continue;
            }
            let GameEvent::PlayerDeath(d) = e else { continue };
            let Some(victim) = tl.slot_of_user(d.user_id) else { continue };
            if let Some(n) = state(victim, t.saturating_sub(1)) {
                kills.push((*t, n.team));
            }
        }
        let gap = ticks(FIGHT_GAP_S as u32);
        let mut round_fights: Vec<Fight> = Vec::new();
        let mut nth = 0;
        let mut i = 0;
        while i < kills.len() {
            let mut j = i + 1;
            while j < kills.len() && kills[j].0 - kills[j - 1].0 <= gap {
                j += 1;
            }
            let fight = &kills[i..j];
            let lost_a = fight.iter().filter(|k| k.1 == ATTACKERS).count() as u32;
            let lost_d = fight.iter().filter(|k| k.1 == DEFENDERS).count() as u32;
            round_fights.push(Fight {
                round,
                from_s: ((fight[0].0 - from) as f64 / second) as u32,
                to_s: ((fight[fight.len() - 1].0 - from) as f64 / second) as u32,
                lost: (lost_a, lost_d),
            });
            if lost_d > lost_a {
                nth += 1;
                let end = fight[fight.len() - 1].0;
                let first = ((end - from) as f64 / second) as usize;
                let window = moved.iter().skip(first).take(AFTER_S as usize);
                let window_s = window.clone().count() as u32;
                out.conversions.push(Conversion {
                    round,
                    nth,
                    at_s: secs(end),
                    lost: (lost_a, lost_d),
                    window_s,
                    moving_s: window.filter(|m| **m).count() as u32,
                    jump_tick: tl.tick_of(fight[0].0.saturating_sub(ticks(LEAD_S))),
                });
            }
            i = j;
        }
        out.holds.extend(holds(round, &pushed, &wheres, &round_fights, |s| tl.tick_of((from + ticks(s)).saturating_sub(ticks(LEAD_S)))));
        out.fights.extend(round_fights);
        out.rounds.push(r);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hold_is_a_still_cart_with_fights_in_it_and_breaks_when_it_rolls() {
        // 40 s: rolls 5, still 20 (two pushes turned back, one won), rolls
        // 5, still 10 with no fight in it.
        let mut moved = vec![true; 5];
        moved.extend(vec![false; 20]);
        moved.extend(vec![true; 5]);
        moved.extend(vec![false; 10]);
        let wheres = vec![[100.0, 200.0, 0.0]; moved.len()];
        let f = |from_s, to_s, a, d| Fight { round: 0, from_s, to_s, lost: (a, d) };
        let fights = [f(8, 10, 2, 1), f(15, 17, 3, 0), f(22, 25, 0, 3)];
        let h = holds(0, &moved, &wheres, &fights, |s| s * 10);
        assert_eq!(h.len(), 1, "the second still stretch had no fight: not a hold");
        let h = h[0];
        assert_eq!((h.from_s, h.seconds), (5, 20));
        assert_eq!((h.fights, h.pushes_failed), (3, 2));
        assert_eq!(h.lost, (5, 4));
        assert!(h.broke);
        assert_eq!(h.pos, [100.0, 200.0]);
        assert_eq!(h.jump_tick, 50);
        // Still to the end with a fight: held, not broken.
        let moved = vec![false; 15];
        let h = holds(0, &moved, &vec![[0.0; 3]; 15], &[f(4, 6, 2, 0)], |s| s);
        assert!(!h[0].broke);
    }
    use crate::timeline::{Change, Field, Person, Sample, Track};
    use std::borrow::Cow;
    use tf_demo_parser::demo::gameevent_gen::{TeamPlayRoundWinEvent, TeamPlaySetupFinishedEvent};

    /// At 10 ticks a second: four BLU and one RED alive for a 30 s round.
    /// The cart stands for 10 s, rolls for 10, stands for 10 with a BLU
    /// player on it.
    fn round() -> Timeline {
        let player = |team: u8, x: f32| Track {
            samples: (0..=300).step_by(5).map(|t| Sample { t, pos: [x, 0.0, 0.0], yaw: 0.0, pitch: 0.0 }).collect(),
            changes: [Field::Alive(true), Field::Here(true), Field::Team(team), Field::Class(1)].map(|field| Change { t: 0, field }).to_vec(),
        };
        let cart = |t: u32, x: i32| ObjectRow {
            t,
            entity: 7,
            kind: Cow::Borrowed("cart"),
            pos: [x, 0, 0],
            builder: None,
            team: None,
            level: None,
            health: None,
            building: false,
            sapped: false,
        };
        let mut objects = vec![cart(0, 0)];
        // Rolling 50 units a second from 10 s to 20 s, then parked beside
        // the pusher standing at x = 600.
        for s in 10..=20 {
            objects.push(cart(s * 10, ((s - 10) * 50) as i32));
        }
        Timeline {
            tick_rate: 10.0,
            people: (0..5).map(|i| Person { steamid: format!("[U:1:{i}]"), name: i.to_string() }).collect(),
            tracks: vec![player(3, 5000.0), player(3, 5000.0), player(3, 5000.0), player(3, 600.0), player(2, -5000.0)],
            objects,
            events: vec![
                (0, GameEvent::TeamPlaySetupFinished(TeamPlaySetupFinishedEvent {})),
                (
                    300,
                    GameEvent::TeamPlayRoundWin(TeamPlayRoundWinEvent {
                        team: 3,
                        win_reason: 1,
                        flag_cap_limit: 0,
                        full_round: 1,
                        round_time: 30.0,
                        losing_team_num_caps: 0,
                        was_sudden_death: 0,
                    }),
                ),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn still_seconds_while_up_split_into_empty_and_blocked() {
        let r = cart(&round()).expect("a cart");
        assert_eq!(r.rounds.len(), 1);
        let x = &r.rounds[0];
        assert_eq!(x.seconds, 30);
        assert_eq!(x.moving_s, 10);
        // Up 4-1 = +3 throughout.
        assert_eq!(x.up_s, 30);
        assert_eq!(x.up_still_s, 20);
        // Nobody near for the first 10 s; the pusher at 600 is beside the
        // parked cart (at 500) for the last 10.
        assert_eq!(x.up_still_empty_s, 10);
        let lens: Vec<u32> = r.stalls.iter().map(|s| s.seconds).collect();
        assert_eq!(lens, vec![10, 10]);
        assert_eq!(r.stalls[0].empty_s, 10);
        assert_eq!(r.stalls[1].empty_s, 0);
    }

    #[test]
    fn a_demo_without_a_cart_has_no_report() {
        let mut tl = round();
        tl.objects.clear();
        assert!(cart(&tl).is_none());
    }
}
