//! Sessions. Only the SHA-256 of each token is stored (docs/architecture/security.md §H.1).

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// Session joined with its user, for authenticating a request (data-model.md Q6).
#[derive(Debug, Clone)]
pub struct AuthRow {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub username: String,
    pub role: String,
    pub status: String,
    pub suspended_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct SessionListRow {
    pub id: Uuid,
    pub client_label: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
}

pub async fn insert(
    conn: &mut PgConnection,
    id: Uuid,
    user_id: Uuid,
    token_hash: &[u8],
    client_label: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO sessions (id, user_id, token_hash, client_label, expires_at)
         VALUES ($1, $2, $3, $4, now() + interval '30 days')",
        id,
        user_id,
        token_hash,
        client_label
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn find_active_by_token_hash(
    pool: &PgPool,
    token_hash: &[u8],
) -> Result<Option<AuthRow>, sqlx::Error> {
    sqlx::query_as!(
        AuthRow,
        r#"SELECT s.id AS session_id, s.user_id, u.username, u.role, u.status, u.suspended_until
           FROM sessions s JOIN users u ON u.id = s.user_id
           WHERE s.token_hash = $1 AND s.revoked_at IS NULL AND s.expires_at > now() AND u.status <> 'deleted'"#,
        token_hash
    )
    .fetch_optional(pool)
    .await
}

/// Extends the sliding expiry. At most one write every ten minutes per session. The absolute limit is 180 days from
/// creation.
pub async fn touch(pool: &PgPool, session_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE sessions
         SET last_used_at = now(),
             expires_at = LEAST(now() + interval '30 days', created_at + interval '180 days')
         WHERE id = $1 AND last_used_at < now() - interval '10 minutes'",
        session_id
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn revoke(pool: &PgPool, session_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE sessions SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL",
        session_id
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Revokes every active session of a user, except `keep` if it is given.
pub async fn revoke_all_for_user(
    conn: &mut PgConnection,
    user_id: Uuid,
    keep: Option<Uuid>,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query!(
        "UPDATE sessions SET revoked_at = now()
         WHERE user_id = $1 AND revoked_at IS NULL AND ($2::uuid IS NULL OR id <> $2)",
        user_id,
        keep
    )
    .execute(conn)
    .await?;
    Ok(result.rows_affected())
}

/// Revokes one session, but only if it belongs to `user_id`. Returns whether a session was revoked.
pub async fn revoke_for_user(pool: &PgPool, user_id: Uuid, session_id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "UPDATE sessions SET revoked_at = now()
         WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL",
        session_id,
        user_id
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn list_for_user(pool: &PgPool, user_id: Uuid) -> Result<Vec<SessionListRow>, sqlx::Error> {
    sqlx::query_as!(
        SessionListRow,
        "SELECT id, client_label, created_at, last_used_at FROM sessions
         WHERE user_id = $1 AND revoked_at IS NULL AND expires_at > now()
         ORDER BY last_used_at DESC",
        user_id
    )
    .fetch_all(pool)
    .await
}
