//! Aim, read from a demo (PLAN §14, Q16b).
//!
//! The demo carries its own kill events, each stamped with the exact tick, so
//! the shot itself needs no guessing from log times (those are only good to
//! the second, and a demo's start is itself an estimate). For every kill the
//! demo carried both ends of, three numbers come out:
//!
//! - **Crosshair error**: the angle between where the killer looked and the
//!   line to the victim's head, in degrees. Small means the crosshair was
//!   already there; large means it had to travel.
//! - **Flick**: how far the view turned in the half second before the shot.
//!   A small error after a large flick is a reaction; a small error after no
//!   flick is placement.
//! - **Range**: the distance to the victim at the shot, in map units, and the
//!   height difference. Exact here, unlike the log's rounded positions.
//!
//! **Every player, not only the recorder.** This pass used to take a single
//! SteamID and answer for that player alone, which is what made the Aim tab
//! lie the moment the match page's player select moved (Q16). A demo carries
//! all eighteen sets of eye angles, so there was never a reason to read one —
//! the work is in saying honestly *which* of them are worth believing:
//!
//! - In an **STV** demo every player is carried throughout, and the angles
//!   are the server's own.
//! - In a **POV** demo only the recorder is always there. Everyone else is
//!   carried while the recorder can see them (`in_pvs`), and their eye angles
//!   arrive quantized over the network rather than sampled from their mouse.
//!   Each [`Shot`] and [`Death`] therefore says whether the demo held that
//!   player across the whole window, and the caller decides what to keep.
//!
//! Angles use Source's convention: yaw counts anticlockwise from +x, pitch is
//! positive looking **down**. A player's origin is at their feet, so the head
//! is [`HEAD_HEIGHT`] above it.

use crate::deep::{has, DeepAnalyser};
use crate::parse::view_dir;
use crate::timeline::{Recorder, Stored, Timeline};
use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::path::Path;
use tf_demo_parser::demo::data::game_state::{PlayerCondition, PlayerState};
use tf_demo_parser::demo::gamevent::GameEvent;
use tf_demo_parser::demo::parser::analyser::{Class, Team};
use tf_demo_parser::{Demo, DemoParser};

/// A teammate this close is covering you: a Scout's fight is about this wide,
/// and a Medic on you is far closer.
pub const MATE_NEAR_UNITS: f32 = 900.0;

/// Eyes above the origin for a standing player. TF2's standing view offset.
pub const HEAD_HEIGHT: f32 = 68.0;

/// How long before the shot the "before" reading is taken.
pub const LEAD_S: f64 = 1.0;
/// How often to keep a position while walking a life: every this many ticks,
/// so about four a second. Fine enough to show a route, small enough that a
/// whole history is a couple of megabytes.
pub const PATH_STRIDE: u32 = 16;

/// How many points of the crosshair's path to keep per kill, the last one
/// being the shot itself. Eight over a second is enough to show a flick and
/// its overshoot without keeping every tick.
pub const PATH_POINTS: usize = 8;
/// The window the flick is measured over.
pub const FLICK_S: f64 = 0.5;

/// What the demo says about one kill.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shot {
    /// The demo tick of the kill, from the demo's own kill event.
    pub tick: u32,
    /// The tick "a second before" was read at: [`LEAD_S`] back, or as near
    /// as the demo holds.
    pub before_tick: u32,
    /// Who fired, as the demo writes SteamIDs (`[U:1:139131191]`).
    pub shooter: String,
    /// The victim, written the same way.
    pub victim: String,
    /// The weapon the demo names, which is not always the log's name.
    pub weapon: String,
    /// Degrees between the view and the victim's head at the shot, and
    /// [`LEAD_S`] before it.
    pub error_deg: f32,
    pub error_before_deg: f32,
    /// The same miss split into sideways and vertical degrees, so it can be
    /// drawn on a target: positive is right of the head and above it.
    pub dx_deg: f32,
    pub dy_deg: f32,
    pub before_dx_deg: f32,
    pub before_dy_deg: f32,
    /// Degrees the view turned over the [`FLICK_S`] before the shot.
    pub flick_deg: f32,
    /// Distance to the victim, and how far above the shooter they stood.
    pub range: f32,
    pub height: f32,
    /// The demo carried the **shooter** for the whole window. False in a POV
    /// demo for anyone who was out of the recorder's view a second before the
    /// shot, whose angles are then stale rather than measured.
    pub shooter_seen: bool,
    /// The same for the victim. A POV demo drops players it never showed.
    pub victim_seen: bool,
    /// Where the crosshair sat through the second before, oldest first and
    /// ending at the shot: `(sideways, vertical)` degrees from the head, the
    /// same convention as `dx_deg`.
    pub path: Vec<(f32, f32)>,
}

