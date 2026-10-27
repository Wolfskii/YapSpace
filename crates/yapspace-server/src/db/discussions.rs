//! Discussions and their identity keys (data-model.md Q1, Q4).

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct DiscussionRow {
    pub id: Uuid,
    pub display_url: String,
    pub site: String,
    pub title: Option<String>,
    pub status: String,
    pub merged_into_id: Option<Uuid>,
    pub last_seq: i64,
    pub message_count: i32,
    pub created_at: DateTime<Utc>,
    pub last_message_at: Option<DateTime<Utc>>,
}

/// Looks up a discussion by identity key hash (Q1).
pub async fn find_by_key_hash(pool: &PgPool, key_hash: &[u8]) -> Result<Option<DiscussionRow>, sqlx::Error> {
    sqlx::query_as!(
        DiscussionRow,
        "SELECT d.id, d.display_url, d.site, d.title, d.status, d.merged_into_id, d.last_seq,
                d.message_count, d.created_at, d.last_message_at
         FROM discussion_identities di
         JOIN discussions d ON d.id = di.discussion_id
         WHERE di.key_hash = $1",
        key_hash
    )
    .fetch_optional(pool)
    .await
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<DiscussionRow>, sqlx::Error> {
    sqlx::query_as!(
        DiscussionRow,
        "SELECT id, display_url, site, title, status, merged_into_id, last_seq, message_count,
                created_at, last_message_at
         FROM discussions WHERE id = $1",
        id
    )
    .fetch_optional(pool)
    .await
}

pub async fn insert(
    conn: &mut PgConnection,
    id: Uuid,
    display_url: &str,
    site: &str,
    title: Option<&str>,
    created_by: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO discussions (id, display_url, site, title, created_by) VALUES ($1, $2, $3, $4, $5)",
        id,
        display_url,
        site,
        title,
        created_by
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Stores an identity key. Returns the discussion it points to, or `None` if the key already existed (Q4).
pub async fn insert_identity(
    conn: &mut PgConnection,
    key_hash: &[u8],
    key: &str,
    discussion_id: Uuid,
    kind: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO discussion_identities (key_hash, key, discussion_id, ruleset, kind)
         VALUES ($1, $2, $3, 1, $4)
         ON CONFLICT (key_hash) DO NOTHING
         RETURNING discussion_id",
        key_hash,
        key,
        discussion_id,
        kind
    )
    .fetch_optional(conn)
    .await
}
