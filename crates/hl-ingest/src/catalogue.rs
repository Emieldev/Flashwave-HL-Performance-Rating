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
//! **Which division a player is** (Flashy, ETF2L's own rule): in a season,
//! a division counts for them if they played at least [`MIN_FOR_DIVISION`]
//! officials in it, or played in the Grand Final of the division one below
//! (the finalists move up). Where both apply, the higher one. One merc game
//! in Premiership makes nobody a Premiership player. Their highest division
//! is the best one that counted in any season; every per-season division
//! -- the tags on a match, the ranks -- follows the same rule.

use crate::leagues::season_order;
use crate::sources::Sources;
use anyhow::{Context, Result};
use hl_core::SteamId;
use hl_db::{CatMatch, Db, Etf2lPlayer};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

/// Officials in a division in one season before it counts as theirs.
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

/// An event's MVP (Flashy: "whoever played the finals and did the best in
/// the grand final", as HLTV names one). One per class for each Grand Final
/// -- the best player of the class on either finalist -- and one for the
/// event, the best of the winning team's.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mvp {
    pub season: i64,
    pub season_name: String,
    pub division: String,
    pub tier: Option<i64>,
    pub class: String,
    pub account_id: u32,
    pub team: Team,
    /// Their team won the final.
    pub won: bool,
    /// The event MVP, not only the class's.
    pub event: bool,
    /// What it was picked on: [`MVP_FINAL_WEIGHT`] of the Grand Final, the
    /// rest the other playoff games.
    pub score: f64,
    pub final_rating: f64,
    pub final_maps: usize,
    /// Their other playoff games: average and logs.
    pub playoffs_rating: Option<f64>,
    pub playoffs_maps: usize,
}

/// How much of an MVP's score is the Grand Final; the rest is the playoff
/// run before it. The final decides it, as HLTV's does, but one good map
/// against four bad ones does not.
pub const MVP_FINAL_WEIGHT: f64 = 0.7;

/// One official a player played.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Official {
    pub match_id: i64,
    pub time: Option<i64>,
    pub season: i64,
    /// ETF2L's name for the competition ("Highlander Experimental Cup #10").
    pub competition: String,
    pub division: String,
    pub tier: Option<i64>,
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
    /// The events they were MVP of, on their class or overall.
    pub mvps: Vec<Mvp>,
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

/// The rounds a medal is read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RoundKind {
    Final,
    Third,
    Semi,
    LowerFinal,
}

impl Catalogue {
    pub async fn load(db: &Db) -> Result<Catalogue> {
        let mut matches: HashMap<i64, CatMatch> = db.catalogue_matches().await?.into_iter().map(|m| (m.match_id, m)).collect();
        // A competition's stage and division as the parser reads its name
        // now: one stored before the parser knew "3rd Place Match" was kept
        // as a regular division called "Premiership 3rd Place Match".
        for m in matches.values_mut() {
            if let Some((division, stage)) = crate::leagues::division_and_stage(&m.comp_name).filter(|_| !m.comp_name.is_empty()) {
                m.comp_division = division;
                m.stage = stage;
            }
        }
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
        // The newest numbered season: 134 (AFA 2025) is older than 35.
        let newest_season = matches.values().map(|m| m.season).filter(|s| *s < crate::leagues::OFF_SEASON).max().unwrap_or(0);
        Ok(Catalogue { matches, rosters, teams: db.etf2l_teams().await?, tiers, tier_by_name, newest_season })
    }

    fn team(&self, id: i64) -> Team {
        let (name, avatar) = self.teams.get(&id).cloned().unwrap_or_else(|| (format!("Team {id}"), None));
        Team { id, name, avatar }
    }

