//! Blocks (migration 0002). A block is personal: it changes what the blocker's client shows, not what others see.

use crate::{
    api::extractors::AuthUser,
    app::AppState,
    db::{
        blocks::{self, BlockRow},
        users,
    },
    domain::validation::normalize_username,
    error::{AppError, FieldError},
};

pub const MAX_BLOCKS: i64 = 1000;

pub async fn list(state: &AppState, user: &AuthUser) -> Result<Vec<BlockRow>, AppError> {
    Ok(blocks::list(&state.db, user.user_id).await?)
}

/// Blocks a user. Returns whether the block is new. Blocking yourself is refused.
pub async fn block(state: &AppState, user: &AuthUser, username: &str) -> Result<bool, AppError> {
    user.ensure_can_write()?;
    let name = normalize_username(username).map_err(|e| AppError::Validation(vec![e]))?;
    let target = users::find_by_username(&state.db, &name)
        .await?
        .ok_or(AppError::NotFound)?;
    if target.id == user.user_id {
        return Err(AppError::Validation(vec![FieldError::new(
            "username",
            "self",
            "you cannot block yourself",
        )]));
    }
    let mut tx = state.db.begin().await?;
    if blocks::count(&mut tx, user.user_id).await? >= MAX_BLOCKS {
        return Err(AppError::Conflict("too_many_blocks"));
    }
    let created = blocks::insert(&mut tx, user.user_id, target.id).await?;
    tx.commit().await?;
    Ok(created)
}

/// Removes a block. Removing one that does not exist is not an error.
pub async fn unblock(state: &AppState, user: &AuthUser, username: &str) -> Result<(), AppError> {
    let name = normalize_username(username).map_err(|e| AppError::Validation(vec![e]))?;
    if let Some(target) = users::find_by_username(&state.db, &name).await? {
        blocks::delete(&state.db, user.user_id, target.id).await?;
    }
    Ok(())
}
