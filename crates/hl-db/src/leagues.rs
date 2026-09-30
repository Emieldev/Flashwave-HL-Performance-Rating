//! Every Highlander team's officials, from ETF2L (Q29): competitions, teams,
//! results, per-map scores and who played. Source data only; the views are
//! built in `hl_ingest::leagues`.

use crate::Db;
use anyhow::Result;
use serde::Serialize;
use sqlx::Row;

pub struct CompetitionRow<'a> {
    pub competition_id: i64,
    pub season: i64,
    pub season_name: &'a str,
    pub division: &'a str,
    pub stage: &'a str,
    pub name: &'a str,
    pub archived: bool,
    pub pool: Option<&'a str>,
}

pub struct SeasonMatchRow<'a> {
    pub match_id: i64,
    pub competition_id: i64,
    pub division: Option<&'a str>,
    pub tier: Option<i64>,
    pub week: Option<i64>,
    pub round: Option<&'a str>,
    pub time: Option<i64>,
    pub clan1_id: i64,
    pub clan2_id: i64,
    pub r1: Option<i64>,
    pub r2: Option<i64>,
    pub default_win: bool,
    pub maps: &'a str,
}

/// A stored competition, as the views use it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Competition {
    pub competition_id: i64,
    pub season: i64,
    pub season_name: String,
    pub division: String,
    pub stage: String,
    pub archived: bool,
    pub pool: Vec<String>,
    pub fetched_at: i64,
}

/// One official with its competition and both teams' names.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonMatch {
    pub match_id: i64,
    pub competition_id: i64,
    pub season: i64,
    pub division: String,
    pub tier: Option<i64>,
    pub stage: String,
    pub week: Option<i64>,
    pub round: Option<String>,
    pub time: Option<i64>,
    pub clan1_id: i64,
    pub clan1_name: String,
    pub clan2_id: i64,
    pub clan2_name: String,
    pub r1: Option<i64>,
    pub r2: Option<i64>,
    pub default_win: bool,
    pub maps: Vec<String>,
}

/// One map of a match: rounds each side took.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonMap {
    pub match_id: i64,
    pub map: String,
    pub clan1: i64,
    pub clan2: i64,
}

