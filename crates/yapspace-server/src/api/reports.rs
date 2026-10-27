//! `POST /v1/reports` (docs/architecture/api.md §F.2).

use axum::{Json, extract::State, http::StatusCode};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    api::{
        dto::rfc3339_ms,
        extractors::{ApiJson, AuthUser},
    },
    app::AppState,
    db::reports::ReportRow,
    domain::reports as service,
    error::AppError,
};

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateReportRequest {
    /// `message`, `user` or `discussion`.
    pub target_type: String,
    pub target_id: Uuid,
    /// One of the reasons in api.md §F.2, for example `spam` or `wrong_discussion`.
    pub reason: String,
    #[serde(default)]
    pub details: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReportDto {
    pub id: Uuid,
    pub target_type: String,
    pub target_id: Uuid,
    pub reason: String,
    pub details: Option<String>,
    pub status: String,
    #[serde(with = "rfc3339_ms")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTime<Utc>,
}

pub fn report_dto(row: &ReportRow) -> ReportDto {
    ReportDto {
        id: row.id,
        target_type: row.target_type.clone(),
        target_id: row.target_id,
        reason: row.reason.clone(),
        details: row.details.clone(),
        status: row.status.clone(),
        created_at: row.created_at,
    }
}

/// Reports something. Reporting the same thing again returns the existing report with 200.
#[utoipa::path(
    post,
    path = "/v1/reports",
    tag = "reports",
    security(("session_token" = [])),
    request_body = CreateReportRequest,
    responses(
        (status = 201, description = "Report filed", body = ReportDto),
        (status = 200, description = "Already reported", body = ReportDto),
        (status = 401, description = "Not signed in"),
        (status = 404, description = "Target not found"),
        (status = 422, description = "Validation failed"),
        (status = 429, description = "Too many reports today"),
    )
)]
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    ApiJson(body): ApiJson<CreateReportRequest>,
) -> Result<(StatusCode, Json<ReportDto>), AppError> {
    let (row, created) = service::create(
        &state,
        &user,
        &body.target_type,
        body.target_id,
        &body.reason,
        body.details.as_deref(),
    )
    .await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(report_dto(&row))))
}
