//! Posting, reading, and deleting messages (data-model.md Q2, Q3, Q5; api.md §F.1 pagination and idempotency).

use std::net::IpAddr;

use uuid::Uuid;

use crate::{
    api::extractors::AuthUser,
    app::AppState,
    db::{
        discussions,
        messages::{self, MessageRow},
    },
    domain::{auth::is_unique_violation, validation::normalize_body},
    error::{AppError, FieldError},
};

pub const DEFAULT_PAGE: i64 = 50;
pub const MAX_PAGE: i64 = 100;
/// Accounts younger than a day may post into at most this many discussions per window (spam.rs).
const NEW_ACCOUNT_DISCUSSIONS: usize = crate::spam::NEW_ACCOUNT_DISCUSSION_LIMIT;

/// Result of posting. `created` is false when the same client id was already stored (an idempotent retry).
pub struct Posted {
    pub message: MessageRow,
    pub created: bool,
}

/// One page of history, always in ascending seq order.
pub struct Page {
    pub items: Vec<MessageRow>,
    pub has_more: bool,
    pub next_before_seq: Option<i64>,
    pub next_after_seq: Option<i64>,
}

async fn load(state: &AppState, id: Uuid) -> Result<MessageRow, AppError> {
    messages::find_by_id(&state.db, id)
        .await?
        .ok_or(AppError::NotFound)
}

async fn ensure_visible_discussion(state: &AppState, discussion_id: Uuid) -> Result<(), AppError> {
    match discussions::find_by_id(&state.db, discussion_id).await? {
        Some(row) if row.status != "hidden" => Ok(()),
        _ => Err(AppError::NotFound),
    }
}

/// Posts a message. Steps follow data-model.md Q5. The seq comes from the discussion row lock, and the message is
/// stored in the same transaction, so seqs are gapless. A retry with the same `client_msg_id` returns the stored
/// message and does not insert again.
pub async fn post(
    state: &AppState,
    author: &AuthUser,
    ip: IpAddr,
    discussion_id: Uuid,
    raw_body: &str,
    parent_id: Option<Uuid>,
    client_msg_id: Uuid,
) -> Result<Posted, AppError> {
    author.ensure_can_write()?;

    // A retry is not a new message, so it returns the stored one before any limit or spam check runs.
    if let Some(existing) =
        messages::find_by_client_id(&mut *state.db.acquire().await?, author.user_id, client_msg_id).await?
    {
        return Ok(Posted {
            message: load(state, existing).await?,
            created: false,
        });
    }

    state.limits.post_ip.check(&ip)?;
    if author.is_new_account() {
        state.limits.post_user_new.check(&author.user_id)?;
    } else {
        state.limits.post_user.check(&author.user_id)?;
    }
    let body = normalize_body(raw_body).map_err(|err| AppError::Validation(vec![err]))?;
    if state.spam.is_duplicate(author.user_id, &body) {
        return Err(AppError::Validation(vec![FieldError::new(
            "body",
            "duplicate_message",
            "you already posted this message recently",
        )]));
    }
    if author.is_new_account() {
        let discussions_seen = state.spam.note_discussion(author.user_id, discussion_id);
        if discussions_seen > NEW_ACCOUNT_DISCUSSIONS {
            return Err(AppError::RateLimited { retry_after_s: 300 });
        }
    }
    ensure_visible_discussion(state, discussion_id).await?;

    let mut tx = state.db.begin().await?;

    if let Some(existing) = messages::find_by_client_id(&mut tx, author.user_id, client_msg_id).await? {
        tx.rollback().await?;
        return Ok(Posted {
            message: load(state, existing).await?,
            created: false,
        });
    }

    if let Some(parent) = parent_id {
        match messages::status_in_discussion(&mut tx, discussion_id, parent).await? {
            Some(status) if status == "visible" => {}
            _ => return Err(AppError::InvalidParent),
        }
    }

    let Some(seq) = messages::allocate_seq(&mut tx, discussion_id).await? else {
        return Err(match messages::discussion_state(&mut tx, discussion_id).await? {
            Some((_, true)) => AppError::Conflict("discussion_merged"),
            Some((status, false)) if status == "locked" => AppError::DiscussionLocked,
            _ => AppError::NotFound,
        });
    };

    let id = Uuid::now_v7();
    match messages::insert(
        &mut tx,
        id,
        discussion_id,
        seq,
        author.user_id,
        parent_id,
        &body,
        client_msg_id,
    )
    .await
    {
        Ok(()) => {}
        Err(err) if is_unique_violation(&err, "messages_author_client_key") => {
            // A concurrent retry with the same client id won the race. Our seq increment rolls back with the transaction.
            tx.rollback().await?;
            let existing =
                messages::find_by_client_id(&mut *state.db.acquire().await?, author.user_id, client_msg_id)
                    .await?
                    .ok_or(AppError::NotFound)?;
            return Ok(Posted {
                message: load(state, existing).await?,
                created: false,
            });
        }
        Err(err) => return Err(err.into()),
    }
    tx.commit().await?;

    // Realtime publish happens here, after commit (realtime.md §G.3). Added in Phase 3.
    state.spam.remember(author.user_id, &body);
    metrics::counter!("messages_created_total").increment(1);

    Ok(Posted {
        message: load(state, id).await?,
        created: true,
    })
}

