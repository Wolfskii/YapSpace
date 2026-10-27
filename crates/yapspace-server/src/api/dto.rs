//! Response shapes shared by the endpoints (docs/architecture/api.md §F.3). Timestamps are RFC 3339 with milliseconds.

use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::db::users::UserRow;

/// Serializes `DateTime<Utc>` as RFC 3339 with milliseconds and a `Z` suffix.
pub mod rfc3339_ms {
    use chrono::{DateTime, SecondsFormat, Utc};
    use serde::Serializer;

    pub fn serialize<S: Serializer>(value: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&value.to_rfc3339_opts(SecondsFormat::Millis, true))
    }
}

pub mod rfc3339_ms_opt {
    use chrono::{DateTime, SecondsFormat, Utc};
    use serde::Serializer;

    pub fn serialize<S: Serializer>(value: &Option<DateTime<Utc>>, serializer: S) -> Result<S::Ok, S::Error> {
        match value {
            Some(at) => serializer.serialize_str(&at.to_rfc3339_opts(SecondsFormat::Millis, true)),
            None => serializer.serialize_none(),
        }
    }
}

/// The signed-in user's own profile.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UserDto {
    pub id: Uuid,
    pub username: String,
    pub role: String,
    #[serde(with = "rfc3339_ms")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTime<Utc>,
}

/// A user as other people see them. No role and no status.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicUserDto {
    pub id: Uuid,
    pub username: String,
    #[serde(with = "rfc3339_ms")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTime<Utc>,
}

/// Returned by register, login and recovery. `recovery_code` is shown once, on register and recovery only.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AuthResponse {
    pub user: UserDto,
    pub session_token: String,
    pub recovery_code: Option<String>,
}

pub fn user_dto(row: &UserRow) -> UserDto {
    UserDto {
        id: row.id,
        username: row.username.clone(),
        role: row.role.clone(),
        created_at: row.created_at,
    }
}

pub fn public_user_dto(row: &UserRow) -> PublicUserDto {
    PublicUserDto {
        id: row.id,
        username: row.username.clone(),
        created_at: row.created_at,
    }
}

/// A discussion. `merged_from` is set when the requested id was merged into this discussion.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DiscussionDto {
    pub id: Uuid,
    pub display_url: String,
    pub site: String,
    pub title: Option<String>,
    pub status: String,
    pub message_count: i32,
    pub last_seq: i64,
    #[serde(with = "rfc3339_ms")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTime<Utc>,
    #[serde(with = "rfc3339_ms_opt")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub last_message_at: Option<DateTime<Utc>>,
    pub merged_from: Option<Uuid>,
}

/// The identity a page resolved to. This is shown in the panel, so users can check which discussion they are on.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct IdentityDto {
    /// `url`, `canonical`, or `adapter`.
    pub kind: String,
    pub display_url: String,
    pub site: String,
    pub ruleset: u16,
}

pub fn discussion_dto(
    row: &crate::db::discussions::DiscussionRow,
    merged_from: Option<Uuid>,
) -> DiscussionDto {
    DiscussionDto {
        id: row.id,
        display_url: row.display_url.clone(),
        site: row.site.clone(),
        title: row.title.clone(),
        status: row.status.clone(),
        message_count: row.message_count,
        last_seq: row.last_seq,
        created_at: row.created_at,
        last_message_at: row.last_message_at,
        merged_from,
    }
}

/// Author of a message. `null` when the author's account has been deleted.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AuthorDto {
    pub id: Uuid,
    pub username: String,
}

/// The message a reply points to. The excerpt and author are hidden when that message is not visible.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ParentDto {
    pub id: Uuid,
    pub seq: Option<i64>,
    pub author_username: Option<String>,
    pub excerpt: Option<String>,
    pub status: String,
}

/// A message. `body` is `null` unless the message is visible, so removed and deleted text never leaves the server.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MessageDto {
    pub id: Uuid,
    pub discussion_id: Uuid,
    pub seq: i64,
    pub author: Option<AuthorDto>,
    pub parent: Option<ParentDto>,
    pub body: Option<String>,
    pub status: String,
    #[serde(with = "rfc3339_ms")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTime<Utc>,
    #[serde(with = "rfc3339_ms_opt")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub edited_at: Option<DateTime<Utc>>,
}

pub fn message_dto(row: &crate::db::messages::MessageRow) -> MessageDto {
    let visible = row.status == "visible";
    let parent_visible = row.parent_status.as_deref() == Some("visible");
    MessageDto {
        id: row.id,
        discussion_id: row.discussion_id,
        seq: row.seq,
        author: (row.author_status != "deleted").then(|| AuthorDto {
            id: row.author_id,
            username: row.author_username.clone(),
        }),
        parent: row.parent_id.map(|id| ParentDto {
            id,
            seq: row.parent_seq,
            author_username: if parent_visible {
                row.parent_author_username.clone()
            } else {
                None
            },
            excerpt: if parent_visible {
                row.parent_excerpt.clone()
            } else {
                None
            },
            status: row.parent_status.clone().unwrap_or_else(|| "deleted".into()),
        }),
        body: visible.then(|| row.body.clone()),
        status: row.status.clone(),
        created_at: row.created_at,
        edited_at: row.edited_at,
    }
}
