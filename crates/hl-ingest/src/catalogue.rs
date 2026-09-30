//! Player profiles (Q35, Flashy; PLAN §26): who everyone in the league is,
//! from the ETF2L officials the league job has downloaded and who played
//! them.
//!
//! **Seasons.** Each official says who played it and for which team; a
//! player's season is the teams they played for, the division each was in,
//! and the record. Playoff matches carry no division of their own, so theirs
//! comes from the competition's name ("Low Playoffs") and its tier from that
//! season's regular matches in the same division.
//!
//! **Medals.** Worked out, not fetched -- ETF2L's API has no awards. For
//! each season and division: a playoff Grand Final gives gold to its winner
//! and silver to its loser, and a 3rd Place match gives bronze. A division
//! with no playoffs (Premiership, High and Mid in several seasons) is settled
//! by its final table -- won, then map score -- and only once the season is
//! over. A player earns their team's medal by playing at least one official
//! for it that season.
//!
//! **Highest division.** The top tier they played at least
//! [`MIN_FOR_DIVISION`] officials in, so one merc game in Premiership does
//! not make someone a Premiership player.

use crate::sources::Sources;
use anyhow::{Context, Result};
use hl_core::SteamId;
use hl_db::{CatMatch, Db, Etf2lPlayer};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

/// Officials in a tier before it counts as a player's division.
pub const MIN_FOR_DIVISION: usize = 3;
/// ETF2L's page for a player is fetched again after this long.
const ETF2L_PLAYER_KEEP_S: i64 = 7 * 24 * 3600;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Team {
    pub id: i64,
    pub name: String,
    pub avatar: Option<String>,
}

/// One team a player played for in one season.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSeason {
    pub season: i64,
    pub season_name: String,
    pub division: String,
    pub tier: Option<i64>,
    pub team: Team,
    pub played: u32,
    pub won: u32,
    pub lost: u32,
    /// 1, 2 or 3 when the team took a medal that season.
    pub place: Option<u8>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Medal {
    pub season: i64,
    pub season_name: String,
    pub division: String,
    pub tier: Option<i64>,
    /// 1 gold, 2 silver, 3 bronze.
    pub place: u8,
    pub team: Team,
    /// "Grand Final 5-4", "3rd place match 6-0", "League table".
    pub how: String,
}

/// One official a player played.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Official {
    pub match_id: i64,
    pub time: Option<i64>,
    pub season: i64,
    pub division: String,
    pub stage: String,
    pub round: Option<String>,
    pub team: Team,
    pub opponent: Team,
    pub score_for: Option<i64>,
    pub score_against: Option<i64>,
    pub won: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Division {
    pub name: String,
    pub tier: i64,
}

/// One row of a search.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub account_id: u32,
    pub name: String,
    pub highest: Option<Division>,
    pub main_class: Option<String>,
    /// Gold, silver, bronze.
    pub medals: [u32; 3],
    pub officials: u32,
    pub last_seen: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub account_id: u32,
    pub steamid64: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub country: Option<String>,
    pub avatar: Option<String>,
    /// The classes they signed up as on ETF2L.
    pub declared_classes: Vec<String>,
    /// Games per main class in the logs held, most first.
    pub played_classes: Vec<(String, i64)>,
    pub main_class: Option<String>,
    pub current: Option<PlayerSeason>,
    pub highest: Option<Division>,
    pub medals: Vec<Medal>,
    /// Newest season first.
    pub seasons: Vec<PlayerSeason>,
    /// Newest first.
    pub officials: Vec<Official>,
    pub etf2l_id: Option<i64>,
}

/// Everything the profiles are worked out from, loaded once per request.
pub struct Catalogue {
    matches: HashMap<i64, CatMatch>,
    /// match -> (account, team, name)
    rosters: HashMap<i64, Vec<(u32, Option<i64>, String)>>,
    teams: HashMap<i64, (String, Option<String>)>,
    /// (season, division name) -> tier, from regular matches; and division
    /// name -> tier over every season, for what a season does not settle.
    tiers: HashMap<(i64, String), i64>,
    tier_by_name: HashMap<String, i64>,
    newest_season: i64,
}