/// What the demo says about one death.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Death {
    pub tick: u32,
    /// Who died, as the demo writes SteamIDs.
    pub who: String,
    /// Who killed them, written the same way.
    pub killer: String,
    pub killer_range: Option<f32>,
    /// Where the killer stood relative to where the victim was looking: the
    /// answer to "did I even see them". Positive is to their right and above.
    pub killer_dx_deg: Option<f32>,
    pub killer_dy_deg: Option<f32>,
    /// How far the closest living teammate was, and how many were inside
    /// [`MATE_NEAR_UNITS`].
    pub nearest_mate: Option<f32>,
    pub mates_near: u8,
    /// They were scoped in at some point in the second before it.
    pub scoped: bool,
    /// The demo carried the player who died across the whole window.
    pub seen: bool,
}

/// A gap this long in what the demo carried starts a new route: a POV demo
/// drops players who walk out of sight, and joining the two ends would draw a
/// line through a wall.
pub const PATH_GAP_TICKS: u32 = 64;

/// One life, as a route across the map.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LifePath {
    /// Whose route it is, as the demo writes SteamIDs.
    pub steamid: String,
    /// The tick the life starts and ends on, in the demo's own clock.
    pub from_tick: u32,
    pub to_tick: u32,
    /// It ended in a death rather than a round end or the demo running out.
    pub died: bool,
    /// Where they were, every [`PATH_STRIDE`] ticks: `(tick, x, y, z)`,
    /// rounded to whole units because a map is 8,000 units across.
    pub points: Vec<Step>,
}

/// One hit two seconds after another by the same player on the same Spy is
/// the same spycheck: a minigun held on a cloaked Spy is one read, not
/// thirty (ivg's own correction to their suggestion).
pub const SPYCHECK_COOLDOWN_S: f64 = 2.0;

/// A spycheck (Q27, ivg): damage on an enemy Spy who, the tick *before*
/// the hit, was fully cloaked -- not flickering, not on fire. That is a hit
/// on someone the shooter could not see, which is what checking means.
///
/// The tick before, because the hit itself sets the flicker: read at the
/// hit's own tick, every hit on a cloaked Spy would look like one on a
/// visible Spy and nothing would ever count.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Spycheck {
    pub tick: u32,
    pub attacker: String,
    pub spy: String,
}

/// How one player spent the match.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Time {
    pub alive_ticks: u32,
    pub scoped_ticks: u32,
}

/// One pass over a demo: the kills, the deaths, and how the time was spent.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pass {
    pub shots: Vec<Shot>,
    pub deaths: Vec<Death>,
    /// Where everyone walked, one route per life (PLAN §14). A POV demo
    /// carries its recorder throughout, so their routes are whole; everyone
    /// else is only in the demo while the recorder could see them, so their
    /// routes break wherever the demo lost them.
    pub lives: Vec<LifePath>,
    /// Ticks alive and ticks scoped, per player. Only the recorder's is
    /// complete in a POV demo: everyone else stops being counted the moment
    /// they leave the recorder's view.
    pub time: HashMap<String, Time>,
    /// Hits on cloaked Spies, cooldown applied (Q27).
    pub spychecks: Vec<Spycheck>,
}

impl Pass {
    /// The share of a player's living time spent scoped, where there was any.
    pub fn scoped_share(&self, steamid: &str) -> Option<f64> {
        let t = self.time.get(steamid)?;
        (t.alive_ticks > 0).then(|| f64::from(t.scoped_ticks) / f64::from(t.alive_ticks))
    }
}

/// Read the demo at `path` once: every kill the demo carried with the aim
/// behind it, every death with who was nearby, and how each player's time was
/// spent.
///
/// `owner` is the POV recorder's SteamID, or empty for an STV demo. The
/// recorder is the one player a POV demo carries even when `in_pvs` says
/// otherwise, so naming them is what keeps their own routes and angles whole.
///
/// Only the last [`LEAD_S`] of ticks is held, so memory stays flat however
/// long the demo is.
pub fn pass(path: &Path, owner: &str, tick_rate: f64) -> Result<Pass> {
    Ok(pass_recording(path, owner, tick_rate, None, &mut |_| {})?.0)
}

/// How often `on_tick` hears where the walk is: about four times a second
/// of play, which is plenty for a progress bar and costs nothing.
pub const PROGRESS_EVERY: u32 = 256;

