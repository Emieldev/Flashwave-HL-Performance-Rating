//! more.tf as a stand-in for logs.tf (Flashy, September 2026).
//!
//! logs.tf turns an address away for a while after too many requests, and a
//! game just played then does not show up. more.tf keeps its own parse of
//! every logs.tf log, served at `more.tf/api/log/<id>`: every player's totals
//! and class lines, heal spread, ubers and drops with their times, rounds,
//! chat, and every kill with its time and both players' positions. That is
//! enough to build what logs.tf would have given:
//!
//! - a logs.tf-shaped JSON, which the normalizer reads like any other; and
//! - a server log written from the kills, chat and ubers, which the raw-log
//!   parser reads like any other.
//!
//! Checked on a KOTH and a stopwatch payload log (September 2026): every
//! player's kills, deaths, assists, damage, damage taken, heals, heals
//! received, ubers and drops the same as logs.tf's, and each round's team
//! numbers too. more.tf's times are the server's clock read as UTC, the frame
//! the raw log is written in.
//!
//! What more.tf does not have: the time of each capture (only the first),
//! who assisted each kill, headshots and backstabs per kill (only totals),
//! spawns, and which colour each team wore in a stopwatch round -- that is
//! worked out from the round's player numbers against each colour's. So a
//! stand-in is stored as one (`log_stand_in`) and the sync asks logs.tf for
//! the real log again until it gets it.

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::fmt::Write as _;

pub const SOURCE: &str = "more.tf";

/// A log rebuilt from more.tf.
pub struct StandIn {
    /// logs.tf's shape, for `log_raw`.
    pub json: Value,
    /// A server log, for `rawlog`.
    pub text: String,
    pub kills: usize,
}

/// One round as more.tf has it, with what the rest needs worked out.
struct Round<'a> {
    raw: &'a Value,
    start: i64,
    end: i64,
    /// The teams wore each other's colours (stopwatch's second half).
    swapped: bool,
}