    /// A match's division, and its tier where one can be found.
    fn division_of(&self, m: &CatMatch) -> (String, Option<i64>) {
        let name = m.division.clone().unwrap_or_else(|| m.comp_division.clone());
        // Today's ladder first; ETF2L's own tier only for a name that is not
        // a division.
        let tier = crate::leagues::canonical_tier(&name)
            .or(m.tier)
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

    /// The division a medal is for, as ETF2L ran it: "Division 1" and
    /// "Division 2" each had their own final, so each has its own medals,
    /// though today's ladder calls both High (Flashy's audit: the merged name
    /// gave S25-S30 two golds a division). Groups of one division share it
    /// ("Division 2A" and "2B" are Division 2, "Open B" is Open), and a
    /// division today's ladder knows is called by today's name. A final with
    /// no division of its own (Autumn 2023's Grand Final is a round of the
    /// season) takes the one its teams played that season.
    fn medal_division(&self, m: &CatMatch) -> String {
        let (name, _) = self.division_of(m);
        let name = Self::ungrouped(&name);
        if Self::numbered(&name).is_some() {
            return name;
        }
        let mut name = name;
        if crate::leagues::canonical_tier(&name).is_none() {
            let mut seen: HashMap<String, u32> = HashMap::new();
            for x in self.matches.values().filter(|x| x.season == m.season && x.stage == "regular" && x.match_id != m.match_id) {
                if [x.clan1, x.clan2].iter().any(|t| *t == m.clan1 || *t == m.clan2) {
                    let (d, _) = self.division_of(x);
                    if crate::leagues::canonical_tier(&d).is_some() {
                        *seen.entry(d).or_default() += 1;
                    }
                }
            }
            if let Some((d, _)) = seen.into_iter().max_by_key(|(_, n)| *n) {
                name = d;
            }
        }
        let name = Self::ungrouped(&name);
        // Numbered divisions, and S33's Freshest beside its Fresh: each
        // had its own final.
        if Self::numbered(&name).is_some() || name.eq_ignore_ascii_case("freshest") {
            return name;
        }
        crate::leagues::canonical_tier(&name).and_then(|t| crate::leagues::TIER_NAMES.get(t as usize)).map_or(name, |n| n.to_string())
    }

    /// "Division 2A" -> "Division 2", "Open B" -> "Open", "Premiership
    /// Division" -> "Premiership": one division's groups, one name.
    fn ungrouped(name: &str) -> String {
        let n = name.trim();
        let n = n.strip_suffix(" Division").unwrap_or(n);
        match n.rsplit_once(' ') {
            Some((head, last)) if last.len() == 1 && last.chars().all(|c| c.is_ascii_alphabetic()) => head.to_string(),
            Some((head, last))
                if last.len() >= 2 && last[..last.len() - 1].chars().all(|c| c.is_ascii_digit()) && last.ends_with(|c: char| c.is_ascii_alphabetic()) =>
            {
                format!("{head} {}", &last[..last.len() - 1])
            }
            _ => n.to_string(),
        }
    }

    /// The number of a "Division 3" / "Div 3", if it is one.
    fn numbered(name: &str) -> Option<u32> {
        let d = name.to_ascii_lowercase();
        d.strip_prefix("division ").or_else(|| d.strip_prefix("div ")).and_then(|n| n.trim().parse().ok())
    }

    /// What a round is, whatever ETF2L called it that season: "Grand Final",
    /// "Grand-Final", "Division 1 Grand Final", "3rd Place " (with the
    /// space), "Semi-Finals", a double-elimination "Lower Bracket Final".
    fn round_kind(round: Option<&str>) -> Option<RoundKind> {
        let r = round?.to_ascii_lowercase().replace('-', " ");
        let r = r.split_whitespace().collect::<Vec<_>>().join(" ");
        if r.contains("grand final") || r == "final" || r.starts_with("final ") {
            Some(RoundKind::Final)
        } else if r.contains("3rd place") || r.contains("third place") || r.contains("bronze") {
            Some(RoundKind::Third)
        } else if r.contains("lower bracket final") {
            Some(RoundKind::LowerFinal)
        } else if r.contains("semi") && !r.contains("bracket") {
            Some(RoundKind::Semi)
        } else {
            None
        }
    }

    /// A league final: a Playoffs competition's final, a "Grand Final"
    /// competition, or -- in the seasons without a separate playoff
    /// competition (Autumn 2023) -- a regular match whose round is the Grand
    /// Final. A preseason cup's final is not the league's.
    fn is_final(m: &CatMatch) -> bool {
        m.stage != "Cup" && (m.stage == "Grand Final" || Self::round_kind(m.round.as_deref()) == Some(RoundKind::Final))
    }

    fn played(m: &CatMatch) -> bool {
        !m.default_win && m.r1.unwrap_or(0) + m.r2.unwrap_or(0) > 0
    }

    /// Every medal of every season and division: `(season, division) ->
    /// [(place, team, how)]`.
    pub fn medals(&self) -> BTreeMap<(i64, String), Vec<(u8, i64, String)>> {
        let mut out: BTreeMap<(i64, String), Vec<(u8, i64, String)>> = BTreeMap::new();
        // A forfeit decides a final as surely as a match does (AFA 2025's
        // Division 1 Grand Final was won by default): it says so.
        let score = |m: &CatMatch, winner: i64| {
            let (f, a, _) = Self::result_for(m, winner);
            format!("{}-{}{}", f.unwrap_or(0), a.unwrap_or(0), if m.default_win { " by default" } else { "" })
        };
        let winner_loser = |m: &CatMatch| -> Option<(i64, i64)> {
            match (m.r1?, m.r2?) {
                (a, b) if a > b => Some((m.clan1, m.clan2)),
                (a, b) if b > a => Some((m.clan2, m.clan1)),
                _ => None,
            }
        };
        let decided = |m: &&CatMatch| Self::played(m) || m.default_win;
        for m in self.matches.values().filter(decided) {
            let key = (m.season, self.medal_division(m));
            if Self::is_final(m) {
                if let Some((w, l)) = winner_loser(m) {
                    let e = out.entry(key).or_default();
                    e.push((1, w, format!("Grand Final {}", score(m, w))));
                    e.push((2, l, format!("Grand Final {}", score(m, l))));
                }
            } else if m.stage == "Cup" {
                continue;
            } else if m.stage == "3rd Place" || Self::round_kind(m.round.as_deref()) == Some(RoundKind::Third) {
                if let Some((w, _)) = winner_loser(m) {
                    out.entry(key).or_default().push((3, w, format!("3rd place match {}", score(m, w))));
                }
            } else if Self::round_kind(m.round.as_deref()) == Some(RoundKind::LowerFinal) {
                // Double elimination: losing the lower bracket's final is
                // third outright.
                if let Some((_, l)) = winner_loser(m) {
                    out.entry(key).or_default().push((3, l, format!("lower bracket final {}", score(m, l))));
                }
            }
        }
        // No 3rd-place match: the two losing semi-finalists share third, as
        // ETF2L gives it (Flashy's S34 Mid: lost the semi-final 3-6, and
        // there was no match for third).
        let mut semis: BTreeMap<(i64, String), Vec<(i64, String)>> = BTreeMap::new();
        // Semi-finals in a Playoffs competition, or -- in the seasons whose
        // playoffs were rounds of the season ("Top Tiers") -- in the season.
        for m in self.matches.values().filter(|m| decided(m) && m.stage != "Cup") {
            if Self::round_kind(m.round.as_deref()) == Some(RoundKind::Semi) {
                if let Some((_, l)) = winner_loser(m) {
                    semis.entry((m.season, self.medal_division(m))).or_default().push((l, format!("joint 3rd: semi-final {}", score(m, l))));
                }
            }
        }
        for (key, losers) in semis {
            if let Some(e) = out.get_mut(&key) {
                if !e.iter().any(|(p, _, _)| *p == 3) {
                    e.extend(losers.into_iter().map(|(team, how)| (3u8, team, how)));
                }
            }
        }
        // Divisions without a final: the table, once the season is over.
        let mut tables: HashMap<(i64, String), HashMap<i64, (u32, i64)>> = HashMap::new();
        for m in self.matches.values().filter(|m| Self::played(m) && m.stage == "regular" && season_order(m.season) < season_order(self.newest_season)) {
            let t = tables.entry((m.season, self.medal_division(m))).or_default();
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

    /// A season's own name, not a tournament's kept with it.
    fn season_name(&self, s: i64) -> String {
        let of = |m: &&CatMatch| m.season == s;
        self.matches
            .values()
            .filter(of)
            .find(|m| m.stage != "Cup")
            .or_else(|| self.matches.values().find(of))
            .map(|m| m.season_name.clone())
            .unwrap_or_default()
    }

    /// Every event's MVPs, from the rated games: per Grand Final (played,
    /// not forfeited), the best player of each class on the two finalists,
    /// and the event's -- the best class MVP of the winning team. A final
    /// whose logs are not held has none, rather than a guess.
    pub fn mvps(&self, games: &[hl_db::RatedGame]) -> Vec<Mvp> {
        let mut by_match: HashMap<i64, Vec<&hl_db::RatedGame>> = HashMap::new();
        for g in games {
            if let Some(id) = g.etf2l_match_id {
                by_match.entry(id).or_default().push(g);
            }
        }
        // (season, division) -> the playoff matches before the final.
        let mut playoffs: HashMap<(i64, String), Vec<i64>> = HashMap::new();
        for m in self.matches.values().filter(|m| Self::played(m) && m.stage != "Cup") {
            let playoff = matches!(m.stage.as_str(), "Playoffs" | "3rd Place") || Self::round_kind(m.round.as_deref()).is_some();
            if playoff && !Self::is_final(m) {
                playoffs.entry((m.season, self.medal_division(m))).or_default().push(m.match_id);
            }
        }
        let mean = |v: &[f64]| (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64);
        let mut out = Vec::new();
        for f in self.matches.values().filter(|m| Self::played(m) && Self::is_final(m)) {
            let Some(in_final) = by_match.get(&f.match_id) else { continue };
            let winner = match (f.r1, f.r2) {
                (Some(a), Some(b)) if a > b => f.clan1,
                (Some(a), Some(b)) if b > a => f.clan2,
                _ => continue,
            };
            let division = self.medal_division(f);
            let key = (f.season, division.clone());
            let team_of: HashMap<u32, i64> =
                self.rosters.get(&f.match_id).map(|r| r.iter().filter_map(|(a, t, _)| Some((*a, (*t)?))).collect()).unwrap_or_default();
            let before: Vec<&hl_db::RatedGame> =
                playoffs.get(&key).into_iter().flatten().filter_map(|id| by_match.get(id)).flatten().copied().collect();
            let tier = crate::leagues::canonical_tier(&division).or_else(|| self.tiers.get(&key).copied()).or_else(|| self.tier_by_name.get(&division).copied());
            // A final is several logs: one per KOTH round, one per stopwatch
            // half. A candidate played more than half of them -- a sub with
            // one good half, or one map of two, is not the final's MVP.
            let mut logs: Vec<i64> = in_final.iter().map(|g| g.log_id).collect();
            logs.sort_unstable();
            logs.dedup();
            let mut played: HashMap<u32, std::collections::HashSet<i64>> = HashMap::new();
            for g in in_final.iter() {
                played.entry(g.account_id).or_default().insert(g.log_id);
            }
            // (account, class) -> final scores
            let mut fin: HashMap<(u32, &str), Vec<f64>> = HashMap::new();
            for g in in_final.iter() {
                fin.entry((g.account_id, g.class.as_str())).or_default().push(g.score);
            }
            let candidates: Vec<Mvp> = fin
                .into_iter()
                .filter_map(|((a, class), scores)| {
                    let team = *team_of.get(&a)?;
                    if team != f.clan1 && team != f.clan2 {
                        return None;
                    }
                    if played.get(&a).map_or(0, |p| p.len()) * 2 <= logs.len() && logs.len() > 1 {
                        return None;
                    }
                    let final_rating = mean(&scores)?;
                    let rest: Vec<f64> = before.iter().filter(|g| g.account_id == a && g.class == class).map(|g| g.score).collect();
                    let playoffs_rating = mean(&rest);
                    let score = playoffs_rating.map_or(final_rating, |p| MVP_FINAL_WEIGHT * final_rating + (1.0 - MVP_FINAL_WEIGHT) * p);
                    Some(Mvp {
                        season: f.season,
                        season_name: self.season_name(f.season),
                        division: division.clone(),
                        tier,
                        class: class.to_string(),
                        account_id: a,
                        team: self.team(team),
                        won: team == winner,
                        event: false,
                        score,
                        final_rating,
                        final_maps: scores.len(),
                        playoffs_rating,
                        playoffs_maps: rest.len(),
                    })
                })
                .collect();
            // Best score; a tie goes to the team that won.
            let better = |x: &&Mvp, y: &&Mvp| x.score.total_cmp(&y.score).then(x.won.cmp(&y.won));
            let mut classes: Vec<&str> = candidates.iter().map(|c| c.class.as_str()).collect();
            classes.sort_unstable();
            classes.dedup();
            let mut class_mvps: Vec<Mvp> =
                classes.iter().filter_map(|c| candidates.iter().filter(|m| m.class == *c).max_by(better).cloned()).collect();
            // The event's: the best player of the team that won it, as
            // HLTV's goes to the champions -- whether or not they out-rated
            // the losing side's player on their own class.
            if let Some(top) = candidates.iter().filter(|m| m.won).max_by(better) {
                let mut e = top.clone();
                e.event = true;
                class_mvps.push(e);
            }
            out.extend(class_mvps);
        }
        out.sort_by(|a, b| season_order(b.season).cmp(&season_order(a.season)).then(a.tier.cmp(&b.tier)).then(b.event.cmp(&a.event)));
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
            // A tournament is their official, not their season.
            if m.stage != "Cup" || m.comp_name.is_empty() || crate::leagues::parse_name(&m.comp_name).is_some() {
                let e = seasons.entry((m.season, team)).or_default();
                *e.0.entry((division.clone(), tier)).or_default() += 1;
                e.1 += 1;
                e.2 += u32::from(won == Some(true));
                e.3 += u32::from(won == Some(false));
            }
            officials.push(Official {
                match_id: m.match_id,
                time: m.time,
                season: m.season,
                competition: m.comp_name.clone(),
                tier,
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

        // A season's own name, not a tournament's kept with it.
        let season_name = |s: i64| {
            let of = |m: &&CatMatch| m.season == s;
            self.matches
                .values()
                .filter(of)
                .find(|m| m.stage != "Cup")
                .or_else(|| self.matches.values().find(of))
                .map(|m| m.season_name.clone())
                .unwrap_or_default()
        };
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
                        tier: crate::leagues::canonical_tier(d).or_else(|| self.tiers.get(&(season, d.clone())).copied()).or_else(|| self.tier_by_name.get(d).copied()),
                        place: *p,
                        team: self.team(team),
                        how: how.clone(),
                    });
                }
                PlayerSeason { season, season_name: season_name(season), division, tier, team: self.team(team), played, won, lost, place: place.map(|p| p.0) }
            })
            .collect();
        out.sort_by(|a, b| season_order(b.season).cmp(&season_order(a.season)).then(b.played.cmp(&a.played)));
        won_medals.sort_by(|a, b| season_order(b.season).cmp(&season_order(a.season)).then(a.place.cmp(&b.place)));
        (out, won_medals, officials)
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
    let divisions = cat.divisions();
    let classes = db.main_class_games().await?;
    Ok(found
        .into_iter()
        .map(|a| {
            let (_, won, _) = cat.player(a, &medals);
            let mut m = [0u32; 3];
            for x in &won {
                m[usize::from(x.place - 1)] += 1;
            }
            Hit {
                account_id: a,
                name: names[&a].first().cloned().unwrap_or_default(),
                highest: Catalogue::highest_of(&divisions, a),
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
    let rated = db.rated_games(hl_rating::MODEL_VERSION, None).await?;
    let mvps: Vec<Mvp> = cat.mvps(&rated).into_iter().filter(|m| m.account_id == account).collect();
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
        highest: Catalogue::highest_of(&cat.divisions(), account),
        medals: won,
        mvps,
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

/// One season's medals by division: `(place, team name, how)`, for checking
/// against ETF2L (`hl medals <season>`).
pub async fn season_medals(db: &Db, season: i64) -> Result<Vec<(String, Vec<(u8, String, String)>)>> {
    let cat = Catalogue::load(db).await?;
    Ok(cat
        .medals()
        .into_iter()
        .filter(|((s, _), _)| *s == season)
        .map(|((_, d), mut list)| {
            list.sort_by_key(|x| x.0);
            (d, list.into_iter().map(|(p, t, how)| (p, cat.team(t).name, how)).collect())
        })
        .collect())
}

// ---- Ratings, stat bars and ranks (Q36) ----------------------------------

/// Officials in a season, class and division before a player is ranked.
/// Counted as officials, not logs: the owner's matches hold one combined log
/// per official and the league sample one log per map.
pub const MIN_RANK_GAMES: usize = 4;
/// "Recent form": the games in this long before a player's newest.
const RECENT_S: i64 = 90 * 24 * 3600;
/// Recent games needed before the bars show recent form, not the career.
const MIN_RECENT_GAMES: usize = 5;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassStats {
    pub class: String,
    pub games: usize,
    pub career: f64,
    pub recent: Option<f64>,
    pub recent_games: usize,
    pub best: f64,
    /// Each component group, 0-100 (recent form where there is enough of
    /// it, else the career): the stat bars.
    pub groups: Vec<(String, f64)>,
    pub groups_recent: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rank {
    pub season: i64,
    pub season_name: String,
    pub division: String,
    pub tier: i64,
    pub class: String,
    pub rank: usize,
    pub of: usize,
    pub avg: f64,
    pub games: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerStats {
    /// Most played first.
    pub classes: Vec<ClassStats>,
    /// Newest season first, best rank first within it.
    pub ranks: Vec<Rank>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankRow {
    pub rank: usize,
    pub account_id: u32,
    pub name: String,
    pub team: Option<Team>,
    pub games: usize,
    pub avg: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rankings {
    pub season: i64,
    pub season_name: String,
    pub division: String,
    pub tier: i64,
    pub class: String,
    pub rows: Vec<RankRow>,
    /// Seasons that can be ranked, newest first: `(season, name)`.
    pub seasons: Vec<(i64, String)>,
    /// Tiers seen in the chosen season: `(tier, name)`.
    pub divisions: Vec<(i64, String)>,
}

impl Catalogue {
    /// Each season's dates, from its officials: `(season, name, from, to)`,
    /// a few days either side for the matches played around them. Newest
    /// first.
    fn season_windows(&self) -> Vec<(i64, String, i64, i64)> {
        // keyed by the season's place in time: AFA 2025 between 34 and 35
        let mut w: BTreeMap<i64, (i64, String, i64, i64)> = BTreeMap::new();
        // The league's dates: a tournament kept with a season can be months off.
        for m in self.matches.values().filter(|m| m.stage != "Cup") {
            let Some(t) = m.time else { continue };
            let e = w.entry(season_order(m.season)).or_insert((m.season, m.season_name.clone(), t, t));
            e.2 = e.2.min(t);
            e.3 = e.3.max(t);
        }
        const SLACK: i64 = 3 * 24 * 3600;
        w.into_values().rev().map(|(s, n, a, b)| (s, n, a - SLACK, b + SLACK)).collect()
    }

    /// Every player's division each season, by the rule in the module doc:
    /// `(tier, division, team)`. A season with no division that counts is
    /// left out.
    fn divisions(&self) -> HashMap<(u32, i64), (i64, String, i64)> {
        // (account, season) -> tier -> (officials, team -> officials)
        let mut count: HashMap<(u32, i64), BTreeMap<i64, (u32, HashMap<i64, u32>)>> = HashMap::new();
        // (account, season) -> tiers whose Grand Final they played, and for whom
        let mut finals: HashMap<(u32, i64), Vec<(i64, i64)>> = HashMap::new();
        // (season, tier) -> the division's name that season
        let mut names: HashMap<(i64, i64), String> = HashMap::new();
        for (match_id, roster) in &self.rosters {
            let Some(m) = self.matches.get(match_id) else { continue };
            if !Self::played(m) {
                continue;
            }
            // A preseason cup is an official, not the league.
            if m.stage == "Cup" {
                continue;
            }
            let (division, Some(tier)) = self.division_of(m) else { continue };
            names.entry((m.season, tier)).or_insert_with(|| division.clone());
            let grand_final = Self::is_final(m);
            for (a, team, _) in roster {
                let Some(team) = team else { continue };
                let e = count.entry((*a, m.season)).or_default().entry(tier).or_default();
                e.0 += 1;
                *e.1.entry(*team).or_default() += 1;
                if grand_final {
                    finals.entry((*a, m.season)).or_default().push((tier, *team));
                }
            }
        }
        let name = |season: i64, tier: i64| {
            crate::leagues::TIER_NAMES
                .get(tier as usize)
                .map(|n| n.to_string())
                .or_else(|| names.get(&(season, tier)).cloned())
                .unwrap_or_default()
        };
        let mut out = HashMap::new();
        for (key, tiers) in count {
            // Three officials in a division...
            let by_officials = tiers
                .iter()
                .filter(|(_, (n, _))| *n as usize >= MIN_FOR_DIVISION)
                .map(|(t, (_, teams))| (*t, *teams.iter().max_by_key(|(_, n)| **n).map(|(team, _)| team).unwrap_or(&0)));
            // ...or its Grand Final, one division below.
            let by_final = finals.get(&key).into_iter().flatten().filter(|(t, _)| *t > 0).map(|(t, team)| (t - 1, *team));
            if let Some((tier, team)) = by_officials.chain(by_final).min_by_key(|(t, _)| *t) {
                out.insert(key, (tier, name(key.1, tier), team));
            }
        }
        out
    }

    /// Every player's division as played each season -- the one they played
    /// most league officials in, without the Grand Final's step up -- for
    /// ranks: a season is ranked among the division it was played in
    /// (Flashy: "just list the div of the season").
    fn played_divisions(&self) -> HashMap<(u32, i64), (i64, String, i64)> {
        let mut count: HashMap<(u32, i64), HashMap<(i64, i64), u32>> = HashMap::new();
        for (match_id, roster) in &self.rosters {
            let Some(m) = self.matches.get(match_id) else { continue };
            if !Self::played(m) || m.stage == "Cup" {
                continue;
            }
            let (_, Some(tier)) = self.division_of(m) else { continue };
            for (a, team, _) in roster {
                let Some(team) = team else { continue };
                *count.entry((*a, m.season)).or_default().entry((tier, *team)).or_default() += 1;
            }
        }
        count
            .into_iter()
            .filter_map(|(k, v)| {
                let ((tier, team), _) = v.into_iter().max_by_key(|(_, n)| *n)?;
                let name = crate::leagues::TIER_NAMES.get(tier as usize).map_or_else(String::new, |n| n.to_string());
                Some((k, (tier, name, team)))
            })
            .collect()
    }

    /// The best division that counted for them in any season.
    fn highest_of(divisions: &HashMap<(u32, i64), (i64, String, i64)>, account: u32) -> Option<Division> {
        divisions
            .iter()
            .filter(|((a, _), _)| *a == account)
            .min_by_key(|((_, season), (tier, _, _))| (*tier, -season))
            .map(|(_, (tier, name, _))| Division { name: name.clone(), tier: *tier })
    }

    fn name_of(&self, account: u32) -> String {
        let mut newest: Option<(i64, &str)> = None;
        for (match_id, roster) in &self.rosters {
            let t = self.matches.get(match_id).and_then(|m| m.time).unwrap_or(0);
            for (a, _, n) in roster {
                if *a == account && !n.is_empty() && newest.is_none_or(|(nt, _)| t > nt) {
                    newest = Some((t, n));
                }
            }
        }
        newest.map_or_else(|| SteamId::from_account_id(account).to_steamid64(), |(_, n)| n.to_string())
    }
}

/// A rated game's season: its ETF2L match's, where known, else by date.
/// Seasons overlap by date (AFA 2025 began before Season 34's playoffs were
/// over), and an official belongs to its competition's season.
fn game_season(cat: &Catalogue, windows: &[(i64, String, i64, i64)], g: &hl_db::RatedGame) -> Option<i64> {
    match g.etf2l_match_id.and_then(|id| cat.matches.get(&id)) {
        // A cup is an official but not the season's league: not ranked.
        Some(m) if m.stage == "Cup" => None,
        Some(m) => Some(m.season),
        None => season_of(windows, g.played_at),
    }
}

fn season_of(windows: &[(i64, String, i64, i64)], at: i64) -> Option<i64> {
    windows.iter().find(|(_, _, a, b)| at >= *a && at <= *b).map(|(s, _, _, _)| *s)
}

fn groups_of(g: &hl_db::RatedGame) -> Vec<(String, f64)> {
    if let Some(groups) = &g.groups {
        return serde_json::from_str::<serde_json::Map<String, Value>>(groups)
            .map(|m| m.into_iter().filter_map(|(k, v)| Some((k, v.as_f64()?))).collect())
            .unwrap_or_default();
    }
    let Some(parts) = g.parts.as_deref().and_then(|p| serde_json::from_str::<Vec<hl_rating::model::Part>>(p).ok()) else { return Vec::new() };
    hl_rating::guide::group_scores(&parts).into_iter().map(|(g, v)| (g.key().to_string(), v)).collect()
}

fn mean(xs: impl Iterator<Item = f64>) -> Option<f64> {
    let (s, n) = xs.fold((0.0, 0usize), |(s, n), x| (s + x, n + 1));
    (n > 0).then(|| s / n as f64)
}

/// One season, division and class, everyone with enough officials, best
/// first: `(account, officials, average rating, team)`. The average is over
/// their logs; the threshold is over officials (see [`MIN_RANK_GAMES`]).
fn rank_table(
    cat: &Catalogue,
    games: &[hl_db::RatedGame],
    windows: &[(i64, String, i64, i64)],
    divisions: &HashMap<(u32, i64), (i64, String, i64)>,
    season: i64,
    tier: i64,
    class: &str,
) -> Vec<(u32, usize, f64, i64)> {
    // account -> (scores, officials)
    let mut by: HashMap<u32, (Vec<f64>, std::collections::HashSet<i64>)> = HashMap::new();
    // Officials only: the sample holds nothing else, so a player's pugs and
    // scrims would put them on a different footing from everyone else's.
    for g in games.iter().filter(|g| g.class == class && g.official) {
        if game_season(cat, windows, g) != Some(season) {
            continue;
        }
        if divisions.get(&(g.account_id, season)).is_some_and(|(t, _, _)| *t == tier) {
            let e = by.entry(g.account_id).or_default();
            e.0.push(g.score);
            e.1.insert(g.etf2l_match_id.unwrap_or(-g.log_id));
        }
    }
    let mut rows: Vec<(u32, usize, f64, i64)> = by
        .into_iter()
        .filter(|(_, (_, officials))| officials.len() >= MIN_RANK_GAMES)
        .map(|(a, (v, officials))| (a, officials.len(), v.iter().sum::<f64>() / v.len() as f64, divisions[&(a, season)].2))
        .collect();
    rows.sort_by(|a, b| b.2.total_cmp(&a.2));
    rows
}

/// One player's ratings per class, stat bars and ranks.
pub async fn player_stats(db: &Db, account: u32) -> Result<PlayerStats> {
    let version = hl_rating::MODEL_VERSION;
    let mine = db.rated_games(version, Some(account)).await?;
    let mut by_class: HashMap<&str, Vec<&hl_db::RatedGame>> = HashMap::new();
    for g in &mine {
        by_class.entry(g.class.as_str()).or_default().push(g);
    }
    let order = |k: &str| hl_rating::guide::Group::ALL.iter().position(|g| g.key() == k).unwrap_or(9);
    let mut classes: Vec<ClassStats> = Vec::new();
    for (class, games) in by_class {
        let newest = games.iter().map(|g| g.played_at).max().unwrap_or(0);
        let recent: Vec<&hl_db::RatedGame> = games.iter().copied().filter(|g| g.played_at >= newest - RECENT_S).collect();
        let use_recent = recent.len() >= MIN_RECENT_GAMES;
        let pool: &[&hl_db::RatedGame] = if use_recent { &recent } else { &games };
        let mut sums: BTreeMap<String, (f64, usize)> = BTreeMap::new();
        for g in pool {
            for (k, v) in groups_of(g) {
                let e = sums.entry(k).or_default();
                e.0 += v;
                e.1 += 1;
            }
        }
        let mut groups: Vec<(String, f64)> = sums.into_iter().map(|(k, (s, n))| (k, s / n as f64)).collect();
        groups.sort_by_key(|(k, _)| order(k));
        classes.push(ClassStats {
            class: class.to_string(),
            games: games.len(),
            career: mean(games.iter().map(|g| g.score)).unwrap_or(0.0),
            recent: mean(recent.iter().map(|g| g.score)).filter(|_| use_recent),
            recent_games: recent.len(),
            best: games.iter().map(|g| g.score).fold(f64::MIN, f64::max),
            groups,
            groups_recent: use_recent,
        });
    }
    classes.sort_by(|a, b| b.games.cmp(&a.games));

    // Ranks: every season they played officials in, among the division they
    // played, on every class they played enough that season.
    let cat = Catalogue::load(db).await?;
    let windows = cat.season_windows();
    let divisions = cat.played_divisions();
    let mut theirs: BTreeMap<(i64, String), std::collections::HashSet<i64>> = BTreeMap::new();
    for g in mine.iter().filter(|g| g.official) {
        if let Some(s) = game_season(&cat, &windows, g) {
            theirs.entry((s, g.class.clone())).or_default().insert(g.etf2l_match_id.unwrap_or(-g.log_id));
        }
    }
    let mut ranks = Vec::new();
    if theirs.values().any(|n| n.len() >= MIN_RANK_GAMES) {
        let everyone = db.rated_games(version, None).await?;
        for ((season, class), n) in theirs {
            if n.len() < MIN_RANK_GAMES {
                continue;
            }
            let Some((tier, division, _)) = divisions.get(&(account, season)).cloned() else { continue };
            let table = rank_table(&cat, &everyone, &windows, &divisions, season, tier, &class);
            if let Some(i) = table.iter().position(|r| r.0 == account) {
                let name = windows.iter().find(|w| w.0 == season).map(|w| w.1.clone()).unwrap_or_default();
                ranks.push(Rank { season, season_name: name, division, tier, class, rank: i + 1, of: table.len(), avg: table[i].2, games: table[i].1 });
            }
        }
    }
    ranks.sort_by(|a, b| season_order(b.season).cmp(&season_order(a.season)).then(a.rank.cmp(&b.rank)));
    Ok(PlayerStats { classes, ranks })
}

/// A season's ranking for one division and class; the newest season with
/// rated games, and the top tier, when not given.
pub async fn rankings(db: &Db, season: Option<i64>, tier: Option<i64>, class: &str) -> Result<Rankings> {
    let cat = Catalogue::load(db).await?;
    let windows = cat.season_windows();
    let divisions = cat.played_divisions();
    let everyone = db.rated_games(hl_rating::MODEL_VERSION, None).await?;
    let seasons: Vec<(i64, String)> = windows
        .iter()
        .filter(|(_, _, a, b)| everyone.iter().any(|g| g.played_at >= *a && g.played_at <= *b))
        .map(|(s, n, _, _)| (*s, n.clone()))
        .collect();
    let season = season.or_else(|| seasons.first().map(|s| s.0)).unwrap_or(0);
    let mut tiers: BTreeMap<i64, String> = BTreeMap::new();
    for ((_, s), (t, d, _)) in &divisions {
        if *s == season {
            tiers.entry(*t).or_insert_with(|| d.clone());
        }
    }
    let tier = tier.or_else(|| tiers.keys().next().copied()).unwrap_or(0);
    let rows = rank_table(&cat, &everyone, &windows, &divisions, season, tier, class)
        .into_iter()
        .enumerate()
        .map(|(i, (a, n, avg, team))| RankRow { rank: i + 1, account_id: a, name: cat.name_of(a), team: Some(cat.team(team)), games: n, avg })
        .collect();
    Ok(Rankings {
        season,
        season_name: seasons.iter().find(|s| s.0 == season).map(|s| s.1.clone()).unwrap_or_default(),
        division: tiers.get(&tier).cloned().unwrap_or_default(),
        tier,
        class: class.to_string(),
        rows,
        seasons,
        divisions: tiers.into_iter().collect(),
    })
}

// ---- The division of the people you play (Q38) ----------------------------

/// How far from a season a match may be and still take its division.
const NEAREST_SEASON_S: i64 = 365 * 24 * 3600;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerDivision {
    pub tier: i64,
    pub division: String,
    pub season: i64,
    /// The match was inside that season; false when it was between seasons
    /// or in one the player did not play, and the nearest was used.
    pub exact: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchDivisions {
    /// Account -> their division at the time of the match.
    pub players: HashMap<u32, PlayerDivision>,
    /// Tier -> its newest name, to name a side's average.
    pub tier_names: BTreeMap<i64, String>,
}

/// Every player's ETF2L division at the time of a match (Q38): the season
/// the match was in, else the nearest season they played within a year.
/// A player's division at a time: the season it fell in, else their nearest
/// season within [`NEAREST_SEASON_S`]. `(tier, division, season, exact)`.
fn division_at(
    windows: &[(i64, String, i64, i64)],
    divisions: &HashMap<(u32, i64), (i64, String, i64)>,
    account: u32,
    at: i64,
) -> Option<(i64, String, i64, bool)> {
    windows
        .iter()
        .filter_map(|(s, _, from, to)| {
            let (tier, division, _) = divisions.get(&(account, *s))?;
            let gap = if at < *from { from - at } else if at > *to { at - to } else { 0 };
            (gap <= NEAREST_SEASON_S).then(|| (gap, *s, *tier, division.clone()))
        })
        .min_by_key(|(gap, s, ..)| (*gap, -s))
        .map(|(gap, s, tier, division)| (tier, division, s, gap == 0))
}

/// Each game's opposite number's division, for the profile's "Who you
/// played" by division: `(log_id, their account, when)` in, `log_id ->
/// (tier, today's name for it)` out.
pub async fn opponent_divisions(db: &Db, games: &[(i64, u32, i64)]) -> Result<HashMap<i64, (i64, String)>> {
    if games.is_empty() {
        return Ok(HashMap::new());
    }
    let cat = Catalogue::load(db).await?;
    let windows = cat.season_windows();
    let divisions = cat.divisions();
    Ok(games
        .iter()
        .filter_map(|(log_id, account, at)| {
            let (tier, ..) = division_at(&windows, &divisions, *account, *at)?;
            let name = crate::leagues::TIER_NAMES.get(tier as usize)?.to_string();
            Some((*log_id, (tier, name)))
        })
        .collect())
}

pub async fn match_divisions(db: &Db, log_id: i64) -> Result<MatchDivisions> {
    let Some((played_at, accounts)) = db.match_accounts(log_id).await? else {
        return Ok(MatchDivisions { players: HashMap::new(), tier_names: BTreeMap::new() });
    };
    let cat = Catalogue::load(db).await?;
    let windows = cat.season_windows();
    let divisions = cat.divisions();
    let mut players = HashMap::new();
    for a in accounts {
        if let Some((tier, division, season, exact)) = division_at(&windows, &divisions, a, played_at) {
            players.insert(a, PlayerDivision { tier, division, season, exact });
        }
    }
    // Every tier by today's name: "High", not the older "Division 1".
    let tier_names: BTreeMap<i64, String> = crate::leagues::TIER_NAMES.iter().enumerate().map(|(t, n)| (t as i64, n.to_string())).collect();
    Ok(MatchDivisions { players, tier_names })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mvps_come_from_the_final_and_the_event_goes_to_the_winners() {
        // Semi-final 1 (team 11 beat 10), Grand Final 3 (11 beat 12, 6-3).
        let mut c = cat(vec![
            m(1, 34, "Playoffs", None, "Mid", Some("Semi-finals"), 10, 11, 3, 6),
            m(3, 34, "Playoffs", None, "Mid", Some("Grand Final"), 12, 11, 3, 6),
        ]);
        c.rosters.get_mut(&3).unwrap().extend([(211, Some(11), "medic".to_string()), (311, Some(11), "sub".to_string())]);
        let g = |account: u32, log_id: i64, class: &str, score: f64, match_id: i64| hl_db::RatedGame {
            account_id: account,
            log_id,
            played_at: 0,
            class: class.into(),
            score,
            parts: None,
            groups: None,
            official: true,
            etf2l_match_id: Some(match_id),
        };
        let games = vec![
            // The losing Sniper had the better final; the winners' Medic the
            // best of the team that won.
            g(12, 31, "sniper", 1.5, 3),
            g(12, 32, "sniper", 1.5, 3),
            g(11, 31, "sniper", 1.2, 3),
            g(11, 32, "sniper", 1.2, 3),
            g(11, 10, "sniper", 0.9, 1),
            g(211, 31, "medic", 1.3, 3),
            g(211, 32, "medic", 1.3, 3),
            // A sub with one great half of two: not a candidate.
            g(311, 31, "scout", 2.0, 3),
        ];
        let v = c.mvps(&games);
        let sniper = v.iter().find(|x| x.class == "sniper" && !x.event).unwrap();
        assert_eq!((sniper.account_id, sniper.won), (12, false), "the class MVP can come from the losing finalist");
        let won_sniper = 0.7 * 1.2 + 0.3 * 0.9;
        assert!(won_sniper < 1.3);
        let event = v.iter().find(|x| x.event).unwrap();
        assert_eq!((event.account_id, event.class.as_str(), event.won), (211, "medic", true));
        assert!(!v.iter().any(|x| x.account_id == 311), "more than half the final to be a candidate");
        assert_eq!(event.final_maps, 2);
    }

    #[test]
    fn every_way_etf2l_named_a_playoff_round_is_read() {
        use RoundKind::*;
        for (r, k) in [
            ("Grand Final", Some(Final)),
            ("Grand-Final", Some(Final)),
            ("Division 1 Grand Final", Some(Final)),
            ("Premiership Grand Final", Some(Final)),
            ("Final", Some(Final)),
            ("3rd Place ", Some(Third)),
            ("3rd Place Match", Some(Third)),
            ("Semi-Finals", Some(Semi)),
            ("Division 2 Semi-Finals", Some(Semi)),
            ("Lower Bracket Final", Some(LowerFinal)),
            ("Upper Bracket Final", None),
            ("Week 3", None),
            ("Round 4", None),
        ] {
            assert_eq!(Catalogue::round_kind(Some(r)), k, "{r}");
        }
    }

    #[test]
    fn a_numbered_division_keeps_its_number_and_groups_fold() {
        assert_eq!(Catalogue::ungrouped("Division 2A"), "Division 2");
        assert_eq!(Catalogue::ungrouped("Division 2"), "Division 2");
        assert_eq!(Catalogue::ungrouped("Open B"), "Open");
        assert_eq!(Catalogue::ungrouped("Premiership Division"), "Premiership");
        assert_eq!(Catalogue::ungrouped("Mid"), "Mid");
        assert_eq!(Catalogue::numbered("Division 3"), Some(3));
        assert_eq!(Catalogue::numbered("Mid"), None);
    }

    fn m(id: i64, season: i64, stage: &str, div: Option<&str>, comp_div: &str, round: Option<&str>, c1: i64, c2: i64, r1: i64, r2: i64) -> CatMatch {
        CatMatch {
            match_id: id,
            competition_id: 1,
            season,
            season_name: format!("S{season}"),
            comp_name: String::new(),
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
        assert_eq!(seasons.iter().find(|s| s.season == 35).unwrap().tier, Some(3), "the playoffs are Low, tier 3 on the ladder");
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
    fn without_a_third_place_match_both_semi_final_losers_share_bronze() {
        // S34 Mid: two semi-finals, a Grand Final, no match for third.
        let c = cat(vec![
            m(1, 34, "Playoffs", None, "Mid", Some("Semi-finals"), 10, 11, 3, 6),
            m(2, 34, "Playoffs", None, "Mid", Some("Semi-finals"), 12, 13, 6, 0),
            m(3, 34, "Playoffs", None, "Mid", Some("Grand Final"), 12, 11, 3, 6),
        ]);
        let mid = &c.medals()[&(34, "Mid".to_string())];
        let bronze: Vec<i64> = mid.iter().filter(|x| x.0 == 3).map(|x| x.1).collect();
        assert_eq!(bronze.len(), 2);
        assert!(bronze.contains(&10) && bronze.contains(&13), "both losing semi-finalists");
        assert!(mid.contains(&(3, 10, "joint 3rd: semi-final 3-6".into())));
        // With a 3rd-place match, only its winner.
        let c = cat(vec![
            m(1, 34, "Playoffs", None, "Mid", Some("Semi-finals"), 10, 11, 3, 6),
            m(2, 34, "Playoffs", None, "Mid", Some("Semi-finals"), 12, 13, 6, 0),
            m(3, 34, "Playoffs", None, "Mid", Some("Grand Final"), 12, 11, 3, 6),
            m(4, 34, "3rd Place", None, "Mid", None, 10, 13, 6, 3),
        ]);
        let bronze: Vec<i64> = c.medals()[&(34, "Mid".to_string())].iter().filter(|x| x.0 == 3).map(|x| x.1).collect();
        assert_eq!(bronze, vec![10]);
    }

    #[test]
    fn a_final_with_no_division_takes_its_teams_division() {
        let c = cat(vec![
            m(1, 30, "regular", Some("Open B"), "", Some("Week 1"), 10, 11, 6, 0),
            m(2, 30, "regular", None, "", Some("Grand Final"), 10, 12, 5, 1),
        ]);
        assert!(c.medals()[&(30, "Open".to_string())].contains(&(1, 10, "Grand Final 5-1".into())));
    }

    #[test]
    fn a_grand_final_inside_the_season_counts_but_a_cups_does_not() {
        // Autumn 2023: the Grand Final is a round of the season itself.
        let c = cat(vec![
            m(1, 30, "regular", Some("Open B"), "", Some("Week 1"), 10, 11, 6, 0),
            m(2, 30, "regular", Some("Open B"), "", Some("Grand Final"), 10, 12, 5, 1),
            m(3, 31, "Cup", Some("Low A"), "", Some("Grand Final"), 10, 13, 6, 0),
        ]);
        let medals = c.medals();
        assert!(medals[&(30, "Open".to_string())].contains(&(1, 10, "Grand Final 5-1".into())), "Open B is Open on the ladder");
        assert!(!medals.contains_key(&(31, "Low A".to_string())), "a preseason cup is not the league");
    }

    /// Matches with their own tier, and one roster: player 1 for team 10.
    fn tiered(ms: Vec<(i64, i64, &str, Option<&str>, i64)>) -> Catalogue {
        let matches: Vec<CatMatch> = ms
            .into_iter()
            .map(|(id, season, stage, round, tier)| CatMatch {
                match_id: id,
                competition_id: 1,
                season,
                season_name: format!("S{season}"),
                comp_name: String::new(),
                comp_division: format!("t{tier}"),
                stage: stage.into(),
                division: (stage == "regular").then(|| format!("t{tier}")),
                tier: (stage == "regular").then_some(tier),
                round: round.map(str::to_string),
                time: Some(id),
                clan1: 10,
                clan2: 11,
                r1: Some(6),
                r2: Some(0),
                default_win: false,
            })
            .collect();
        let mut c = cat(matches);
        for r in c.rosters.values_mut() {
            r.retain(|(_, t, _)| *t == Some(10));
            r[0].0 = 1;
        }
        for m in c.matches.values() {
            if let Some(d) = &m.division {
                c.tier_by_name.insert(d.clone(), m.tier.unwrap());
            }
        }
        c
    }

    #[test]
    fn a_division_counts_from_three_officials() {
        // Two Premiership mercs and nine High games: High, not Premiership.
        let mut ms: Vec<(i64, i64, &str, Option<&str>, i64)> = vec![(1, 30, "regular", None, 0), (2, 30, "regular", None, 0)];
        ms.extend((3..12).map(|id| (id, 30, "regular", None, 1)));
        let c = tiered(ms);
        let d = c.divisions();
        assert_eq!(d[&(1, 30)].0, 1);
        assert_eq!(Catalogue::highest_of(&d, 1).map(|x| x.tier), Some(1));
        // Two officials anywhere: no division at all.
        let c = tiered(vec![(1, 31, "regular", None, 2), (2, 31, "regular", None, 2)]);
        assert!(c.divisions().is_empty());
    }

    #[test]
    fn a_grand_final_counts_for_the_division_above() {
        // Four Mid games and the Mid Grand Final: High that season.
        let mut ms: Vec<(i64, i64, &str, Option<&str>, i64)> = (1..5).map(|id| (id, 32, "regular", None, 2)).collect();
        ms.push((5, 32, "Playoffs", Some("Grand Final"), 2));
        let c = tiered(ms);
        assert_eq!(c.divisions()[&(1, 32)].0, 1, "a Mid finalist counts as High");
    }
}