impl Catalogue {
    pub async fn load(db: &Db) -> Result<Catalogue> {
        let matches: HashMap<i64, CatMatch> = db.catalogue_matches().await?.into_iter().map(|m| (m.match_id, m)).collect();
        let mut rosters: HashMap<i64, Vec<(u32, Option<i64>, String)>> = HashMap::new();
        for (m, a, t, n) in db.catalogue_rosters(None).await? {
            rosters.entry(m).or_default().push((a, t, n));
        }
        let mut tiers: HashMap<(i64, String), i64> = HashMap::new();
        let mut votes: HashMap<String, HashMap<i64, u32>> = HashMap::new();
        for m in matches.values() {
            if let (Some(d), Some(t)) = (&m.division, m.tier) {
                tiers.entry((m.season, d.clone())).or_insert(t);
                *votes.entry(d.clone()).or_default().entry(t).or_default() += 1;
            }
        }
        let tier_by_name = votes.into_iter().filter_map(|(d, v)| Some((d, v.into_iter().max_by_key(|(_, n)| *n)?.0))).collect();
        let newest_season = matches.values().map(|m| m.season).max().unwrap_or(0);
        Ok(Catalogue { matches, rosters, teams: db.etf2l_teams().await?, tiers, tier_by_name, newest_season })
    }

    fn team(&self, id: i64) -> Team {
        let (name, avatar) = self.teams.get(&id).cloned().unwrap_or_else(|| (format!("Team {id}"), None));
        Team { id, name, avatar }
    }

    /// A match's division, and its tier where one can be found.
    fn division_of(&self, m: &CatMatch) -> (String, Option<i64>) {
        let name = m.division.clone().unwrap_or_else(|| m.comp_division.clone());
        let tier = m
            .tier
            .or_else(|| self.tiers.get(&(m.season, name.clone())).copied())
            .or_else(|| self.tier_by_name.get(&name).copied());
        (name, tier)
    }

    /// The team's result in a played match: `(for, against, won)`.
    fn result_for(m: &CatMatch, team: i64) -> (Option<i64>, Option<i64>, Option<bool>) {
        let (f, a) = if m.clan1 == team { (m.r1, m.r2) } else { (m.r2, m.r1) };
        let won = match (f, a) {
            (Some(f), Some(a)) if f != a => Some(f > a),
            _ => None,
        };
        (f, a, won)
    }

    fn played(m: &CatMatch) -> bool {
        !m.default_win && m.r1.unwrap_or(0) + m.r2.unwrap_or(0) > 0
    }

    /// Every medal of every season and division: `(season, division) ->
    /// [(place, team, how)]`.
    pub fn medals(&self) -> BTreeMap<(i64, String), Vec<(u8, i64, String)>> {
        let mut out: BTreeMap<(i64, String), Vec<(u8, i64, String)>> = BTreeMap::new();
        let score = |m: &CatMatch, winner: i64| {
            let (f, a, _) = Self::result_for(m, winner);
            format!("{}-{}", f.unwrap_or(0), a.unwrap_or(0))
        };
        let winner_loser = |m: &CatMatch| -> Option<(i64, i64)> {
            match (m.r1?, m.r2?) {
                (a, b) if a > b => Some((m.clan1, m.clan2)),
                (a, b) if b > a => Some((m.clan2, m.clan1)),
                _ => None,
            }
        };
        for m in self.matches.values().filter(|m| Self::played(m)) {
            let key = (m.season, m.comp_division.clone());
            let final_round = m.round.as_deref().is_some_and(|r| r.eq_ignore_ascii_case("Grand Final") || r.eq_ignore_ascii_case("Final"));
            if m.stage == "Playoffs" && final_round || m.stage == "Grand Final" {
                if let Some((w, l)) = winner_loser(m) {
                    let e = out.entry(key).or_default();
                    e.push((1, w, format!("Grand Final {}", score(m, w))));
                    e.push((2, l, format!("Grand Final {}", score(m, l))));
                }
            } else if m.stage == "3rd Place" {
                if let Some((w, _)) = winner_loser(m) {
                    out.entry(key).or_default().push((3, w, format!("3rd place match {}", score(m, w))));
                }
            }
        }
        // Divisions without a final: the table, once the season is over.
        let mut tables: HashMap<(i64, String), HashMap<i64, (u32, i64)>> = HashMap::new();
        for m in self.matches.values().filter(|m| Self::played(m) && m.stage == "regular" && m.season < self.newest_season) {
            let (div, _) = self.division_of(m);
            let t = tables.entry((m.season, div)).or_default();
            for team in [m.clan1, m.clan2] {
                let (f, a, won) = Self::result_for(m, team);
                let e = t.entry(team).or_default();
                e.0 += u32::from(won == Some(true));
                e.1 += f.unwrap_or(0) - a.unwrap_or(0);
            }
        }
        for (key, table) in tables {
            if out.contains_key(&key) || table.len() < 4 {
                continue;
            }
            let mut rows: Vec<(i64, (u32, i64))> = table.into_iter().collect();
            rows.sort_by(|a, b| (b.1).cmp(&a.1).then(a.0.cmp(&b.0)));
            let e = out.entry(key).or_default();
            for (i, (team, _)) in rows.into_iter().take(3).enumerate() {
                e.push((i as u8 + 1, team, "League table".to_string()));
            }
        }
        out
    }

