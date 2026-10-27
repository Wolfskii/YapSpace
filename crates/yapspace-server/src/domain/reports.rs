//! Reports from users (docs/architecture/security.md §H.4, data-model.md §D.2 `reports`).

use uuid::Uuid;

use crate::{
    api::extractors::AuthUser,
    app::AppState,
    db::{
        discussions, messages,
        reports::{self, ReportRow},
        users,
    },
    error::{AppError, FieldError},
};

/// Reasons a report may give (api.md §F.2). Changing this list is an API change.
pub const REASONS: &[&str] = &[
    "spam",
    "harassment",
    "hate",
    "sexual",
    "violence",
    "self_harm",
    "illegal",
    "privacy",
    "wrong_discussion",
    "other",
];
pub const TARGET_TYPES: &[&str] = &["message", "user", "discussion"];
pub const MAX_DETAILS_CHARS: usize = 1000;

fn invalid(field: &str, message: &str) -> AppError {
    AppError::Validation(vec![FieldError::new(field, "invalid", message)])
}

/// Files a report. Reporting the same thing twice returns the existing report (200), not a second row.
pub async fn create(
    state: &AppState,
    reporter: &AuthUser,
    target_type: &str,
    target_id: Uuid,
    reason: &str,
    details: Option<&str>,
) -> Result<(ReportRow, bool), AppError> {
    reporter.ensure_can_write()?;
    if !TARGET_TYPES.contains(&target_type) {
        return Err(invalid("target_type", "must be message, user or discussion"));
    }
    if !REASONS.contains(&reason) {
        return Err(invalid("reason", "unknown reason"));
    }
    let details = details.map(str::trim).filter(|d| !d.is_empty());
    if details.is_some_and(|d| d.chars().count() > MAX_DETAILS_CHARS) {
        return Err(invalid("details", "must be 1000 characters or fewer"));
    }
    state.limits.report.check(&reporter.user_id)?;

    match target_type {
        "message" => {
            messages::find_by_id(&state.db, target_id)
                .await?
                .ok_or(AppError::NotFound)?;
        }
        "user" => {
            if target_id == reporter.user_id {
                return Err(invalid("target_id", "you cannot report yourself"));
            }
            users::find_by_id(&state.db, target_id)
                .await?
                .ok_or(AppError::NotFound)?;
        }
        _ => {
            discussions::find_by_id(&state.db, target_id)
                .await?
                .ok_or(AppError::NotFound)?;
        }
    }

    let mut tx = state.db.begin().await?;
    let result = reports::insert_or_existing(
        &mut tx,
        Uuid::now_v7(),
        reporter.user_id,
        target_type,
        target_id,
        reason,
        details,
    )
    .await?;
    tx.commit().await?;
    Ok(result)
}
