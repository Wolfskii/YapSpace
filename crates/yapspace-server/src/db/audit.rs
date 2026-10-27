//! Audit events for security-relevant actions (docs/architecture/data-model.md §D.2 `audit_events`).

use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

pub async fn record(
    conn: &mut PgConnection,
    actor_id: Option<Uuid>,
    action: &str,
    target_type: Option<&str>,
    target_id: Option<Uuid>,
    metadata: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO audit_events (actor_id, action, target_type, target_id, metadata)
         VALUES ($1, $2, $3, $4, $5)",
        actor_id,
        action,
        target_type,
        target_id,
        metadata
    )
    .execute(conn)
    .await?;
    Ok(())
}
