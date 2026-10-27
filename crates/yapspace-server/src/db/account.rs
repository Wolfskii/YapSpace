//! Account deletion, data export, and the retention purges (docs/architecture/security.md §H.5).

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// Turns the account into a tombstone. The name is freed and the status is `deleted`. Returns whether it changed.
pub async fn mark_deleted(
    conn: &mut PgConnection,
    id: Uuid,
    tombstone_name: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "UPDATE users SET status = 'deleted', username = $2, deleted_at = now(), updated_at = now()
         WHERE id = $1 AND status <> 'deleted'",
        id,
        tombstone_name
    )
    .execute(conn)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn delete_credentials(conn: &mut PgConnection, user_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM password_credentials WHERE user_id = $1", user_id)
        .execute(conn)
        .await?;
    Ok(())
}

/// One of the user's messages, for the export file.
#[derive(Debug, Clone)]
pub struct ExportMessage {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub discussion_display_url: String,
    pub seq: i64,
    pub body: String,
    pub status: String,
    pub edited_at: Option<DateTime<Utc>>,
}

/// A batch of the user's messages after a keyset cursor `(created_at, id)`.
pub async fn export_page(
    pool: &PgPool,
    user: Uuid,
    after: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<ExportMessage>, sqlx::Error> {
    let (after_at, after_id) = match after {
        Some((at, id)) => (Some(at), Some(id)),
        None => (None, None),
    };
    sqlx::query_as!(
        ExportMessage,
        "SELECT m.id, m.created_at, d.display_url AS discussion_display_url, m.seq, m.body, m.status, m.edited_at
         FROM messages m JOIN discussions d ON d.id = m.discussion_id
         WHERE m.author_id = $1
           AND ($2::timestamptz IS NULL OR (m.created_at, m.id) > ($2, $3::uuid))
         ORDER BY m.created_at, m.id
         LIMIT $4",
        user,
        after_at,
        after_id,
        limit
    )
    .fetch_all(pool)
    .await
}

/// Erases the bodies of messages written by deleted accounts, in batches. Returns how many changed.
pub async fn erase_deleted_users_content(pool: &PgPool, batch: i64) -> Result<u64, sqlx::Error> {
    let result = sqlx::query!(
        "UPDATE messages SET body = '', status = 'deleted', status_changed_at = now()
         WHERE id IN (
           SELECT m.id FROM messages m JOIN users u ON u.id = m.author_id
           WHERE u.status = 'deleted' AND (m.body <> '' OR m.status <> 'deleted')
           LIMIT $1
         )",
        batch
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Erases bodies of removed messages older than the cutoff. Removed text is kept for appeals first (§H.5).
pub async fn erase_removed_before(
    pool: &PgPool,
    cutoff: DateTime<Utc>,
    batch: i64,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query!(
        "UPDATE messages SET body = ''
         WHERE id IN (
           SELECT id FROM messages
           WHERE status = 'removed' AND status_changed_at < $1 AND body <> ''
           LIMIT $2
         )",
        cutoff,
        batch
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Deletes sessions that expired or were revoked before the cutoff.
pub async fn purge_sessions_before(pool: &PgPool, cutoff: DateTime<Utc>) -> Result<u64, sqlx::Error> {
    let result = sqlx::query!(
        "DELETE FROM sessions WHERE expires_at < $1 OR revoked_at < $1",
        cutoff
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Deletes audit events older than the cutoff (kept for two years, §H.5).
pub async fn purge_audit_before(pool: &PgPool, cutoff: DateTime<Utc>) -> Result<u64, sqlx::Error> {
    let result = sqlx::query!("DELETE FROM audit_events WHERE created_at < $1", cutoff)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}