/// [`pass`], and in the same walk through the demo, its timeline sampled
/// every `stride` ticks when one is asked for -- already in its stored form,
/// written as the walk goes rather than held and encoded at the end. Reading
/// a demo is the slow part, so keeping it costs one walk, not two.
///
/// `on_tick` is told the demo tick every [`PROGRESS_EVERY`] ticks, for a
/// progress bar.
pub fn pass_recording(
    path: &Path,
    owner: &str,
    tick_rate: f64,
    stride: Option<u32>,
    on_tick: &mut dyn FnMut(u32),
) -> Result<(Pass, Option<Stored>)> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let demo = Demo::new(&bytes);
    let mut recorder = stride.map(|s| Recorder::new(s, tick_rate));
    let (_, mut ticker) = DemoParser::new_with_analyser(demo.get_stream(), DeepAnalyser::new())
        .ticker()
        .with_context(|| format!("opening {}", path.display()))?;

    let depth = ((LEAD_S * tick_rate).round() as usize).max(2);
    // How far back "a second before" and the flick look, in ticks. One
    // tick short of the rounded second, which is where they always read on
    // a demo holding every tick.
    let lead = (depth as u32).saturating_sub(1);
    let flick_ticks = (FLICK_S * tick_rate).round() as u32;
    // The last `depth` ticks: where everyone stood and looked.
    let mut recent: VecDeque<Frame> = VecDeque::with_capacity(depth + 1);
    let mut out = Pass::default();
    let mut done = 0usize;
    // Game events already read, and the last tick each (attacker, spy) pair
    // was counted, for the cooldown.
    let mut events_done = 0usize;
    let cooldown = (SPYCHECK_COOLDOWN_S * tick_rate).round() as u32;
    let mut last_check: HashMap<(u16, u16), u32> = HashMap::new();
    let mut last = u32::MAX;
    // A route per player, still being walked: the points so far, and the tick
    // the demo last carried them.
    let mut routes: HashMap<String, OpenRoute> = HashMap::new();
    // Each player's SteamID, once, by user id: handed out to every tick's
    // frame as a shared reference instead of copied per player per tick.
    let mut ids: HashMap<u16, Rc<str>> = HashMap::new();

    while ticker.tick().with_context(|| format!("parsing {}", path.display()))? {
        let deep = ticker.state();
        if let Some(r) = recorder.as_mut() {
            r.observe(deep, owner);
        }
        let state = deep.state();
        let tick = u32::from(state.tick);
        if tick == last {
            continue;
        }
        if tick % PROGRESS_EVERY == 0 {
            on_tick(tick);
        }
        // One recording can hold two matches, and the tick counter restarts
        // when the map does. Everything in flight belongs to the match that
        // just ended, so it is closed at the seam rather than carried across
        // it: a route joined over a restart draws a line between two maps,
        // and the "second before" reading would come from the other one.
        //
        // The subtraction below is why this is a guard and not a comment.
        // It has always been there, and in a release build a tick that went
        // backwards wrapped to a huge number, which closed the route and so
        // happened to look right; in a debug build the same line panics.
        if last != u32::MAX && tick < last {
            for (steamid, (points, _)) in routes.iter_mut() {
                if let Some(done) = close(steamid, points, false) {
                    out.lives.push(done);
                }
            }
            routes.clear();
            recent.clear();
        }
        last = tick;

        // This tick: every player the demo named, by user id. The frame
        // falling out of the window lends its map, so a demo's hundreds of
        // thousands of ticks do not each allocate one.
        let mut players = if recent.len() == depth {
            recent.pop_front().map(|f| f.players).unwrap_or_default()
        } else {
            HashMap::new()
        };
        players.clear();
        let mut frame = Frame { tick, players };
        for p in &state.players {
            let Some(info) = &p.info else { continue };
            let user_id = u16::from(info.user_id);
            // One shared copy of each SteamID for the whole walk, rather than
            // a fresh String per player per tick.
            let id = match ids.get(&user_id) {
                Some(id) if **id == *info.steam_id => id.clone(),
                _ => {
                    let id: Rc<str> = Rc::from(info.steam_id.as_str());
                    ids.insert(user_id, id.clone());
                    id
                }
            };
            // The recorder is always there, whatever `in_pvs` reports of the
            // entity carrying them.
            let here = p.in_pvs || info.steam_id == owner;
            // Read off the wire, not through the parser's `has_condition`,
            // which misses a condition whenever a higher one in the same byte
            // is set (see `deep`).
            let bits = deep.conds_of(p.entity);
            let seen = Seen {
                steamid: id,
                pos: [p.position.x, p.position.y, p.position.z],
                yaw: p.view_angle,
                pitch: p.pitch_angle,
                team: p.team,
                alive: p.state == PlayerState::Alive,
                here,
                scoped: has(&bits, PlayerCondition::Zoomed),
                invisible: p.class == Class::Spy
                    && has(&bits, PlayerCondition::Stealthed)
                    && !has(&bits, PlayerCondition::StealthedBlink)
                    && !has(&bits, PlayerCondition::Burning),
            };
            if seen.alive && here {
                // Looked up by reference; the key is only copied the first
                // time a player is seen alive.
                let t = match out.time.get_mut(&*seen.steamid) {
                    Some(t) => t,
                    None => out.time.entry(seen.steamid.to_string()).or_default(),
                };
                t.alive_ticks += 1;
                t.scoped_ticks += u32::from(seen.scoped);
            }
            frame.players.insert(user_id, seen);
        }
        // Everyone's route. A player the demo is not carrying this tick, or
        // who is dead, has their route closed rather than extended.
        if tick % PATH_STRIDE == 0 {
            for p in frame.players.values() {
                let walking = p.alive && p.here;
                let step = (tick, p.pos[0] as i32, p.pos[1] as i32, p.pos[2] as i32);
                match routes.get_mut(&*p.steamid) {
                    Some((points, last_seen)) => {
                        if !walking || tick.saturating_sub(*last_seen) > PATH_GAP_TICKS {
                            if let Some(done) = close(&p.steamid, points, false) {
                                out.lives.push(done);
                            }
                        }
                        if walking {
                            points.push(step);
                            *last_seen = tick;
                        }
                    }
                    None if walking => {
                        routes.insert(p.steamid.to_string(), (vec![step], tick));
                    }
                    None => {}
                }
            }
        }

        recent.push_back(frame);

        // Kills are appended to the state as they happen, so anything new
        // since the last tick belongs to this moment, with the view still fresh.
        for kill in state.kills.iter().skip(done) {
            if let Some(shot) = shot_for(&recent, kill.attacker_id, kill.victim_id, &kill.weapon, lead, flick_ticks) {
                out.shots.push(shot);
            }
            // Whoever died has their route closed here, so it ends where they
            // fell rather than at the next tick they were not carried.
            if let Some(who) = now_id(&recent, kill.victim_id) {
                if let Some((points, _)) = routes.get_mut(&who) {
                    if let Some(done) = close(&who, points, true) {
                        out.lives.push(done);
                    }
                }
            }
            if let Some(death) = death_for(&recent, kill.attacker_id, kill.victim_id) {
                out.deaths.push(death);
            }
        }
        done = state.kills.len();

        // Hits since the last tick, judged against where things stood the
        // tick before: `recent` now ends with this tick, so the frame before
        // it is the one that says whether the Spy could be seen.
        let prev = recent.len().checked_sub(2).and_then(|i| recent.get(i));
        for (_, event) in state.events.iter().skip(events_done) {
            let GameEvent::PlayerHurt(h) = event else { continue };
            let (spy_id, by) = (h.user_id, h.attacker);
            if spy_id == by {
                continue;
            }
            let Some(before) = prev.and_then(|f| f.players.get(&spy_id)) else { continue };
            // Invisible, and carried by the demo -- a POV demo only knows a
            // Spy's cloak while its recorder can see where he is.
            if !(before.invisible && before.here) {
                continue;
            }
            let Some(now) = recent.back() else { continue };
            let Some(attacker) = now.players.get(&by) else { continue };
            if attacker.team == before.team {
                continue;
            }
            if let Some(&t) = last_check.get(&(by, spy_id)) {
                if tick.saturating_sub(t) < cooldown {
                    continue;
                }
            }
            last_check.insert((by, spy_id), tick);
            out.spychecks.push(Spycheck { tick, attacker: attacker.steamid.to_string(), spy: before.steamid.to_string() });
        }
        events_done = state.events.len();
    }
    // The demo ran out: close whatever was still being walked.
    for (steamid, (points, _)) in routes.iter_mut() {
        if let Some(done) = close(steamid, points, false) {
            out.lives.push(done);
        }
    }
    // Under a second of someone flickering through the demo's view is not a
    // route: it draws as a speck and there are thousands of them.
    out.lives.retain(|l| l.points.len() >= 4);
    out.lives.sort_by_key(|l| (l.from_tick, l.steamid.clone()));
    let stored = recorder.map(Recorder::finish).transpose()?;
    Ok((out, stored))
}

