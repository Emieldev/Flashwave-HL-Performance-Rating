//! Teams and seasons (Q29, Flashy): a year of ETF2L Highlander, every team.
//!
//! "Add a teams page per season... look at their win percentages on each map
//! in the map pool. List their best players... profile pages for teams."
//! All of it from ETF2L's own API, which lists each season's divisions,
//! their map pool, results and, per match, who played and each map's score.
//! The player ratings on a team page are the pool's (players who appear in
//! the owner's matches); a whole team's rated history is Q14b.

use crate::sources::Sources;
use anyhow::Result;
use hl_core::SteamId;
use hl_db::{CompetitionRow, Db, SeasonMatchRow};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

/// Seasons kept besides the current one: two before it is about a year.
pub const SEASONS_BACK: i64 = 2;
/// Match pages read per sync. ~450 matches a year at ETF2L's 60 a minute
/// would add eight minutes to one sync; spread out, a year fills in over a
/// few, and a live season keeps up easily.
pub const DETAILS_PER_SYNC: i64 = 60;
/// Competition-list pages read at most: a guard, not a limit.
const MAX_LIST_PAGES: i64 = 25;

/// `"Highlander Season 36 (Autumn 2026): Open Playoffs"` ->
/// `(36, "Autumn 2026", "Open", "Playoffs")`. Seasons before 32 are named
/// without the brackets -- `"Highlander Season 22: Premiership Qualifiers"`,
/// or just `"Highlander Season 22"` -- and read as season `"Season 22"`.
pub fn parse_name(name: &str) -> Option<(i64, String, String, String)> {
    let rest = name.strip_prefix("Highlander Season ")?;
    let digits = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    let season: i64 = rest[..digits].parse().ok()?;
    let rest = rest[digits..].trim_start();
    let (season_name, rest) = match rest.strip_prefix('(') {
        Some(r) => {
            let (n, r) = r.split_once(')')?;
            (n.to_string(), r)
        }
        None => (format!("Season {season}"), rest),
    };
    let label = rest.trim_start_matches(':').trim();
    const STAGES: [&str; 6] = ["Grand Final", "3rd Place", "Playoffs", "Final", "Relegation", "Qualifiers"];
    let (division, stage) = STAGES
        .iter()
        .find_map(|s| label.strip_suffix(s).map(|d| (d.trim().to_string(), (*s).to_string())))
        .unwrap_or_else(|| (label.to_string(), "regular".to_string()));
    Some((season, season_name, if division.is_empty() { label.to_string() } else { division }, stage))
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchSummary {
    pub competitions: usize,
    pub results: usize,
    pub details: usize,
    pub failed: usize,
}

fn team_of(v: &Value) -> Option<(i64, String, Option<String>, Option<String>)> {
    Some((
        v.get("id")?.as_i64()?,
        v.get("name")?.as_str()?.to_string(),
        v.get("country").and_then(Value::as_str).map(str::to_string),
        v.get("steam").and_then(|s| s.get("avatar")).and_then(Value::as_str).map(str::to_string),
    ))
}

/// Refresh the last year's Highlander seasons. Best effort: ETF2L being
/// down costs this sync's refresh, nothing else.
pub async fn fetch(db: &Db, sources: &Sources, progress: impl FnMut(usize, usize)) -> Result<FetchSummary> {
    fetch_seasons(db, sources, SEASONS_BACK, DETAILS_PER_SYNC, progress).await
}

/// [`fetch`] over `seasons_back` seasons before the newest, reading up to
/// `details` match pages from the last [`SEASONS_BACK`]. The league sample
/// asks for years of results and reads its own matches' pages.
pub async fn fetch_seasons(
    db: &Db,
    sources: &Sources,
    seasons_back: i64,
    details: i64,
    mut progress: impl FnMut(usize, usize),
) -> Result<FetchSummary> {
    let mut out = FetchSummary::default();

    // 1. Which competitions: newest first, until the seasons run out.
    let mut found: Vec<(i64, String, bool)> = Vec::new();
    let mut newest: Option<i64> = None;
    for page in 1..=MAX_LIST_PAGES {
        let Some(body) = sources.etf2l_get(&format!("/competition/list?page={page}")).await? else { break };
        // ETF2L answers a busy moment with an HTML page: the walk ends
        // there, and what it found so far is still read.
        let Ok(v) = serde_json::from_str::<Value>(&body) else {
            tracing::warn!(page, "ETF2L competition list page was not JSON; stopping there");
            break;
        };
        let data = v.pointer("/competitions/data").and_then(Value::as_array).cloned().unwrap_or_default();
        if data.is_empty() {
            break;
        }
        // Other formats and cups are mixed in; the walk ends at a page with
        // an older Highlander season on it and none of the wanted ones.
        let (mut wanted, mut older) = (false, false);
        for c in &data {
            let (Some(id), Some(name)) = (c.get("id").and_then(Value::as_i64), c.get("name").and_then(Value::as_str)) else { continue };
            if c.get("category").and_then(Value::as_str) != Some("Highlander Season") {
                continue;
            }
            let Some((season, ..)) = parse_name(name) else { continue };
            let top = *newest.get_or_insert(season);
            if season >= top - seasons_back {
                wanted = true;
                found.push((id, name.to_string(), c.get("archived").and_then(Value::as_bool).unwrap_or(false)));
            } else {
                older = true;
            }
        }
        if older && !wanted {
            break;
        }
    }

    // 2. Each competition's pool and results. An archived one already held is
    // settled and not asked for again.
    let held: HashMap<i64, bool> = db.competitions().await?.into_iter().map(|c| (c.competition_id, c.archived)).collect();
    let todo: Vec<&(i64, String, bool)> = found.iter().filter(|(id, _, archived)| !(*archived && held.get(id) == Some(&true))).collect();
    for (i, (id, name, archived)) in todo.iter().enumerate() {
        progress(i, todo.len());
        let Some((season, season_name, division, stage)) = parse_name(name) else { continue };
        let pool = match sources.etf2l_get(&format!("/competition/{id}")).await {
            Ok(Some(body)) => serde_json::from_str::<Value>(&body).ok().and_then(|v| v.pointer("/competition/pool").map(|p| p.to_string())),
            _ => None,
        };
        db.upsert_competition(&CompetitionRow {
            competition_id: *id,
            season,
            season_name: &season_name,
            division: &division,
            stage: &stage,
            name,
            archived: *archived,
            pool: pool.as_deref(),
        })
        .await?;
        out.competitions += 1;
        for page in 1..=20 {
            let Some(body) = sources.etf2l_get(&format!("/competition/{id}/results?page={page}")).await? else { break };
            let Ok(v) = serde_json::from_str::<Value>(&body) else {
                tracing::warn!(competition = id, page, "ETF2L results page was not JSON; next refresh reads it");
                break;
            };
            let data = v.pointer("/results/data").and_then(Value::as_array).cloned().unwrap_or_default();
            for m in &data {
                let (Some(c1), Some(c2)) = (m.get("clan1").and_then(team_of), m.get("clan2").and_then(team_of)) else { continue };
                let Some(match_id) = m.get("id").and_then(Value::as_i64) else { continue };
                for (tid, tname, country, avatar) in [&c1, &c2] {
                    db.upsert_etf2l_team(*tid, tname, country.as_deref(), avatar.as_deref()).await?;
                }
                let maps = m.get("maps").map(|x| x.to_string()).unwrap_or_else(|| "[]".into());
                db.upsert_season_match(&SeasonMatchRow {
                    match_id,
                    competition_id: *id,
                    division: m.pointer("/division/name").and_then(Value::as_str),
                    tier: m.pointer("/division/tier").and_then(Value::as_i64),
                    week: m.get("week").and_then(Value::as_i64),
                    round: m.get("round").and_then(Value::as_str),
                    time: m.get("time").and_then(Value::as_i64),
                    clan1_id: c1.0,
                    clan2_id: c2.0,
                    r1: m.get("r1").and_then(Value::as_i64),
                    r2: m.get("r2").and_then(Value::as_i64),
                    default_win: m.get("defaultwin").and_then(Value::as_bool).unwrap_or(false),
                    maps: &maps,
                })
                .await?;
                out.results += 1;
            }
            let last = v.pointer("/results/last_page").and_then(Value::as_i64).unwrap_or(1);
            if page >= last {
                break;
            }
        }
    }

    // 3. Match pages: who played, and each map's score.
    let recent = newest.unwrap_or(0) - SEASONS_BACK;
    for match_id in db.season_matches_without_detail(details, recent).await? {
        if !fetch_match_detail(db, sources, match_id).await? {
            out.failed += 1;
            continue;
        }
        out.details += 1;
    }
    Ok(out)
}

/// Read one match's page: each map's score and who played. False when
/// ETF2L would not give it.
pub async fn fetch_match_detail(db: &Db, sources: &Sources, match_id: i64) -> Result<bool> {
    {
        let body = match sources.etf2l_get(&format!("/matches/{match_id}")).await {
            Ok(Some(b)) => b,
            _ => return Ok(false),
        };
        let Ok(v) = serde_json::from_str::<Value>(&body) else {
            return Ok(false);
        };
        let m = v.get("match").unwrap_or(&Value::Null);
        let maps: Vec<(i64, String, i64, i64, bool)> = m
            .get("map_results")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|r| {
                Some((
                    r.get("match_order")?.as_i64()?,
                    r.get("map")?.as_str()?.to_string(),
                    r.get("clan1")?.as_i64()?,
                    r.get("clan2")?.as_i64()?,
                    r.get("golden_cap").and_then(Value::as_bool).unwrap_or(false),
                ))
            })
            .collect();
        let players: Vec<(u32, Option<i64>, String)> = m
            .get("players")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|p| {
                let id3 = p.pointer("/steam/id3")?.as_str()?;
                let account = SteamId::parse(id3).ok()?.account_id();
                Some((account, p.get("team_id").and_then(Value::as_i64), p.get("name").and_then(Value::as_str).unwrap_or("").to_string()))
            })
            .collect();
        let maps_ref: Vec<(i64, &str, i64, i64, bool)> = maps.iter().map(|(o, m, a, b, g)| (*o, m.as_str(), *a, *b, *g)).collect();
        let players_ref: Vec<(u32, Option<i64>, &str)> = players.iter().map(|(a, t, n)| (*a, *t, n.as_str())).collect();
        db.put_season_match_detail(match_id, &maps_ref, &players_ref).await?;
    }
    Ok(true)
}

