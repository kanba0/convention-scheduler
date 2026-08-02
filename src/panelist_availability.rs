//! Structured availability windows for a panelist -- the machine-readable
//! counterpart to the free-text `availability_note`. No windows = available
//! whenever the program runs.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::error::AppError;
use crate::state::AppState;

/// One availability window, as returned to clients.
#[derive(Serialize)]
pub struct AvailabilityWindow {
    id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    starts_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    ends_at: OffsetDateTime,
}

#[derive(Deserialize)]
pub struct CreateWindow {
    #[serde(with = "time::serde::rfc3339")]
    starts_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    ends_at: OffsetDateTime,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/panelists/{panelist_id}/availability", get(list).post(add))
        .route(
            "/panelists/{panelist_id}/availability/{window_id}",
            delete(remove),
        )
}

/// `GET /panelists/{panelist_id}/availability` — a panelist's windows, earliest first, or 404.
async fn list(
    State(state): State<AppState>,
    Path(panelist_id): Path<Uuid>,
) -> Result<Json<Vec<AvailabilityWindow>>, AppError> {
    let exists = sqlx::query_scalar!(
        "SELECT EXISTS(SELECT 1 FROM panelists WHERE id = $1)",
        panelist_id,
    )
    .fetch_one(&state.pool)
    .await?;
    if exists != Some(true) {
        return Err(AppError::NotFound);
    }

    let windows = sqlx::query_as!(
        AvailabilityWindow,
        r#"
        SELECT id, starts_at, ends_at
        FROM panelist_availability
        WHERE panelist_id = $1
        ORDER BY starts_at
        "#,
        panelist_id,
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(windows))
}

/// `POST /panelists/{panelist_id}/availability` — add a window, return 201, or 404.
async fn add(
    State(state): State<AppState>,
    Path(panelist_id): Path<Uuid>,
    Json(body): Json<CreateWindow>,
) -> Result<(StatusCode, Json<AvailabilityWindow>), AppError> {
    // The SELECT yields a row only when the panelist exists, so a missing one inserts
    // nothing (-> None -> 404); the CHECK handles time-ordering (-> 422 via the mapping).
    let window = sqlx::query_as!(
        AvailabilityWindow,
        r#"
        INSERT INTO panelist_availability (panelist_id, starts_at, ends_at)
        SELECT p.id, $2, $3
        FROM panelists p
        WHERE p.id = $1
        RETURNING id, starts_at, ends_at
        "#,
        panelist_id,
        body.starts_at,
        body.ends_at,
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok((StatusCode::CREATED, Json(window)))
}

/// `DELETE /panelists/{panelist_id}/availability/{window_id}` — remove a window, or 404.
async fn remove(
    State(state): State<AppState>,
    Path((panelist_id, window_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    let deleted = sqlx::query!(
        "DELETE FROM panelist_availability WHERE id = $2 AND panelist_id = $1",
        panelist_id,
        window_id,
    )
    .execute(&state.pool)
    .await?
    .rows_affected();

    if deleted == 0 {
        Err(AppError::NotFound)
    } else {
        Ok(StatusCode::NO_CONTENT)
    }
}
