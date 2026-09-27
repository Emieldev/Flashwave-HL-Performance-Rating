//! Q25, reopened (§18b): what a capture does to the respawn of teammates who
//! were dead when it went in, measured from the log's own `spawned as` lines
//! rather than by counting heads.
//!
//! For every death the log shows when the player came back, so each dead
//! teammate at a cap has a real wait. Against it goes the usual wait: the
//! median of that team's waits in the same log and map with no capture in
//! between. The difference is what the cap did to them.

use crate::fights::analyse;
use crate::situation::{kill_states, CapWorth, KillState, Mode, MAX_DIFF};
use crate::state::{GameState, LifeEnd};
use hl_core::matchdata::Team;
use std::collections::HashMap;
use std::fmt;

/// A wait longer than this is somebody who went AFK or tabbed out, not a
/// respawn timer.
const MAX_WAIT_S: i64 = 40;

/// How much longer than usual a dead teammate waited, in the §18b buckets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Delay {
    /// Nobody on the capping team was dead.
    NoneDead,
    /// Dead, but back within 3 s of their usual wait (or sooner).
    Short,
    /// 3 to 8 s longer.
    Medium,
    /// 8 s or more: a wave missed because of the cap.
    Long,
}

impl Delay {
    pub const ALL: [Delay; 4] = [Delay::NoneDead, Delay::Short, Delay::Medium, Delay::Long];

    fn of(extra: f64) -> Delay {
        match extra {
            e if e < 3.0 => Delay::Short,
            e if e < LONG_DELAY_S => Delay::Medium,
            _ => Delay::Long,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Delay::NoneDead => "nobody dead",
            Delay::Short => "under 3 s",
            Delay::Medium => "3-8 s",
            Delay::Long => "8 s or more",
        }
    }
}

/// How long a dead teammate had already waited when the cap went in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Waited {
    /// Died at most 2 s before: boSe's "died off-timing".
    JustDied,
    /// 3 to 7 s.
    Waiting,
    /// 8 s or more: about to walk out.
    AboutToSpawn,
}

impl Waited {
    pub const ALL: [Waited; 3] = [Waited::JustDied, Waited::Waiting, Waited::AboutToSpawn];

    fn of(secs: i64) -> Waited {
        match secs {
            s if s <= 2 => Waited::JustDied,
            s if s < 8 => Waited::Waiting,
            _ => Waited::AboutToSpawn,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Waited::JustDied => "died 0-2 s before",
            Waited::Waiting => "dead 3-7 s",
            Waited::AboutToSpawn => "dead 8 s or more",
        }
    }
}

/// Waits of the dead, and the usual waits they are measured against.
#[derive(Debug, Clone, Copy, Default)]
pub struct Waits {
    pub n: u32,
    /// Sums, in seconds.
    pub wait: f64,
    pub usual: f64,
    /// How many waited 8 s or more longer than usual.
    pub long: u32,
}

impl Waits {
    fn add(&mut self, wait: i64, usual: f64) {
        self.n += 1;
        self.wait += wait as f64;
        self.usual += usual;
        self.long += u32::from(wait as f64 - usual >= LONG_DELAY_S);
    }
}

#[derive(Debug, Clone, Default)]
pub struct SpawnDelays {
    pub logs: usize,
    pub unmapped: u32,
    /// Deaths with no capture before the respawn: the usual waits, per mode.
    pub clean: HashMap<Mode, Waits>,
    /// The dead at a cap by their own team, by mode and how long they had
    /// already waited.
    pub at_own_cap: HashMap<(Mode, Waited), Waits>,
    /// The same for caps by the enemy: whether taking a point moves the
    /// other side's timer too.
    pub at_enemy_cap: HashMap<Mode, Waits>,
    /// Caps by the worst delay among the capping team's dead, per mode.
    pub by_delay: HashMap<(Delay, Mode), CapWorth>,
    /// Caps with an 8 s+ delay, by cappers on the point: 3+ or not.
    pub long_by_cappers: HashMap<(bool, Mode), CapWorth>,
}

/// A death and the respawn that ended it.
#[derive(Debug, Clone, Copy)]
pub struct Death {
    pub team: Team,
    pub died: i64,
    pub back: i64,
}

impl Death {
    pub fn wait(&self) -> i64 {
        self.back - self.died
    }
}

