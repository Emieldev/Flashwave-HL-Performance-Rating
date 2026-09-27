//! Stored demo timelines (PLAN Q3). The format belongs to
//! `hl_demos::timeline`; this only keeps and returns the row.

use crate::Db;
use anyhow::Result;
use sqlx::Row;

/// One demo's timeline, as stored.
#[derive(Debug, Clone, Default)]
pub struct TimelineRow {
    pub version: i64,
    pub tick_rate: f64,
    pub stride: i64,
    pub head: String,
    pub samples: Vec<u8>,
    pub changes: Vec<u8>,
    pub objects: Vec<u8>,
    pub events: Vec<u8>,
    pub raw_bytes: i64,
    pub stored_bytes: i64,
}

/// How many demos are kept as timelines, and what that costs.
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineTotals {
    pub demos: i64,
    pub stored_bytes: i64,
    /// Of those, how many have lost their file: kept only as a timeline.
    pub file_gone: i64,
}

impl Db {
    pub async fn put_timeline(&self, demo_id: i64, row: &TimelineRow) -> Result<()> {
        sqlx::query(
            "INSERT INTO demo_timeline
                (demo_id, version, tick_rate, stride, head, samples, changes, objects, events, raw_bytes, stored_bytes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(demo_id) DO UPDATE SET
                version = excluded.version, tick_rate = excluded.tick_rate, stride = excluded.stride,
                head = excluded.head, samples = excluded.samples, changes = excluded.changes,
                objects = excluded.objects, events = excluded.events,
                raw_bytes = excluded.raw_bytes, stored_bytes = excluded.stored_bytes,
                recorded_at = unixepoch()",
        )
        .bind(demo_id)
        .bind(row.version)
        .bind(row.tick_rate)
        .bind(row.stride)
        .bind(&row.head)
        .bind(&row.samples)
        .bind(&row.changes)
        .bind(&row.objects)
        .bind(&row.events)
        .bind(row.raw_bytes)
        .bind(row.stored_bytes)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// The version a demo's timeline was recorded at, if it has one.
    pub async fn timeline_version(&self, demo_id: i64) -> Result<Option<i64>> {
        Ok(sqlx::query_scalar("SELECT version FROM demo_timeline WHERE demo_id = ?1")
            .bind(demo_id)
            .fetch_optional(self.pool())
            .await?)
    }

    pub async fn timeline(&self, demo_id: i64) -> Result<Option<TimelineRow>> {
        let Some(r) = sqlx::query(
            "SELECT version, tick_rate, stride, head, samples, changes, objects, events, raw_bytes, stored_bytes
             FROM demo_timeline WHERE demo_id = ?1",
        )
        .bind(demo_id)
        .fetch_optional(self.pool())
        .await?
        else {
            return Ok(None);
        };
        Ok(Some(TimelineRow {
            version: r.get("version"),
            tick_rate: r.get("tick_rate"),
            stride: r.get("stride"),
            head: r.get("head"),
            samples: r.get("samples"),
            changes: r.get("changes"),
            objects: r.get("objects"),
            events: r.get("events"),
            raw_bytes: r.get("raw_bytes"),
            stored_bytes: r.get("stored_bytes"),
        }))
    }

    /// Every kept demo: `(demo_id, file name, stored bytes, file gone)`,
    /// newest first.
    pub async fn timeline_list(&self) -> Result<Vec<(i64, String, i64, bool)>> {
        let rows = sqlx::query(
            "SELECT t.demo_id, d.file_name, t.stored_bytes, d.deleted_at IS NOT NULL AS gone
             FROM demo_timeline t JOIN demo d ON d.demo_id = t.demo_id
             ORDER BY d.start_utc DESC",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|r| (r.get("demo_id"), r.get("file_name"), r.get("stored_bytes"), r.get("gone"))).collect())
    }

    pub async fn timeline_totals(&self) -> Result<TimelineTotals> {
        let r = sqlx::query(
            "SELECT COUNT(*) AS demos, COALESCE(SUM(t.stored_bytes), 0) AS bytes,
                    COALESCE(SUM(d.deleted_at IS NOT NULL), 0) AS gone
             FROM demo_timeline t JOIN demo d ON d.demo_id = t.demo_id",
        )
        .fetch_one(self.pool())
        .await?;
        Ok(TimelineTotals { demos: r.get("demos"), stored_bytes: r.get("bytes"), file_gone: r.get("gone") })
    }
}
