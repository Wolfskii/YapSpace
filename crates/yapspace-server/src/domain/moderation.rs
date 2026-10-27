//! Moderator actions (docs/architecture/security.md §H.4). Every action writes a `moderation_actions` row in the same
//! transaction, and publishes the change to live subscribers after commit.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    api::{dto::discussion_dto, extractors::AuthUser},
    app::AppState,
    db::{
        discussions, messages,
        moderation::{self, record},
        reports::{self, ReportRow},
    },
    error::AppError,
    realtime,
};

pub const MAX_REASON_CHARS: usize = 1000;
pub const QUEUE_PAGE: i64 = 50;

/// One page of open reports. The cursor is an opaque string around `(created_at, id)`.
pub struct QueuePage {
    pub items: Vec<ReportRow>,
    pub next_cursor: Option<String>,
}

fn encode_cursor(row: &ReportRow) -> String {
    URL_SAFE_NO_PAD.encode(format!("{}|{}", row.created_at.to_rfc3339(), row.id))
}

fn decode_cursor(cursor: &str) -> Result<(DateTime<Utc>, Uuid), AppError> {
    let bad = || AppError::BadRequest("cursor is not valid".into());
    let raw = URL_SAFE_NO_PAD.decode(cursor).map_err(|_| bad())?;
    let text = String::from_utf8(raw).map_err(|_| bad())?;
    let (at, id) = text.split_once('|').ok_or_else(bad)?;
    let at = DateTime::parse_from_rfc3339(at)
        .map_err(|_| bad())?
        .with_timezone(&Utc);
    let id: Uuid = id.parse().map_err(|_| bad())?;
    Ok((at, id))
}

pub async fn queue(state: &AppState, cursor: Option<&str>) -> Result<QueuePage, AppError> {
    let after = cursor.map(decode_cursor).transpose()?;
    let items = reports::page_open(&state.db, after, QUEUE_PAGE + 1).await?;
    let has_more = items.len() as i64 > QUEUE_PAGE;
    let mut items = items;
    items.truncate(QUEUE_PAGE as usize);
    let next_cursor = if has_more {
        items.last().map(encode_cursor)
    } else {
        None
    };
    Ok(QueuePage { items, next_cursor })
}

fn check_reason(reason: Option<&str>) -> Result<Option<String>, AppError> {
    let trimmed = reason.map(str::trim).filter(|r| !r.is_empty()).map(str::to_owned);
    if trimmed
        .as_deref()
        .is_some_and(|r| r.chars().count() > MAX_REASON_CHARS)
    {
        return Err(AppError::Validation(vec![crate::error::FieldError::new(
            "reason",
            "length",
            "must be 1000 characters or fewer",
        )]));
    }
    Ok(trimmed)
}