    /// One player's seasons, medals and officials.
    fn player(&self, account: u32, medals: &BTreeMap<(i64, String), Vec<(u8, i64, String)>>) -> (Vec<PlayerSeason>, Vec<Medal>, Vec<Official>) {
        let mut officials: Vec<Official> = Vec::new();
        // (season, team) -> (division counts, played, won, lost)
        let mut seasons: HashMap<(i64, i64), (HashMap<(String, Option<i64>), u32>, u32, u32, u32)> = HashMap::new();
        for (match_id, roster) in &self.rosters {
            let Some(&(_, Some(team), _)) = roster.iter().find(|(a, _, _)| *a == account) else { continue };
            let Some(m) = self.matches.get(match_id) else { continue };
            if !Self::played(m) {
                continue;
            }
            let (division, tier) = self.division_of(m);
            let (f, a, won) = Self::result_for(m, team);
            let e = seasons.entry((m.season, team)).or_default();
            *e.0.entry((division.clone(), tier)).or_default() += 1;
            e.1 += 1;
            e.2 += u32::from(won == Some(true));
            e.3 += u32::from(won == Some(false));
            officials.push(Official {
                match_id: m.match_id,
                time: m.time,
                season: m.season,
                division,
                stage: m.stage.clone(),
                round: m.round.clone(),
                team: self.team(team),
                opponent: self.team(if m.clan1 == team { m.clan2 } else { m.clan1 }),
                score_for: f,
                score_against: a,
                won,
            });
        }
        officials.sort_by_key(|o| std::cmp::Reverse(o.time.unwrap_or(0)));

        let season_name = |s: i64| self.matches.values().find(|m| m.season == s).map(|m| m.season_name.clone()).unwrap_or_default();
        let mut won_medals: Vec<Medal> = Vec::new();
        let mut out: Vec<PlayerSeason> = seasons
            .into_iter()
            .map(|((season, team), (divs, played, won, lost))| {
                // The division they played most for this team that season.
                let ((division, tier), _) = divs.into_iter().max_by_key(|(_, n)| *n).unwrap_or(((String::new(), None), 0));
                let place = medals.iter().filter(|((s, _), _)| *s == season).find_map(|((_, d), list)| {
                    list.iter().find(|(_, t, _)| *t == team).map(|(p, _, how)| (*p, d.clone(), how.clone()))
                });
                if let Some((p, d, how)) = &place {
                    won_medals.push(Medal {
                        season,
                        season_name: season_name(season),
                        division: d.clone(),
                        tier: self.tiers.get(&(season, d.clone())).copied().or_else(|| self.tier_by_name.get(d).copied()),
                        place: *p,
                        team: self.team(team),
                        how: how.clone(),
                    });
                }
                PlayerSeason { season, season_name: season_name(season), division, tier, team: self.team(team), played, won, lost, place: place.map(|p| p.0) }
            })
            .collect();
        out.sort_by(|a, b| b.season.cmp(&a.season).then(b.played.cmp(&a.played)));
        won_medals.sort_by(|a, b| b.season.cmp(&a.season).then(a.place.cmp(&b.place)));
        (out, won_medals, officials)
    }

