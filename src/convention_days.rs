//! Per-day program hours. Days aren't POSTed — they're seeded from the
//! convention's date span (`seed`); the operator only reads them and sets hours.

use axum::extract::{Path, State};
use axum::routing::{get, patch};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use time::{Date, Time};
use uuid::Uuid;

use crate::error::AppError;
use crate::state::AppState;

time::serde::format_description!(iso_date, Date, "[year]-[month]-[day]");
time::serde::format_description!(hh_mm, Time, "[hour]:[minute]");

/// One program day and its hours. Hours are null until the operator sets them.
#[derive(Serialize)]
pub struct ConventionDay {
    #[serde(with = "iso_date")]
    day: Date,
    #[serde(with = "hh_mm::option")]
    opens_at: Option<Time>,
    #[serde(with = "hh_mm::option")]
    closes_at: Option<Time>,
}

#[derive(Deserialize)]
pub struct SetDayHours {
    #[serde(with = "hh_mm")]
    opens_at: Time,
    #[serde(with = "hh_mm")]
    closes_at: Time,
}

/// Seed one row per date in the span. Idempotent (ON CONFLICT DO NOTHING) so it
/// re-runs as an additive sync when the span grows; shrinking never deletes days.
pub(crate) async fn seed(
    executor: impl sqlx::PgExecutor<'_>,
    convention_id: Uuid,
    starts_on: Date,
    ends_on: Date,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO convention_days (convention_id, day)
        SELECT $1, series::date
        FROM generate_series($2::date::timestamp, $3::date::timestamp, interval '1 day') AS series
        ON CONFLICT (convention_id, day) DO NOTHING
        "#,
        convention_id,
        starts_on,
        ends_on,
    )
    .execute(executor)
    .await?;

    Ok(())
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/conventions/{convention_id}/days", get(list))
        .route("/conventions/{convention_id}/days/{day}", patch(set_hours))
}

/// `GET /conventions/{convention_id}/days` — program days, earliest first, or 404.
async fn list(
    State(state): State<AppState>,
    Path(convention_id): Path<Uuid>,
) -> Result<Json<Vec<ConventionDay>>, AppError> {
    let exists = sqlx::query_scalar!(
        "SELECT EXISTS(SELECT 1 FROM conventions WHERE id = $1)",
        convention_id,
    )
    .fetch_one(&state.pool)
    .await?;
    if exists != Some(true) {
        return Err(AppError::NotFound);
    }

    let days = sqlx::query_as!(
        ConventionDay,
        r#"
        SELECT day, opens_at, closes_at
        FROM convention_days
        WHERE convention_id = $1
        ORDER BY day
        "#,
        convention_id,
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(days))
}

/// `PATCH /conventions/{convention_id}/days/{day}` — set one day's hours, or 404.
async fn set_hours(
    State(state): State<AppState>,
    Path((convention_id, day)): Path<(Uuid, String)>,
    Json(body): Json<SetDayHours>,
) -> Result<Json<ConventionDay>, AppError> {
    let format = time::macros::format_description!("[year]-[month]-[day]");
    let day = Date::parse(&day, format)
        .map_err(|_| AppError::Validation("day must be formatted YYYY-MM-DD".into()))?;

    let updated = sqlx::query_as!(
        ConventionDay,
        r#"
        UPDATE convention_days
        SET opens_at = $3, closes_at = $4
        WHERE convention_id = $1 AND day = $2
        RETURNING day, opens_at, closes_at
        "#,
        convention_id,
        day,
        body.opens_at,
        body.closes_at,
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(Json(updated))
}
