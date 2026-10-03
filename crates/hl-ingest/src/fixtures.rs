//! Upcoming officials and head-to-head records (Q48, Flashy; PLAN §27).
//!
//! ETF2L lists every scheduled match in one short list
//! (`/matches?scheduled=1`, a handful at a time even mid-season); it is
//! read at most every half hour and kept in `app_config`. Head-to-head
//! needs no request: every official between two teams is in the database.

use crate::sources::Sources;
use anyhow::Result;
use hl_db::Db;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const KEY: &str = "etf2l_fixtures";
const FRESH: i64 = 30 * 60;
/// A match that started up to three hours ago is still "on".
const GRACE: i64 = 3 * 3600;

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureTeam {
    pub id: i64,
    pub name: String,
    pub avatar: Option<String>,
}

/// One scheduled official, with the two teams' record against each other
/// (from `clan1`'s side).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fixture {
    pub match_id: i64,
    pub time: i64,
    pub competition: String,
    pub season: Option<i64>,
    pub division: Option<String>,
    pub tier: Option<i64>,
    pub round: Option<String>,
    pub week: Option<i64>,
    pub clan1: FixtureTeam,
    pub clan2: FixtureTeam,
    /// Empty while the maps are still to be picked ("variable").
    pub maps: Vec<String>,
    #[serde(default)]
    pub h2h: H2hRecord,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct H2hRecord {
    pub played: u32,
    pub won: u32,
    pub lost: u32,
    pub drawn: u32,
}

/// The Highlander matches in one page of `/matches?scheduled=1`, and the
/// number of pages.
pub fn parse_page(body: &str) -> Option<(Vec<Fixture>, i64)> {
    let v: Value = serde_json::from_str(body).ok()?;
    // The list comes back under "results" (not "matches" as /team/.../matches has it).
    let page = v.get("results").or_else(|| v.get("matches"))?;
    let last = page["last_page"].as_i64().unwrap_or(1);
    let team = |c: &Value| -> Option<FixtureTeam> {
        Some(FixtureTeam {
            id: c["id"].as_i64()?,
            name: c["name"].as_str().unwrap_or("").to_string(),
            avatar: c["steam"]["avatar"].as_str().filter(|a| !a.ends_with('/')).map(str::to_string),
        })
    };
    let mut out = Vec::new();
    for m in page["data"].as_array()? {
        let comp = &m["competition"];
        if comp["type"].as_str() != Some("Highlander") {
            continue;
        }
        let (Some(match_id), Some(time), Some(clan1), Some(clan2)) = (m["id"].as_i64(), m["time"].as_i64(), team(&m["clan1"]), team(&m["clan2"])) else { continue };
        let competition = comp["name"].as_str().unwrap_or("").to_string();
        let parsed = crate::leagues::parse_name(&competition);
        // A playoff competition has no division of its own: "Open Playoffs"
        // names it.
        let division = m["division"]["name"].as_str().map(str::to_string).or_else(|| parsed.as_ref().map(|p| p.2.clone()).filter(|d| !d.is_empty()));
        let tier = m["division"]["tier"].as_i64().or_else(|| division.as_deref().and_then(crate::leagues::canonical_tier));
        out.push(Fixture {
            match_id,
            time,
            season: parsed.map(|(s, ..)| s),
            competition,
            division,
            tier,
            round: m["round"].as_str().map(str::to_string),
            week: m["week"].as_i64(),
            clan1,
            clan2,
            maps: m["maps"].as_array().into_iter().flatten().filter_map(Value::as_str).filter(|x| *x != "variable").map(str::to_string).collect(),
            h2h: H2hRecord::default(),
        });
    }
    Some((out, last))
}

/// Every upcoming Highlander official, soonest first, each with its
/// head-to-head. Best effort: a failed read keeps the last list.
pub async fn upcoming(db: &Db, sources: &Sources) -> Result<Vec<Fixture>> {
    let held: Option<(i64, Vec<Fixture>)> = db.get_setting(KEY).await?.and_then(|v| serde_json::from_str(&v).ok());
    let mut list = match held {
        Some((at, list)) if now() - at < FRESH => list,
        held => match read_all(sources).await {
            Ok(list) => {
                db.set_setting(KEY, &serde_json::to_string(&(now(), &list))?).await?;
                list
            }
            Err(e) => {
                tracing::warn!(error = %format!("{e:#}"), "reading ETF2L's scheduled matches failed");
                held.map(|(_, l)| l).unwrap_or_default()
            }
        },
    };
    list.retain(|f| f.time > now() - GRACE);
    list.sort_by_key(|f| f.time);
    for f in &mut list {
        f.h2h = record(&db.head_to_head(f.clan1.id, f.clan2.id).await?);
    }
    Ok(list)
}

