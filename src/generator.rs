//! `GET /conventions/{id}/schedule/generate` — propose a placement for every
//! attraction that doesn't have one yet, leaving existing slots untouched. Nothing
//! is written; saving the plan is a separate step. This module loads, [`engine`]
//! packs and never touches a database.

mod engine;

use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use uuid::Uuid;

pub use crate::attractions::AttractionKind;
pub use crate::rooms::RoomKind;
pub use engine::{
    Busy, Day, GenerateInput, PlacedSlot, Plan, Room, ToPlace, Unplaced, UnplacedReason, Window,
    generate,
};

use crate::error::AppError;
use crate::state::AppState;

/// Deriving the step from the durations in play is a TODO, so it isn't requestable yet.
const STEP_MINUTES: i64 = 60;

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/conventions/{convention_id}/schedule/generate",
        get(generate_plan),
    )
}

/// `GET /conventions/{convention_id}/schedule/generate` — the proposed plan, or 404.
async fn generate_plan(
    State(state): State<AppState>,
    Path(convention_id): Path<Uuid>,
) -> Result<Json<Plan>, AppError> {
    let exists = sqlx::query_scalar!(
        "SELECT EXISTS(SELECT 1 FROM conventions WHERE id = $1)",
        convention_id,
    )
    .fetch_one(&state.pool)
    .await?;
    if exists != Some(true) {
        return Err(AppError::NotFound);
    }

    let day_rows = sqlx::query!(
        r#"
        SELECT day, opens_at AS "opens_at!", closes_at AS "closes_at!"
        FROM convention_days
        WHERE convention_id = $1 AND opens_at IS NOT NULL AND closes_at IS NOT NULL
        ORDER BY day
        "#,
        convention_id,
    )
    .fetch_all(&state.pool)
    .await?;
    let days = day_rows
        .into_iter()
        .map(|row| Day {
            date: row.day,
            opens_at: row.opens_at,
            closes_at: row.closes_at,
        })
        .collect();

    let room_rows = sqlx::query!(
        r#"SELECT id, kind AS "kind: RoomKind" FROM rooms WHERE convention_id = $1 ORDER BY name"#,
        convention_id,
    )
    .fetch_all(&state.pool)
    .await?;
    let rooms = room_rows
        .into_iter()
        .map(|row| Room {
            id: row.id,
            kind: row.kind,
        })
        .collect();

    let host_rows = sqlx::query!(
        r#"
        SELECT ap.attraction_id, ap.panelist_id
        FROM attraction_panelists ap
        JOIN attractions a ON a.id = ap.attraction_id
        WHERE a.convention_id = $1
        "#,
        convention_id,
    )
    .fetch_all(&state.pool)
    .await?;
    let mut hosts_by_attraction: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for row in host_rows {
        hosts_by_attraction
            .entry(row.attraction_id)
            .or_default()
            .push(row.panelist_id);
    }

    let attraction_rows = sqlx::query!(
        r#"
        SELECT a.id, a.kind AS "kind: AttractionKind", a.duration_minutes
        FROM attractions a
        WHERE a.convention_id = $1
          AND NOT EXISTS (SELECT 1 FROM slots s WHERE s.attraction_id = a.id)
        ORDER BY a.title
        "#,
        convention_id,
    )
    .fetch_all(&state.pool)
    .await?;
    let to_place = attraction_rows
        .into_iter()
        .map(|row| ToPlace {
            id: row.id,
            kind: row.kind,
            duration_minutes: row.duration_minutes,
            host_ids: hosts_by_attraction
                .get(&row.id)
                .cloned()
                .unwrap_or_default(),
        })
        .collect();

    let window_rows = sqlx::query!(
        r#"
        SELECT pa.panelist_id, pa.starts_at, pa.ends_at
        FROM panelist_availability pa
        JOIN panelists p ON p.id = pa.panelist_id
        WHERE p.convention_id = $1
        ORDER BY pa.starts_at
        "#,
        convention_id,
    )
    .fetch_all(&state.pool)
    .await?;
    let mut availability: HashMap<Uuid, Vec<Window>> = HashMap::new();
    for row in window_rows {
        availability
            .entry(row.panelist_id)
            .or_default()
            .push(Window {
                starts_at: row.starts_at,
                ends_at: row.ends_at,
            });
    }

    let slot_rows = sqlx::query!(
        r#"
        SELECT s.room_id, s.attraction_id, s.starts_at, s.ends_at
        FROM slots s
        JOIN attractions a ON a.id = s.attraction_id
        WHERE a.convention_id = $1
        "#,
        convention_id,
    )
    .fetch_all(&state.pool)
    .await?;
    let busy = slot_rows
        .into_iter()
        .map(|row| Busy {
            room_id: row.room_id,
            host_ids: hosts_by_attraction
                .get(&row.attraction_id)
                .cloned()
                .unwrap_or_default(),
            starts_at: row.starts_at,
            ends_at: row.ends_at,
        })
        .collect();

    Ok(Json(generate(GenerateInput {
        days,
        rooms,
        to_place,
        availability,
        busy,
        step_minutes: STEP_MINUTES,
    })))
}
