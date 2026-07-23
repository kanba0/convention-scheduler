//! The `panelists` resource — hosts, scoped to a convention: list/create under
//! `/conventions/{id}/panelists`, a single host at `/panelists/{id}`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::error::AppError;
use crate::state::AppState;

/// A full panelist row, as returned to clients.
#[derive(Serialize)]
pub struct Panelist {
    pub(crate) id: Uuid,
    pub(crate) convention_id: Uuid,
    pub(crate) nick: String,
    // Nullable: a human memo only. Structured availability is a separate table later (see TODO.md).
    pub(crate) availability_note: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub(crate) created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub(crate) updated_at: OffsetDateTime,
}

/// Accepted on POST. The convention comes from the path, not the body.
#[derive(Deserialize)]
pub struct CreatePanelist {
    nick: String,
    availability_note: Option<String>,
}

/// Accepted on PATCH — every field optional; `None` leaves that column untouched.
#[derive(Deserialize)]
pub struct UpdatePanelist {
    nick: Option<String>,
    availability_note: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/conventions/{convention_id}/panelists",
            get(list).post(create),
        )
        .route("/panelists/{id}", get(get_one).patch(update).delete(delete))
}

/// `GET /conventions/{convention_id}/panelists` — hosts of one convention, by nick.
async fn list(
    State(state): State<AppState>,
    Path(convention_id): Path<Uuid>,
) -> Result<Json<Vec<Panelist>>, AppError> {
    let panelists = sqlx::query_as!(
        Panelist,
        r#"
        SELECT id, convention_id, nick, availability_note, created_at, updated_at
        FROM panelists
        WHERE convention_id = $1
        ORDER BY nick
        "#,
        convention_id,
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(panelists))
}

/// `POST /conventions/{convention_id}/panelists` — create one, return 201.
async fn create(
    State(state): State<AppState>,
    Path(convention_id): Path<Uuid>,
    Json(body): Json<CreatePanelist>,
) -> Result<(StatusCode, Json<Panelist>), AppError> {
    let panelist = sqlx::query_as!(
        Panelist,
        r#"
        INSERT INTO panelists (convention_id, nick, availability_note)
        VALUES ($1, $2, $3)
        RETURNING id, convention_id, nick, availability_note, created_at, updated_at
        "#,
        convention_id,
        body.nick,
        body.availability_note,
    )
    .fetch_one(&state.pool)
    .await?;

    Ok((StatusCode::CREATED, Json(panelist)))
}

/// `GET /panelists/{id}` — one host, or 404.
async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Panelist>, AppError> {
    let panelist = sqlx::query_as!(
        Panelist,
        r#"
        SELECT id, convention_id, nick, availability_note, created_at, updated_at
        FROM panelists
        WHERE id = $1
        "#,
        id,
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(Json(panelist))
}

/// `PATCH /panelists/{id}` — partial update, or 404.
async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdatePanelist>,
) -> Result<Json<Panelist>, AppError> {
    // COALESCE means an omitted field keeps its column; a consequence is that
    // availability_note can be overwritten but not cleared back to NULL here.
    let panelist = sqlx::query_as!(
        Panelist,
        r#"
        UPDATE panelists
        SET nick              = COALESCE($2, nick),
            availability_note = COALESCE($3, availability_note)
        WHERE id = $1
        RETURNING id, convention_id, nick, availability_note, created_at, updated_at
        "#,
        id,
        body.nick,
        body.availability_note,
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(Json(panelist))
}

/// `DELETE /panelists/{id}` — 204 on success, 404 if it wasn't there.
async fn delete(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let result = sqlx::query!("DELETE FROM panelists WHERE id = $1", id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        Err(AppError::NotFound)
    } else {
        Ok(StatusCode::NO_CONTENT)
    }
}
