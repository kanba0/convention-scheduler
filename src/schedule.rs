//! `GET /conventions/{id}/schedule` — the assembled grid. One read that stitches
//! every placed slot in a convention together with its room, attraction, and hosts,
//! so a client gets the whole timetable in a single nested response.

use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use crate::attractions::AttractionKind;
use crate::error::AppError;
use crate::rooms::RoomKind;
use crate::state::AppState;

// `Date` has no rfc3339 (no timezone), so its wire format is explicit (as in conventions).
time::serde::format_description!(iso_date, Date, "[year]-[month]-[day]");

/// The whole timetable for one convention.
#[derive(Serialize)]
pub struct Schedule {
    convention: ConventionSummary,
    slots: Vec<ScheduledSlot>,
}

#[derive(Serialize)]
struct ConventionSummary {
    id: Uuid,
    name: String,
    #[serde(with = "iso_date")]
    starts_on: Date,
    #[serde(with = "iso_date")]
    ends_on: Date,
}

/// One placed slot, with everything needed to render its grid cell.
#[derive(Serialize)]
struct ScheduledSlot {
    id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    starts_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    ends_at: OffsetDateTime,
    room: RoomRef,
    attraction: AttractionRef,
    hosts: Vec<HostRef>,
}

#[derive(Serialize)]
struct RoomRef {
    id: Uuid,
    name: String,
    kind: RoomKind,
}

#[derive(Serialize)]
struct AttractionRef {
    id: Uuid,
    title: String,
    kind: AttractionKind,
    duration_minutes: i32,
}

#[derive(Serialize)]
struct HostRef {
    id: Uuid,
    nick: String,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/conventions/{convention_id}/schedule", get(get_schedule))
}

/// `GET /conventions/{convention_id}/schedule` — assembled grid, or 404 if the
/// convention doesn't exist.
async fn get_schedule(
    State(state): State<AppState>,
    Path(convention_id): Path<Uuid>,
) -> Result<Json<Schedule>, AppError> {
    // Doubles as the existence check: a missing convention is a 404, not an empty grid.
    let convention = sqlx::query_as!(
        ConventionSummary,
        r#"SELECT id, name, starts_on, ends_on FROM conventions WHERE id = $1"#,
        convention_id,
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    // The placements, joined to their room + attraction. `!` forces non-null on the
    // joined columns (an INNER JOIN guarantees a match, but sqlx infers conservatively).
    let rows = sqlx::query!(
        r#"
        SELECT s.id AS slot_id, s.starts_at, s.ends_at,
               r.id AS "room_id!", r.name AS "room_name!", r.kind AS "room_kind!: RoomKind",
               a.id AS "attraction_id!", a.title AS "attraction_title!",
               a.kind AS "attraction_kind!: AttractionKind", a.duration_minutes AS "duration_minutes!"
        FROM slots s
        JOIN rooms r ON r.id = s.room_id
        JOIN attractions a ON a.id = s.attraction_id
        WHERE a.convention_id = $1
        ORDER BY s.starts_at, r.name
        "#,
        convention_id,
    )
    .fetch_all(&state.pool)
    .await?;

    // Hosts for the whole convention in one query, then grouped — avoids an N+1
    // (one host query per slot).
    let host_rows = sqlx::query!(
        r#"
        SELECT ap.attraction_id, p.id AS "panelist_id!", p.nick AS "nick!"
        FROM attraction_panelists ap
        JOIN panelists p ON p.id = ap.panelist_id
        JOIN attractions a ON a.id = ap.attraction_id
        WHERE a.convention_id = $1
        ORDER BY p.nick
        "#,
        convention_id,
    )
    .fetch_all(&state.pool)
    .await?;

    let mut hosts_by_attraction: HashMap<Uuid, Vec<HostRef>> = HashMap::new();
    for h in host_rows {
        hosts_by_attraction
            .entry(h.attraction_id)
            .or_default()
            .push(HostRef { id: h.panelist_id, nick: h.nick });
    }

    let slots = rows
        .into_iter()
        .map(|row| ScheduledSlot {
            id: row.slot_id,
            starts_at: row.starts_at,
            ends_at: row.ends_at,
            room: RoomRef {
                id: row.room_id,
                name: row.room_name,
                kind: row.room_kind,
            },
            attraction: AttractionRef {
                id: row.attraction_id,
                title: row.attraction_title,
                kind: row.attraction_kind,
                duration_minutes: row.duration_minutes,
            },
            // An attraction has at most one slot, so each id appears once -> remove is safe.
            hosts: hosts_by_attraction.remove(&row.attraction_id).unwrap_or_default(),
        })
        .collect();

    Ok(Json(Schedule { convention, slots }))
}
