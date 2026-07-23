//! The `slots` resource — placement of an attraction into a room for a time
//! range. Scoped to a convention via its attraction; list/create under
//! `/conventions/{id}/slots`, a single slot at `/slots/{id}`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::error::AppError;
use crate::state::AppState;

/// A full slot row, as returned to clients.
#[derive(Serialize)]
pub struct Slot {
    id: Uuid,
    attraction_id: Uuid,
    room_id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    starts_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    ends_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
}

/// Accepted on POST. The convention comes from the path; the attraction and room
/// must both belong to it.
#[derive(Deserialize)]
pub struct CreateSlot {
    attraction_id: Uuid,
    room_id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    starts_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    ends_at: OffsetDateTime,
}

/// Accepted on PATCH — move the placement. `attraction_id` is fixed (a slot *is*
/// an attraction's placement); only room/time move. `None` leaves a field untouched.
#[derive(Deserialize)]
pub struct UpdateSlot {
    room_id: Option<Uuid>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    starts_at: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    ends_at: Option<OffsetDateTime>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/conventions/{convention_id}/slots", get(list).post(create))
        .route("/slots/{id}", get(get_one).patch(update).delete(delete))
}

/// `GET /conventions/{convention_id}/slots` — placements in one convention, by time.
async fn list(
    State(state): State<AppState>,
    Path(convention_id): Path<Uuid>,
) -> Result<Json<Vec<Slot>>, AppError> {
    let slots = sqlx::query_as!(
        Slot,
        r#"
        SELECT s.id, s.attraction_id, s.room_id, s.starts_at, s.ends_at, s.created_at, s.updated_at
        FROM slots s
        JOIN attractions a ON a.id = s.attraction_id
        WHERE a.convention_id = $1
        ORDER BY s.starts_at, s.room_id
        "#,
        convention_id,
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(slots))
}

/// `POST /conventions/{convention_id}/slots` — place an attraction, return 201.
async fn create(
    State(state): State<AppState>,
    Path(convention_id): Path<Uuid>,
    Json(body): Json<CreateSlot>,
) -> Result<(StatusCode, Json<Slot>), AppError> {
    // The SELECT matches only when both refs share this convention, so a bad ref
    // inserts nothing (-> None -> 422); the CHECK and UNIQUE constraints handle
    // time-ordering and single-placement (-> 422 / 409 via the error mapping).
    let slot = sqlx::query_as!(
        Slot,
        r#"
        INSERT INTO slots (attraction_id, room_id, starts_at, ends_at)
        SELECT a.id, r.id, $4, $5
        FROM attractions a, rooms r
        WHERE a.id = $2 AND r.id = $3
          AND a.convention_id = $1 AND r.convention_id = $1
        RETURNING id, attraction_id, room_id, starts_at, ends_at, created_at, updated_at
        "#,
        convention_id,
        body.attraction_id,
        body.room_id,
        body.starts_at,
        body.ends_at,
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| {
        AppError::Validation("attraction and room must both belong to this convention".to_string())
    })?;

    Ok((StatusCode::CREATED, Json(slot)))
}

/// `GET /slots/{id}` — one placement, or 404.
async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Slot>, AppError> {
    let slot = sqlx::query_as!(
        Slot,
        r#"
        SELECT id, attraction_id, room_id, starts_at, ends_at, created_at, updated_at
        FROM slots
        WHERE id = $1
        "#,
        id,
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(Json(slot))
}

/// `PATCH /slots/{id}` — move the placement (room/time), or 404.
async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateSlot>,
) -> Result<Json<Slot>, AppError> {
    // The guarded WHERE only updates when a new room (if given) belongs to the slot's
    // attraction's convention; otherwise zero rows. Time ordering stays a DB CHECK.
    let slot = sqlx::query_as!(
        Slot,
        r#"
        UPDATE slots
        SET room_id   = COALESCE($2::uuid, room_id),
            starts_at = COALESCE($3, starts_at),
            ends_at   = COALESCE($4, ends_at)
        WHERE id = $1
          AND ($2::uuid IS NULL OR EXISTS (
              SELECT 1 FROM attractions a
              JOIN rooms r ON r.convention_id = a.convention_id
              WHERE a.id = slots.attraction_id AND r.id = $2::uuid
          ))
        RETURNING id, attraction_id, room_id, starts_at, ends_at, created_at, updated_at
        "#,
        id,
        body.room_id,
        body.starts_at,
        body.ends_at,
    )
    .fetch_optional(&state.pool)
    .await?;

    match slot {
        Some(slot) => Ok(Json(slot)),
        // Zero rows is ambiguous: a missing slot (404) or a cross-convention room (422).
        None => {
            let slot_exists =
                sqlx::query_scalar!("SELECT EXISTS(SELECT 1 FROM slots WHERE id = $1)", id,)
                    .fetch_one(&state.pool)
                    .await?
                    .unwrap_or(false);

            if slot_exists {
                Err(AppError::Validation(
                    "room must belong to the slot's convention".to_string(),
                ))
            } else {
                Err(AppError::NotFound)
            }
        }
    }
}

/// `DELETE /slots/{id}` — 204 on success, 404 if it wasn't there.
async fn delete(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let result = sqlx::query!("DELETE FROM slots WHERE id = $1", id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        Err(AppError::NotFound)
    } else {
        Ok(StatusCode::NO_CONTENT)
    }
}
