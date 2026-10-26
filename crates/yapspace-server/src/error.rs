//! Application errors and their RFC 9457 `application/problem+json` responses (see docs/architecture/api.md §F.1).
//!
//! `request_id` is added to the body by the `fill_request_id` middleware in `app.rs`, because the handler does
//! not see the request.

use axum::{
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_json::{Value, json};

/// One invalid field in a validation error.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FieldError {
    pub field: String,
    pub code: String,
    pub message: String,
}

impl FieldError {
    pub fn new(field: &str, code: &str, message: &str) -> Self {
        Self {
            field: field.into(),
            code: code.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("validation failed")]
    Validation(Vec<FieldError>),
    #[error("unauthorized")]
    Unauthorized,
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("forbidden")]
    Forbidden,
    #[error("account suspended")]
    AccountSuspended,
    #[error("discussion locked")]
    DiscussionLocked,
    #[error("not found")]
    NotFound,
    #[error("username taken")]
    UsernameTaken,
    #[error("conflict: {0}")]
    Conflict(&'static str),
    #[error("url unsupported: {0}")]
    UrlUnsupported(&'static str),
    #[error("invalid parent")]
    InvalidParent,
    #[error("rate limited")]
    RateLimited { retry_after_s: u64 },
    #[error("payload too large")]
    PayloadTooLarge,
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("service unavailable")]
    Unavailable,
    #[error("internal error")]
    Internal(anyhow::Error),
}

impl AppError {
    /// HTTP status, machine-readable code, and human title for each variant.
    fn meta(&self) -> (StatusCode, &'static str, &'static str) {
        match self {
            Self::Validation(_) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_failed",
                "Validation failed",
            ),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", "Unauthorized"),
            Self::InvalidCredentials => (
                StatusCode::UNAUTHORIZED,
                "invalid_credentials",
                "Invalid credentials",
            ),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden", "Forbidden"),
            Self::AccountSuspended => (StatusCode::FORBIDDEN, "account_suspended", "Account suspended"),
            Self::DiscussionLocked => (StatusCode::FORBIDDEN, "discussion_locked", "Discussion locked"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found", "Not found"),
            Self::UsernameTaken => (StatusCode::CONFLICT, "username_taken", "Username taken"),
            Self::Conflict(_) => (StatusCode::CONFLICT, "conflict", "Conflict"),
            Self::UrlUnsupported(_) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "url_unsupported",
                "URL not supported",
            ),
            Self::InvalidParent => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_parent",
                "Invalid parent message",
            ),
            Self::RateLimited { .. } => (StatusCode::TOO_MANY_REQUESTS, "rate_limited", "Too many requests"),
            Self::PayloadTooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                "payload_too_large",
                "Payload too large",
            ),
            Self::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request", "Bad request"),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "unavailable",
                "Service unavailable",
            ),
            Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal", "Internal error"),
        }
    }

    /// Detail text. Internal errors never reveal their cause.
    fn detail(&self) -> String {
        match self {
            Self::Validation(_) => "one or more fields are invalid".into(),
            Self::Conflict(reason) => (*reason).into(),
            Self::UrlUnsupported(reason) => format!("this page cannot be discussed: {reason}"),
            Self::BadRequest(detail) => detail.clone(),
            Self::Internal(_) => "an unexpected error occurred".into(),
            other => other.to_string(),
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => Self::NotFound,
            other => Self::Internal(other.into()),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, title) = self.meta();
        if let Self::Internal(err) = &self {
            tracing::error!(error = ?err, "internal error while handling request");
        }

        let mut body = json!({
            "type": format!("https://yapspace.app/errors/{code}"),
            "title": title,
            "status": status.as_u16(),
            "code": code,
            "detail": self.detail(),
        });
        let mut retry_after = None;
        match &self {
            Self::Validation(errors) => {
                body["errors"] = serde_json::to_value(errors).unwrap_or(Value::Null);
            }
            Self::UrlUnsupported(reason) => {
                body["reason"] = json!(reason);
            }
            Self::RateLimited { retry_after_s } => {
                body["retry_after_s"] = json!(retry_after_s);
                retry_after = Some(*retry_after_s);
            }
            _ => {}
        }

        let mut response = (status, body.to_string()).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        if let Some(seconds) = retry_after
            && let Ok(value) = HeaderValue::from_str(&seconds.to_string())
        {
            response.headers_mut().insert(header::RETRY_AFTER, value);
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;

    use super::*;

    async fn parts(err: AppError) -> (StatusCode, HeaderMapCopy, Value) {
        let response = err.into_response();
        let status = response.status();
        let content_type = response.headers().get(header::CONTENT_TYPE).cloned();
        let retry_after = response.headers().get(header::RETRY_AFTER).cloned();
        let bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        (
            status,
            HeaderMapCopy {
                content_type,
                retry_after,
            },
            serde_json::from_slice(&bytes).unwrap(),
        )
    }

    struct HeaderMapCopy {
        content_type: Option<HeaderValue>,
        retry_after: Option<HeaderValue>,
    }

    #[tokio::test]
    async fn validation_error_has_problem_shape_and_field_list() {
        let (status, headers, body) = parts(AppError::Validation(vec![FieldError::new(
            "body",
            "length",
            "must be 1-2000 characters",
        )]))
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(headers.content_type.unwrap(), "application/problem+json");
        assert_eq!(body["code"], "validation_failed");
        assert_eq!(body["status"], 422);
        assert_eq!(body["errors"][0]["field"], "body");
    }

    #[tokio::test]
    async fn rate_limit_sets_retry_after_header_and_field() {
        let (status, headers, body) = parts(AppError::RateLimited { retry_after_s: 42 }).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(headers.retry_after.unwrap(), "42");
        assert_eq!(body["retry_after_s"], 42);
    }

    #[tokio::test]
    async fn internal_error_never_leaks_its_cause() {
        let (status, _, body) =
            parts(AppError::Internal(anyhow::anyhow!("secret table name users_pw"))).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["detail"], "an unexpected error occurred");
        assert!(!body.to_string().contains("secret"));
    }

    #[tokio::test]
    async fn url_unsupported_carries_reason() {
        let (_, _, body) = parts(AppError::UrlUnsupported("blocked_host")).await;
        assert_eq!(body["code"], "url_unsupported");
        assert_eq!(body["reason"], "blocked_host");
    }

    #[test]
    fn row_not_found_maps_to_not_found() {
        assert!(matches!(
            AppError::from(sqlx::Error::RowNotFound),
            AppError::NotFound
        ));
    }
}
