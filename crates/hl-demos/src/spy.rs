//! Spychecks (Q27, ivg): hits on a Spy nobody could see.
//!
//! "Number of spy hits when he is fully cloaked and not blinking / on fire,
//! with a slight cooldown so tracking with a pistol or minigun counts once."
//! Cloak is a condition, and conditions exist only in a demo, so this is a
//! pure function over a [`Timeline`]: the hurt events say who hit whom and
//! when, the recorded conditions say whether the Spy could be seen.

use crate::timeline::{GameEvent, Now, PlayerCondition as C, Timeline};

/// Cloak fades in over `tf_spy_invis_time`, a second: until then the Spy is
/// a shimmer anyone can shoot at.
pub const FADE_SECONDS: f64 = 1.0;
/// Hits by one attacker on one Spy closer together than this are one check:
/// a minigun held on him is a read and a fire rate, and only the read counts.
pub const COOLDOWN_SECONDS: f64 = 2.0;
/// The Spy class number in `tf_demo_parser`.
const SPY: u8 = 8;

/// One hit on a fully cloaked Spy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spycheck {
    pub t: u32,
    /// Slots in the timeline.
    pub attacker: usize,
    pub spy: usize,
    pub damage: u16,
    /// Whether the hit killed him.
    pub killed: bool,
}

/// Why a hit on a cloaked Spy did not count, for checking the rules against
/// a demo by hand.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Skipped {
    /// Still fading in: under a second cloaked.
    pub fading: u32,
    /// Already blinking from an earlier hit or bump.
    pub blinking: u32,
    /// Burning, jarated, milked or bleeding: marked, so not a read.
    pub marked: u32,
    /// Within the cooldown of the same attacker's last hit on him.
    pub cooldown: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Spychecks {
    pub checks: Vec<Spycheck>,
    pub skipped: Skipped,
}

impl Spychecks {
    /// Spychecks made by `slot`.
    pub fn by(&self, slot: usize) -> usize {
        self.checks.iter().filter(|c| c.attacker == slot).count()
    }

    /// Times `slot` was found while cloaked.
    pub fn on(&self, slot: usize) -> usize {
        self.checks.iter().filter(|c| c.spy == slot).count()
    }
}

/// Everything that makes a cloaked Spy visible besides blinking.
fn marked(n: &Now) -> bool {
    n.has(C::Burning) || n.has(C::Urine) || n.has(C::MadMilk) || n.has(C::Bleeding)
}