pub fn stand_in(body: &Value) -> Result<StandIn> {
    let info = body.get("info").context("more.tf log has no info")?;
    let players = body.get("players").and_then(Value::as_object).context("more.tf log has no players")?;
    let m3 = body.get("m3").unwrap_or(&Value::Null);
    let map = str_of(info, "map");
    let title = str_of(info, "title");

    // Each player's team for the log as a whole, as logs.tf keys it.
    let overall: HashMap<&str, &str> = players
        .iter()
        .filter_map(|(id, p)| Some((id.as_str(), p.get("team")?.as_str().filter(|t| *t == "Red" || *t == "Blue")?)))
        .collect();
    if overall.len() < 4 {
        bail!("more.tf log has {} players on a team: not a match", overall.len());
    }

    let round_stats = arr(m3, "playerRoundStats");
    let rounds: Vec<Round> = arr(m3, "rounds")
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            let start = int(r, "startTimestamp")?;
            let end = int(r, "endTimestamp").unwrap_or(start + int(r, "durationSeconds").unwrap_or(0));
            let number = int(r, "roundNumber").unwrap_or(i as i64 + 1);
            Some(Round { raw: r, start, end, swapped: swapped(r, number, round_stats, &overall) })
        })
        .collect();
    let first_start = rounds.iter().map(|r| r.start).min().or_else(|| int(info, "startTime")).context("more.tf log has no times")?;
    let round_at = |t: i64| rounds.iter().position(|r| t >= r.start && t <= r.end);
    // The colour a player wore at `t`: their team, flipped in a swapped round.
    let colour = |id: &str, t: i64| -> &str {
        let team = overall.get(id).copied().unwrap_or("");
        match round_at(t) {
            Some(i) if rounds[i].swapped => other(team),
            _ => team,
        }
    };

    // ---- the logs.tf JSON ----------------------------------------------------

    let mut players_json = Map::new();
    let mut names = Map::new();
    for (id, p) in players {
        let Some(team) = overall.get(id.as_str()) else { continue };
        let class_stats: Vec<Value> = arr(p, "classStats")
            .iter()
            .map(|c| {
                json!({
                    "type": c.get("type"),
                    "kills": c.get("kills"),
                    "assists": c.get("assists"),
                    "deaths": c.get("deaths"),
                    "dmg": c.get("damage"),
                    "total_time": c.get("totalTimeSeconds"),
                })
            })
            .collect();
        players_json.insert(
            id.clone(),
            json!({
                "team": team,
                "class_stats": class_stats,
                "kills": p.get("kills"),
                "deaths": p.get("deaths"),
                "assists": p.get("assists"),
                "suicides": p.get("suicides"),
                "dmg": p.get("damage"),
                "dt": p.get("damageTaken"),
                "hr": p.get("healingReceived"),
                "heal": p.get("healing"),
                "ubers": p.get("charges"),
                "ubertypes": p.get("chargesByType"),
                "drops": p.get("drops"),
                // logs.tf's `headshots` are headshot kills; more.tf's are hits.
                "headshots": p.get("headshotKills"),
                "headshots_hit": p.get("headshots"),
                "backstabs": p.get("backstabs"),
                "medkits": p.get("medkits"),
                "medkits_hp": p.get("medkitsHp"),
                "cpc": p.get("pointCaptures"),
                "ic": p.get("intelCaptures"),
                "lks": p.get("longestKillStreak"),
                "as": p.get("airshots"),
            }),
        );
        names.insert(id.clone(), p.get("name").cloned().unwrap_or(Value::Null));
    }

    // Round events, in logs.tf's clock: seconds since the log began.
    let mut events: Vec<Vec<Value>> = vec![Vec::new(); rounds.len()];
    let at = |t: i64| t - first_start;
    for u in arr(m3, "ubers") {
        let (Some(t), Some(medic)) = (int(u, "timestamp"), u.get("medicSteamId").and_then(Value::as_str)) else { continue };
        let Some(i) = round_at(t) else { continue };
        match u.get("type").and_then(Value::as_str) {
            Some("deployed") => events[i].push(json!({
                "type": "charge", "time": at(t), "team": colour(medic, t), "steamid": medic,
                "medigun": u.get("medigunType").and_then(Value::as_str).unwrap_or("medigun"),
            })),
            Some("dropped") => events[i].push(json!({ "type": "drop", "time": at(t), "team": colour(medic, t), "steamid": medic })),
            _ => {}
        }
    }
    let kills = arr(m3, "kills");
    for k in kills {
        if k.get("victimClass").and_then(Value::as_str) != Some("medic") {
            continue;
        }
        let (Some(t), Some(victim)) = (int(k, "timestamp"), k.get("victimSteamId").and_then(Value::as_str)) else { continue };
        let Some(i) = round_at(t) else { continue };
        events[i].push(json!({ "type": "medic_death", "time": at(t), "team": colour(victim, t), "steamid": victim, "killer": k.get("killerSteamId") }));
    }

    let mut wins: HashMap<&str, i64> = HashMap::new();
    let rounds_json: Vec<Value> = rounds
        .iter()
        .zip(events)
        .enumerate()
        .map(|(i, (r, mut events))| {
            let winner = r.raw.get("winner").and_then(Value::as_str).filter(|w| *w == "Red" || *w == "Blue");
            if let Some(w) = winner {
                *wins.entry(w).or_default() += 1;
                // Where the normalizer pins the round's start: see `round_base`.
                events.push(json!({ "type": "round_win", "time": at(r.end), "team": w }));
            }
            events.sort_by_key(|e| e.get("time").and_then(Value::as_i64).unwrap_or(0));
            let team = |c: &str| {
                let s = r.raw.get("teamStats").and_then(|t| t.get(c));
                json!({
                    "kills": s.and_then(|s| s.get("kills")),
                    "dmg": s.and_then(|s| s.get("damage")),
                    "ubers": s.and_then(|s| s.get("ubers")),
                })
            };
            let number = int(r.raw, "roundNumber").unwrap_or(i as i64 + 1);
            let round_players: Map<String, Value> = round_stats
                .iter()
                .filter(|s| int(s, "roundNumber") == Some(number))
                .filter_map(|s| {
                    let id = s.get("steamId")?.as_str()?;
                    let team = overall.get(id)?;
                    let team = if r.swapped { other(team) } else { team };
                    Some((id.to_string(), json!({ "team": team, "kills": s.get("kills"), "dmg": s.get("damage") })))
                })
                .collect();
            json!({
                "start_time": r.start,
                "winner": winner,
                "team": { "Red": team("Red"), "Blue": team("Blue") },
                "events": events,
                "players": round_players,
                "firstcap": r.raw.get("firstCap").and_then(|f| f.get("team")),
                "length": r.end - r.start,
            })
        })
        .collect();

    let last_end = rounds.iter().map(|r| r.end).max().unwrap_or(first_start);
    let json = json!({
        "version": 3,
        // logs.tf's score is rounds won by colour; more.tf gives 0-0 on stopwatch.
        "teams": {
            "Red": { "score": wins.get("Red").copied().unwrap_or(0) },
            "Blue": { "score": wins.get("Blue").copied().unwrap_or(0) },
        },
        "length": int(info, "durationSeconds").unwrap_or(last_end - first_start),
        "players": players_json,
        "names": names,
        "rounds": rounds_json,
        "classkills": m3.get("classKills").cloned().unwrap_or(json!({})),
        "classdeaths": m3.get("classDeaths").cloned().unwrap_or(json!({})),
        "info": {
            "map": map,
            "title": title,
            // A real log's date is its upload, moments after the last round.
            "date": int(info, "endTime").unwrap_or(last_end).max(last_end) + 20,
            "supplemental": true,
            "hasRealDamage": false,
            "hasWeaponDamage": false,
            "hasAccuracy": false,
            "hasHP": true,
            "hasHP_real": false,
            "hasHS": true,
            "hasHS_hit": true,
            "hasBS": true,
            "hasCP": true,
            "hasSB": false,
            "hasDT": true,
            "hasAS": true,
            "hasHR": true,
            "hasIntel": false,
            "AD_scoring": false,
            "stand_in": SOURCE,
        },
    });

    // ---- the server log ------------------------------------------------------

    // (time, order within the second, line): a round starts before its first
    // kill and ends after its last, as the server writes them.
    let mut lines: Vec<(i64, u8, String)> = Vec::new();
    let uid: HashMap<&str, usize> = overall.keys().enumerate().map(|(i, id)| (*id, i + 2)).collect();
    let actor = |id: &str, t: i64| -> String {
        let name = players.get(id).and_then(|p| p.get("name")).and_then(Value::as_str).unwrap_or("?");
        // Quotes in a name would end the actor early for the parser.
        format!("\"{}<{}><{id}><{}>\"", name.replace('"', "'"), uid.get(id).copied().unwrap_or(0), colour(id, t))
    };
    for r in &rounds {
        lines.push((r.start, 0, "World triggered \"Round_Start\"".into()));
        match r.raw.get("winner").and_then(Value::as_str) {
            Some(w @ ("Red" | "Blue")) => lines.push((r.end, 3, format!("World triggered \"Round_Win\" (winner \"{w}\")"))),
            _ => lines.push((r.end, 3, "World triggered \"Round_Stalemate\"".into())),
        }
    }
    // The parser takes a victim's class from their latest `changed role to`.
    let mut class_of: HashMap<&str, &str> = HashMap::new();
    let mut counted = 0;
    let pos = |p: Option<&Value>| -> String {
        let c = |k: &str| p.and_then(|p| p.get(k)).and_then(Value::as_i64).unwrap_or(0);
        format!("{} {} {}", c("x"), c("y"), c("z"))
    };
    let mut kills_sorted: Vec<&Value> = kills.iter().collect();
    kills_sorted.sort_by_key(|k| int(k, "timestamp").unwrap_or(0));
    for k in kills_sorted {
        let (Some(t), Some(killer), Some(victim)) = (
            int(k, "timestamp"),
            k.get("killerSteamId").and_then(Value::as_str),
            k.get("victimSteamId").and_then(Value::as_str),
        ) else {
            continue;
        };
        for (id, key) in [(killer, "killerClass"), (victim, "victimClass")] {
            if let Some(c) = k.get(key).and_then(Value::as_str) {
                if class_of.insert(id, c) != Some(c) {
                    lines.push((t, 1, format!("{} changed role to \"{c}\"", actor(id, t))));
                }
            }
        }
        let weapon = k.get("weapon").and_then(Value::as_str).unwrap_or("world");
        let line = if killer == victim {
            format!("{} committed suicide with \"{weapon}\" (attacker_position \"{}\")", actor(victim, t), pos(k.get("victimPosition")))
        } else {
            counted += 1;
            format!(
                "{} killed {} with \"{weapon}\" (attacker_position \"{}\") (victim_position \"{}\")",
                actor(killer, t),
                actor(victim, t),
                pos(k.get("killerPosition")),
                pos(k.get("victimPosition"))
            )
        };
        lines.push((t, 2, line));
    }
    for u in arr(m3, "ubers") {
        let (Some(t), Some(medic)) = (int(u, "timestamp"), u.get("medicSteamId").and_then(Value::as_str)) else { continue };
        if u.get("type").and_then(Value::as_str) != Some("deployed") {
            continue;
        }
        let gun = u.get("medigunType").and_then(Value::as_str).unwrap_or("medigun");
        lines.push((t, 2, format!("{} triggered \"chargedeployed\" (medigun \"{gun}\")", actor(medic, t))));
        if let Some(end) = int(u, "endTimestamp") {
            lines.push((end, 2, format!("{} triggered \"chargeended\"", actor(medic, end))));
        }
    }
    for c in arr(m3, "chat") {
        let (Some(t), Some(id)) = (int(c, "timestamp"), c.get("steamId").and_then(Value::as_str)) else { continue };
        let say = if c.get("isTeamChat").and_then(Value::as_bool) == Some(true) { "say_team" } else { "say" };
        let message = c.get("message").and_then(Value::as_str).unwrap_or("").replace('"', "'");
        lines.push((t, 2, format!("{} {say} \"{message}\"", actor(id, t))));
    }
    lines.sort_by_key(|(t, order, _)| (*t, *order));

    let stamp = |t: i64| chrono::DateTime::from_timestamp(t, 0).map_or_else(String::new, |d| d.format("%m/%d/%Y - %H:%M:%S").to_string());
    let mut text = String::new();
    let _ = writeln!(text, "L {}: Log file started (file \"more.tf\") (game \"tf\") (version \"stand-in\")", stamp(first_start - 1));
    let _ = writeln!(text, "L {}: World triggered \"meta_data\" (map \"{map}\")", stamp(first_start - 1));
    for (t, _, line) in &lines {
        let _ = writeln!(text, "L {}: {line}", stamp(*t));
    }
    let _ = writeln!(text, "L {}: Log file closed.", stamp(last_end));

    Ok(StandIn { json, text, kills: counted })
}