/// Take the points walked so far as a finished route. `None` when there is
/// nothing to take.
fn close(steamid: &str, points: &mut Vec<Step>, died: bool) -> Option<LifePath> {
    if points.is_empty() {
        return None;
    }
    let points = std::mem::take(points);
    Some(LifePath {
        steamid: steamid.to_string(),
        from_tick: points.first().map_or(0, |p| p.0),
        to_tick: points.last().map_or(0, |p| p.0),
        died,
        points,
    })
}

/// The SteamID behind a user id, as of the latest tick.
fn now_id(recent: &VecDeque<Frame>, user_id: u16) -> Option<String> {
    Some(recent.back()?.players.get(&user_id)?.steamid.to_string())
}

/// One death, from the tick it happened on.
fn death_for(recent: &VecDeque<Frame>, attacker: u16, victim: u16) -> Option<Death> {
    let now = recent.back()?;
    let died = now.players.get(&victim)?;
    let killer = now.players.get(&attacker);
    let killer_seen = killer.filter(|k| k.here).map(|k| k.pos);
    let killer_range = killer_seen.map(|p| dist(died.pos, p));
    // offset_to says where the crosshair was relative to them; negate it to
    // say where they were relative to the crosshair.
    let killer_offset = killer_seen.map(|p| {
        let (x, y) = offset_to(died.pos, died.yaw, died.pitch, p);
        (-x, -y)
    });
    // Living teammates the demo carried, for "was anyone watching my flank".
    let mut nearest: Option<f32> = None;
    let mut near = 0u8;
    for m in now.players.values() {
        if m.steamid == died.steamid || m.team != died.team || !m.alive || !m.here {
            continue;
        }
        let d = dist(died.pos, m.pos);
        nearest = Some(nearest.map_or(d, |n: f32| n.min(d)));
        if d <= MATE_NEAR_UNITS {
            near += 1;
        }
    }
    let scoped = recent.iter().any(|f| f.players.get(&victim).is_some_and(|p| p.scoped));
    Some(Death {
        tick: now.tick,
        who: died.steamid.to_string(),
        killer: killer.map(|k| k.steamid.to_string()).unwrap_or_default(),
        killer_range,
        killer_dx_deg: killer_offset.map(|(x, _)| x),
        killer_dy_deg: killer_offset.map(|(_, y)| y),
        nearest_mate: nearest,
        mates_near: near,
        scoped,
        seen: carried(recent, victim),
    })
}

