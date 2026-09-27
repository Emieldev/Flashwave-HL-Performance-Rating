//! A log from a demo alone (Q18, beowulf): for a match played on a server
//! with no logs.tf config, where no log exists at all.
//!
//! The whole app is built on logs, so rather than a second path through it,
//! the demo is turned into the two things a log is made of here:
//!
//! - **the server log's lines** -- spawns, kills with positions, damage,
//!   assists, ubers, Medic deaths, rounds and captures -- in the format
//!   `hl_ingest::rawlog` reads, so the fights pass, the game state, spawn
//!   delays and every other raw-log pass run on it unchanged;
//! - **the logs.tf summary** -- the scoreboard, time on class, class-vs-class
//!   and rounds with their events -- in the shape `normalize` reads, so the
//!   match page and the rating work as for any other log.
//!
//! An STV carries every line a log would, and more: the demo's own events
//! are the ones the server's logger writes from. What it cannot give is
//! anything logs.tf computes from the plugin's extra lines (real damage,
//! accuracy, airshots), which the summary marks as absent.

use crate::timeline::{GameEvent, Now, Timeline};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

/// `tf_demo_parser`'s class numbers, as logs.tf names them.
const CLASSES: [&str; 10] = ["", "scout", "sniper", "soldier", "demoman", "medic", "heavyweapons", "pyro", "spy", "engineer"];
/// The same, as a server log's `spawned as` writes them.
const LOG_CLASSES: [&str; 10] = ["", "Scout", "Sniper", "Soldier", "Demoman", "Medic", "HeavyWeapons", "Pyro", "Spy", "Engineer"];

/// `player_death`'s flag for a Dead Ringer's fake death.
const FEIGN_DEATH: u16 = 0x20;
/// `player_hurt`'s custom damage kind for a backstab.
const BACKSTAB: u16 = 2;

fn team_name(team: u8) -> Option<&'static str> {
    match team {
        2 => Some("Red"),
        3 => Some("Blue"),
        _ => None,
    }
}

/// What came out, and enough about it to say so.
#[derive(Debug, Clone)]
pub struct Synth {
    /// The server log, `L MM/DD/YYYY - HH:MM:SS: ...` lines.
    pub text: String,
    /// The logs.tf summary.
    pub json: Value,
    pub rounds: usize,
    pub kills: usize,
    pub players: usize,
}

#[derive(Default, Clone)]
struct ClassLine {
    time: f64,
    kills: i64,
    assists: i64,
    deaths: i64,
    dmg: i64,
}

#[derive(Default)]
struct Tally {
    kills: i64,
    deaths: i64,
    assists: i64,
    suicides: i64,
    dmg: i64,
    dt: i64,
    heal: i64,
    hr: i64,
    ubers: i64,
    drops: i64,
    headshots: i64,
    backstabs: i64,
    cpc: i64,
    streak: i64,
    lks: i64,
    classes: BTreeMap<u8, ClassLine>,
    class_kills: BTreeMap<u8, i64>,
    class_deaths: BTreeMap<u8, i64>,
    class_assists: BTreeMap<u8, i64>,
}

/// One round as logs.tf keeps it.
struct RoundSum {
    start: u32,
    end: Option<u32>,
    winner: Option<u8>,
    firstcap: Option<u8>,
    kills: [i64; 2],
    dmg: [i64; 2],
    ubers: [i64; 2],
    events: Vec<Value>,
    players: HashMap<usize, (u8, i64, i64)>,
}

/// The round being played, if one is.
fn open(rounds: &mut [RoundSum]) -> Option<&mut RoundSum> {
    rounds.last_mut().filter(|r| r.end.is_none())
}