/// Reads a page of history. Give `before_seq` for older messages, `after_seq` for newer ones, or neither for the
/// newest page.
pub async fn list(
    state: &AppState,
    discussion_id: Uuid,
    before_seq: Option<i64>,
    after_seq: Option<i64>,
    limit: Option<i64>,
) -> Result<Page, AppError> {
    if before_seq.is_some() && after_seq.is_some() {
        return Err(AppError::Validation(vec![FieldError::new(
            "before_seq",
            "conflict",
            "give before_seq or after_seq, not both",
        )]));
    }
    for (name, value) in [("before_seq", before_seq), ("after_seq", after_seq)] {
        if value.is_some_and(|v| v < 0) {
            return Err(AppError::Validation(vec![FieldError::new(
                name,
                "range",
                "must be 0 or more",
            )]));
        }
    }
    let limit = limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE);
    ensure_visible_discussion(state, discussion_id).await?;

    if let Some(after) = after_seq {
        let mut rows = messages::page_after(&state.db, discussion_id, after, limit + 1).await?;
        let has_more = rows.len() as i64 > limit;
        rows.truncate(limit as usize);
        let next_after_seq = if has_more {
            rows.last().map(|m| m.seq)
        } else {
            None
        };
        return Ok(Page {
            items: rows,
            has_more,
            next_before_seq: None,
            next_after_seq,
        });
    }

    let before = before_seq.unwrap_or(i64::MAX);
    let mut rows = messages::page_before(&state.db, discussion_id, before, limit + 1).await?;
    let has_more = rows.len() as i64 > limit;
    rows.truncate(limit as usize);
    rows.reverse();
    let next_before_seq = if has_more {
        rows.first().map(|m| m.seq)
    } else {
        None
    };
    Ok(Page {
        items: rows,
        has_more,
        next_before_seq,
        next_after_seq: None,
    })
}

/// Reads one message. Messages in hidden discussions return 404.
pub async fn get(state: &AppState, id: Uuid) -> Result<MessageRow, AppError> {
    let row = load(state, id).await?;
    ensure_visible_discussion(state, row.discussion_id).await?;
    Ok(row)
}

/// Erases the body of one of your messages. Only the author can do this, and anyone else gets 404. Repeating the call
/// succeeds.
pub async fn delete(state: &AppState, author: &AuthUser, id: Uuid) -> Result<(), AppError> {
    author.ensure_can_write()?;
    let row = load(state, id).await?;
    if row.author_id != author.user_id {
        return Err(AppError::NotFound);
    }
    messages::delete_by_author(&state.db, id, author.user_id).await?;
    Ok(())
}