async fn read_all(sources: &Sources) -> Result<Vec<Fixture>> {
    let mut out = Vec::new();
    let mut page = 1;
    loop {
        let Some(body) = sources.etf2l_get(&format!("/matches?scheduled=1&page={page}")).await? else { break };
        let Some((rows, last)) = parse_page(&body) else { anyhow::bail!("ETF2L's scheduled matches did not parse") };
        out.extend(rows);
        if page >= last || page >= 10 {
            break;
        }
        page += 1;
    }
    Ok(out)
}

/// One official between two teams, from the first team's side.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct H2hMatch {
    pub match_id: i64,
    pub time: Option<i64>,
    pub season: i64,
    pub division: Option<String>,
    pub round: Option<String>,
    pub score_for: Option<i64>,
    pub score_against: Option<i64>,
    pub default_win: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadToHead {
    pub record: H2hRecord,
    /// Newest first.
    pub matches: Vec<H2hMatch>,
}

fn record(ms: &[hl_db::H2hRow]) -> H2hRecord {
    let mut r = H2hRecord::default();
    for m in ms.iter().filter(|m| !m.default_win) {
        let (Some(a), Some(b)) = (m.score_a, m.score_b) else { continue };
        if a + b == 0 {
            continue;
        }
        r.played += 1;
        match a.cmp(&b) {
            std::cmp::Ordering::Greater => r.won += 1,
            std::cmp::Ordering::Less => r.lost += 1,
            std::cmp::Ordering::Equal => r.drawn += 1,
        }
    }
    r
}

/// Every official between two teams, from `a`'s side.
pub async fn head_to_head(db: &Db, a: i64, b: i64) -> Result<HeadToHead> {
    let rows = db.head_to_head(a, b).await?;
    Ok(HeadToHead {
        record: record(&rows),
        matches: rows
            .into_iter()
            .map(|m| H2hMatch {
                match_id: m.match_id,
                time: m.time,
                season: m.season,
                division: m.division,
                round: m.round,
                score_for: m.score_a,
                score_against: m.score_b,
                default_win: m.default_win,
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduled_highlander_matches_are_read_and_others_skipped() {
        let body = r#"{"results":{"current_page":1,"last_page":1,"data":[
          {"id":93067,"time":1791141300,"week":9,"round":"Lower Bracket Semi-Final","maps":["variable","variable"],
           "competition":{"type":"Highlander","name":"Highlander Season 36 (Autumn 2026): High"},
           "division":{"name":"High","tier":1},
           "clan1":{"id":37539,"name":"AKATSUKI","steam":{"avatar":"https://x/a.png"}},
           "clan2":{"id":37921,"name":"BOLOTO","steam":{"avatar":"https://etf2l.org/wp-content/uploads/avatars/"}}},
          {"id":93070,"time":1791136800,"round":"Quarter Finals","maps":["variable"],
           "competition":{"type":"Highlander","name":"Highlander Season 36 (Autumn 2026): Open Playoffs"},
           "division":{"name":null,"tier":null},
           "clan1":{"id":37555,"name":"TR9s"},"clan2":{"id":37860,"name":"DPM. Inc"}},
          {"id":1,"time":1791141300,"competition":{"type":"6v6","name":"6v6 Season 53"},
           "clan1":{"id":1,"name":"a"},"clan2":{"id":2,"name":"b"}}]}}"#;
        let (rows, last) = parse_page(body).unwrap();
        assert_eq!(last, 1);
        assert_eq!(rows.len(), 2, "6v6 left out");
        assert_eq!((rows[1].division.as_deref(), rows[1].tier), (Some("Open"), Some(4)), "a playoff's division from its name");
        let f = &rows[0];
        assert_eq!((f.match_id, f.season, f.tier, f.week), (93067, Some(36), Some(1), Some(9)));
        assert!(f.maps.is_empty(), "maps still to be picked");
        assert_eq!(f.clan2.avatar, None, "an empty avatar path is no avatar");
        assert_eq!(f.round.as_deref(), Some("Lower Bracket Semi-Final"));
    }

    #[test]
    fn a_head_to_head_counts_played_matches_only() {
        let row = |a: Option<i64>, b: Option<i64>, dw: bool| hl_db::H2hRow { match_id: 1, time: None, season: 1, division: None, round: None, score_a: a, score_b: b, default_win: dw };
        let r = record(&[row(Some(6), Some(0), false), row(Some(2), Some(4), false), row(Some(3), Some(3), false), row(Some(6), Some(0), true), row(None, None, false)]);
        assert_eq!(r, H2hRecord { played: 3, won: 1, lost: 1, drawn: 1 });
    }
}