pub fn spychecks(tl: &Timeline) -> Spychecks {
    let fade = (FADE_SECONDS * tl.tick_rate).round() as u32;
    let cooldown = (COOLDOWN_SECONDS * tl.tick_rate).round() as u32;
    // Stretches only for players who were ever a Spy: the rest are never
    // looked up, and building them is the costly part.
    let stretches: Vec<Vec<(u32, u32, Now)>> = (0..tl.people.len())
        .map(|slot| {
            let spied = tl.tracks[slot].changes.iter().any(|c| c.field == crate::timeline::Field::Class(SPY));
            if spied { tl.stretches(slot) } else { Vec::new() }
        })
        .collect();
    // The state strictly before `t`: a hit sets the blink on its own tick,
    // and the question is whether he could be seen before it landed.
    let before = |slot: usize, t: u32| -> Option<(usize, &(u32, u32, Now))> {
        let s = &stretches[slot];
        let i = s.partition_point(|(from, _, _)| *from < t).checked_sub(1)?;
        let st = &s[i];
        (st.1 >= t).then_some((i, st))
    };
    let team_at = |slot: usize, t: u32| tl.now(slot, t).team;

    let mut out = Spychecks::default();
    // Last hit per (attacker, spy), counted or not: the cooldown runs from
    // the latest one, so a held minigun stays one check for as long as it's held.
    let mut last: std::collections::HashMap<(usize, usize), u32> = std::collections::HashMap::new();
    for (t, e) in &tl.events {
        let GameEvent::PlayerHurt(h) = e else { continue };
        let (Some(spy), Some(attacker)) = (tl.slot_of_user(h.user_id), tl.slot_of_user(h.attacker)) else { continue };
        if spy == attacker || stretches[spy].is_empty() {
            continue;
        }
        let Some((i, (_, _, n))) = before(spy, *t) else { continue };
        if n.class != SPY || !n.live() || !n.has(C::Stealthed) {
            continue;
        }
        // Teammates' hits don't hurt, so any hurt is an enemy's; checked
        // anyway, as a demo can carry the odd self-inflicted or world hit.
        if team_at(attacker, *t) == n.team {
            continue;
        }
        let previous = last.insert((attacker, spy), *t);
        // Cloaked since the start of the unbroken run of cloaked stretches.
        let s = &stretches[spy];
        let mut since = s[i].0;
        for (from, to, m) in s[..i].iter().rev() {
            if *to != since || !(m.has(C::Stealthed) && m.live()) {
                break;
            }
            since = *from;
        }
        if t - since < fade {
            out.skipped.fading += 1;
        } else if n.has(C::StealthedBlink) {
            out.skipped.blinking += 1;
        } else if marked(n) {
            out.skipped.marked += 1;
        } else if previous.is_some_and(|p| t - p < cooldown) {
            out.skipped.cooldown += 1;
        } else {
            out.checks.push(Spycheck { t: *t, attacker, spy, damage: h.damage_amount, killed: h.health == 0 });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::{Change, Field, Person, Track};
    use tf_demo_parser::demo::gameevent_gen::PlayerHurtEvent;

    fn bits(conds: &[C]) -> crate::deep::Conds {
        let mut b = [0u8; 20];
        for c in conds {
            let n = *c as usize;
            b[n / 8] |= 1 << (n % 8);
        }
        b
    }

    fn hurt(t: u32, attacker: u16, victim: u16) -> (u32, GameEvent) {
        (
            t,
            GameEvent::PlayerHurt(PlayerHurtEvent {
                user_id: victim,
                health: 50,
                attacker,
                damage_amount: 20,
                custom: 0,
                show_disguised_crit: false,
                crit: false,
                mini_crit: false,
                all_see_crit: false,
                weapon_id: 0,
                bonus_effect: 0,
            }),
        )
    }

    /// A Heavy (user 10) and a Spy (user 20) who cloaks at 0, blinks at
    /// 1000, catches fire at 1500 and decloaks at 2000; at 100 ticks a second.
    fn demo(events: Vec<(u32, GameEvent)>) -> Timeline {
        let person = |n: &str| Person { steamid: format!("[U:1:{n}]"), name: n.into() };
        let alive = |t, team, class| {
            [Field::Alive(true), Field::Here(true), Field::Team(team), Field::Class(class)].map(|field| Change { t, field })
        };
        let heavy = Track { samples: Vec::new(), changes: alive(0, 3, 6).to_vec() };
        let mut spy = alive(0, 2, SPY).to_vec();
        for (t, conds) in [
            (0, bits(&[C::Stealthed])),
            (1000, bits(&[C::Stealthed, C::StealthedBlink])),
            (1100, bits(&[C::Stealthed])),
            (1500, bits(&[C::Stealthed, C::Burning])),
            (1600, bits(&[C::Stealthed])),
            (2000, bits(&[])),
        ] {
            spy.push(Change { t, field: Field::Conds(conds) });
        }
        Timeline {
            tick_rate: 100.0,
            people: vec![person("1"), person("2")],
            tracks: vec![heavy, Track { samples: Vec::new(), changes: spy }],
            user_ids: vec![(10, 0), (20, 1)],
            events,
            ..Default::default()
        }
    }

    #[test]
    fn counts_only_the_hits_nobody_could_see_coming() {
        let found = spychecks(&demo(vec![
            hurt(50, 10, 20),   // still fading in
            hurt(300, 10, 20),  // counts
            hurt(350, 10, 20),  // cooldown: the same Heavy half a second later
            hurt(450, 10, 20),  // cooldown again: runs from the latest hit
            hurt(700, 10, 20),  // counts: 2.5 s after the last
            hurt(1050, 10, 20), // blinking
            hurt(1550, 10, 20), // burning
            hurt(2500, 10, 20), // decloaked: not a hit on a cloaked Spy at all
        ]));
        assert_eq!(found.checks.iter().map(|c| c.t).collect::<Vec<_>>(), vec![300, 700]);
        assert_eq!(found.skipped, Skipped { fading: 1, blinking: 1, marked: 1, cooldown: 2 });
        assert_eq!((found.by(0), found.on(1)), (2, 2));
    }

    #[test]
    fn the_hit_that_starts_the_blink_is_the_check() {
        // The blink lands on the same tick as the hit that caused it; the
        // state that matters is the one before.
        let found = spychecks(&demo(vec![hurt(1000, 10, 20)]));
        assert_eq!(found.checks.len(), 1);
    }
}
