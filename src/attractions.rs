//! The `attractions` resource — panels and contests, scoped to a convention:
//! list/create under `/conventions/{id}/attractions`, a single one at `/attractions/{id}`.
//! Host assignment (the attraction_panelists many-to-many) is a separate slice.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::error::AppError;
use crate::state::AppState;

/// What an attraction *is*. Mirrors the `attraction_kind` Postgres ENUM.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "attraction_kind", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum AttractionKind {
    Panel,
    Contest,
}

/// A full attraction row, as returned to clients.
#[derive(Serialize)]
pub struct Attraction {
    id: Uuid,
    convention_id: Uuid,
    title: String,
    kind: AttractionKind,
    duration_minutes: i32,
    description: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
}

/// Accepted on POST. The convention comes from the path, not the body.
#[derive(Deserialize)]
pub struct CreateAttraction {
    title: String,
    kind: AttractionKind,
    duration_minutes: i32,
    description: Option<String>,
}

/// Accepted on PATCH — every field optional; `None` leaves that column untouched.
#[derive(Deserialize)]
pub struct UpdateAttraction {
    title: Option<String>,
    kind: Option<AttractionKind>,
    duration_minutes: Option<i32>,
    description: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/conventions/{convention_id}/attractions",
            get(list).post(create),
        )
        .route(
            "/attractions/{id}",
            get(get_one).patch(update).delete(delete),
        )
}

/// `GET /conventions/{convention_id}/attractions` — attractions of one convention, by title.
async fn list(
    State(state): State<AppState>,
    Path(convention_id): Path<Uuid>,
) -> Result<Json<Vec<Attraction>>, AppError> {
    let attractions = sqlx::query_as!(
        Attraction,
        r#"
        SELECT id, convention_id, title, kind AS "kind: AttractionKind",
               duration_minutes, description, created_at, updated_at
        FROM attractions
        WHERE convention_id = $1
        ORDER BY title
        "#,
        convention_id,
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(attractions))
}

/// `POST /conventions/{convention_id}/attractions` — create one, return 201.
async fn create(
    State(state): State<AppState>,
    Path(convention_id): Path<Uuid>,
    Json(body): Json<CreateAttraction>,
) -> Result<(StatusCode, Json<Attraction>), AppError> {
    // `$3::attraction_kind` casts the bound param to the ENUM for compile-time checking;
    // a non-positive duration trips the CHECK -> 422, a bad convention_id the FK -> 422.
    let attraction = sqlx::query_as!(
        Attraction,
        r#"
        INSERT INTO attractions (convention_id, title, kind, duration_minutes, description)
        VALUES ($1, $2, $3::attraction_kind, $4, $5)
        RETURNING id, convention_id, title, kind AS "kind: AttractionKind",
                  duration_minutes, description, created_at, updated_at
        "#,
        convention_id,
        body.title,
        body.kind as AttractionKind,
        body.duration_minutes,
        body.description,
    )
    .fetch_one(&state.pool)
    .await?;

    Ok((StatusCode::CREATED, Json(attraction)))
}

/// `GET /attractions/{id}` — one attraction, or 404.
async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Attraction>, AppError> {
    let attraction = sqlx::query_as!(
        Attraction,
        r#"
        SELECT id, convention_id, title, kind AS "kind: AttractionKind",
               duration_minutes, description, created_at, updated_at
        FROM attractions
        WHERE id = $1
        "#,
        id,
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(Json(attraction))
}

/// `PATCH /attractions/{id}` — partial update, or 404.
async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateAttraction>,
) -> Result<Json<Attraction>, AppError> {
    // COALESCE keeps omitted fields; the nullable description can be overwritten but not cleared here.
    let attraction = sqlx::query_as!(
        Attraction,
        r#"
        UPDATE attractions
        SET title            = COALESCE($2, title),
            kind             = COALESCE($3::attraction_kind, kind),
            duration_minutes = COALESCE($4, duration_minutes),
            description      = COALESCE($5, description)
        WHERE id = $1
        RETURNING id, convention_id, title, kind AS "kind: AttractionKind",
                  duration_minutes, description, created_at, updated_at
        "#,
        id,
        body.title,
        body.kind as Option<AttractionKind>,
        body.duration_minutes,
        body.description,
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(Json(attraction))
}

/// `DELETE /attractions/{id}` — 204 on success, 404 if it wasn't there.
async fn delete(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let result = sqlx::query!("DELETE FROM attractions WHERE id = $1", id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        Err(AppError::NotFound)
    } else {
        Ok(StatusCode::NO_CONTENT)
    }
}
