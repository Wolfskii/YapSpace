//! Administrative commands behind `yapspace admin …` (docs/architecture/security.md §H.2: role changes happen only here).
//! Every change writes an audit event with no actor and metadata `{"via": "cli"}`.

use anyhow::{Context, bail};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    config::{Config, Env},
    db::audit,
    domain::{passwords::PasswordHasher, tokens, validation::normalize_username},
};

const DEV_PASSWORD: &str = "password123!";

/// Sets a user's role to `role` (`user`, `moderator` or `admin`).
pub async fn set_role(pool: &PgPool, username: &str, role: &str) -> anyhow::Result<()> {
    let username = normalize_username(username).map_err(|e| anyhow::anyhow!("{}: {}", e.field, e.message))?;
    let mut tx = pool.begin().await?;
    let row = sqlx::query!(
        "UPDATE users SET role = $2, updated_at = now() WHERE username = $1 AND status <> 'deleted' RETURNING id",
        username,
        role
    )
    .fetch_optional(&mut *tx)
    .await?
    .with_context(|| format!("no user named {username}"))?;
    audit::record(
        &mut tx,
        None,
        "user.role_changed",
        Some("user"),
        Some(row.id),
        json!({ "via": "cli", "role": role }),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Revokes every active session, or only the sessions of `username`. Returns how many were revoked.
pub async fn revoke_all_sessions(pool: &PgPool, username: Option<&str>) -> anyhow::Result<u64> {
    let mut tx = pool.begin().await?;
    let revoked = match username {
        Some(name) => {
            let name = normalize_username(name).map_err(|e| anyhow::anyhow!("{}: {}", e.field, e.message))?;
            let result = sqlx::query!(
                "UPDATE sessions SET revoked_at = now()
                 WHERE revoked_at IS NULL AND user_id = (SELECT id FROM users WHERE username = $1)",
                name
            )
            .execute(&mut *tx)
            .await?;
            result.rows_affected()
        }
        None => {
            let result = sqlx::query!("UPDATE sessions SET revoked_at = now() WHERE revoked_at IS NULL")
                .execute(&mut *tx)
                .await?;
            result.rows_affected()
        }
    };
    audit::record(
        &mut tx,
        None,
        "sessions.revoked",
        username.map(|_| "user"),
        None,
        json!({ "via": "cli", "count": revoked, "username": username }),
    )
    .await?;
    tx.commit().await?;
    Ok(revoked)
}

/// Creates demo users, discussions and messages for manual testing. Refuses to run in production.
pub async fn seed_dev(pool: &PgPool, config: &Config, hasher: &PasswordHasher) -> anyhow::Result<()> {
    if config.env == Env::Prod {
        bail!("seed-dev refuses to run with YAPSPACE_ENV=prod");
    }
    let hash = hasher.hash(DEV_PASSWORD.to_owned()).await?;
    let (_, recovery_hash) = tokens::new_recovery_code();

    let mut tx = pool.begin().await?;
    let mut users = Vec::new();
    for (name, role) in [("alice", "user"), ("bob", "user"), ("mod", "moderator")] {
        let id = Uuid::now_v7();
        sqlx::query!(
            "INSERT INTO users (id, username, role) VALUES ($1, $2, $3)
             ON CONFLICT (username) DO NOTHING",
            id,
            name,
            role
        )
        .execute(&mut *tx)
        .await?;
        let row = sqlx::query!("SELECT id FROM users WHERE username = $1", name)
            .fetch_one(&mut *tx)
            .await?;
        sqlx::query!(
            "INSERT INTO password_credentials (user_id, password_hash, recovery_code_hash)
             VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
            row.id,
            hash,
            recovery_hash.as_slice()
        )
        .execute(&mut *tx)
        .await?;
        users.push(row.id);
    }

    for (index, url) in [
        "https://example.com/articles/first",
        "https://example.com/articles/second",
        "https://example.com/articles/third",
    ]
    .iter()
    .enumerate()
    {
        let discussion = Uuid::now_v7();
        sqlx::query!(
            "INSERT INTO discussions (id, display_url, site, title, created_by)
             VALUES ($1, $2, 'example.com', $3, $4)",
            discussion,
            url,
            format!("Seeded discussion {}", index + 1),
            users[0]
        )
        .execute(&mut *tx)
        .await?;
        let key = format!("v1|url|{}", url.trim_start_matches("https://"));
        let key_hash = yapspace_identity::key_hash(&key);
        sqlx::query!(
            "INSERT INTO discussion_identities (key_hash, key, discussion_id, ruleset, kind)
             VALUES ($1, $2, $3, 1, 'url')",
            key_hash.as_slice(),
            key,
            discussion
        )
        .execute(&mut *tx)
        .await?;
        for seq in 1..=30i64 {
            let author = users[(seq as usize) % users.len()];
            sqlx::query!(
                "INSERT INTO messages (id, discussion_id, seq, author_id, body, client_msg_id)
                 VALUES ($1, $2, $3, $4, $5, $6)",
                Uuid::now_v7(),
                discussion,
                seq,
                author,
                format!("Seeded message {seq} in discussion {}", index + 1),
                Uuid::now_v7()
            )
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query!(
            "UPDATE discussions SET last_seq = 30, message_count = 30, last_message_at = now() WHERE id = $1",
            discussion
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