/// Every death with the respawn that ended it, inside one round. Deaths
/// never followed by a spawn (the round or the log ended) have no wait.
pub fn respawns(gs: &GameState) -> Vec<Death> {
    let mut deaths = Vec::new();
    for (i, l) in gs.lives.iter().enumerate() {
        if !matches!(l.end, LifeEnd::Killed { .. } | LifeEnd::Suicide) {
            continue;
        }
        let Some(next) = gs.lives[i + 1..].iter().filter(|n| n.account == l.account && n.from >= l.to).min_by_key(|n| n.from)
        else {
            continue;
        };
        let same_round = gs.round_at(l.to).map(|r| r.num) == gs.round_at(next.from).map(|r| r.num);
        if next.team != l.team || !same_round || next.from - l.to > MAX_WAIT_S {
            continue;
        }
        deaths.push(Death { team: l.team, died: l.to, back: next.from });
    }
    deaths
}

/// The usual wait per `key` (a map, or nothing) and colour -- a stopwatch
/// side is a colour -- as the median of the waits no capture touched.
pub fn usual_waits<K: std::hash::Hash + Eq + Copy>(
    gs: &GameState,
    deaths: &[Death],
    key: impl Fn(i64) -> Option<K>,
) -> HashMap<(K, Team), f64> {
    let capped_between = |d: &Death| gs.caps.iter().any(|c| c.at >= d.died && c.at < d.back);
    let mut clean: HashMap<(K, Team), Vec<i64>> = HashMap::new();
    for d in deaths.iter().filter(|d| !capped_between(d)) {
        let Some(k) = key(d.died) else { continue };
        clean.entry((k, d.team)).or_default().push(d.wait());
    }
    clean.into_iter().filter_map(|(k, mut v)| Some((k, median(&mut v)?))).collect()
}

/// An extra wait at or over this is a missed wave: the capper's cost (§18b).
/// Anything shorter is a mate who died off-timing, and is theirs.
pub const LONG_DELAY_S: f64 = 8.0;

fn median(v: &mut [i64]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_unstable();
    let m = v.len() / 2;
    Some(if v.len() % 2 == 1 { v[m] as f64 } else { (v[m - 1] + v[m]) as f64 / 2.0 })
}

/// One capture, reduced to what measuring it needs.
struct Seen {
    mode: Mode,
    state: KillState,
    worst: Delay,
    cappers: usize,
    won: bool,
}

pub async fn spawn_delays(db: &hl_db::Db) -> anyhow::Result<SpawnDelays> {
    let maps = db.all_round_map_names().await?;
    let mut out = SpawnDelays::default();
    let mut moments: Vec<(Mode, KillState, bool)> = Vec::new();
    let mut caps: Vec<Seen> = Vec::new();

    for log_id in db.rawlog_ids().await? {
        let Some(zip) = db.rawlog(log_id).await? else { continue };
        let raw = crate::rawlog::parse(&crate::rawlog::unzip(&zip)?);
        let gs = GameState::build(&raw);
        let tags = analyse(&raw, &gs).tags;
        let states = kill_states(&raw, &gs, &tags);
        let rounds = maps.get(&log_id);
        let map_of = |t: i64| {
            let r = gs.round_at(t)?;
            rounds.and_then(|m| m.get(&i64::from(r.num))).map(String::as_str)
        };
        out.logs += 1;

        // The same numbers baseline as Q17: how often a team in each state
        // went on to win the round.
        for (i, st) in states.iter().enumerate() {
            let Some((state, _)) = *st else { continue };
            let k = &raw.kills[i];
            let Some(kt) = k.killer.team else { continue };
            let Some(round) = gs.round_at(k.at) else { continue };
            let Some(won) = round.winner else { continue };
            let Some(map) = map_of(k.at) else { continue };
            moments.push((Mode::of(map), state, won == kt));
        }

        let deaths = respawns(&gs);
        for d in &deaths {
            if gs.caps.iter().any(|c| c.at >= d.died && c.at < d.back) {
                continue;
            }
            let Some(map) = map_of(d.died) else { continue };
            let w = out.clean.entry(Mode::of(map)).or_default();
            w.n += 1;
            w.wait += d.wait() as f64;
        }
        let usual = usual_waits(&gs, &deaths, map_of);

        for c in &gs.caps {
            let Some(team) = c.team else { continue };
            let Some(round) = gs.round_at(c.at) else { continue };
            let Some(won) = round.winner else { continue };
            let Some(map) = map_of(c.at) else {
                out.unmapped += 1;
                continue;
            };
            let mode = Mode::of(map);
            let mut worst = Delay::NoneDead;
            for d in deaths.iter().filter(|d| d.died <= c.at && d.back > c.at) {
                let Some(&u) = usual.get(&(map, d.team)) else { continue };
                let wait = d.back - d.died;
                if d.team == team {
                    out.at_own_cap.entry((mode, Waited::of(c.at - d.died))).or_default().add(wait, u);
                    worst = worst.max(Delay::of(wait as f64 - u));
                } else {
                    out.at_enemy_cap.entry(mode).or_default().add(wait, u);
                }
            }
            let at = c.at - 1;
            let n = gs.numbers_at(at);
            let alive = |t: Team| if t == Team::Red { n[0] } else { n[1] };
            let state = KillState {
                diff: (i16::from(alive(team)) - i16::from(alive(team.other())))
                    .clamp(-i16::from(MAX_DIFF), i16::from(MAX_DIFF)) as i8,
                adv: match gs.advantage_at(at) {
                    Some(t) if t == team => 1,
                    Some(_) => -1,
                    None => 0,
                },
            };
            caps.push(Seen { mode, state, worst, cappers: c.cappers.len(), won: won == team });
        }
    }

    let mut base: HashMap<(Mode, KillState), (u32, u32)> = HashMap::new();
    for &(mode, state, won) in &moments {
        let a = base.entry((mode, state)).or_default();
        a.0 += 1;
        a.1 += u32::from(won);
        let b = base.entry((mode, state.flip())).or_default();
        b.0 += 1;
        b.1 += u32::from(!won);
    }
    for c in caps {
        let Some(&(n, w)) = base.get(&(c.mode, c.state)) else { continue };
        let expected = f64::from(w) / f64::from(n);
        let add = |e: &mut CapWorth| {
            e.caps += 1;
            e.won += u32::from(c.won);
            e.expected += expected;
        };
        add(out.by_delay.entry((c.worst, c.mode)).or_default());
        if c.worst == Delay::Long {
            add(out.long_by_cappers.entry((c.cappers >= 3, c.mode)).or_default());
        }
    }
    Ok(out)
}

