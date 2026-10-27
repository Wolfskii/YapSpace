//! Discussion resolution, creation and lookup (docs/architecture/url-identity.md §E.5, data-model.md Q1/Q4).

use uuid::Uuid;
use yapspace_identity::{Candidate, candidates, key_hash};

use crate::{
    app::AppState,
    db::discussions::{self, DiscussionRow},
    domain::validation::sanitize_title,
    error::AppError,
};

/// The result of resolving a page. `discussion` is `None` if no discussion exists yet.
pub struct Resolution {
    pub identity: Candidate,
    pub discussion: Option<DiscussionRow>,
    /// Set when the discussion was merged into another one. Holds the id that was asked for.
    pub merged_from: Option<Uuid>,
}

fn candidates_for(state: &AppState, url: &str, canonical: Option<&str>) -> Result<Vec<Candidate>, AppError> {
    candidates(url, canonical, &state.config.extra_blocked_hosts)
        .map_err(|reason| AppError::UrlUnsupported(reason.as_str()))
}

/// Visible means not hidden. Hiding is a moderator action (Phase 5), so every user sees the same thing for now.
fn visible(row: DiscussionRow) -> Option<DiscussionRow> {
    (row.status != "hidden").then_some(row)
}

/// Follows one merge hop, so a merged discussion resolves to its target.
async fn follow_merge(
    state: &AppState,
    row: DiscussionRow,
) -> Result<(DiscussionRow, Option<Uuid>), AppError> {
    match row.merged_into_id {
        None => Ok((row, None)),
        Some(target) => {
            let source = row.id;
            let target_row = discussions::find_by_id(&state.db, target)
                .await?
                .and_then(visible)
                .ok_or(AppError::NotFound)?;
            Ok((target_row, Some(source)))
        }
    }
}

/// Looks up the first identity candidate that has a discussion. Never creates anything.
pub async fn resolve(state: &AppState, url: &str, canonical: Option<&str>) -> Result<Resolution, AppError> {
    let list = candidates_for(state, url, canonical)?;
    let mut found = None;
    for candidate in &list {
        let hash = key_hash(&candidate.key);
        if let Some(row) = discussions::find_by_key_hash(&state.db, &hash)
            .await?
            .and_then(visible)
        {
            found = Some(row);
            break;
        }
    }
    let (discussion, merged_from) = match found {
        Some(row) => {
            let (row, merged_from) = follow_merge(state, row).await?;
            (Some(row), merged_from)
        }
        None => (None, None),
    };
    Ok(Resolution {
        identity: list[0].clone(),
        discussion,
        merged_from,
    })
}

/// Creates the discussion for a page, or returns the existing one. Returns `(discussion, created)`.
///
/// Two requests can race to create the same page. The unique key on `discussion_identities` makes exactly one of
/// them win. The loser rolls back and returns the winner's discussion.
pub async fn create(
    state: &AppState,
    created_by: Uuid,
    url: &str,
    canonical: Option<&str>,
    title: Option<&str>,
) -> Result<(DiscussionRow, bool), AppError> {
    let list = candidates_for(state, url, canonical)?;
    let primary = &list[0];

    let resolved = resolve(state, url, canonical).await?;
    if let Some(existing) = resolved.discussion {
        return Ok((existing, false));
    }

    let id = Uuid::now_v7();
    let title = title.and_then(sanitize_title);
    let mut tx = state.db.begin().await?;
    discussions::insert(
        &mut tx,
        id,
        &primary.display_url,
        &primary.site,
        title.as_deref(),
        created_by,
    )
    .await?;
    let stored = discussions::insert_identity(
        &mut tx,
        &key_hash(&primary.key),
        &primary.key,
        id,
        primary.kind.as_str(),
    )
    .await?;
    if stored.is_none() {
        // Another request stored this page first. Dropping the transaction undoes the discussion row.
        drop(tx);
        let existing = resolve(state, url, canonical)
            .await?
            .discussion
            .ok_or(AppError::NotFound)?;
        return Ok((existing, false));
    }
    tx.commit().await?;

    let created = discussions::find_by_id(&state.db, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok((created, true))
}

/// Loads a discussion by id. A merged discussion returns its target, and `merged_from` names the id that was asked
/// for.
pub async fn get(state: &AppState, id: Uuid) -> Result<(DiscussionRow, Option<Uuid>), AppError> {
    let row = discussions::find_by_id(&state.db, id)
        .await?
        .and_then(visible)
        .ok_or(AppError::NotFound)?;
    follow_merge(state, row).await
}