    fn highest(seasons: &[PlayerSeason]) -> Option<Division> {
        let mut per_tier: BTreeMap<i64, (usize, String)> = BTreeMap::new();
        for s in seasons {
            if let Some(t) = s.tier {
                let e = per_tier.entry(t).or_insert((0, s.division.clone()));
                e.0 += s.played as usize;
            }
        }
        per_tier
            .iter()
            .find(|(_, (n, _))| *n >= MIN_FOR_DIVISION)
            .or_else(|| per_tier.iter().next())
            .map(|(t, (_, name))| Division { name: name.clone(), tier: *t })
    }
}

fn main_of(classes: Option<&HashMap<String, i64>>) -> (Vec<(String, i64)>, Option<String>) {
    let mut played: Vec<(String, i64)> = classes.map(|c| c.iter().map(|(k, v)| (k.clone(), *v)).collect()).unwrap_or_default();
    played.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let main = played.first().map(|(c, _)| c.clone());
    (played, main)
}

/// Search the catalogue and the owner's matches by name or Steam ID.
pub async fn search(db: &Db, query: &str, limit: usize) -> Result<Vec<Hit>> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let by_id = SteamId::parse(query.trim()).ok().map(|s| s.account_id());
    let cat = Catalogue::load(db).await?;
    let log_names = db.log_names().await?;
    // Every name each account has used, newest first where known.
    let mut names: HashMap<u32, Vec<String>> = log_names;
    let mut officials: HashMap<u32, (u32, i64)> = HashMap::new();
    for (match_id, roster) in &cat.rosters {
        let t = cat.matches.get(match_id).and_then(|m| m.time).unwrap_or(0);
        for (a, _, n) in roster {
            let e = officials.entry(*a).or_default();
            e.0 += 1;
            e.1 = e.1.max(t);
            if !n.is_empty() {
                let v = names.entry(*a).or_default();
                if !v.contains(n) {
                    v.insert(0, n.clone());
                }
            }
        }
    }
    let mut found: Vec<u32> = names
        .iter()
        .filter(|(a, ns)| by_id == Some(**a) || (by_id.is_none() && ns.iter().any(|n| n.to_lowercase().contains(&q))))
        .map(|(a, _)| *a)
        .collect();
    // Exact and prefix matches first, then the most officials played.
    let rank = |a: &u32| {
        let ns = &names[a];
        let exact = ns.iter().any(|n| n.to_lowercase() == q);
        let prefix = ns.iter().any(|n| n.to_lowercase().starts_with(&q));
        (std::cmp::Reverse(exact), std::cmp::Reverse(prefix), std::cmp::Reverse(officials.get(a).map_or(0, |o| o.0)))
    };
    found.sort_by_key(rank);
    found.truncate(limit);

    let medals = cat.medals();
    let classes = db.main_class_games().await?;
    Ok(found
        .into_iter()
        .map(|a| {
            let (seasons, won, _) = cat.player(a, &medals);
            let mut m = [0u32; 3];
            for x in &won {
                m[usize::from(x.place - 1)] += 1;
            }
            Hit {
                account_id: a,
                name: names[&a].first().cloned().unwrap_or_default(),
                highest: Catalogue::highest(&seasons),
                main_class: main_of(classes.get(&a)).1,
                medals: m,
                officials: officials.get(&a).map_or(0, |o| o.0),
                last_seen: officials.get(&a).map(|o| o.1).filter(|t| *t > 0),
            }
        })
        .collect())
}

