//! The player catalogue (Q35): every ETF2L official in the database and who
//! played it, for player profiles. Plain loaders; the seasons, divisions and
//! medals are worked out in `hl_ingest::catalogue`, where they can be tested.

use crate::Db;
use anyhow::Result;
use sqlx::Row;
use std::collections::HashMap;

/// One ETF2L official with its competition.
#[derive(Debug, Clone)]
pub struct CatMatch {
    pub match_id: i64,
    pub competition_id: i64,
    pub season: i64,
    pub season_name: String,
    /// The competition's division ("Low" for "Low Playoffs"), and stage.
    pub comp_division: String,
    pub stage: String,
    /// The match's own division and tier: absent on playoff matches.
    pub division: Option<String>,
    pub tier: Option<i64>,
    pub round: Option<String>,
    pub time: Option<i64>,
    pub clan1: i64,
    pub clan2: i64,
    pub r1: Option<i64>,
    pub r2: Option<i64>,
    pub default_win: bool,
}

/// One player in one official: `(match, account, team, name)`.
pub type RosterRow = (i64, u32, Option<i64>, String);

/// ETF2L's page for a player, as kept.
#[derive(Debug, Clone, Default)]
pub struct Etf2lPlayer {
    pub etf2l_id: Option<i64>,
    pub name: Option<String>,
    pub country: Option<String>,
    pub classes: Vec<String>,
    pub avatar: Option<String>,
    pub registered: Option<i64>,
    pub fetched_at: i64,
}

