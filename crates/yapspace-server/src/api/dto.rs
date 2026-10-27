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