/// Whether a round was played in swapped colours: the numbers of the players
/// whose team is Red add up to the Blue colour's, not the Red's. more.tf
/// writes each round's team numbers by colour but gives no player a colour.
fn swapped(r: &Value, number: i64, round_stats: &[Value], overall: &HashMap<&str, &str>) -> bool {
    let (mut kills, mut dmg) = (0, 0);
    for s in round_stats.iter().filter(|s| int(s, "roundNumber") == Some(number)) {
        if s.get("steamId").and_then(Value::as_str).and_then(|id| overall.get(id)) == Some(&"Red") {
            kills += int(s, "kills").unwrap_or(0);
            dmg += int(s, "damage").unwrap_or(0);
        }
    }
    let off = |colour: &str| {
        let t = r.get("teamStats").and_then(|t| t.get(colour));
        let k = t.and_then(|t| int(t, "kills")).unwrap_or(0);
        let d = t.and_then(|t| int(t, "damage")).unwrap_or(0);
        (kills - k).abs() * 100 + (dmg - d).abs()
    };
    r.get("teamStats").is_some() && off("Blue") < off("Red")
}

fn other(team: &str) -> &str {
    match team {
        "Red" => "Blue",
        "Blue" => "Red",
        t => t,
    }
}

fn arr<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

fn int(v: &Value, key: &str) -> Option<i64> {
    let x = v.get(key)?;
    x.as_i64().or_else(|| x.as_f64().map(|f| f.round() as i64))
}

