//! User rows. Soft-deleted users are excluded from every lookup here.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct UserRow {
    pub id: Uuid,
    pub username: String,
    pub role: String,
    pub status: String,
    pub suspended_until: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

pub async fn insert(conn: &mut PgConnection, id: Uuid, username: &str) -> Result<(), sqlx::Error> {
    sqlx::query!("INSERT INTO users (id, username) VALUES ($1, $2)", id, username)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn find_by_username(pool: &PgPool, username: &str) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as!(
        UserRow,
        "SELECT id, username, role, status, suspended_until, created_at
         FROM users WHERE username = $1 AND status <> 'deleted'",
        username
    )
    .fetch_optional(pool)
    .await
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as!(
        UserRow,
        "SELECT id, username, role, status, suspended_until, created_at
         FROM users WHERE id = $1 AND status <> 'deleted'",
        id
    )
    .fetch_optional(pool)
    .await
}