impl fmt::Display for SpawnDelays {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let modes = [Mode::Koth, Mode::Stopwatch];
        writeln!(f, "{} logs ({} caps skipped: the round's map is unknown)\n", self.logs, self.unmapped)?;

        writeln!(f, "1. Does a capture lengthen the wait of the capping team's dead?")?;
        writeln!(f, "   (usual = that team's median wait on that map with no capture in between)\n")?;
        for m in modes {
            let c = self.clean.get(&m).copied().unwrap_or_default();
            if c.n > 0 {
                writeln!(f, "   {:<10} no capture: {:>6} deaths, mean wait {:>5.1} s", mode_name(m), c.n, c.wait / f64::from(c.n))?;
            }
        }
        writeln!(
            f,
            "\n   {:<10} {:<20} {:>6} {:>7} {:>7} {:>7} {:>8}",
            "mode", "when the cap went in", "dead", "waited", "usual", "extra", "8 s+"
        )?;
        let row = |f: &mut fmt::Formatter<'_>, m: Mode, label: &str, w: Waits| -> fmt::Result {
            if w.n == 0 {
                return Ok(());
            }
            let n = f64::from(w.n);
            writeln!(
                f,
                "   {:<10} {:<20} {:>6} {:>6.1}s {:>6.1}s {:>+6.1}s {:>7.0}%",
                mode_name(m),
                label,
                w.n,
                w.wait / n,
                w.usual / n,
                (w.wait - w.usual) / n,
                100.0 * f64::from(w.long) / n
            )
        };
        for m in modes {
            for wd in Waited::ALL {
                row(f, m, wd.as_str(), self.at_own_cap.get(&(m, wd)).copied().unwrap_or_default())?;
            }
            row(f, m, "(the enemy's cap)", self.at_enemy_cap.get(&m).copied().unwrap_or_default())?;
        }

        writeln!(f, "\n2. Caps by the longest delay among the capping team's dead,")?;
        writeln!(f, "   won over what the numbers before the cap predicted\n")?;
        writeln!(f, "   {:<14} {:>18} {:>18}", "delay", "KOTH", "stopwatch")?;
        let cell = |w: CapWorth| match w.excess() {
            Some(e) => format!("{:>6}  {:>+7.2}%", w.caps, e * 100.0),
            None => format!("{:>6}  {:>8}", w.caps, "-"),
        };
        for d in Delay::ALL {
            let g = |m: Mode| self.by_delay.get(&(d, m)).copied().unwrap_or_default();
            writeln!(f, "   {:<14} {} {}", d.as_str(), cell(g(Mode::Koth)), cell(g(Mode::Stopwatch)))?;
        }
        writeln!(f, "\n   8 s+ delays, by cappers on the point (ivg)")?;
        for (many, label) in [(false, "1-2 cappers"), (true, "3+ cappers")] {
            let g = |m: Mode| self.long_by_cappers.get(&(many, m)).copied().unwrap_or_default();
            writeln!(f, "   {:<14} {} {}", label, cell(g(Mode::Koth)), cell(g(Mode::Stopwatch)))?;
        }
        Ok(())
    }
}

fn mode_name(m: Mode) -> &'static str {
    match m {
        Mode::Koth => "KOTH",
        Mode::Stopwatch => "stopwatch",
        Mode::Other => "other",
    }
}