impl Db {
    pub async fn catalogue_matches(&self) -> Result<Vec<CatMatch>> {
        let rows = sqlx::query(
            "SELECT m.match_id, m.competition_id, c.season, c.season_name, c.division AS comp_division, c.stage,
                    m.division, m.tier, m.round, m.time, m.clan1_id, m.clan2_id, m.r1, m.r2, m.default_win
             FROM etf2l_season_match m JOIN etf2l_competition c ON c.competition_id = m.competition_id",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| CatMatch {
                match_id: r.get("match_id"),
                competition_id: r.get("competition_id"),
                season: r.get("season"),
                season_name: r.get("season_name"),
                comp_division: r.get("comp_division"),
                stage: r.get("stage"),
                division: r.get("division"),
                tier: r.get("tier"),
                round: r.get("round"),
                time: r.get("time"),
                clan1: r.get("clan1_id"),
                clan2: r.get("clan2_id"),
                r1: r.get("r1"),
                r2: r.get("r2"),
                default_win: r.get::<i64, _>("default_win") != 0,
            })
            .collect())
    }

    /// Everyone who played each official, for the matches `account` is
    /// given (or every match without it).
    pub async fn catalogue_rosters(&self, account: Option<u32>) -> Result<Vec<RosterRow>> {
        let rows = sqlx::query(
            "SELECT match_id, account_id, team_id, COALESCE(name, '') AS name FROM etf2l_season_player
             WHERE ?1 IS NULL OR match_id IN (SELECT match_id FROM etf2l_season_player WHERE account_id = ?1)",
        )
        .bind(account.map(i64::from))
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.get("match_id"), r.get::<i64, _>("account_id") as u32, r.get("team_id"), r.get("name")))
            .collect())
    }

    /// Every ETF2L team: name and avatar.
    pub async fn etf2l_teams(&self) -> Result<HashMap<i64, (String, Option<String>)>> {
        let rows = sqlx::query("SELECT team_id, name, avatar FROM etf2l_team").fetch_all(self.pool()).await?;
        Ok(rows.into_iter().map(|r| (r.get("team_id"), (r.get("name"), r.get("avatar")))).collect())
    }

    /// Games per main class, for every account: from the owner's matches
    /// and the league sample together.
    pub async fn main_class_games(&self) -> Result<HashMap<u32, HashMap<String, i64>>> {
        let rows = sqlx::query(
            "SELECT account_id, main_class AS class, COUNT(*) AS n FROM match_player
               WHERE main_class IS NOT NULL GROUP BY account_id, main_class
             UNION ALL
             SELECT account_id, class, COUNT(*) FROM league_log_player
               WHERE class IS NOT NULL AND log_id NOT IN (SELECT log_id FROM log_index)
               GROUP BY account_id, class",
        )
        .fetch_all(self.pool())
        .await?;
        let mut out: HashMap<u32, HashMap<String, i64>> = HashMap::new();
        for r in rows {
            *out.entry(r.get::<i64, _>("account_id") as u32).or_default().entry(r.get("class")).or_default() += r.get::<i64, _>("n");
        }
        Ok(out)
    }

    /// Every name each player has appeared under in the owner's matches,
    /// newest first: for search, beside the ETF2L names.
    pub async fn log_names(&self) -> Result<HashMap<u32, Vec<String>>> {
        let rows = sqlx::query(
            "SELECT p.account_id, p.name FROM match_player p JOIN match m ON m.log_id = p.log_id
             WHERE p.name IS NOT NULL GROUP BY p.account_id, p.name ORDER BY MAX(m.played_at) DESC",
        )
        .fetch_all(self.pool())
        .await?;
        let mut out: HashMap<u32, Vec<String>> = HashMap::new();
        for r in rows {
            out.entry(r.get::<i64, _>("account_id") as u32).or_default().push(r.get("name"));
        }
        Ok(out)
    }

    /// League-sample logs with JSON whose players are not indexed yet.
    pub async fn league_logs_unindexed(&self, limit: i64) -> Result<Vec<(i64, String)>> {
        let rows = sqlx::query(
            "SELECT j.log_id, j.json FROM league_log_json j
             WHERE NOT EXISTS (SELECT 1 FROM league_log_player p WHERE p.log_id = j.log_id) LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|r| (r.get("log_id"), r.get("json"))).collect())
    }

    /// One log's players: `(account, team, main class, seconds)`.
    pub async fn put_league_players(&self, log_id: i64, players: &[(u32, Option<&str>, Option<&str>, i64)]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM league_log_player WHERE log_id = ?1").bind(log_id).execute(&mut *tx).await?;
        for (account, team, class, seconds) in players {
            sqlx::query("INSERT INTO league_log_player (log_id, account_id, team, class, seconds) VALUES (?1, ?2, ?3, ?4, ?5)")
                .bind(log_id)
                .bind(i64::from(*account))
                .bind(team)
                .bind(class)
                .bind(seconds)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn etf2l_player(&self, account: u32) -> Result<Option<Etf2lPlayer>> {
        let Some(r) = sqlx::query("SELECT * FROM etf2l_player WHERE account_id = ?1").bind(i64::from(account)).fetch_optional(self.pool()).await? else {
            return Ok(None);
        };
        Ok(Some(Etf2lPlayer {
            etf2l_id: r.get("etf2l_id"),
            name: r.get("name"),
            country: r.get("country"),
            classes: r.get::<Option<String>, _>("classes").and_then(|c| serde_json::from_str(&c).ok()).unwrap_or_default(),
            avatar: r.get("avatar"),
            registered: r.get("registered"),
            fetched_at: r.get("fetched_at"),
        }))
    }

    pub async fn put_etf2l_player(&self, account: u32, p: &Etf2lPlayer) -> Result<()> {
        sqlx::query(
            "INSERT INTO etf2l_player (account_id, etf2l_id, name, country, classes, avatar, registered, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT (account_id) DO UPDATE SET etf2l_id = excluded.etf2l_id, name = excluded.name,
                country = excluded.country, classes = excluded.classes, avatar = excluded.avatar,
                registered = excluded.registered, fetched_at = excluded.fetched_at",
        )
        .bind(i64::from(account))
        .bind(p.etf2l_id)
        .bind(&p.name)
        .bind(&p.country)
        .bind(serde_json::to_string(&p.classes)?)
        .bind(&p.avatar)
        .bind(p.registered)
        .bind(p.fetched_at)
        .execute(self.pool())
        .await?;
        Ok(())
    }
}