/// The demo held this player at both ends of the window. A player who was
/// only there for the shot has their "second before" reading taken from
/// wherever they were last carried, which measures nothing.
fn carried(recent: &VecDeque<Frame>, user_id: u16) -> bool {
    [recent.front(), recent.back()]
        .iter()
        .flatten()
        .all(|f| f.players.get(&user_id).is_some_and(|p| p.here))
}

struct Frame {
    tick: u32,
    /// Everyone the demo named this tick, by user id.
    players: HashMap<u16, Seen>,
}

/// One player at one tick.
struct Seen {
    /// Shared across ticks: see `ids` in `pass_recording`.
    steamid: Rc<str>,
    pos: Pos,
    yaw: f32,
    pitch: f32,
    team: Team,
    alive: bool,
    /// The demo carried them this tick. Always true for an STV demo and for a
    /// POV demo's own recorder; false for anyone the recorder could not see.
    here: bool,
    scoped: bool,
    /// A Spy who could not be seen: cloaked, not flickering, not burning.
    invisible: bool,
}

type Pos = [f32; 3];

/// A point on a route: the tick, and where they stood, in whole map units.
type Step = (u32, i32, i32, i32);

/// A route still being walked: its points, and the tick the demo last
/// carried that player.
type OpenRoute = (Vec<Step>, u32);

/// The aim behind one kill, from the ticks leading up to it.
/// The frame nearest `ticks` before the newest one. By tick and not by
/// position in `recent`: a POV demo is written as often as the client was
/// sent updates, which skips ticks, so "66 frames back" there is more than a
/// second.
fn back_by(recent: &VecDeque<Frame>, ticks: u32) -> Option<&Frame> {
    let target = recent.back()?.tick.saturating_sub(ticks);
    recent.iter().min_by_key(|f| f.tick.abs_diff(target))
}

fn shot_for(recent: &VecDeque<Frame>, attacker: u16, victim: u16, weapon: &str, lead: u32, flick_ticks: u32) -> Option<Shot> {
    let now = recent.back()?;
    // Not a suicide, and both ends named this tick.
    if attacker == victim {
        return None;
    }
    let shooter = now.players.get(&attacker)?;
    let victim_now = now.players.get(&victim)?;
    let (pos, yaw, pitch) = (shooter.pos, shooter.yaw, shooter.pitch);
    let victim_pos = victim_now.pos;
    let error = angle_to(pos, yaw, pitch, victim_pos);
    let (dx, dy) = offset_to(pos, yaw, pitch, victim_pos);

    // A second earlier, against where the victim was then.
    let then = back_by(recent, lead)?;
    let earlier = then.players.get(&attacker).zip(then.players.get(&victim));
    let before = earlier.map_or(error, |(s, v)| angle_to(s.pos, s.yaw, s.pitch, v.pos));
    let (before_dx, before_dy) = earlier.map_or((dx, dy), |(s, v)| offset_to(s.pos, s.yaw, s.pitch, v.pos));

    // The path the crosshair took to get there, thinned to PATH_POINTS.
    let step = (recent.len() / PATH_POINTS).max(1);
    let path: Vec<(f32, f32)> = recent
        .iter()
        .enumerate()
        .filter(|(i, _)| i % step == 0 || *i == recent.len() - 1)
        .filter_map(|(_, f)| {
            let s = f.players.get(&attacker)?;
            let v = f.players.get(&victim)?;
            (s.here && v.here).then(|| offset_to(s.pos, s.yaw, s.pitch, v.pos))
        })
        .collect();

    // The flick: how far the view turned over the last FLICK_S.
    let flick = back_by(recent, flick_ticks)
        .and_then(|f| f.players.get(&attacker))
        .map_or(0.0, |s| angle_between(view_dir(s.yaw, s.pitch), view_dir(yaw, pitch)));

    Some(Shot {
        tick: now.tick,
        before_tick: then.tick,
        shooter: shooter.steamid.to_string(),
        victim: victim_now.steamid.to_string(),
        weapon: weapon.to_string(),
        error_deg: error,
        error_before_deg: before,
        dx_deg: dx,
        dy_deg: dy,
        before_dx_deg: before_dx,
        before_dy_deg: before_dy,
        flick_deg: flick,
        range: dist(pos, victim_pos),
        height: victim_pos[2] - pos[2],
        shooter_seen: carried(recent, attacker),
        victim_seen: carried(recent, victim),
        path,
    })
}

/// The angle between where a player looks and their line to a head, degrees.
fn angle_to(from: Pos, yaw: f32, pitch: f32, target: Pos) -> f32 {
    let eye = [from[0], from[1], from[2] + HEAD_HEIGHT];
    let head = [target[0], target[1], target[2] + HEAD_HEIGHT];
    let to = [head[0] - eye[0], head[1] - eye[1], head[2] - eye[2]];
    angle_between(view_dir(yaw, pitch), to)
}

