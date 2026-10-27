//! Moderation actions and their effects on messages, users and discussions (data-model.md §D.2 `moderation_actions`).

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

/// Appends one row to the moderation trail. Every moderator action goes through here.
#[allow(clippy::too_many_arguments)]
pub async fn record(
    conn: &mut PgConnection,
    moderator: Uuid,
    action: &str,
    target_type: &str,
    target_id: Uuid,
    report_id: Option<Uuid>,
    reason: Option<&str>,
    expires_at: Option<DateTime<Utc>>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO moderation_actions (id, moderator_id, action, target_type, target_id, report_id, reason, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        Uuid::now_v7(),
        moderator,
        action,
        target_type,
        target_id,
        report_id,
        reason,
        expires_at
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Sets a message's status. `removed` keeps the body for appeals (purged after 90 days by a job). Returns whether a
/// row changed.
pub async fn set_message_status(
    conn: &mut PgConnection,
    id: Uuid,
    status: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "UPDATE messages SET status = $2, status_changed_at = now() WHERE id = $1 AND status <> 'deleted'",
        id,
        status
    )
    .execute(conn)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Suspends an account until `until` (or indefinitely when `None`). Suspended accounts can still read.
pub async fn suspend_user(
    conn: &mut PgConnection,
    id: Uuid,
    until: Option<DateTime<Utc>>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "UPDATE users SET status = 'suspended', suspended_until = $2, updated_at = now()
         WHERE id = $1 AND status <> 'deleted'",
        id,
        until
    )
    .execute(conn)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn unsuspend_user(conn: &mut PgConnection, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "UPDATE users SET status = 'active', suspended_until = NULL, updated_at = now()
         WHERE id = $1 AND status <> 'deleted'",
        id
    )
    .execute(conn)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Sets a discussion's status: `open`, `locked` or `hidden`.
pub async fn set_discussion_status(
    conn: &mut PgConnection,
    id: Uuid,
    status: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!("UPDATE discussions SET status = $2 WHERE id = $1", id, status)
        .execute(conn)
        .await?;
    Ok(result.rows_affected() == 1)
}

/// The role of a user, used to refuse moderator actions against moderators and admins.
pub async fn role_of(conn: &mut PgConnection, id: Uuid) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!("SELECT role FROM users WHERE id = $1", id)
        .fetch_optional(conn)
        .await
}
