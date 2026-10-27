//! User blocks (migration 0002). A block hides the blocked user's messages on the client. The server still returns them.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct BlockRow {
    pub blocked_id: Uuid,
    pub username: String,
    pub created_at: DateTime<Utc>,
}

pub async fn list(pool: &PgPool, blocker: Uuid) -> Result<Vec<BlockRow>, sqlx::Error> {
    sqlx::query_as!(
        BlockRow,
        "SELECT b.blocked_id, u.username, b.created_at
         FROM user_blocks b JOIN users u ON u.id = b.blocked_id
         WHERE b.blocker_id = $1 AND u.status <> 'deleted'
         ORDER BY b.created_at DESC",
        blocker
    )
    .fetch_all(pool)
    .await
}

pub async fn count(conn: &mut PgConnection, blocker: Uuid) -> Result<i64, sqlx::Error> {
    let row = sqlx::query_scalar!(
        "SELECT count(*) AS \"count!\" FROM user_blocks WHERE blocker_id = $1",
        blocker
    )
    .fetch_one(conn)
    .await?;
    Ok(row)
}

/// Adds a block. Returns whether it was new.
pub async fn insert(conn: &mut PgConnection, blocker: Uuid, blocked: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "INSERT INTO user_blocks (blocker_id, blocked_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        blocker,
        blocked
    )
    .execute(conn)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn delete(pool: &PgPool, blocker: Uuid, blocked: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "DELETE FROM user_blocks WHERE blocker_id = $1 AND blocked_id = $2",
        blocker,
        blocked
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}