// ---- views ------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonInfo {
    pub season: i64,
    pub name: String,
    pub divisions: Vec<String>,
    pub pool: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub played: u32,
    pub won: u32,
    pub lost: u32,
    pub drawn: u32,
}

impl Record {
    fn add(&mut self, us: i64, them: i64) {
        self.played += 1;
        match us.cmp(&them) {
            std::cmp::Ordering::Greater => self.won += 1,
            std::cmp::Ordering::Less => self.lost += 1,
            std::cmp::Ordering::Equal => self.drawn += 1,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableRow {
    pub team_id: i64,
    pub name: String,
    pub avatar: Option<String>,
    pub record: Record,
    /// ETF2L's own score, summed: what the league table ranks on.
    pub score_for: i64,
    pub score_against: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DivisionTable {
    pub division: String,
    pub teams: Vec<TableRow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonView {
    pub seasons: Vec<SeasonInfo>,
    pub season: Option<SeasonInfo>,
    pub divisions: Vec<DivisionTable>,
    /// Matches still to be read in detail, for saying the maps are partial.
    pub pending_details: usize,
}

fn seasons_of(comps: &[hl_db::Competition]) -> Vec<SeasonInfo> {
    let mut by: BTreeMap<i64, SeasonInfo> = BTreeMap::new();
    for c in comps {
        let s = by.entry(c.season).or_insert_with(|| SeasonInfo { season: c.season, name: c.season_name.clone(), divisions: Vec::new(), pool: Vec::new() });
        if s.pool.is_empty() {
            s.pool = c.pool.clone();
        }
    }
    by.into_values().rev().collect()
}

/// One season's division tables, regular season only. `None` picks the newest.
pub async fn season(db: &Db, season: Option<i64>) -> Result<SeasonView> {
    let comps = db.competitions().await?;
    let seasons = seasons_of(&comps);
    let Some(chosen) = season.and_then(|s| seasons.iter().find(|x| x.season == s)).or(seasons.first()).cloned() else {
        return Ok(SeasonView { seasons, season: None, divisions: Vec::new(), pending_details: 0 });
    };
    let matches = db.season_matches(Some(chosen.season), None).await?;
    // Divisions from the matches, top tier first.
    let mut order: Vec<(i64, String)> = Vec::new();
    for m in matches.iter().filter(|m| m.stage == "regular" && !m.division.is_empty()) {
        let tier = m.tier.unwrap_or(99);
        match order.iter_mut().find(|(_, d)| *d == m.division) {
            Some(o) => o.0 = o.0.min(tier),
            None => order.push((tier, m.division.clone())),
        }
    }
    order.sort();
    let mut chosen = chosen;
    chosen.divisions = order.into_iter().map(|(_, d)| d).collect();
    let mut tables: BTreeMap<String, HashMap<i64, TableRow>> = BTreeMap::new();
    for m in matches.iter().filter(|m| m.stage == "regular") {
        let (Some(r1), Some(r2)) = (m.r1, m.r2) else { continue };
        let t = tables.entry(m.division.clone()).or_default();
        for (id, name, us, them) in [(m.clan1_id, &m.clan1_name, r1, r2), (m.clan2_id, &m.clan2_name, r2, r1)] {
            let row = t.entry(id).or_insert_with(|| TableRow { team_id: id, name: name.clone(), avatar: None, record: Record::default(), score_for: 0, score_against: 0 });
            row.record.add(us, them);
            row.score_for += us;
            row.score_against += them;
        }
    }
    let mut divisions: Vec<DivisionTable> = Vec::new();
    for div in &chosen.divisions {
        let Some(t) = tables.remove(div) else { continue };
        let mut teams: Vec<TableRow> = t.into_values().collect();
        for row in &mut teams {
            row.avatar = db.etf2l_team(row.team_id).await?.and_then(|(_, _, a)| a);
        }
        teams.sort_by_key(|t| std::cmp::Reverse((t.record.won, t.score_for - t.score_against)));
        divisions.push(DivisionTable { division: div.clone(), teams });
    }
    let recent = db.newest_etf2l_season().await?.unwrap_or(0) - SEASONS_BACK;
    let pending_details = db.season_matches_without_detail(10_000, recent).await?.len();
    Ok(SeasonView { seasons, season: Some(chosen), divisions, pending_details })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapRecord {
    pub map: String,
    pub record: Record,
    pub rounds_for: i64,
    pub rounds_against: i64,
    /// Whether it is in the newest season's pool.
    pub in_pool: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultRow {
    pub match_id: i64,
    pub season: i64,
    pub division: String,
    pub stage: String,
    pub round: Option<String>,
    pub time: Option<i64>,
    pub opponent_id: i64,
    pub opponent: String,
    pub score_for: Option<i64>,
    pub score_against: Option<i64>,
    pub default_win: bool,
    pub maps: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RosterRow {
    pub account_id: u32,
    pub name: String,
    pub matches: i64,
    pub last_played: i64,
    /// Their most played class in the pool, games on it, and average rating.
    pub class: Option<String>,
    pub games: i64,
    pub rating: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamView {
    pub team_id: i64,
    pub name: String,
    pub country: Option<String>,
    pub avatar: Option<String>,
    /// `(season, division)` played in, newest first.
    pub seasons: Vec<(i64, String)>,
    pub record: Record,
    pub maps: Vec<MapRecord>,
    pub results: Vec<ResultRow>,
    pub roster: Vec<RosterRow>,
}

pub async fn team(db: &Db, team_id: i64) -> Result<Option<TeamView>> {
    let Some((name, country, avatar)) = db.etf2l_team(team_id).await? else { return Ok(None) };
    let comps = db.competitions().await?;
    let pool: Vec<String> = seasons_of(&comps).first().map(|s| s.pool.clone()).unwrap_or_default();
    let matches = db.season_matches(None, Some(team_id)).await?;
    let map_rows = db.season_maps(Some(team_id)).await?;
    let mut by_match: HashMap<i64, Vec<&hl_db::SeasonMap>> = HashMap::new();
    for r in &map_rows {
        by_match.entry(r.match_id).or_default().push(r);
    }

    let mut record = Record::default();
    let mut maps: BTreeMap<String, MapRecord> = BTreeMap::new();
    let mut seasons: Vec<(i64, String)> = Vec::new();
    let mut results = Vec::new();
    for m in &matches {
        let first = m.clan1_id == team_id;
        let (us, them) = if first { (m.r1, m.r2) } else { (m.r2, m.r1) };
        let (opp_id, opp) = if first { (m.clan2_id, m.clan2_name.clone()) } else { (m.clan1_id, m.clan1_name.clone()) };
        if !seasons.iter().any(|(s, d)| *s == m.season && *d == m.division) {
            seasons.push((m.season, m.division.clone()));
        }
        if let (Some(a), Some(b)) = (us, them) {
            record.add(a, b);
            // Per map from the match page where read; otherwise the match's
            // map, which in Highlander is one map played twice.
            match by_match.get(&m.match_id) {
                Some(rows) if !m.default_win => {
                    // A stopwatch map is played as two halves, each its own
                    // row: one map is their sum, won or lost once.
                    let mut per_map: BTreeMap<&str, (i64, i64)> = BTreeMap::new();
                    for r in rows {
                        let (f, g) = if first { (r.clan1, r.clan2) } else { (r.clan2, r.clan1) };
                        let e = per_map.entry(r.map.as_str()).or_default();
                        e.0 += f;
                        e.1 += g;
                    }
                    for (map, (f, g)) in per_map {
                        let e = maps.entry(map.to_string()).or_insert_with(|| MapRecord { map: map.to_string(), record: Record::default(), rounds_for: 0, rounds_against: 0, in_pool: false });
                        e.record.add(f, g);
                        e.rounds_for += f;
                        e.rounds_against += g;
                    }
                }
                _ if !m.default_win => {
                    if let Some(map) = m.maps.first() {
                        let e = maps.entry(map.clone()).or_insert_with(|| MapRecord { map: map.clone(), record: Record::default(), rounds_for: 0, rounds_against: 0, in_pool: false });
                        e.record.add(a, b);
                    }
                }
                _ => {}
            }
        }
        results.push(ResultRow {
            match_id: m.match_id,
            season: m.season,
            division: m.division.clone(),
            stage: m.stage.clone(),
            round: m.round.clone(),
            time: m.time,
            opponent_id: opp_id,
            opponent: opp,
            score_for: us,
            score_against: them,
            default_win: m.default_win,
            maps: m.maps.clone(),
        });
    }
    let mut maps: Vec<MapRecord> = maps.into_values().collect();
    for m in &mut maps {
        m.in_pool = pool.iter().any(|p| p.eq_ignore_ascii_case(&m.map));
    }
    maps.sort_by(|a, b| b.in_pool.cmp(&a.in_pool).then(b.record.played.cmp(&a.record.played)));

    let players = db.season_players(team_id).await?;
    let accounts: Vec<u32> = players.iter().map(|p| p.0).collect();
    let rated = db.pool_ratings(&accounts, hl_rating::MODEL_VERSION).await?;
    let roster = players
        .into_iter()
        .map(|(account_id, name, matches, last_played)| {
            let best = rated.iter().find(|r| r.0 == account_id);
            RosterRow {
                account_id,
                name,
                matches,
                last_played,
                class: best.map(|r| r.1.clone()),
                games: best.map_or(0, |r| r.2),
                rating: best.map(|r| r.3),
            }
        })
        .collect();

    Ok(Some(TeamView { team_id, name, country, avatar, seasons, record, maps, results, roster }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_competition_name_is_its_season_division_and_stage() {
        assert_eq!(parse_name("Highlander Season 36 (Autumn 2026): High"), Some((36, "Autumn 2026".into(), "High".into(), "regular".into())));
        assert_eq!(parse_name("Highlander Season 36 (Autumn 2026): Open Playoffs"), Some((36, "Autumn 2026".into(), "Open".into(), "Playoffs".into())));
        assert_eq!(parse_name("Highlander Season 36 (Autumn 2026): Low 3rd Place"), Some((36, "Autumn 2026".into(), "Low".into(), "3rd Place".into())));
        assert_eq!(parse_name("6v6 Season 50: Premiership"), None);
        // Before season 32: no season name in brackets.
        assert_eq!(parse_name("Highlander Season 22"), Some((22, "Season 22".into(), "".into(), "regular".into())));
        assert_eq!(parse_name("Highlander Season 22: Premiership Qualifiers"), Some((22, "Season 22".into(), "Premiership".into(), "Qualifiers".into())));
    }
}
