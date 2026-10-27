//! Reports and the moderation queue (data-model.md §D.2 `reports`, Q7).

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ReportRow {
    pub id: Uuid,
    pub reporter_id: Uuid,
    pub target_type: String,
    pub target_id: Uuid,
    pub reason: String,
    pub details: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub resolved_by: Option<Uuid>,
    pub resolved_at: Option<DateTime<Utc>>,
}

/// Stores a report, or returns the existing one for the same reporter and target (the unique key makes this safe).
/// The boolean is true when a new report was created.
pub async fn insert_or_existing(
    conn: &mut PgConnection,
    id: Uuid,
    reporter: Uuid,
    target_type: &str,
    target_id: Uuid,
    reason: &str,
    details: Option<&str>,
) -> Result<(ReportRow, bool), sqlx::Error> {
    let inserted = sqlx::query_scalar!(
        "INSERT INTO reports (id, reporter_id, target_type, target_id, reason, details)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (reporter_id, target_type, target_id) DO NOTHING
         RETURNING id",
        id,
        reporter,
        target_type,
        target_id,
        reason,
        details
    )
    .fetch_optional(&mut *conn)
    .await?;
    let created = inserted.is_some();
    let row = sqlx::query_as!(
        ReportRow,
        "SELECT id, reporter_id, target_type, target_id, reason, details, status, created_at,
                resolved_by, resolved_at
         FROM reports WHERE reporter_id = $1 AND target_type = $2 AND target_id = $3",
        reporter,
        target_type,
        target_id
    )
    .fetch_one(conn)
    .await?;
    Ok((row, created))
}

pub async fn find(pool: &PgPool, id: Uuid) -> Result<Option<ReportRow>, sqlx::Error> {
    sqlx::query_as!(
        ReportRow,
        "SELECT id, reporter_id, target_type, target_id, reason, details, status, created_at,
                resolved_by, resolved_at
         FROM reports WHERE id = $1",
        id
    )
    .fetch_optional(pool)
    .await
}

/// Open reports, oldest first, after a keyset cursor (Q7).
pub async fn page_open(
    pool: &PgPool,
    after: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<ReportRow>, sqlx::Error> {
    let (after_at, after_id) = match after {
        Some((at, id)) => (Some(at), Some(id)),
        None => (None, None),
    };
    sqlx::query_as!(
        ReportRow,
        "SELECT id, reporter_id, target_type, target_id, reason, details, status, created_at,
                resolved_by, resolved_at
         FROM reports
         WHERE status = 'open'
           AND ($1::timestamptz IS NULL OR (created_at, id) > ($1, $2::uuid))
         ORDER BY created_at, id
         LIMIT $3",
        after_at,
        after_id,
        limit
    )
    .fetch_all(pool)
    .await
}

/// Marks a report as actioned or dismissed. Returns whether a row changed.
pub async fn resolve(
    conn: &mut PgConnection,
    id: Uuid,
    outcome: &str,
    resolver: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "UPDATE reports SET status = $2, resolved_by = $3, resolved_at = now()
         WHERE id = $1 AND status = 'open'",
        id,
        outcome,
        resolver
    )
    .execute(conn)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Deletes resolved reports older than the cutoff (retention, §H.5).
pub async fn purge_resolved_before(pool: &PgPool, cutoff: DateTime<Utc>) -> Result<u64, sqlx::Error> {
    let result = sqlx::query!(
        "DELETE FROM reports WHERE status <> 'open' AND resolved_at < $1",
        cutoff
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}
