//! The `rooms` resource. Rooms are scoped to a convention, so list/create are
//! nested under `/conventions/{id}/rooms`; a single room is addressed flatly.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::error::AppError;
use crate::state::AppState;

/// What a room can host. Mirrors the `room_kind` Postgres ENUM.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "room_kind", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum RoomKind {
    Panel,
    Contest,
    PanelContest,
}

/// A full room row, as returned to clients.
#[derive(Serialize)]
pub struct Room {
    id: Uuid,
    convention_id: Uuid,
    name: String,
    kind: RoomKind,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
}

/// Accepted on POST. The convention comes from the path, not the body.
#[derive(Deserialize)]
pub struct CreateRoom {
    name: String,
    kind: RoomKind,
}

/// Accepted on PATCH — every field optional; `None` leaves that column untouched.
#[derive(Deserialize)]
pub struct UpdateRoom {
    name: Option<String>,
    kind: Option<RoomKind>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/conventions/{convention_id}/rooms",
            get(list).post(create),
        )
        .route("/rooms/{id}", get(get_one).patch(update).delete(delete))
}

/// `GET /conventions/{convention_id}/rooms` — rooms of one convention, by name.
async fn list(
    State(state): State<AppState>,
    Path(convention_id): Path<Uuid>,
) -> Result<Json<Vec<Room>>, AppError> {
    // `kind AS "kind: RoomKind"` tells the macro to decode the column as our
    // enum rather than guessing a type for the custom `room_kind`.
    let rooms = sqlx::query_as!(
        Room,
        r#"
        SELECT id, convention_id, name, kind AS "kind: RoomKind", created_at, updated_at
        FROM rooms
        WHERE convention_id = $1
        ORDER BY name
        "#,
        convention_id,
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rooms))
}

/// `POST /conventions/{convention_id}/rooms` — create one, return 201.
async fn create(
    State(state): State<AppState>,
    Path(convention_id): Path<Uuid>,
    Json(body): Json<CreateRoom>,
) -> Result<(StatusCode, Json<Room>), AppError> {
    // `$3::room_kind` casts the bound parameter to the ENUM so the macro can
    // type-check it; a missing convention_id trips the FK -> 422 (see error.rs).
    let room = sqlx::query_as!(
        Room,
        r#"
        INSERT INTO rooms (convention_id, name, kind)
        VALUES ($1, $2, $3::room_kind)
        RETURNING id, convention_id, name, kind AS "kind: RoomKind", created_at, updated_at
        "#,
        convention_id,
        body.name,
        body.kind as RoomKind,
    )
    .fetch_one(&state.pool)
    .await?;

    Ok((StatusCode::CREATED, Json(room)))
}

/// `GET /rooms/{id}` — one room, or 404.
async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Room>, AppError> {
    let room = sqlx::query_as!(
        Room,
        r#"
        SELECT id, convention_id, name, kind AS "kind: RoomKind", created_at, updated_at
        FROM rooms
        WHERE id = $1
        "#,
        id,
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(Json(room))
}

/// `PATCH /rooms/{id}` — partial update, or 404.
async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateRoom>,
) -> Result<Json<Room>, AppError> {
    let room = sqlx::query_as!(
        Room,
        r#"
        UPDATE rooms
        SET name = COALESCE($2, name),
            kind = COALESCE($3::room_kind, kind)
        WHERE id = $1
        RETURNING id, convention_id, name, kind AS "kind: RoomKind", created_at, updated_at
        "#,
        id,
        body.name,
        body.kind as Option<RoomKind>,
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(Json(room))
}

/// `DELETE /rooms/{id}` — 204 on success, 404 if it wasn't there.
async fn delete(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let result = sqlx::query!("DELETE FROM rooms WHERE id = $1", id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        Err(AppError::NotFound)
    } else {
        Ok(StatusCode::NO_CONTENT)
    }
}