impl Db {
    pub async fn upsert_competition(&self, c: &CompetitionRow<'_>) -> Result<()> {
        sqlx::query(
            "INSERT INTO etf2l_competition (competition_id, season, season_name, division, stage, name, archived, pool, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, unixepoch())
             ON CONFLICT(competition_id) DO UPDATE SET
                season = excluded.season, season_name = excluded.season_name, division = excluded.division,
                stage = excluded.stage, name = excluded.name, archived = excluded.archived,
                pool = COALESCE(excluded.pool, etf2l_competition.pool), fetched_at = excluded.fetched_at",
        )
        .bind(c.competition_id)
        .bind(c.season)
        .bind(c.season_name)
        .bind(c.division)
        .bind(c.stage)
        .bind(c.name)
        .bind(c.archived)
        .bind(c.pool)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn competitions(&self) -> Result<Vec<Competition>> {
        let rows = sqlx::query(
            "SELECT competition_id, season, season_name, division, stage, archived, pool, fetched_at
             FROM etf2l_competition
             ORDER BY CASE WHEN season >= 100 THEN (season - 100) * 2 + 1 ELSE season * 2 END DESC, competition_id",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| Competition {
                competition_id: r.get("competition_id"),
                season: r.get("season"),
                season_name: r.get("season_name"),
                division: r.get("division"),
                stage: r.get("stage"),
                archived: r.get::<i64, _>("archived") != 0,
                pool: r.get::<Option<String>, _>("pool").and_then(|p| serde_json::from_str(&p).ok()).unwrap_or_default(),
                fetched_at: r.get("fetched_at"),
            })
            .collect())
    }

    pub async fn upsert_etf2l_team(&self, team_id: i64, name: &str, country: Option<&str>, avatar: Option<&str>) -> Result<()> {
        sqlx::query(
            "INSERT INTO etf2l_team (team_id, name, country, avatar) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(team_id) DO UPDATE SET name = excluded.name,
                country = COALESCE(excluded.country, etf2l_team.country), avatar = COALESCE(excluded.avatar, etf2l_team.avatar)",
        )
        .bind(team_id)
        .bind(name)
        .bind(country)
        .bind(avatar)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Store a result; a match already read in detail keeps that.
    pub async fn upsert_season_match(&self, m: &SeasonMatchRow<'_>) -> Result<()> {
        sqlx::query(
            "INSERT INTO etf2l_season_match (match_id, competition_id, week, round, time, clan1_id, clan2_id, r1, r2, default_win, maps, division, tier)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(match_id) DO UPDATE SET
                competition_id = excluded.competition_id, division = excluded.division, tier = excluded.tier,
                week = excluded.week, round = excluded.round, time = excluded.time,
                clan1_id = excluded.clan1_id, clan2_id = excluded.clan2_id, r1 = excluded.r1, r2 = excluded.r2,
                default_win = excluded.default_win, maps = excluded.maps",
        )
        .bind(m.match_id)
        .bind(m.competition_id)
        .bind(m.week)
        .bind(m.round)
        .bind(m.time)
        .bind(m.clan1_id)
        .bind(m.clan2_id)
        .bind(m.r1)
        .bind(m.r2)
        .bind(m.default_win)
        .bind(m.maps)
        .bind(m.division)
        .bind(m.tier)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Played matches whose own page has not been read yet, newest first,
    /// from `min_season` on: the league sample (0033) holds years of older
    /// seasons whose pages it reads itself, and a sync should not.
    pub async fn season_matches_without_detail(&self, limit: i64, min_season: i64) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT m.match_id FROM etf2l_season_match m JOIN etf2l_competition c ON c.competition_id = m.competition_id
             WHERE m.detail_fetched = 0 AND m.default_win = 0 AND c.season >= ?2
             ORDER BY m.time DESC LIMIT ?1",
        )
        .bind(limit)
        .bind(min_season)
        .fetch_all(self.pool())
        .await?)
    }

    /// A match's per-map scores and players, replacing what was there.
    pub async fn put_season_match_detail(
        &self,
        match_id: i64,
        maps: &[(i64, &str, i64, i64, bool)],
        players: &[(u32, Option<i64>, &str)],
    ) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM etf2l_season_map WHERE match_id = ?1").bind(match_id).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM etf2l_season_player WHERE match_id = ?1").bind(match_id).execute(&mut *tx).await?;
        for (order, map, c1, c2, golden) in maps {
            sqlx::query("INSERT OR REPLACE INTO etf2l_season_map (match_id, match_order, map, clan1, clan2, golden_cap) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")
                .bind(match_id)
                .bind(order)
                .bind(map)
                .bind(c1)
                .bind(c2)
                .bind(golden)
                .execute(&mut *tx)
                .await?;
        }
        for (account, team, name) in players {
            sqlx::query("INSERT OR REPLACE INTO etf2l_season_player (match_id, account_id, team_id, name) VALUES (?1, ?2, ?3, ?4)")
                .bind(match_id)
                .bind(i64::from(*account))
                .bind(team)
                .bind(name)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("UPDATE etf2l_season_match SET detail_fetched = 1 WHERE match_id = ?1").bind(match_id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Every stored official, with names; `season` narrows to one.
    pub async fn season_matches(&self, season: Option<i64>, team: Option<i64>) -> Result<Vec<SeasonMatch>> {
        let rows = sqlx::query(
            "SELECT m.match_id, m.competition_id, c.season, COALESCE(NULLIF(m.division, ''), c.division) AS division, m.tier, c.stage, m.week, m.round, m.time,
                    m.clan1_id, COALESCE(t1.name, '') AS clan1_name, m.clan2_id, COALESCE(t2.name, '') AS clan2_name,
                    m.r1, m.r2, m.default_win, m.maps
             FROM etf2l_season_match m
             JOIN etf2l_competition c ON c.competition_id = m.competition_id
             LEFT JOIN etf2l_team t1 ON t1.team_id = m.clan1_id
             LEFT JOIN etf2l_team t2 ON t2.team_id = m.clan2_id
             WHERE (?1 IS NULL OR c.season = ?1) AND (?2 IS NULL OR m.clan1_id = ?2 OR m.clan2_id = ?2)
             ORDER BY m.time DESC",
        )
        .bind(season)
        .bind(team)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| SeasonMatch {
                match_id: r.get("match_id"),
                competition_id: r.get("competition_id"),
                season: r.get("season"),
                division: r.get("division"),
                tier: r.get("tier"),
                stage: r.get("stage"),
                week: r.get("week"),
                round: r.get("round"),
                time: r.get("time"),
                clan1_id: r.get("clan1_id"),
                clan1_name: r.get("clan1_name"),
                clan2_id: r.get("clan2_id"),
                clan2_name: r.get("clan2_name"),
                r1: r.get("r1"),
                r2: r.get("r2"),
                default_win: r.get::<i64, _>("default_win") != 0,
                maps: r.get::<Option<String>, _>("maps").and_then(|p| serde_json::from_str(&p).ok()).unwrap_or_default(),
            })
            .collect())
    }

    /// Per-map scores for matches read in detail; `team` narrows to one.
    pub async fn season_maps(&self, team: Option<i64>) -> Result<Vec<SeasonMap>> {
        let rows = sqlx::query(
            "SELECT p.match_id, p.map, p.clan1, p.clan2 FROM etf2l_season_map p
             JOIN etf2l_season_match m ON m.match_id = p.match_id
             WHERE ?1 IS NULL OR m.clan1_id = ?1 OR m.clan2_id = ?1
             ORDER BY p.match_id, p.match_order",
        )
        .bind(team)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| SeasonMap { match_id: r.get("match_id"), map: r.get("map"), clan1: r.get("clan1"), clan2: r.get("clan2") })
            .collect())
    }

    /// Who played for `team`: `(account, name, matches, last played)`, most first.
    pub async fn season_players(&self, team: i64) -> Result<Vec<(u32, String, i64, i64)>> {
        let rows = sqlx::query(
            "SELECT p.account_id, MAX(p.name) AS name, COUNT(*) AS n, MAX(m.time) AS last
             FROM etf2l_season_player p JOIN etf2l_season_match m ON m.match_id = p.match_id
             WHERE p.team_id = ?1 GROUP BY p.account_id ORDER BY n DESC, last DESC",
        )
        .bind(team)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.get::<i64, _>("account_id") as u32, r.get::<Option<String>, _>("name").unwrap_or_default(), r.get("n"), r.get::<Option<i64>, _>("last").unwrap_or(0)))
            .collect())
    }

    /// Ratings the pool holds for these accounts: `(account, class, games,
    /// average score)`, most played first per account.
    pub async fn pool_ratings(&self, accounts: &[u32], model_version: &str) -> Result<Vec<(u32, String, i64, f64)>> {
        if accounts.is_empty() {
            return Ok(Vec::new());
        }
        let marks = vec!["?"; accounts.len()].join(",");
        let sql = format!(
            "SELECT account_id, class, COUNT(*) AS n, AVG(score) AS avg FROM rating
             WHERE model_version = ? AND account_id IN ({marks}) GROUP BY account_id, class ORDER BY account_id, n DESC"
        );
        let mut q = sqlx::query(&sql).bind(model_version);
        for a in accounts {
            q = q.bind(i64::from(*a));
        }
        Ok(q.fetch_all(self.pool())
            .await?
            .into_iter()
            .map(|r| (r.get::<i64, _>("account_id") as u32, r.get("class"), r.get("n"), r.get("avg")))
            .collect())
    }

    /// Every player's ETF2L name, from the official rosters already stored:
    /// the owner's matches and the season matches read for the league
    /// pages. No request of its own. The name from the latest match wins,
    /// since players rename.
    pub async fn etf2l_names(&self) -> Result<std::collections::HashMap<u32, String>> {
        let rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT account_id, name FROM (
                 SELECT r.account_id, r.name, COALESCE(m.time, 0) AS t, r.match_id
                 FROM etf2l_roster r LEFT JOIN etf2l_match m ON m.match_id = r.match_id
                 UNION ALL
                 SELECT p.account_id, p.name, COALESCE(s.time, 0), p.match_id
                 FROM etf2l_season_player p LEFT JOIN etf2l_season_match s ON s.match_id = p.match_id
             )
             WHERE name IS NOT NULL AND TRIM(name) <> ''
             ORDER BY account_id, t, match_id",
        )
        .fetch_all(self.pool())
        .await?;
        // Ordered oldest first, so each later row overwrites.
        Ok(rows.into_iter().map(|(a, n)| (a as u32, n.trim().to_string())).collect())
    }

    /// A stored team's name, country and avatar.
    pub async fn etf2l_team(&self, team: i64) -> Result<Option<(String, Option<String>, Option<String>)>> {
        Ok(sqlx::query_as("SELECT name, country, avatar FROM etf2l_team WHERE team_id = ?1").bind(team).fetch_optional(self.pool()).await?)
    }
}