/// `start_unix` is the wall-clock second of the demo's first tick, written
/// into the log "as if UTC" the way a server writes its local time.
pub fn synthesize(tl: &Timeline, map: &str, start_unix: i64, title: &str) -> Synth {
    let rate = tl.tick_rate.max(1.0);
    let secs = |t: u32| f64::from(t) / rate;
    let stretches: Vec<Vec<(u32, u32, Now)>> = (0..tl.people.len()).map(|s| tl.stretches(s)).collect();
    let state = |slot: usize, t: u32| -> Now {
        let s = &stretches[slot];
        match s.partition_point(|(from, _, _)| *from <= t).checked_sub(1) {
            Some(i) if s[i].1 > t || i + 1 == s.len() => s[i].2,
            _ => Now::default(),
        }
    };
    let user = |uid: u16| tl.slot_of_user(uid);
    let actor = |slot: usize, t: u32| -> String {
        let p = &tl.people[slot];
        let uid = tl.user_ids.iter().find(|(_, s)| usize::from(*s) == slot).map_or(0, |(u, _)| *u);
        let team = team_name(state(slot, t).team).unwrap_or("");
        // Quotes in a name would end the actor early for the parser.
        format!("\"{}<{uid}><{}><{team}>\"", p.name.replace('"', "'"), p.steamid)
    };
    let pos = |slot: usize, t: u32| -> String {
        tl.sample_near(slot, t, (rate as u32).max(1))
            .map_or_else(|| "0 0 0".to_string(), |s| format!("{} {} {}", s.pos[0] as i64, s.pos[1] as i64, s.pos[2] as i64))
    };

    // Lines as (t, order, text); sorted by time, keeping generation order
    // within a tick so a kill's assist follows it.
    let mut lines: Vec<(u32, usize, String)> = Vec::new();
    let push = |t: u32, s: String, lines: &mut Vec<(u32, usize, String)>| {
        let n = lines.len();
        lines.push((t, n, s));
    };

    let mut tally: Vec<Tally> = (0..tl.people.len()).map(|_| Tally::default()).collect();

    // Spawns and class changes, and time on class, from each player's state.
    for (slot, st) in stretches.iter().enumerate() {
        let mut prev: Option<Now> = None;
        for (from, to, n) in st {
            let was_alive = prev.is_some_and(|p| p.alive);
            if n.alive && n.here && (1..=9).contains(&n.class) {
                if !was_alive {
                    push(*from, format!("{} spawned as \"{}\"", actor(slot, *from), LOG_CLASSES[usize::from(n.class)]), &mut lines);
                } else if prev.is_some_and(|p| p.class != n.class) {
                    push(*from, format!("{} changed role to \"{}\"", actor(slot, *from), CLASSES[usize::from(n.class)]), &mut lines);
                }
                tally[slot].classes.entry(n.class).or_default().time += secs(to - from);
            }
            prev = Some(*n);
        }
    }

    // Charge states per Medic: ready when the meter reaches 100, ended when
    // a meter that was full has drained back to nothing.
    for (slot, track) in tl.tracks.iter().enumerate() {
        let (mut ready, mut draining) = (false, false);
        for c in &track.changes {
            let crate::timeline::Field::Charge(v) = c.field else { continue };
            if v >= 100 {
                if !ready {
                    push(c.t, format!("{} triggered \"chargeready\"", actor(slot, c.t)), &mut lines);
                }
                ready = true;
            } else if ready {
                ready = false;
                draining = true;
            }
            if v == 0 && draining {
                draining = false;
                push(c.t, format!("{} triggered \"chargeended\"", actor(slot, c.t)), &mut lines);
            }
        }
    }

    // Healing. `player_healed` fires for only some heals (a tenth of a
    // Medic's, measured against a real log), so healing is read the way it
    // happened: the patient's health going up while a Medic had them as
    // heal target. A respawn's full health is not a heal.
    let mut healers: Vec<Vec<(u32, u32, usize)>> = vec![Vec::new(); tl.people.len()];
    for (medic, st) in stretches.iter().enumerate() {
        for (from, to, n) in st {
            if let Some(target) = n.heal_target.map(usize::from).filter(|t| *t < healers.len() && *t != medic) {
                if n.live() {
                    healers[target].push((*from, *to, medic));
                }
            }
        }
    }
    for (patient, track) in tl.tracks.iter().enumerate() {
        if healers[patient].is_empty() {
            continue;
        }
        let mut last: Option<u16> = None;
        for c in &track.changes {
            match c.field {
                crate::timeline::Field::Alive(false) => last = None,
                crate::timeline::Field::Health(v) => {
                    if let Some(prev) = last.filter(|p| v > *p) {
                        if let Some(&(_, _, medic)) = healers[patient].iter().find(|(a, b, _)| *a <= c.t && c.t < *b) {
                            let gained = i64::from(v - prev);
                            tally[medic].heal += gained;
                            tally[patient].hr += gained;
                        }
                    }
                    if state(patient, c.t).alive {
                        last = Some(v);
                    }
                }
                _ => {}
            }
        }
    }

    let mut rounds: Vec<RoundSum> = Vec::new();
    let side = |team: u8| usize::from(team == 3);

    for (t, e) in &tl.events {
        let t = *t;
        match e {
            GameEvent::TeamPlayRoundActive(_) => {
                push(t, "World triggered \"Round_Start\"".into(), &mut lines);
                rounds.push(RoundSum {
                    start: t,
                    end: None,
                    winner: None,
                    firstcap: None,
                    kills: [0; 2],
                    dmg: [0; 2],
                    ubers: [0; 2],
                    events: Vec::new(),
                    players: HashMap::new(),
                });
            }
            GameEvent::TeamPlaySetupFinished(_) => push(t, "World triggered \"Round_Setup_End\"".into(), &mut lines),
            GameEvent::TeamPlayRoundWin(w) => {
                let winner = team_name(w.team).unwrap_or("");
                push(t, format!("World triggered \"Round_Win\" (winner \"{winner}\")"), &mut lines);
                if let Some(r) = open(&mut rounds) {
                    r.end = Some(t);
                    r.winner = Some(w.team);
                    r.events.push(json!({ "type": "round_win", "time": secs(t) as i64, "team": winner }));
                }
            }
            GameEvent::TeamPlayRoundStalemate(_) => {
                push(t, "World triggered \"Round_Stalemate\"".into(), &mut lines);
                if let Some(r) = open(&mut rounds) {
                    r.end = Some(t);
                }
            }
            GameEvent::TeamPlayGameOver(_) => push(t, "World triggered \"Game_Over\" reason \"Reached Win Limit\"".into(), &mut lines),
            GameEvent::TeamPlayPointCaptured(c) => {
                let Some(team) = team_name(c.team) else { continue };
                // Cappers are entity indices, one byte each.
                let cappers: Vec<usize> = c
                    .cappers
                    .as_ref()
                    .bytes()
                    .filter_map(|b| tl.slots_of_entity(u32::from(b)).find(|s| state(*s, t).team == c.team))
                    .collect();
                let mut line = format!(
                    "Team \"{team}\" triggered \"pointcaptured\" (cp \"{}\") (cpname \"{}\") (numcappers \"{}\")",
                    c.cp,
                    c.cp_name.as_ref().replace('"', "'"),
                    cappers.len()
                );
                for (i, s) in cappers.iter().enumerate() {
                    let _ = write!(line, " (player{} {}) (position{} \"{}\")", i + 1, actor(*s, t), i + 1, pos(*s, t));
                    tally[*s].cpc += 1;
                }
                push(t, line, &mut lines);
                if let Some(r) = open(&mut rounds) {
                    r.firstcap.get_or_insert(c.team);
                    r.events.push(json!({ "type": "pointcap", "time": secs(t) as i64, "team": team, "point": i64::from(c.cp) + 1 }));
                }
            }
            GameEvent::TeamPlayCaptureBlocked(b) => {
                if let Some(s) = tl.slots_of_entity(u32::from(b.blocker)).find(|s| state(*s, t).alive) {
                    push(t, format!("{} triggered \"captureblocked\" (cp \"{}\") (cpname \"{}\")", actor(s, t), b.cp, b.cp_name.as_ref().replace('"', "'")), &mut lines);
                }
            }
            GameEvent::PlayerHurt(h) => {
                let (Some(victim), Some(attacker)) = (user(h.user_id), user(h.attacker)) else { continue };
                if victim == attacker || h.damage_amount == 0 {
                    continue;
                }
                let (va, vv) = (state(attacker, t), state(victim, t));
                if va.team == vv.team {
                    continue;
                }
                // A backstab's hit is six times the victim's health; logs.tf
                // counts only the health there was to take.
                let mut amount = i64::from(h.damage_amount);
                if h.custom == BACKSTAB {
                    amount = amount.min(i64::from(state(victim, t.saturating_sub(1)).health).max(1));
                }
                push(t, format!("{} triggered \"damage\" against {} (damage \"{amount}\")", actor(attacker, t), actor(victim, t)), &mut lines);
                tally[attacker].dmg += amount;
                tally[victim].dt += amount;
                tally[attacker].classes.entry(va.class).or_default().dmg += amount;
                if let Some(r) = open(&mut rounds) {
                    if va.team == 2 || va.team == 3 {
                        r.dmg[side(va.team)] += amount;
                        r.players.entry(attacker).or_insert((va.team, 0, 0)).2 += amount;
                    }
                }
            }
            GameEvent::PlayerChargeDeployed(c) => {
                let Some(medic) = user(c.user_id) else { continue };
                let team = state(medic, t).team;
                push(t, format!("{} triggered \"chargedeployed\" (medigun \"medigun\")", actor(medic, t)), &mut lines);
                tally[medic].ubers += 1;
                if let Some(r) = open(&mut rounds) {
                    if team == 2 || team == 3 {
                        r.ubers[side(team)] += 1;
                    }
                    r.events.push(json!({ "type": "charge", "time": secs(t) as i64, "team": team_name(team), "steamid": tl.people[medic].steamid, "medigun": "medigun" }));
                }
            }
            GameEvent::MedicDeath(m) => {
                let Some(medic) = user(m.user_id) else { continue };
                let n = state(medic, t.saturating_sub(1));
                push(t, format!("{} triggered \"medic_death_ex\" (uberpct \"{}\")", actor(medic, t), n.charge), &mut lines);
                let team = team_name(n.team);
                let killer = user(m.attacker).map(|k| tl.people[k].steamid.clone());
                if m.charged {
                    tally[medic].drops += 1;
                }
                if let Some(r) = open(&mut rounds) {
                    r.events.push(json!({ "type": "medic_death", "time": secs(t) as i64, "team": team, "steamid": tl.people[medic].steamid, "killer": killer }));
                    if m.charged {
                        r.events.push(json!({ "type": "drop", "time": secs(t) as i64, "team": team, "steamid": tl.people[medic].steamid }));
                    }
                }
            }
            GameEvent::PlayerDeath(d) => {
                let Some(victim) = user(d.user_id) else { continue };
                let before = t.saturating_sub(1);
                let vn = state(victim, before);
                let attacker = user(d.attacker).filter(|a| *a != victim);
                let Some(attacker) = attacker else {
                    push(t, format!("{} committed suicide with \"{}\" (attacker_position \"{}\")", actor(victim, before), d.weapon, pos(victim, t)), &mut lines);
                    tally[victim].suicides += 1;
                    tally[victim].deaths += 1;
                    tally[victim].streak = 0;
                    continue;
                };
                let an = state(attacker, before);
                // A Dead Ringer's fake death: written as the server writes it,
                // and counted by nobody, as logs.tf counts it.
                let feign = d.death_flags & FEIGN_DEATH != 0;
                let custom = match d.custom_kill {
                    _ if feign => " (customkill \"feign_death\")",
                    1 => " (customkill \"headshot\")",
                    2 => " (customkill \"backstab\")",
                    _ => "",
                };
                push(
                    t,
                    format!(
                        "{} killed {} with \"{}\"{custom} (attacker_position \"{}\") (victim_position \"{}\")",
                        actor(attacker, before),
                        actor(victim, before),
                        d.weapon,
                        pos(attacker, t),
                        pos(victim, t)
                    ),
                    &mut lines,
                );
                // Humiliation after a round's win, and anything before the
                // first round, is not counted by logs.tf either.
                if an.team == vn.team || feign || open(&mut rounds).is_none() {
                    continue;
                }
                let a = &mut tally[attacker];
                a.kills += 1;
                a.streak += 1;
                a.lks = a.lks.max(a.streak);
                a.headshots += i64::from(d.custom_kill == 1);
                a.backstabs += i64::from(d.custom_kill == 2);
                a.classes.entry(an.class).or_default().kills += 1;
                *a.class_kills.entry(vn.class).or_default() += 1;
                let v = &mut tally[victim];
                v.deaths += 1;
                v.streak = 0;
                v.classes.entry(vn.class).or_default().deaths += 1;
                *v.class_deaths.entry(an.class).or_default() += 1;
                if let Some(assister) = user(d.assister).filter(|s| *s != attacker && *s != victim) {
                    push(t, format!("{} triggered \"kill assist\" against {} (assister_position \"{}\")", actor(assister, before), actor(victim, before), pos(assister, t)), &mut lines);
                    let s = &mut tally[assister];
                    s.assists += 1;
                    let class = state(assister, before).class;
                    s.classes.entry(class).or_default().assists += 1;
                    // logs.tf's classkillassists counts kills and assists together.
                    *s.class_assists.entry(vn.class).or_default() += 1;
                }
                *tally[attacker].class_assists.entry(vn.class).or_default() += 1;
                if let Some(r) = open(&mut rounds) {
                    if an.team == 2 || an.team == 3 {
                        r.kills[side(an.team)] += 1;
                        r.players.entry(attacker).or_insert((an.team, 0, 0)).1 += 1;
                    }
                }
            }
            _ => {}
        }
    }

    // ---- the log text ---------------------------------------------------
    lines.sort_by_key(|(t, n, _)| (*t, *n));
    let stamp = |t: u32| {
        let at = start_unix + secs(t) as i64;
        chrono::DateTime::from_timestamp(at, 0).map_or_else(String::new, |d| d.format("%m/%d/%Y - %H:%M:%S").to_string())
    };
    let mut text = String::new();
    let _ = writeln!(text, "L {}: Log file started (file \"demo\") (game \"tf\") (version \"synthesized\")", stamp(0));
    let _ = writeln!(text, "L {}: World triggered \"meta_data\" (map \"{map}\")", stamp(0));
    for (t, _, line) in &lines {
        let _ = writeln!(text, "L {}: {line}", stamp(*t));
    }
    let _ = writeln!(text, "L {}: Log file closed.", stamp(tl.end()));

    // ---- the summary ----------------------------------------------------
    // Each player's team: where they spent most of their live time.
    let team_of: Vec<Option<u8>> = stretches
        .iter()
        .map(|st| {
            let mut by: HashMap<u8, u32> = HashMap::new();
            for (a, b, n) in st.iter().filter(|(_, _, n)| n.live()) {
                *by.entry(n.team).or_default() += b - a;
            }
            by.into_iter().filter(|(t, _)| *t == 2 || *t == 3).max_by_key(|(_, v)| *v).map(|(t, _)| t)
        })
        .collect();

    let mut players = Map::new();
    let mut names = Map::new();
    let (mut classkills, mut classdeaths, mut classassists) = (Map::new(), Map::new(), Map::new());
    let by_class = |m: &BTreeMap<u8, i64>| -> Value {
        Value::Object(m.iter().filter(|(c, _)| (1..=9).contains(*c)).map(|(c, n)| (CLASSES[usize::from(*c)].to_string(), json!(n))).collect())
    };
    for (slot, p) in tl.people.iter().enumerate() {
        let Some(team) = team_of[slot] else { continue };
        let s = &tally[slot];
        let class_stats: Vec<Value> = s
            .classes
            .iter()
            .filter(|(c, l)| (1..=9).contains(*c) && l.time >= 1.0)
            .map(|(c, l)| {
                json!({
                    "type": CLASSES[usize::from(*c)],
                    "kills": l.kills,
                    "assists": l.assists,
                    "deaths": l.deaths,
                    "dmg": l.dmg,
                    "total_time": l.time.round() as i64,
                })
            })
            .collect();
        players.insert(
            p.steamid.clone(),
            json!({
                "team": team_name(team),
                "class_stats": class_stats,
                "kills": s.kills,
                "deaths": s.deaths,
                "assists": s.assists,
                "suicides": s.suicides,
                "dmg": s.dmg,
                "dt": s.dt,
                "hr": s.hr,
                "heal": s.heal,
                "ubers": s.ubers,
                "drops": s.drops,
                "headshots": s.headshots,
                "backstabs": s.backstabs,
                "cpc": s.cpc,
                "lks": s.lks,
            }),
        );
        names.insert(p.steamid.clone(), json!(p.name));
        classkills.insert(p.steamid.clone(), by_class(&s.class_kills));
        classdeaths.insert(p.steamid.clone(), by_class(&s.class_deaths));
        classassists.insert(p.steamid.clone(), by_class(&s.class_assists));
    }

    let wins = |team: u8| rounds.iter().filter(|r| r.winner == Some(team)).count();
    let rounds_json: Vec<Value> = rounds
        .iter()
        .filter(|r| r.end.is_some())
        .map(|r| {
            let end = r.end.unwrap_or(r.start);
            let per_team = |i: usize| json!({ "kills": r.kills[i], "dmg": r.dmg[i], "ubers": r.ubers[i] });
            let players: Map<String, Value> = r
                .players
                .iter()
                .map(|(slot, (team, kills, dmg))| (tl.people[*slot].steamid.clone(), json!({ "team": team_name(*team), "kills": kills, "dmg": dmg })))
                .collect();
            json!({
                "start_time": start_unix + secs(r.start) as i64,
                "winner": r.winner.and_then(team_name),
                "team": { "Red": per_team(0), "Blue": per_team(1) },
                "events": r.events,
                "players": players,
                "firstcap": r.firstcap.and_then(team_name),
                "length": (secs(end) - secs(r.start)).round() as i64,
            })
        })
        .collect();

    let kills: i64 = tally.iter().map(|t| t.kills).sum();
    let json = json!({
        "version": 3,
        "teams": {
            "Red": { "score": wins(2) },
            "Blue": { "score": wins(3) },
        },
        "length": secs(tl.end()).round() as i64,
        "players": players,
        "names": names,
        "rounds": rounds_json,
        "classkills": classkills,
        "classdeaths": classdeaths,
        "classkillassists": classassists,
        "info": {
            "map": map,
            "title": title,
            // A real log's date is its upload, just after the match: the
            // log clock reads the hour offset from it.
            "date": start_unix + secs(tl.end()) as i64 + 60,
            "supplemental": true,
            "hasRealDamage": false,
            "hasWeaponDamage": false,
            "hasAccuracy": false,
            "hasHP": false,
            "hasHP_real": false,
            "hasHS": true,
            "hasHS_hit": false,
            "hasBS": true,
            "hasCP": true,
            "hasSB": false,
            "hasDT": true,
            "hasAS": false,
            "hasHR": true,
            "hasIntel": false,
            "AD_scoring": false,
            "synthesized_from_demo": true,
        },
    });
    Synth {
        text,
        rounds: rounds_json.len(),
        kills: kills as usize,
        players: tally.len(),
        json,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::{Change, Field, Person, Track};
    use tf_demo_parser::demo::gameevent_gen::{PlayerDeathEvent, TeamPlayRoundActiveEvent, TeamPlayRoundWinEvent};

    fn death(t: u32, victim: u16, attacker: u16, flags: u16) -> (u32, GameEvent) {
        (
            t,
            GameEvent::PlayerDeath(Box::new(PlayerDeathEvent {
                user_id: victim,
                victim_ent_index: 0,
                inflictor_ent_index: 0,
                attacker,
                weapon: "scattergun".into(),
                weapon_id: 0,
                damage_bits: 0,
                custom_kill: 0,
                assister: 0,
                weapon_log_class_name: "scattergun".into(),
                stun_flags: 0,
                death_flags: flags,
                silent_kill: false,
                player_penetrate_count: 0,
                assister_fallback: "".into(),
                kill_streak_total: 0,
                kill_streak_wep: 0,
                kill_streak_assist: 0,
                kill_streak_victim: 0,
                ducks_streaked: 0,
                duck_streak_total: 0,
                duck_streak_assist: 0,
                duck_streak_victim: 0,
                rocket_jump: false,
                weapon_def_index: 0,
                crit_type: 0,
            })),
        )
    }

    /// A RED Scout (user 1) and a BLU Spy (user 2) at 10 ticks a second,
    /// both alive throughout; the round runs from 0 s to 30 s.
    fn match_of(events: Vec<(u32, GameEvent)>) -> Timeline {
        let alive = |team, class| Track {
            samples: Vec::new(),
            changes: [Field::Alive(true), Field::Here(true), Field::Team(team), Field::Class(class)].map(|field| Change { t: 0, field }).to_vec(),
        };
        let mut all = vec![(0, GameEvent::TeamPlayRoundActive(TeamPlayRoundActiveEvent {}))];
        all.extend(events);
        all.push((
            300,
            GameEvent::TeamPlayRoundWin(TeamPlayRoundWinEvent {
                team: 2,
                win_reason: 1,
                flag_cap_limit: 0,
                full_round: 1,
                round_time: 30.0,
                losing_team_num_caps: 0,
                was_sudden_death: 0,
            }),
        ));
        all.sort_by_key(|(t, _)| *t);
        Timeline {
            tick_rate: 10.0,
            people: vec![Person { steamid: "[U:1:1]".into(), name: "scout".into() }, Person { steamid: "[U:1:2]".into(), name: "spy".into() }],
            tracks: vec![alive(2, 1), alive(3, 8)],
            user_ids: vec![(1, 0), (2, 1)],
            events: all,
            ..Default::default()
        }
    }

    #[test]
    fn counts_kills_the_way_logs_tf_does() {
        let s = synthesize(
            &match_of(vec![
                death(100, 2, 1, 0),           // counts
                death(150, 2, 1, FEIGN_DEATH), // a Dead Ringer: nobody's
                death(350, 2, 1, 0),           // after the win: humiliation
            ]),
            "koth_test",
            1_790_000_000,
            "t",
        );
        let scout = &s.json["players"]["[U:1:1]"];
        let spy = &s.json["players"]["[U:1:2]"];
        assert_eq!((scout["kills"].as_i64(), spy["deaths"].as_i64()), (Some(1), Some(1)));
        assert_eq!(s.json["rounds"][0]["winner"], "Red");
        assert_eq!(s.json["teams"]["Red"]["score"], 1);
        // Every kill is still written, as a server writes it.
        assert_eq!(s.text.matches(" killed ").count(), 3);
        assert!(s.text.contains("(customkill \"feign_death\")"));
        assert!(s.text.contains("L 09/21/2026 - 14:13:20: World triggered \"Round_Start\""));
        assert_eq!(s.json["players"]["[U:1:1]"]["class_stats"][0]["type"], "scout");
    }
}
