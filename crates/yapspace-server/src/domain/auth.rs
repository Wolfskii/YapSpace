//! Accounts and sessions: register, log in, log out, recover, change password, rotate the recovery code
//! (docs/architecture/api.md §F.2, docs/architecture/security.md §H.1).

use chrono::Utc;
use serde_json::json;
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::{
    app::AppState,
    db::{
        audit,
        credentials::{self, CredentialRow},
        sessions, users::{self, UserRow},
    },
    domain::{
        passwords::PasswordHasher,
        tokens,
        validation::{normalize_username, truncate_label_opt, validate_password},
    },
    error::{AppError, FieldError},
};

/// Result of an operation that starts a session.
pub struct Issued {
    pub user: UserRow,
    pub session_token: String,
    /// Present on register and on recovery. Shown to the user once.
    pub recovery_code: Option<String>,
}

fn invalid_credentials() -> AppError {
    AppError::InvalidCredentials
}

fn internal(err: impl Into<anyhow::Error>) -> AppError {
    AppError::Internal(err.into())
}

/// True when `err` is a unique violation on the named constraint.
pub fn is_unique_violation(err: &sqlx::Error, constraint: &str) -> bool {
    match err {
        sqlx::Error::Database(db) => db.code().as_deref() == Some("23505") && db.constraint() == Some(constraint),
        _ => false,
    }
}

fn validation(err: FieldError) -> AppError {
    AppError::Validation(vec![err])
}

async fn new_session(
    state: &AppState,
    user_id: Uuid,
    label: Option<&str>,
) -> Result<String, AppError> {
    let (token, token_hash) = tokens::new_session_token();
    let mut conn = state.db.acquire().await?;
    sessions::insert(&mut conn, Uuid::now_v7(), user_id, &token_hash, label).await?;
    Ok(token)
}