/// Where the crosshair sat relative to the head, in degrees: sideways first,
/// then vertical, both positive when the crosshair was to the right of the
/// head and above it. Their combination is [`angle_to`], give or take the
/// usual rounding.
///
/// Source counts yaw anticlockwise, so a head at a greater yaw than the view
/// is to the player's left, which puts the crosshair to its right: the
/// sideways term needs no flip. Pitch counts downwards, so the vertical term
/// does.
fn offset_to(from: Pos, yaw: f32, pitch: f32, target: Pos) -> (f32, f32) {
    let eye = [from[0], from[1], from[2] + HEAD_HEIGHT];
    let head = [target[0], target[1], target[2] + HEAD_HEIGHT];
    let (dx, dy, dz) = (head[0] - eye[0], head[1] - eye[1], head[2] - eye[2]);
    let flat = (dx * dx + dy * dy).sqrt();
    if flat == 0.0 && dz == 0.0 {
        return (0.0, 0.0);
    }
    // Where the head is, in the same angles the view uses.
    let head_yaw = dy.atan2(dx).to_degrees();
    let head_pitch = -dz.atan2(flat).to_degrees();
    (wrap180(head_yaw - yaw), head_pitch - pitch)
}

/// An angle difference folded into -180..180 degrees.
fn wrap180(deg: f32) -> f32 {
    let d = (deg + 180.0).rem_euclid(360.0) - 180.0;
    if d == -180.0 {
        180.0
    } else {
        d
    }
}

/// The angle between two vectors, degrees. Zero-length vectors give 0.
fn angle_between(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let len = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt() * (b[0] * b[0] + b[1] * b[1] + b[2] * b[2]).sqrt();
    if len == 0.0 {
        return 0.0;
    }
    (dot / len).clamp(-1.0, 1.0).acos().to_degrees()
}

