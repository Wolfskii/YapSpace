//! Messages: history pages (data-model.md Q2, Q3), single reads, insert with seq allocation (Q5), author delete.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// A message with its author and, if it is a reply, a short excerpt of its parent.
#[derive(Debug, Clone)]
pub struct MessageRow {
    pub id: Uuid,
    pub discussion_id: Uuid,
    pub seq: i64,
    pub body: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub edited_at: Option<DateTime<Utc>>,
    pub author_id: Uuid,
    pub author_username: String,
    pub author_status: String,
    pub parent_id: Option<Uuid>,
    pub parent_seq: Option<i64>,
    pub parent_status: Option<String>,
    pub parent_excerpt: Option<String>,
    pub parent_author_username: Option<String>,
}

/// Newest messages first, below `before_seq` (Q2). Use `i64::MAX` for the newest page.
pub async fn page_before(
    pool: &PgPool,
    discussion_id: Uuid,
    before_seq: i64,
    limit: i64,
) -> Result<Vec<MessageRow>, sqlx::Error> {
    sqlx::query_as!(
        MessageRow,
        r#"SELECT m.id, m.discussion_id, m.seq, m.body, m.status, m.created_at, m.edited_at,
                  m.author_id, u.username AS author_username, u.status AS author_status,
                  m.parent_id, p.seq AS "parent_seq?", p.status AS "parent_status?",
                  CASE WHEN p.status = 'visible' THEN left(p.body, 140) END AS "parent_excerpt?",
                  pu.username AS "parent_author_username?"
           FROM messages m
           JOIN users u ON u.id = m.author_id
           LEFT JOIN messages p ON p.id = m.parent_id
           LEFT JOIN users pu ON pu.id = p.author_id
           WHERE m.discussion_id = $1 AND m.seq < $2
           ORDER BY m.seq DESC
           LIMIT $3"#,
        discussion_id,
        before_seq,
        limit
    )
    .fetch_all(pool)
    .await
}

/// Oldest messages first, above `after_seq` (Q3).
pub async fn page_after(
    pool: &PgPool,
    discussion_id: Uuid,
    after_seq: i64,
    limit: i64,
) -> Result<Vec<MessageRow>, sqlx::Error> {
    sqlx::query_as!(
        MessageRow,
        r#"SELECT m.id, m.discussion_id, m.seq, m.body, m.status, m.created_at, m.edited_at,
                  m.author_id, u.username AS author_username, u.status AS author_status,
                  m.parent_id, p.seq AS "parent_seq?", p.status AS "parent_status?",
                  CASE WHEN p.status = 'visible' THEN left(p.body, 140) END AS "parent_excerpt?",
                  pu.username AS "parent_author_username?"
           FROM messages m
           JOIN users u ON u.id = m.author_id
           LEFT JOIN messages p ON p.id = m.parent_id
           LEFT JOIN users pu ON pu.id = p.author_id
           WHERE m.discussion_id = $1 AND m.seq > $2
           ORDER BY m.seq ASC
           LIMIT $3"#,
        discussion_id,
        after_seq,
        limit
    )
    .fetch_all(pool)
    .await
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<MessageRow>, sqlx::Error> {
    sqlx::query_as!(
        MessageRow,
        r#"SELECT m.id, m.discussion_id, m.seq, m.body, m.status, m.created_at, m.edited_at,
                  m.author_id, u.username AS author_username, u.status AS author_status,
                  m.parent_id, p.seq AS "parent_seq?", p.status AS "parent_status?",
                  CASE WHEN p.status = 'visible' THEN left(p.body, 140) END AS "parent_excerpt?",
                  pu.username AS "parent_author_username?"
           FROM messages m
           JOIN users u ON u.id = m.author_id
           LEFT JOIN messages p ON p.id = m.parent_id
           LEFT JOIN users pu ON pu.id = p.author_id
           WHERE m.id = $1"#,
        id
    )
    .fetch_optional(pool)
    .await
}

/// Finds a message this author already sent with this client id. Used for idempotent retries.
pub async fn find_by_client_id(
    conn: &mut PgConnection,
    author_id: Uuid,
    client_msg_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM messages WHERE author_id = $1 AND client_msg_id = $2",
        author_id,
        client_msg_id
    )
    .fetch_optional(conn)
    .await
}

/// Status of a message in a discussion. Used to validate a parent.
pub async fn status_in_discussion(
    conn: &mut PgConnection,
    discussion_id: Uuid,
    message_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT status FROM messages WHERE discussion_id = $1 AND id = $2",
        discussion_id,
        message_id
    )
    .fetch_optional(conn)
    .await
}

/// Takes the next seq for a discussion. The row lock on the discussion serializes writers. Returns `None` when the
/// discussion is locked, merged, or missing.
pub async fn allocate_seq(conn: &mut PgConnection, discussion_id: Uuid) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar!(
        "UPDATE discussions
         SET last_seq = last_seq + 1, message_count = message_count + 1, last_message_at = now()
         WHERE id = $1 AND status = 'open' AND merged_into_id IS NULL
         RETURNING last_seq",
        discussion_id
    )
    .fetch_optional(conn)
    .await
}

pub async fn discussion_state(
    conn: &mut PgConnection,
    discussion_id: Uuid,
) -> Result<Option<(String, bool)>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT status, merged_into_id IS NOT NULL AS \"merged!\" FROM discussions WHERE id = $1",
        discussion_id
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|r| (r.status, r.merged)))
}

#[allow(clippy::too_many_arguments)]
pub async fn insert(
    conn: &mut PgConnection,
    id: Uuid,
    discussion_id: Uuid,
    seq: i64,
    author_id: Uuid,
    parent_id: Option<Uuid>,
    body: &str,
    client_msg_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO messages (id, discussion_id, seq, author_id, parent_id, body, client_msg_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
        id,
        discussion_id,
        seq,
        author_id,
        parent_id,
        body,
        client_msg_id
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Erases the body of one of the author's messages. Returns whether a row changed.
pub async fn delete_by_author(pool: &PgPool, message_id: Uuid, author_id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "UPDATE messages SET status = 'deleted', body = '', status_changed_at = now()
         WHERE id = $1 AND author_id = $2 AND status <> 'deleted'",
        message_id,
        author_id
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}
