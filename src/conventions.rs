//! The `conventions` resource: data shapes, handlers, and routing in one
//! vertical slice. Other resources (rooms, panelists, …) mirror this layout.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use crate::error::AppError;
use crate::state::AppState;

// `Date` has no rfc3339 (it carries no timezone), so its wire format is explicit.
time::serde::format_description!(iso_date, Date, "[year]-[month]-[day]");

/// A full convention row, as returned to clients.
#[derive(Serialize)]
pub struct Convention {
    id: Uuid,
    name: String,
    #[serde(with = "iso_date")]
    starts_on: Date,
    #[serde(with = "iso_date")]
    ends_on: Date,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
}

/// Accepted on POST — only the fields a client may choose.
#[derive(Deserialize)]
pub struct CreateConvention {
    name: String,
    #[serde(with = "iso_date")]
    starts_on: Date,
    #[serde(with = "iso_date")]
    ends_on: Date,
}

/// Accepted on PATCH — every field optional; `None` leaves that column untouched.
#[derive(Deserialize)]
pub struct UpdateConvention {
    name: Option<String>,
    // `default` lets an absent key deserialize to `None`.
    #[serde(default, with = "iso_date::option")]
    starts_on: Option<Date>,
    #[serde(default, with = "iso_date::option")]
    ends_on: Option<Date>,
}

/// All convention routes, ready to merge into the main router.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/conventions", get(list).post(create))
        // axum 0.8 path-param syntax is `{id}` (older versions used `:id`).
        .route(
            "/conventions/{id}",
            get(get_one).patch(update).delete(delete),
        )
}

/// `GET /conventions` — list all, soonest first.
async fn list(State(state): State<AppState>) -> Result<Json<Vec<Convention>>, AppError> {
    let conventions = sqlx::query_as!(
        Convention,
        r#"
        SELECT id, name, starts_on, ends_on, created_at, updated_at
        FROM conventions
        ORDER BY starts_on, name
        "#
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(conventions))
}

/// `POST /conventions` — create one, return 201 with the created row.
async fn create(
    State(state): State<AppState>,
    Json(body): Json<CreateConvention>,
) -> Result<(StatusCode, Json<Convention>), AppError> {
    let convention = sqlx::query_as!(
        Convention,
        r#"
        INSERT INTO conventions (name, starts_on, ends_on)
        VALUES ($1, $2, $3)
        RETURNING id, name, starts_on, ends_on, created_at, updated_at
        "#,
        body.name,
        body.starts_on,
        body.ends_on,
    )
    .fetch_one(&state.pool)
    .await?;

    Ok((StatusCode::CREATED, Json(convention)))
}

/// `GET /conventions/{id}` — one row, or 404.
async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Convention>, AppError> {
    let convention = sqlx::query_as!(
        Convention,
        r#"
        SELECT id, name, starts_on, ends_on, created_at, updated_at
        FROM conventions
        WHERE id = $1
        "#,
        id,
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(Json(convention))
}

/// `PATCH /conventions/{id}` — partial update, or 404.
async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateConvention>,
) -> Result<Json<Convention>, AppError> {
    // COALESCE($n, col) keeps the existing value when the bound argument is NULL,
    // so an omitted PATCH field (None -> NULL) falls through to the current column.
    // This keeps PATCH a single static query the macro can verify at compile time.
    let convention = sqlx::query_as!(
        Convention,
        r#"
        UPDATE conventions
        SET name      = COALESCE($2, name),
            starts_on = COALESCE($3, starts_on),
            ends_on   = COALESCE($4, ends_on)
        WHERE id = $1
        RETURNING id, name, starts_on, ends_on, created_at, updated_at
        "#,
        id,
        body.name,
        body.starts_on,
        body.ends_on,
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(Json(convention))
}

/// `DELETE /conventions/{id}` — 204 on success, 404 if it wasn't there.
async fn delete(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let result = sqlx::query!("DELETE FROM conventions WHERE id = $1", id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        Err(AppError::NotFound)
    } else {
        Ok(StatusCode::NO_CONTENT)
    }
}