fn dist(a: Pos, b: Pos) -> f32 {
    let (dx, dy, dz) = (b[0] - a[0], b[1] - a[1], b[2] - a[2]);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// How closely a [`Timeline`] reproduces what [`pass`] measured from the
/// demo itself -- the check that what is stored is faithful enough for later
/// passes to be derived from it instead of from the file.
#[derive(Debug, Default)]
pub struct Agreement {
    /// `(steamid, scoped share from the pass, from the timeline)`.
    pub scoped: Vec<(String, f64, f64)>,
    /// Kills whose crosshair error a second before could be recomputed.
    pub shots_compared: usize,
    pub shots_skipped: usize,
    /// Mean and worst absolute difference in that error, degrees.
    pub before_mean_diff: f64,
    pub before_max_diff: f64,
}

pub fn agreement(pass: &Pass, tl: &Timeline) -> Agreement {
    let mut out = Agreement::default();

    // Scoped share: time alive, carried and scoped over time alive and
    // carried.
    for (slot, person) in tl.people.iter().enumerate() {
        let Some(from_pass) = pass.scoped_share(&person.steamid) else { continue };
        let live = tl.ticks_where(slot, |_| true);
        let zoomed = tl.ticks_where(slot, |n| n.has(PlayerCondition::Zoomed));
        if live > 0 {
            out.scoped.push((person.steamid.clone(), from_pass, f64::from(zoomed) / f64::from(live)));
        }
    }

    // Crosshair error a second before each kill, at the same tick the pass
    // read it.
    let mut sum = 0.0;
    // Shots come in the demo's order, so a tick going backwards is the demo
    // restarting: later kills belong to the next stretch, not the first one
    // that happens to hold the same tick number.
    let (mut stretch, mut prev) = (0usize, 0u32);
    for shot in &pass.shots {
        if shot.tick < prev {
            stretch += 1;
        }
        prev = shot.tick;
        let found = (|| {
            if !(shot.shooter_seen && shot.victim_seen) {
                return None;
            }
            // A kill that does not fit this stretch belongs to a later one.
            let t = loop {
                match tl.t_in(stretch, shot.tick) {
                    Some(t) => break t,
                    None if stretch + 1 < tl.seams.len() => stretch += 1,
                    None => return None,
                }
            };
            let t = t.checked_sub(shot.tick.checked_sub(shot.before_tick)?)?;
            let s = tl.sample_near(tl.slot_of(&shot.shooter)?, t, tl.stride.saturating_sub(1))?;
            let v = tl.sample_near(tl.slot_of(&shot.victim)?, t, tl.stride.saturating_sub(1))?;
            Some(angle_to(s.pos, s.yaw, s.pitch, v.pos))
        })();
        match found {
            Some(err) => {
                let d = f64::from((err - shot.error_before_deg).abs());
                sum += d;
                out.before_max_diff = out.before_max_diff.max(d);
                out.shots_compared += 1;
            }
            None => out.shots_skipped += 1,
        }
    }
    if out.shots_compared > 0 {
        out.before_mean_diff = sum / out.shots_compared as f64;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame from `(user_id, steamid, pos, yaw, team, alive, here)`.
    fn frame(tick: u32, players: &[(u16, &str, Pos, f32, Team, bool, bool)]) -> Frame {
        let mut map = HashMap::new();
        for &(uid, steamid, pos, yaw, team, alive, here) in players {
            map.insert(
                uid,
                Seen { steamid: steamid.into(), pos, yaw, pitch: 0.0, team, alive, here, scoped: false, invisible: false },
            );
        }
        Frame { tick, players: map }
    }

    #[test]
    fn looking_straight_at_a_head_is_no_error() {
        // Standing at the origin, looking along +x at someone 500 units away.
        let e = angle_to([0.0, 0.0, 0.0], 0.0, 0.0, [500.0, 0.0, 0.0]);
        assert!(e < 0.01, "{e}");
    }

    #[test]
    fn looking_past_them_is_the_angle_past_them() {
        // They are 500 units along +x; the view is 30 degrees off.
        let e = angle_to([0.0, 0.0, 0.0], 30.0, 0.0, [500.0, 0.0, 0.0]);
        assert!((e - 30.0).abs() < 0.01, "{e}");
        // Above them: the target is 500 up and 500 along, so 45 degrees.
        let e = angle_to([0.0, 0.0, 0.0], 0.0, 0.0, [500.0, 0.0, 500.0]);
        assert!((e - 45.0).abs() < 0.01, "{e}");
    }

    #[test]
    fn pitch_is_positive_looking_down() {
        // Someone 500 along and 500 below: looking 45 degrees down is exact.
        let e = angle_to([0.0, 0.0, 0.0], 0.0, 45.0, [500.0, 0.0, -500.0]);
        assert!(e < 0.01, "{e}");
    }

    #[test]
    fn the_miss_splits_into_sideways_and_vertical_degrees() {
        // Looking along +x; the head is 30 degrees to the left (+y is left of
        // +x in Source's anticlockwise yaw), so the offset is +30 sideways.
        let (dx, dy) = offset_to([0.0, 0.0, 0.0], 0.0, 0.0, [500.0, 289.0, 0.0]);
        assert!((dx - 30.0).abs() < 0.2, "{dx}");
        assert!(dy.abs() < 0.01, "{dy}");
        // Level view, head 500 above: the crosshair sits 45 degrees below it.
        let (dx, dy) = offset_to([0.0, 0.0, 0.0], 0.0, 0.0, [500.0, 0.0, 500.0]);
        assert!(dx.abs() < 0.01, "{dx}");
        assert!((dy + 45.0).abs() < 0.2, "below the head, so negative: {dy}");
        // Head 500 below, level view: the crosshair sits above it.
        let (_, dy) = offset_to([0.0, 0.0, 0.0], 0.0, 0.0, [500.0, 0.0, -500.0]);
        assert!((dy - 45.0).abs() < 0.2, "above the head, so positive: {dy}");
        // Looking right at them: no miss either way.
        let (dx, dy) = offset_to([0.0, 0.0, 0.0], 0.0, 45.0, [500.0, 0.0, -500.0]);
        assert!(dx.abs() < 0.01 && dy.abs() < 0.2, "{dx} {dy}");
    }

    #[test]
    fn angles_wrap_the_short_way_round() {
        assert!((wrap180(350.0) + 10.0).abs() < 1e-6);
        assert!((wrap180(-350.0) - 10.0).abs() < 1e-6);
        assert!((wrap180(10.0) - 10.0).abs() < 1e-6);
    }

    #[test]
    fn a_flick_is_the_turn_between_two_views() {
        assert!((angle_between(view_dir(0.0, 0.0), view_dir(90.0, 0.0)) - 90.0).abs() < 0.01);
        assert!(angle_between(view_dir(10.0, -5.0), view_dir(10.0, -5.0)) < 0.01);
    }

    /// Two ticks a second apart: one player turns onto another and kills
    /// them. The view ends on the head, and the turn is the flick.
    #[test]
    fn a_kill_reads_its_ticks() {
        let mut recent = VecDeque::new();
        for (tick, yaw) in [(0u32, 60.0f32), (66, 0.0)] {
            recent.push_back(frame(
                tick,
                &[
                    (1, "[U:1:1]", [0.0, 0.0, 0.0], yaw, Team::Red, true, true),
                    (2, "[U:1:2]", [500.0, 0.0, 0.0], 0.0, Team::Blue, true, true),
                    (3, "[U:1:3]", [300.0, 0.0, 0.0], 0.0, Team::Red, true, true),
                ],
            ));
        }
        let shot = shot_for(&recent, 1, 2, "sniperrifle", 66, 66).expect("a kill the demo carried");
        assert_eq!(shot.shooter, "[U:1:1]");
        assert_eq!(shot.victim, "[U:1:2]");
        assert!(shot.error_deg < 0.01, "on the head: {}", shot.error_deg);
        assert!((shot.error_before_deg - 60.0).abs() < 0.01, "a second earlier: {}", shot.error_before_deg);
        assert!((shot.flick_deg - 60.0).abs() < 0.01, "the turn: {}", shot.flick_deg);
        assert!((shot.range - 500.0).abs() < 0.01);
        assert!(shot.victim_seen && shot.shooter_seen);
        // The path runs from where the view started to where it ended.
        assert_eq!(shot.path.len(), 2, "{:?}", shot.path);
        assert!((shot.path[0].0 + 60.0).abs() < 0.5, "starts 60 degrees left of the head: {:?}", shot.path[0]);
        assert!(shot.path[1].0.abs() < 0.5, "ends on the head: {:?}", shot.path[1]);
        // A suicide is not a shot, and neither is a kill by someone the demo
        // never named.
        assert!(shot_for(&recent, 1, 1, "x", 66, 66).is_none());
        assert!(shot_for(&recent, 9, 2, "x", 66, 66).is_none());
    }

    /// Every kill in the demo is now read, not only the recorder's: the same
    /// two ticks answer for the other direction too.
    #[test]
    fn a_kill_by_anyone_is_read() {
        let mut recent = VecDeque::new();
        for tick in [0u32, 66] {
            recent.push_back(frame(
                tick,
                &[
                    (1, "[U:1:1]", [0.0, 0.0, 0.0], 0.0, Team::Red, true, true),
                    // Facing back along -x, straight at player 1.
                    (2, "[U:1:2]", [500.0, 0.0, 0.0], 180.0, Team::Blue, true, true),
                ],
            ));
        }
        let theirs = shot_for(&recent, 2, 1, "scattergun", 66, 66).expect("their kill is ours to read now");
        assert_eq!(theirs.shooter, "[U:1:2]");
        assert_eq!(theirs.victim, "[U:1:1]");
        assert!(theirs.error_deg < 0.01, "they were looking right at them: {}", theirs.error_deg);
    }

    /// The kill above, read from the other end: who died, who killed them,
    /// and who was near enough to help.
    #[test]
    fn a_death_reads_who_was_nearby() {
        let mut recent = VecDeque::new();
        recent.push_back(frame(
            10,
            &[
                (1, "[U:1:1]", [0.0, 0.0, 0.0], 0.0, Team::Red, true, true),
                (2, "[U:1:2]", [500.0, 0.0, 0.0], 0.0, Team::Blue, true, true),
                // A teammate close enough to be covering them, and one far off.
                (3, "[U:1:3]", [300.0, 0.0, 0.0], 0.0, Team::Red, true, true),
                (4, "[U:1:4]", [2_000.0, 0.0, 0.0], 0.0, Team::Red, true, true),
                // A dead teammate is not help.
                (5, "[U:1:5]", [100.0, 0.0, 0.0], 0.0, Team::Red, false, true),
            ],
        ));
        let d = death_for(&recent, 2, 1).expect("a death the demo carried");
        assert_eq!(d.who, "[U:1:1]");
        assert_eq!(d.killer, "[U:1:2]");
        assert_eq!(d.killer_range, Some(500.0));
        // They were straight ahead: no angle either way. (A killer 90 degrees
        // to the player's right reads +90; see `the_miss_splits` for the sign.)
        assert!(d.killer_dx_deg.is_some_and(|x| x.abs() < 0.01), "{:?}", d.killer_dx_deg);
        assert!(d.killer_dy_deg.is_some_and(|y| y.abs() < 0.2), "{:?}", d.killer_dy_deg);
        assert_eq!(d.nearest_mate, Some(300.0));
        assert_eq!(d.mates_near, 1, "the other living teammate is 2,000 units away");
        // The enemy is not counted as a teammate nearby, and their own death
        // is read against their own side.
        let other = death_for(&recent, 1, 2).expect("their death too");
        assert_eq!(other.who, "[U:1:2]");
        assert_eq!(other.mates_near, 0, "nobody on their team is near");
    }

    /// A POV demo drops players it cannot see. Their angles then come from
    /// wherever they were last carried, so the shot says so rather than
    /// quietly reporting a stale number as a measurement.
    #[test]
    fn a_player_the_demo_lost_is_marked_unseen() {
        let mut recent = VecDeque::new();
        // A second ago the recorder could not see the shooter.
        recent.push_back(frame(
            0,
            &[
                (1, "[U:1:1]", [0.0, 0.0, 0.0], 60.0, Team::Red, true, false),
                (2, "[U:1:2]", [500.0, 0.0, 0.0], 0.0, Team::Blue, true, true),
            ],
        ));
        recent.push_back(frame(
            66,
            &[
                (1, "[U:1:1]", [0.0, 0.0, 0.0], 0.0, Team::Red, true, true),
                (2, "[U:1:2]", [500.0, 0.0, 0.0], 0.0, Team::Blue, true, true),
            ],
        ));
        let shot = shot_for(&recent, 1, 2, "scattergun", 66, 66).expect("the kill is still read");
        assert!(!shot.shooter_seen, "the demo lost the shooter a second ago");
        assert!(shot.victim_seen, "the victim was there throughout");
        // The path drops the tick the shooter was missing from.
        assert_eq!(shot.path.len(), 1, "{:?}", shot.path);
    }
}
