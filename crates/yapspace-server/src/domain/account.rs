//! Account deletion and export (docs/architecture/security.md §H.5).

use serde_json::json;
use uuid::Uuid;

use crate::{
    api::extractors::AuthUser,
    app::AppState,
    db::{account, audit, credentials, sessions},
    error::AppError,
};

/// Length of the random part of the tombstone name. The result is `deleted_` plus 12 hex characters.
const TOMBSTONE_HEX: usize = 12;

/// The name a deleted account keeps in the database. It is unique, free of personal data, and fits the username rules.
pub fn tombstone_name(user_id: Uuid) -> String {
    let hex: String = user_id.simple().to_string().chars().take(TOMBSTONE_HEX).collect();
    format!("deleted_{hex}")
}

/// Deletes the account after the password is confirmed. Sessions and credentials are removed now. Message bodies are
/// erased by the retention job.
pub async fn delete(state: &AppState, user: &AuthUser, password: &str) -> Result<(), AppError> {
    let creds = credentials::find(&state.db, user.user_id)
        .await?
        .ok_or(AppError::InvalidCredentials)?;
    let matches = state
        .hasher
        .verify(password.to_owned(), creds.password_hash)
        .await
        .map_err(AppError::Internal)?;
    if !matches {
        return Err(AppError::InvalidCredentials);
    }

    let mut tx = state.db.begin().await?;
    if !account::mark_deleted(&mut tx, user.user_id, &tombstone_name(user.user_id)).await? {
        return Err(AppError::NotFound);
    }
    account::delete_credentials(&mut tx, user.user_id).await?;
    sessions::revoke_all_for_user(&mut tx, user.user_id, None).await?;
    audit::record(
        &mut tx,
        Some(user.user_id),
        "user.deleted",
        Some("user"),
        Some(user.user_id),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
