//! Password and recovery-code credentials, kept apart from `users` so other sign-in methods can be added later.

use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// Stored secrets for one user. Hashes only, never plain text.
#[derive(Debug, Clone)]
pub struct CredentialRow {
    pub password_hash: String,
    pub recovery_code_hash: Vec<u8>,
}

pub async fn insert(
    conn: &mut PgConnection,
    user_id: Uuid,
    password_hash: &str,
    recovery_code_hash: &[u8],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO password_credentials (user_id, password_hash, recovery_code_hash) VALUES ($1, $2, $3)",
        user_id,
        password_hash,
        recovery_code_hash
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn find(pool: &PgPool, user_id: Uuid) -> Result<Option<CredentialRow>, sqlx::Error> {
    sqlx::query_as!(
        CredentialRow,
        "SELECT password_hash, recovery_code_hash FROM password_credentials WHERE user_id = $1",
        user_id
    )
    .fetch_optional(pool)
    .await
}

pub async fn update_password(
    conn: &mut PgConnection,
    user_id: Uuid,
    password_hash: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE password_credentials SET password_hash = $2, updated_at = now() WHERE user_id = $1",
        user_id,
        password_hash
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn update_recovery_code(
    conn: &mut PgConnection,
    user_id: Uuid,
    recovery_code_hash: &[u8],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE password_credentials SET recovery_code_hash = $2, updated_at = now() WHERE user_id = $1",
        user_id,
        recovery_code_hash
    )
    .execute(conn)
    .await?;
    Ok(())
}