#[cfg(test)]
mod tests {
    use crate::Db;

    #[tokio::test]
    async fn etf2l_names_take_the_latest_from_either_roster() {
        let db = Db::connect_in_memory().await.unwrap();
        for sql in [
            "INSERT INTO etf2l_match (match_id, time) VALUES (1, 100), (2, 300)",
            "INSERT INTO etf2l_roster (match_id, account_id, name) VALUES (1, 7, 'old'), (2, 7, 'owner era'), (1, 8, '  '), (1, 9, 'nine')",
            "INSERT INTO etf2l_season_match (match_id, competition_id, time, clan1_id, clan2_id) VALUES (10, 1, 200, 1, 2), (11, 1, 400, 1, 2)",
            "INSERT INTO etf2l_season_player (match_id, account_id, name) VALUES (10, 7, 'between'), (11, 9, 'nine renamed'), (10, 8, 'eight')",
        ] {
            sqlx::query(sql).execute(db.pool()).await.unwrap();
        }
        let names = db.etf2l_names().await.unwrap();
        assert_eq!(names.get(&7).map(String::as_str), Some("owner era"), "time 300 beats 200 and 100");
        assert_eq!(names.get(&8).map(String::as_str), Some("eight"), "a blank name is skipped");
        assert_eq!(names.get(&9).map(String::as_str), Some("nine renamed"));
    }
}