fn str_of(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A two-round stopwatch match in more.tf's shape: the teams swap
    /// colours for round 2, which only the round numbers show.
    fn sample() -> Value {
        let p = |id: &str, name: &str, team: &str, kills: i64| {
            json!({ "steamId": id, "name": name, "team": team, "kills": kills, "deaths": 1, "assists": 0, "damage": kills * 300,
                    "damageTaken": 200, "healing": 0, "healingReceived": 50, "charges": 0, "drops": 0, "headshots": 0,
                    "headshotKills": 0, "backstabs": 0, "airshots": 0, "medkits": 0, "medkitsHp": 0, "pointCaptures": 0,
                    "intelCaptures": 0, "longestKillStreak": 1, "suicides": 0,
                    "classStats": [{ "type": "soldier", "kills": kills, "assists": 0, "deaths": 1, "damage": kills * 300, "totalTimeSeconds": 600 }] })
        };
        let ids = ["[U:1:1]", "[U:1:2]", "[U:1:3]", "[U:1:4]"];
        let kill = |t: i64, k: &str, v: &str| json!({ "timestamp": t, "killerSteamId": k, "victimSteamId": v, "weapon": "tf_projectile_rocket",
            "killerClass": "soldier", "victimClass": "soldier", "killerPosition": { "x": 1, "y": 2, "z": 3 }, "victimPosition": { "x": 4, "y": 5, "z": 6 } });
        let prs = |round: i64, id: &str, kills: i64| json!({ "steamId": id, "roundNumber": round, "kills": kills, "damage": kills * 300 });
        json!({
            "success": true,
            "info": { "map": "pl_upward_f12", "title": "test", "durationSeconds": 600, "startTime": 1_000_000, "endTime": 1_000_600 },
            "players": {
                ids[0]: p(ids[0], "a", "Red", 2), ids[1]: p(ids[1], "b", "Red", 0),
                ids[2]: p(ids[2], "c", "Blue", 1), ids[3]: p(ids[3], "d", "Blue", 0),
            },
            "m3": {
                "rounds": [
                    { "roundNumber": 1, "startTimestamp": 1_000_000, "endTimestamp": 1_000_300, "durationSeconds": 300, "winner": "Blue",
                      "teamStats": { "Red": { "kills": 1, "damage": 300 }, "Blue": { "kills": 1, "damage": 300 } } },
                    // Round 2: the Red team wears Blue, and scores 1 kill as "Blue".
                    { "roundNumber": 2, "startTimestamp": 1_000_310, "endTimestamp": 1_000_600, "durationSeconds": 290, "winner": "Blue",
                      "teamStats": { "Red": { "kills": 0, "damage": 0 }, "Blue": { "kills": 1, "damage": 300 } } },
                ],
                "playerRoundStats": [prs(1, ids[0], 1), prs(1, ids[2], 1), prs(2, ids[0], 1)],
                "kills": [kill(1_000_010, ids[0], ids[2]), kill(1_000_020, ids[2], ids[0]), kill(1_000_400, ids[0], ids[3])],
                "ubers": [], "chat": [{ "timestamp": 1_000_050, "steamId": ids[1], "message": "say \"hi\"", "isTeamChat": false }],
                "classKills": {}, "classDeaths": {},
            },
        })
    }

    #[test]
    fn a_stopwatch_swap_is_worked_out_from_the_round_numbers() {
        let s = stand_in(&sample()).unwrap();
        let rounds = s.json["rounds"].as_array().unwrap();
        assert_eq!(rounds[0]["players"]["[U:1:1]"]["team"], "Red");
        assert_eq!(rounds[1]["players"]["[U:1:1]"]["team"], "Blue", "round 2 is swapped");
        // Scores are rounds won by colour, as logs.tf writes them.
        assert_eq!(s.json["teams"]["Blue"]["score"], 2);
        // The normalizer reads it, and sees the swap.
        let log = crate::normalize::normalize(1, &s.json).unwrap();
        assert!(!log.rounds[0].colours_swapped && log.rounds[1].colours_swapped);
        assert_eq!(log.players.len(), 4);
    }

    #[test]
    fn the_server_log_parses_back_to_the_same_kills() {
        let s = stand_in(&sample()).unwrap();
        assert_eq!(s.kills, 3);
        let raw = crate::rawlog::parse(&s.text);
        assert_eq!(raw.kills.len(), 3);
        assert!(raw.kills.iter().all(|k| k.live), "every kill is inside a round");
        assert_eq!(raw.kills[0].killer_pos, Some([1, 2, 3]));
        assert_eq!(raw.kills[0].victim.class, Some(hl_core::TfClass::Soldier));
        // In round 2 the Red team's player writes as Blue.
        assert_eq!(raw.kills[2].killer.team, Some(hl_core::matchdata::Team::Blue));
        assert_eq!(raw.round_starts.len(), 2);
        assert_eq!(raw.chat[0].message, "say 'hi'");
    }

    /// The real thing, when fixtures are at hand: a folder of
    /// `<id>.logstf.json` and `<id>.moretf.json` pairs in `HL_MORETF_FIXTURES`.
    #[test]
    fn matches_logs_tf_on_real_logs() {
        let Ok(dir) = std::env::var("HL_MORETF_FIXTURES") else { return };
        let mut checked = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let Some(id) = name.strip_suffix(".moretf.json") else { continue };
            let more: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            let real: Value = serde_json::from_str(&std::fs::read_to_string(path.with_file_name(format!("{id}.logstf.json"))).unwrap()).unwrap();
            let s = stand_in(&more).unwrap();
            let (a, b) = (crate::normalize::normalize(1, &s.json).unwrap(), crate::normalize::normalize(1, &real).unwrap());
            assert_eq!((a.red_score, a.blue_score), (b.red_score, b.blue_score), "{id} score");
            assert_eq!(a.rounds.len(), b.rounds.len(), "{id} rounds");
            for (x, y) in a.rounds.iter().zip(&b.rounds) {
                assert_eq!((x.winner, x.colours_swapped, x.length_s), (y.winner, y.colours_swapped, y.length_s), "{id} round {}", x.round_num);
            }
            for (x, y) in a.players.iter().zip(&b.players) {
                assert_eq!(x.id, y.id);
                let (p, q) = (&x.stats, &y.stats);
                assert_eq!((p.kills, p.deaths, p.assists, p.dmg, p.heal, p.ubers, p.drops), (q.kills, q.deaths, q.assists, q.dmg, q.heal, q.ubers, q.drops), "{id} {}", x.id);
            }
            let raw = crate::rawlog::parse(&s.text);
            let counted = raw.kills.iter().filter(|k| k.counts()).count() as i64;
            assert_eq!(counted, b.players.iter().map(|p| p.stats.kills).sum::<i64>(), "{id} kills in the server log");
            checked += 1;
        }
        assert!(checked > 0, "no fixtures in {dir}");
    }
}