fn ensure_registration_open(state: &AppState) -> Result<(), AppError> {
    if state.config.registration_open {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

/// Creates an account, a password credential, a recovery code and a first session. Registration is one transaction.
pub async fn register(
    state: &AppState,
    username: &str,
    password: &str,
    client_label: Option<&str>,
) -> Result<Issued, AppError> {
    ensure_registration_open(state)?;
    let username = normalize_username(username).map_err(validation)?;
    validate_password(password, &username).map_err(validation)?;

    let password_hash = state.hasher.hash(password.to_owned()).await.map_err(internal)?;
    let (recovery_code, recovery_hash) = tokens::new_recovery_code();
    let (session_token, token_hash) = tokens::new_session_token();
    let user_id = Uuid::now_v7();
    let label = truncate_label_opt(client_label);

    let mut tx = state.db.begin().await?;
    match users::insert(&mut tx, user_id, &username).await {
        Ok(()) => {}
        Err(err) if is_unique_violation(&err, "users_username_key") => return Err(AppError::UsernameTaken),
        Err(err) => return Err(err.into()),
    }
    credentials::insert(&mut tx, user_id, &password_hash, &recovery_hash).await?;
    sessions::insert(&mut tx, Uuid::now_v7(), user_id, &token_hash, label.as_deref()).await?;
    audit::record(&mut tx, Some(user_id), "user.registered", Some("user"), Some(user_id), json!({})).await?;
    tx.commit().await?;

    let user = users::find_by_id(&state.db, user_id).await?.ok_or(AppError::NotFound)?;
    Ok(Issued {
        user,
        session_token,
        recovery_code: Some(recovery_code),
    })
}

/// Checks a password against the stored hash and starts a session. Unknown users, deleted users and wrong passwords
/// all return the same error, after about the same amount of work.
pub async fn login(
    state: &AppState,
    username: &str,
    password: &str,
    client_label: Option<&str>,
) -> Result<Issued, AppError> {
    let Ok(username) = normalize_username(username) else {
        state.hasher.verify_dummy(password.to_owned()).await.map_err(internal)?;
        return Err(invalid_credentials());
    };
    let Some(user) = users::find_by_username(&state.db, &username).await? else {
        state.hasher.verify_dummy(password.to_owned()).await.map_err(internal)?;
        return Err(invalid_credentials());
    };
    let Some(creds) = credentials::find(&state.db, user.id).await? else {
        state.hasher.verify_dummy(password.to_owned()).await.map_err(internal)?;
        return Err(invalid_credentials());
    };
    let matches = state
        .hasher
        .verify(password.to_owned(), creds.password_hash.clone())
        .await
        .map_err(internal)?;
    if !matches {
        return Err(invalid_credentials());
    }

    if PasswordHasher::needs_rehash(&creds.password_hash) {
        // Best effort: a failed rehash only means the next login hashes again.
        if let Ok(new_hash) = state.hasher.hash(password.to_owned()).await {
            let mut conn = state.db.acquire().await?;
            if let Err(err) = credentials::update_password(&mut conn, user.id, &new_hash).await {
                tracing::warn!(error = %err, "could not store rehashed password");
            }
        }
    }

    let session_token = new_session(state, user.id, truncate_label_opt(client_label).as_deref()).await?;
    Ok(Issued {
        user,
        session_token,
        recovery_code: None,
    })
}

/// Revokes the current session.
pub async fn logout(state: &AppState, session_id: Uuid) -> Result<(), AppError> {
    sessions::revoke(&state.db, session_id).await?;
    Ok(())
}

/// Resets the password with a recovery code. The code is single-use: a new one is issued, and every session is revoked.
pub async fn recover(
    state: &AppState,
    username: &str,
    recovery_code: &str,
    new_password: &str,
    client_label: Option<&str>,
) -> Result<Issued, AppError> {
    let Ok(username) = normalize_username(username) else {
        state.hasher.verify_dummy(new_password.to_owned()).await.map_err(internal)?;
        return Err(invalid_credentials());
    };
    let Some(user) = users::find_by_username(&state.db, &username).await? else {
        state.hasher.verify_dummy(new_password.to_owned()).await.map_err(internal)?;
        return Err(invalid_credentials());
    };
    let creds = credentials::find(&state.db, user.id).await?.ok_or_else(invalid_credentials)?;
    let presented = tokens::hash_recovery_code(recovery_code);
    if !bool::from(presented.ct_eq(creds.recovery_code_hash.as_slice())) {
        return Err(invalid_credentials());
    }
    validate_password(new_password, &user.username).map_err(validation)?;

    let password_hash = state.hasher.hash(new_password.to_owned()).await.map_err(internal)?;
    let (new_code, new_code_hash) = tokens::new_recovery_code();
    let (session_token, token_hash) = tokens::new_session_token();
    let label = truncate_label_opt(client_label);

    let mut tx = state.db.begin().await?;
    credentials::update_password(&mut tx, user.id, &password_hash).await?;
    credentials::update_recovery_code(&mut tx, user.id, &new_code_hash).await?;
    sessions::revoke_all_for_user(&mut tx, user.id, None).await?;
    sessions::insert(&mut tx, Uuid::now_v7(), user.id, &token_hash, label.as_deref()).await?;
    audit::record(&mut tx, Some(user.id), "user.recovered", Some("user"), Some(user.id), json!({})).await?;
    tx.commit().await?;

    Ok(Issued {
        user,
        session_token,
        recovery_code: Some(new_code),
    })
}

/// Loads the stored credentials and checks the current password.
async fn verify_current_password(
    state: &AppState,
    user_id: Uuid,
    current_password: &str,
) -> Result<CredentialRow, AppError> {
    let creds = credentials::find(&state.db, user_id).await?.ok_or_else(invalid_credentials)?;
    let matches = state
        .hasher
        .verify(current_password.to_owned(), creds.password_hash.clone())
        .await
        .map_err(internal)?;
    if matches {
        Ok(creds)
    } else {
        Err(invalid_credentials())
    }
}

/// Changes the password and revokes every other session. The current session stays signed in.
pub async fn change_password(
    state: &AppState,
    user_id: Uuid,
    username: &str,
    session_id: Uuid,
    current_password: &str,
    new_password: &str,
) -> Result<(), AppError> {
    verify_current_password(state, user_id, current_password).await?;
    validate_password(new_password, username).map_err(validation)?;
    let password_hash = state.hasher.hash(new_password.to_owned()).await.map_err(internal)?;

    let mut tx = state.db.begin().await?;
    credentials::update_password(&mut tx, user_id, &password_hash).await?;
    sessions::revoke_all_for_user(&mut tx, user_id, Some(session_id)).await?;
    audit::record(&mut tx, Some(user_id), "user.password_changed", Some("user"), Some(user_id), json!({})).await?;
    tx.commit().await?;
    Ok(())
}

/// Replaces the recovery code. The password must be confirmed first. Returns the new code.
pub async fn rotate_recovery_code(
    state: &AppState,
    user_id: Uuid,
    current_password: &str,
) -> Result<String, AppError> {
    verify_current_password(state, user_id, current_password).await?;
    let (code, hash) = tokens::new_recovery_code();

    let mut tx = state.db.begin().await?;
    credentials::update_recovery_code(&mut tx, user_id, &hash).await?;
    audit::record(&mut tx, Some(user_id), "user.recovery_rotated", Some("user"), Some(user_id), json!({})).await?;
    tx.commit().await?;
    Ok(code)
}

/// True when the account is suspended now. A suspension whose end time has passed counts as lifted.
pub fn is_suspended(status: &str, suspended_until: Option<chrono::DateTime<Utc>>) -> bool {
    status == "suspended" && suspended_until.is_none_or(|until| until > Utc::now())
}

/// Account role. Moderators and admins get extra routes; `admin` is the only role that changes roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Moderator,
    Admin,
}

impl Role {
    pub fn parse(value: &str) -> Self {
        match value {
            "moderator" => Self::Moderator,
            "admin" => Self::Admin,
            _ => Self::User,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Moderator => "moderator",
            Self::Admin => "admin",
        }
    }

    pub fn is_moderator(self) -> bool {
        matches!(self, Self::Moderator | Self::Admin)
    }

    pub fn is_admin(self) -> bool {
        self == Self::Admin
    }
}