/// Marks a report as actioned or dismissed.
pub async fn resolve(
    state: &AppState,
    moderator: &AuthUser,
    report_id: Uuid,
    outcome: &str,
    note: Option<&str>,
) -> Result<(), AppError> {
    moderator.ensure_can_write()?;
    if outcome != "actioned" && outcome != "dismissed" {
        return Err(AppError::Validation(vec![crate::error::FieldError::new(
            "outcome",
            "invalid",
            "must be actioned or dismissed",
        )]));
    }
    let note = check_reason(note)?;
    let mut tx = state.db.begin().await?;
    if !reports::resolve(&mut tx, report_id, outcome, moderator.user_id).await? {
        return Err(AppError::Conflict("report_already_resolved"));
    }
    if outcome == "dismissed" {
        record(
            &mut tx,
            moderator.user_id,
            "dismiss_report",
            "report",
            report_id,
            Some(report_id),
            note.as_deref(),
            None,
        )
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn message_action(
    state: &AppState,
    moderator: &AuthUser,
    message_id: Uuid,
    status: &str,
    action: &str,
    reason: Option<&str>,
    report_id: Option<Uuid>,
) -> Result<(), AppError> {
    moderator.ensure_can_write()?;
    let reason = check_reason(reason)?;
    let mut tx = state.db.begin().await?;
    let before = messages::find_by_id(&state.db, message_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !moderation::set_message_status(&mut tx, message_id, status).await? {
        return Err(AppError::NotFound);
    }
    record(
        &mut tx,
        moderator.user_id,
        action,
        "message",
        message_id,
        report_id,
        reason.as_deref(),
        None,
    )
    .await?;
    if let Some(report) = report_id {
        reports::resolve(&mut tx, report, "actioned", moderator.user_id).await?;
    }
    tx.commit().await?;

    let updated = messages::find_by_id(&state.db, message_id)
        .await?
        .unwrap_or(before);
    realtime::publish_updated(
        state,
        updated.discussion_id,
        updated.seq,
        crate::api::dto::message_dto(&updated),
    );
    Ok(())
}

pub async fn remove_message(
    state: &AppState,
    moderator: &AuthUser,
    message_id: Uuid,
    reason: Option<&str>,
    report_id: Option<Uuid>,
) -> Result<(), AppError> {
    message_action(
        state,
        moderator,
        message_id,
        "removed",
        "remove_message",
        reason,
        report_id,
    )
    .await
}

pub async fn restore_message(
    state: &AppState,
    moderator: &AuthUser,
    message_id: Uuid,
) -> Result<(), AppError> {
    message_action(
        state,
        moderator,
        message_id,
        "visible",
        "restore_message",
        None,
        None,
    )
    .await
}

/// Refuses actions against a moderator or admin unless the actor is an admin. Nobody may act on themselves.
async fn ensure_may_act_on(state: &AppState, actor: &AuthUser, target: Uuid) -> Result<(), AppError> {
    if target == actor.user_id {
        return Err(AppError::Forbidden);
    }
    let mut conn = state.db.acquire().await?;
    let role = moderation::role_of(&mut conn, target)
        .await?
        .ok_or(AppError::NotFound)?;
    let target_is_staff = role == "moderator" || role == "admin";
    if target_is_staff && !actor.role.is_admin() {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

pub async fn suspend_user(
    state: &AppState,
    moderator: &AuthUser,
    target: Uuid,
    reason: Option<&str>,
    until: Option<DateTime<Utc>>,
    report_id: Option<Uuid>,
) -> Result<(), AppError> {
    moderator.ensure_can_write()?;
    ensure_may_act_on(state, moderator, target).await?;
    let reason = check_reason(reason)?;
    let mut tx = state.db.begin().await?;
    if !moderation::suspend_user(&mut tx, target, until).await? {
        return Err(AppError::NotFound);
    }
    record(
        &mut tx,
        moderator.user_id,
        "suspend_user",
        "user",
        target,
        report_id,
        reason.as_deref(),
        until,
    )
    .await?;
    if let Some(report) = report_id {
        reports::resolve(&mut tx, report, "actioned", moderator.user_id).await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn unsuspend_user(state: &AppState, moderator: &AuthUser, target: Uuid) -> Result<(), AppError> {
    moderator.ensure_can_write()?;
    ensure_may_act_on(state, moderator, target).await?;
    let mut tx = state.db.begin().await?;
    if !moderation::unsuspend_user(&mut tx, target).await? {
        return Err(AppError::NotFound);
    }
    record(
        &mut tx,
        moderator.user_id,
        "unsuspend_user",
        "user",
        target,
        None,
        None,
        None,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Locks, unlocks, hides or unhides a discussion, and tells live subscribers.
pub async fn set_discussion_status(
    state: &AppState,
    moderator: &AuthUser,
    discussion_id: Uuid,
    status: &str,
    action: &str,
    reason: Option<&str>,
) -> Result<(), AppError> {
    moderator.ensure_can_write()?;
    let reason = check_reason(reason)?;
    let mut tx = state.db.begin().await?;
    if !moderation::set_discussion_status(&mut tx, discussion_id, status).await? {
        return Err(AppError::NotFound);
    }
    record(
        &mut tx,
        moderator.user_id,
        action,
        "discussion",
        discussion_id,
        None,
        reason.as_deref(),
        None,
    )
    .await?;
    tx.commit().await?;

    let row = discussions::find_by_id(&state.db, discussion_id)
        .await?
        .ok_or(AppError::NotFound)?;
    realtime::publish_discussion(state, &row, discussion_dto(&row, None));
    Ok(())
}
