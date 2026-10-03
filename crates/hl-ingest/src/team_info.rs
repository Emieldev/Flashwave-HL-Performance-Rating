//! A team as ETF2L's API describes it (Q48, Flashy; PLAN §27): its tag,
//! links, former names, who is on it now and in what role, and every
//! competition it entered -- the cups included, which the league tables do
//! not hold.
//!
//! `/team/{id}` is kept in `etf2l_raw` (kind `team`): read again after a day
//! while the team still plays, after a month once it has stopped.

use crate::sources::Sources;
use anyhow::Result;
use hl_core::SteamId;
use hl_db::Db;
use serde::Serialize;
use serde_json::Value;

const DAY: i64 = 24 * 3600;
const ACTIVE: i64 = 120 * DAY;

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TeamInfo {
    pub tag: Option<String>,
    pub homepage: Option<String>,
    pub steam_group: Option<String>,
    /// Oldest first.
    pub former_names: Vec<NameChange>,
    /// Who is on the team now.
    pub members: Vec<Member>,
    /// The cups it entered, newest first.
    pub cups: Vec<Cup>,
    pub fetched_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NameChange {
    pub from: String,
    pub to: String,
    pub time: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub account_id: Option<u32>,
    pub name: String,
    /// "Leader", "Deputy", "Inactive" or "Member", as ETF2L has it.
    pub role: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Cup {
    /// The cup without its stage: "Highlander Experimental Cup #10".
    pub name: String,
    pub division: Option<String>,
    pub tier: Option<i64>,
    /// "1st", "2nd", "3rd" when ETF2L's awards list a placing for it.
    pub place: Option<String>,
    /// The newest competition id of it, for ordering.
    pub competition_id: i64,
}

/// A link worth showing: http(s), not ETF2L's empty placeholders.
fn link(v: &Value) -> Option<String> {
    v.as_str().map(str::trim).filter(|s| s.starts_with("http://") || s.starts_with("https://")).map(str::to_string)
}

/// A cup's name without its stage: ": Mid 3rd Place", ": Playoffs".
fn cup_base(name: &str) -> &str {
    name.split(": ").next().unwrap_or(name).trim()
}

/// Read ETF2L's `/team/{id}` body. `awards` are the team page's
/// `(place, competition)`, for the cups' placings.
pub fn parse(body: &str, awards: &[(String, String)]) -> Option<TeamInfo> {
    let v: Value = serde_json::from_str(body).ok()?;
    let t = v.get("team")?;
    let mut info = TeamInfo {
        tag: t["tag"].as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string),
        homepage: link(&t["homepage"]),
        steam_group: link(&t["steam"]["steam_group"]),
        ..Default::default()
    };
    for c in t["name_changes"].as_array().into_iter().flatten() {
        if let (Some(from), Some(to), Some(time)) = (c["from"].as_str(), c["to"].as_str(), c["time"].as_i64()) {
            info.former_names.push(NameChange { from: from.to_string(), to: to.to_string(), time });
        }
    }
    info.former_names.sort_by_key(|c| c.time);
    for p in t["players"].as_array().into_iter().flatten() {
        let account = p["steam"]["id64"].as_str().and_then(|s| SteamId::parse(s).ok()).map(|s| s.account_id());
        info.members.push(Member {
            account_id: account,
            name: p["name"].as_str().unwrap_or("").to_string(),
            role: p["role"].as_str().filter(|r| !r.is_empty()).unwrap_or("Member").to_string(),
        });
    }
    // The cups: every competition that is not a league season, one entry per
    // cup however many stages it had; the division from the stage that has
    // one, the placing from the awards.
    let mut cups: Vec<Cup> = Vec::new();
    if let Some(comps) = t["competitions"].as_object() {
        for (id, c) in comps {
            let category = c["category"].as_str().unwrap_or("");
            let name = c["competition"].as_str().unwrap_or("");
            if category == "Highlander Season" || name.is_empty() || !name.to_ascii_lowercase().contains("highlander") {
                continue;
            }
            let id: i64 = id.parse().unwrap_or(0);
            let base = cup_base(name).to_string();
            let division = c["division"]["name"].as_str().map(str::to_string);
            let tier = c["division"]["tier"].as_i64();
            match cups.iter_mut().find(|x| x.name == base) {
                Some(x) => {
                    x.competition_id = x.competition_id.max(id);
                    if x.division.is_none() {
                        x.division = division;
                        x.tier = tier;
                    }
                }
                None => cups.push(Cup { name: base, division, tier, place: None, competition_id: id }),
            }
        }
    }
    for cup in &mut cups {
        cup.place = awards.iter().find(|(_, comp)| comp.starts_with(&cup.name)).map(|(p, _)| p.clone());
    }
    cups.sort_by_key(|c| std::cmp::Reverse(c.competition_id));
    info.cups = cups;
    Some(info)
}

/// A team's ETF2L info, read when due. Best effort: a failed read keeps what
/// is stored, and a team never read comes back empty.
pub async fn team_info(db: &Db, sources: &Sources, team: i64) -> Result<TeamInfo> {
    let held = db.etf2l_raw_one("team", team).await?;
    let active = db.team_last_official(team).await?.is_some_and(|t| now() - t < ACTIVE);
    let due = held.as_ref().is_none_or(|(at, _)| now() - at > if active { DAY } else { 30 * DAY });
    let mut body = held.map(|(at, json)| (at, json));
    if due {
        match sources.etf2l_get(&format!("/team/{team}")).await {
            Ok(Some(fresh)) if fresh.trim_start().starts_with('{') => {
                db.store_etf2l_raw("team", team, &fresh).await?;
                body = Some((now(), fresh));
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(team, error = %format!("{e:#}"), "reading a team from ETF2L failed"),
        }
    }
    let Some((at, json)) = body else { return Ok(TeamInfo::default()) };
    let awards: Vec<(String, String)> = crate::catalogue::team_etf2l(db, sources, team).await.map(|p| p.awards.into_iter().map(|a| (a.place, a.competition)).collect()).unwrap_or_default();
    Ok(TeamInfo { fetched_at: Some(at), ..parse(&json, &awards).unwrap_or_default() })
}

#[cfg(test)]
mod tests {
    use super::*;

    // SBQRRA's /team/35600, cut down, 3 October 2026.
    const BODY: &str = r#"{"status":{"code":200},"team":{
        "competitions":{
          "904":{"category":"Highlander Season","competition":"Highlander Winter 2024: Low","division":{"name":"Low","tier":3}},
          "901":{"category":"Fun Cup","competition":"Highlander Winter 2024 Preseason Cup","division":{"name":"Low","tier":3}},
          "998":{"category":"Fun Cup","competition":"Highlander Experimental Cup #10","division":{"name":"Mid","tier":2}},
          "1011":{"category":"Fun Cup","competition":"Highlander Experimental Cup #10: Mid 3rd Place","division":{"name":null,"tier":null}},
          "500":{"category":"6v6 Cup","competition":"6v6 Something Cup","division":{"name":"Open","tier":4}}},
        "country":"Italy","homepage":"https://blackjew.online/","id":35600,"name":"SBQRRA",
        "steam":{"avatar":"x","steam_group":null},"tag":"800A",
        "players":[
          {"name":"bull","role":"Leader","steam":{"id64":"76561199233979726"}},
          {"name":"zero","role":"Inactive","steam":{"id64":"76561198167828355"}},
          {"name":"ghost","role":"","steam":{"id64":null}}],
        "name_changes":[{"from":"Sborram PopvlvsQve Romanvm","to":"SBQRRA","time":1684015965}]}}"#;

    #[test]
    fn a_team_is_read_with_its_roles_names_and_cups() {
        let awards = vec![("1st".to_string(), "Highlander Winter 2024 Preseason Cup (Low A)".to_string())];
        let i = parse(BODY, &awards).unwrap();
        assert_eq!(i.tag.as_deref(), Some("800A"));
        assert_eq!(i.homepage.as_deref(), Some("https://blackjew.online/"));
        assert_eq!(i.steam_group, None);
        assert_eq!(i.former_names, vec![NameChange { from: "Sborram PopvlvsQve Romanvm".into(), to: "SBQRRA".into(), time: 1684015965 }]);
        assert_eq!(i.members.len(), 3);
        assert_eq!((i.members[0].account_id, i.members[0].role.as_str()), (Some(1273713998), "Leader"));
        assert_eq!(i.members[1].role, "Inactive");
        assert_eq!((i.members[2].account_id, i.members[2].role.as_str()), (None, "Member"));
        // Two cups: the experimental cup's two stages are one; the league
        // season and the 6v6 cup are not cups here.
        let names: Vec<&str> = i.cups.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["Highlander Experimental Cup #10", "Highlander Winter 2024 Preseason Cup"]);
        assert_eq!(i.cups[0].division.as_deref(), Some("Mid"), "from the stage that has a division");
        assert_eq!(i.cups[1].place.as_deref(), Some("1st"));
        assert_eq!(i.cups[0].place, None);
    }
}