/// One player's profile. ETF2L's page for them is fetched if it is not
/// held or is over a week old; a failure there leaves those fields empty.
pub async fn profile(db: &Db, sources: &Sources, account: u32) -> Result<Profile> {
    let cat = Catalogue::load(db).await?;
    let medals = cat.medals();
    let (seasons, won, officials) = cat.player(account, &medals);
    let steamid = SteamId::from_account_id(account);
    let etf2l = match db.etf2l_player(account).await? {
        Some(p) if now() - p.fetched_at < ETF2L_PLAYER_KEEP_S => Some(p),
        held => match fetch_etf2l_player(sources, &steamid.to_steamid64()).await {
            Ok(Some(p)) => {
                db.put_etf2l_player(account, &p).await?;
                Some(p)
            }
            Ok(None) => held,
            Err(e) => {
                tracing::warn!(account, error = %format!("{e:#}"), "ETF2L player page");
                held
            }
        },
    };
    let classes = db.main_class_games().await?;
    let (played_classes, main_class) = main_of(classes.get(&account));
    let mut aliases: Vec<String> = Vec::new();
    for (_, roster) in cat.rosters.iter() {
        for (a, _, n) in roster {
            if *a == account && !n.is_empty() && !aliases.contains(n) {
                aliases.push(n.clone());
            }
        }
    }
    for n in db.log_names().await?.remove(&account).unwrap_or_default() {
        if !aliases.contains(&n) {
            aliases.push(n);
        }
    }
    let name = etf2l.as_ref().and_then(|p| p.name.clone()).or_else(|| aliases.first().cloned()).unwrap_or_else(|| steamid.to_steamid64());
    aliases.retain(|a| *a != name);
    Ok(Profile {
        account_id: account,
        steamid64: steamid.to_steamid64(),
        name,
        aliases,
        country: etf2l.as_ref().and_then(|p| p.country.clone()),
        avatar: etf2l.as_ref().and_then(|p| p.avatar.clone()),
        declared_classes: etf2l.as_ref().map(|p| p.classes.clone()).unwrap_or_default(),
        played_classes,
        main_class,
        current: seasons.first().cloned(),
        highest: Catalogue::highest(&seasons),
        medals: won,
        seasons,
        officials,
        etf2l_id: etf2l.and_then(|p| p.etf2l_id),
    })
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

async fn fetch_etf2l_player(sources: &Sources, steamid64: &str) -> Result<Option<Etf2lPlayer>> {
    let Some(body) = sources.etf2l_get(&format!("/player/{steamid64}")).await? else { return Ok(None) };
    let v: Value = serde_json::from_str(&body).context("ETF2L player page")?;
    let p = v.get("player").unwrap_or(&Value::Null);
    if p.is_null() {
        return Ok(None);
    }
    Ok(Some(Etf2lPlayer {
        etf2l_id: p.get("id").and_then(Value::as_i64),
        name: p.get("name").and_then(Value::as_str).map(str::to_string),
        country: p.get("country").and_then(Value::as_str).map(str::to_string),
        classes: p.get("classes").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect(),
        avatar: p.pointer("/steam/avatar").and_then(Value::as_str).map(str::to_string),
        registered: p.get("registered").and_then(Value::as_i64),
        fetched_at: now(),
    }))
}

/// Index the players of league-sample logs that are not indexed yet: their
/// team and main class, so a player's main class reads from the league too.
/// Returns how many logs were indexed.
pub async fn index_league_players(db: &Db, limit: i64) -> Result<usize> {
    let logs = db.league_logs_unindexed(limit).await?;
    let n = logs.len();
    for (log_id, json) in logs {
        let rows: Vec<(u32, String, Option<&'static str>, i64)> = match serde_json::from_str::<Value>(&json).ok().and_then(|v| crate::normalize::normalize(log_id, &v).ok()) {
            Some(log) => log
                .players
                .iter()
                .map(|p| (p.id.account_id(), p.team.as_str().to_string(), p.main_class().map(|c| c.as_str()), p.total_time()))
                .collect(),
            None => Vec::new(),
        };
        let refs: Vec<(u32, Option<&str>, Option<&str>, i64)> = rows.iter().map(|(a, t, c, s)| (*a, Some(t.as_str()), *c, *s)).collect();
        // A log that cannot be read still gets a (empty) mark by being
        // skipped here; it is retried next time, which costs one parse.
        if !refs.is_empty() {
            db.put_league_players(log_id, &refs).await?;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(id: i64, season: i64, stage: &str, div: Option<&str>, comp_div: &str, round: Option<&str>, c1: i64, c2: i64, r1: i64, r2: i64) -> CatMatch {
        CatMatch {
            match_id: id,
            competition_id: 1,
            season,
            season_name: format!("S{season}"),
            comp_division: comp_div.into(),
            stage: stage.into(),
            division: div.map(str::to_string),
            tier: div.map(|_| 2),
            round: round.map(str::to_string),
            time: Some(id),
            clan1: c1,
            clan2: c2,
            r1: Some(r1),
            r2: Some(r2),
            default_win: false,
        }
    }

    fn cat(matches: Vec<CatMatch>) -> Catalogue {
        let mut rosters: HashMap<i64, Vec<(u32, Option<i64>, String)>> = HashMap::new();
        for x in &matches {
            // Player 1 always plays for team 10; others for their team.
            for team in [x.clan1, x.clan2] {
                rosters.entry(x.match_id).or_default().push((team as u32, Some(team), format!("p{team}")));
            }
        }
        let tiers = matches.iter().filter_map(|x| Some(((x.season, x.division.clone()?), x.tier?))).collect();
        Catalogue { newest_season: matches.iter().map(|x| x.season).max().unwrap_or(0), matches: matches.into_iter().map(|x| (x.match_id, x)).collect(), rosters, teams: HashMap::new(), tiers, tier_by_name: HashMap::new() }
    }

    #[test]
    fn a_grand_final_gives_gold_and_silver_and_a_third_place_match_bronze() {
        let c = cat(vec![
            m(1, 35, "regular", Some("Low"), "", None, 10, 11, 6, 0),
            m(2, 35, "Playoffs", None, "Low", Some("Semi-Finals"), 10, 12, 5, 1),
            m(3, 35, "Playoffs", None, "Low", Some("Grand Final"), 10, 11, 4, 5),
            m(4, 35, "3rd Place", None, "Low", None, 12, 13, 6, 3),
            m(5, 36, "regular", Some("Low"), "", None, 10, 11, 6, 0),
        ]);
        let medals = c.medals();
        let low = &medals[&(35, "Low".to_string())];
        assert!(low.contains(&(1, 11, "Grand Final 5-4".into())));
        assert!(low.contains(&(2, 10, "Grand Final 4-5".into())));
        assert!(low.contains(&(3, 12, "3rd place match 6-3".into())));
        // Team 10's player: silver in S35, and their season is Low with its tier.
        let (seasons, won, officials) = c.player(10, &medals);
        assert_eq!(won.len(), 1);
        assert_eq!(won[0].place, 2);
        assert_eq!(seasons.iter().find(|s| s.season == 35).unwrap().tier, Some(2), "the playoffs take the season's tier");
        assert_eq!(officials.len(), 4);
    }

    #[test]
    fn a_division_without_a_final_is_settled_by_its_table_once_the_season_is_over() {
        let mut ms = Vec::new();
        let mut id = 0;
        // Four teams, 20 beats all, 21 beats 22 and 23, 22 beats 23.
        for (a, b) in [(20, 21), (20, 22), (20, 23), (21, 22), (21, 23), (22, 23)] {
            id += 1;
            ms.push(m(id, 34, "regular", Some("High"), "", None, a, b, 6, 0));
        }
        ms.push(m(99, 35, "regular", Some("High"), "", None, 20, 21, 6, 0));
        let c = cat(ms);
        let medals = c.medals();
        assert_eq!(medals[&(34, "High".to_string())].iter().map(|x| (x.0, x.1)).collect::<Vec<_>>(), vec![(1, 20), (2, 21), (3, 22)]);
        assert!(!medals.contains_key(&(35, "High".to_string())), "the newest season is not over");
    }

    #[test]
    fn one_merc_game_does_not_make_a_division() {
        let s = |tier: i64, played: u32| PlayerSeason {
            season: 30,
            season_name: String::new(),
            division: format!("t{tier}"),
            tier: Some(tier),
            team: Team { id: 1, name: String::new(), avatar: None },
            played,
            won: 0,
            lost: 0,
            place: None,
        };
        assert_eq!(Catalogue::highest(&[s(0, 1), s(2, 9)]).map(|d| d.tier), Some(2));
        assert_eq!(Catalogue::highest(&[s(0, 1)]).map(|d| d.tier), Some(0), "nothing else to go on");
    }
}
