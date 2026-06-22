//! `GET /conventions/{id}/conflicts` — the clashes organizers currently mark by hand
//! on a colored grid, computed instead. Three checks, each read-only:
//!
//!   1. a room double-booked (two overlapping slots in one room),
//!   2. a panelist double-booked (hosting two attractions whose slots overlap),
//!   3. an attraction in a room whose type can't host its kind.
//!
//! This is a *report*, not a guard: conflicts are tolerated and surfaced, the way the
//! manual grid tolerates-and-highlights, so the operator can sit in a clashing state
//! mid-edit and resolve it. Nothing here writes; nothing here forbids.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::attractions::AttractionKind;
use crate::error::AppError;
use crate::rooms::RoomKind;
use crate::state::AppState;

/// A slot reduced to what a conflict needs to point at it: its id, the attraction's
/// title, and its time range. Enough for the grid to highlight the cell without a
/// follow-up fetch.
#[derive(Serialize)]
struct SlotRef {
    id: Uuid,
    attraction_title: String,
    #[serde(with = "time::serde::rfc3339")]
    starts_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    ends_at: OffsetDateTime,
}

/// One detected clash. `tag = "type"` puts a discriminator field on the wire
/// (`"type": "room_double_booked"`) so the frontend can switch on it.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Conflict {
    /// Two slots overlap in the same room.
    RoomDoubleBooked {
        room_id: Uuid,
        room_name: String,
        room_kind: RoomKind,
        slots: [SlotRef; 2],
    },
    /// One panelist hosts two attractions whose slots overlap.
    PanelistDoubleBooked {
        panelist_id: Uuid,
        panelist_nick: String,
        slots: [SlotRef; 2],
    },
    /// An attraction is placed in a room whose type can't host its kind.
    RoomTypeMismatch {
        slot: SlotRef,
        room_name: String,
        room_kind: RoomKind,
        attraction_kind: AttractionKind,
    },
}

#[derive(Serialize)]
struct ConflictReport {
    conflicts: Vec<Conflict>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/conventions/{convention_id}/conflicts", get(conflicts))
}

/// `GET /conventions/{convention_id}/conflicts` — every clash in the convention, or 404.
async fn conflicts(
    State(state): State<AppState>,
    Path(convention_id): Path<Uuid>,
) -> Result<Json<ConflictReport>, AppError> {
    let exists = sqlx::query_scalar!(
        "SELECT EXISTS(SELECT 1 FROM conventions WHERE id = $1)",
        convention_id,
    )
    .fetch_one(&state.pool)
    .await?;
    if exists != Some(true) {
        return Err(AppError::NotFound);
    }

    let mut conflicts = Vec::new();

    // Check 1: room double-booking. The self-join pairs distinct slots in the same room
    // (s1.id < s2.id keeps each pair once and rules out self-matches); `&&` on the two
    // `[)` ranges is the overlap test, so touching-but-not-overlapping slots don't trip it.
    let room_rows = sqlx::query!(
        r#"
        SELECT r.id AS "room_id!", r.name AS "room_name!", r.kind AS "room_kind!: RoomKind",
               s1.id AS "slot1_id!", a1.title AS "a1_title!",
               s1.starts_at AS "s1_start!", s1.ends_at AS "s1_end!",
               s2.id AS "slot2_id!", a2.title AS "a2_title!",
               s2.starts_at AS "s2_start!", s2.ends_at AS "s2_end!"
        FROM slots s1
        JOIN slots s2 ON s2.room_id = s1.room_id AND s2.id > s1.id
        JOIN attractions a1 ON a1.id = s1.attraction_id
        JOIN attractions a2 ON a2.id = s2.attraction_id
        JOIN rooms r ON r.id = s1.room_id
        WHERE a1.convention_id = $1
          AND tstzrange(s1.starts_at, s1.ends_at) && tstzrange(s2.starts_at, s2.ends_at)
        ORDER BY s1.starts_at
        "#,
        convention_id,
    )
    .fetch_all(&state.pool)
    .await?;
    for row in room_rows {
        conflicts.push(Conflict::RoomDoubleBooked {
            room_id: row.room_id,
            room_name: row.room_name,
            room_kind: row.room_kind,
            slots: [
                SlotRef { id: row.slot1_id, attraction_title: row.a1_title, starts_at: row.s1_start, ends_at: row.s1_end },
                SlotRef { id: row.slot2_id, attraction_title: row.a2_title, starts_at: row.s2_start, ends_at: row.s2_end },
            ],
        });
    }

    // Check 2: panelist double-booking. Same overlap test, but the join goes through
    // attraction_panelists twice on a shared panelist_id — a person hosting both sides of
    // an overlap, regardless of room.
    let panelist_rows = sqlx::query!(
        r#"
        SELECT p.id AS "panelist_id!", p.nick AS "panelist_nick!",
               s1.id AS "slot1_id!", a1.title AS "a1_title!",
               s1.starts_at AS "s1_start!", s1.ends_at AS "s1_end!",
               s2.id AS "slot2_id!", a2.title AS "a2_title!",
               s2.starts_at AS "s2_start!", s2.ends_at AS "s2_end!"
        FROM slots s1
        JOIN slots s2 ON s2.id > s1.id
        JOIN attraction_panelists ap1 ON ap1.attraction_id = s1.attraction_id
        JOIN attraction_panelists ap2 ON ap2.attraction_id = s2.attraction_id
                                     AND ap2.panelist_id = ap1.panelist_id
        JOIN panelists p ON p.id = ap1.panelist_id
        JOIN attractions a1 ON a1.id = s1.attraction_id
        JOIN attractions a2 ON a2.id = s2.attraction_id
        WHERE a1.convention_id = $1
          AND tstzrange(s1.starts_at, s1.ends_at) && tstzrange(s2.starts_at, s2.ends_at)
        ORDER BY p.nick
        "#,
        convention_id,
    )
    .fetch_all(&state.pool)
    .await?;
    for row in panelist_rows {
        conflicts.push(Conflict::PanelistDoubleBooked {
            panelist_id: row.panelist_id,
            panelist_nick: row.panelist_nick,
            slots: [
                SlotRef { id: row.slot1_id, attraction_title: row.a1_title, starts_at: row.s1_start, ends_at: row.s1_end },
                SlotRef { id: row.slot2_id, attraction_title: row.a2_title, starts_at: row.s2_start, ends_at: row.s2_end },
            ],
        });
    }

    // Check 3: room-type mismatch. A `panel_contest` room hosts anything, so it's excluded;
    // otherwise the room's kind label must equal the attraction's. Casting both enums to
    // text lets the two distinct enum types compare on their shared 'panel'/'contest' labels.
    let mismatch_rows = sqlx::query!(
        r#"
        SELECT s.id AS "slot_id!", a.title AS "attraction_title!",
               s.starts_at AS "starts_at!", s.ends_at AS "ends_at!",
               a.kind AS "attraction_kind!: AttractionKind",
               r.name AS "room_name!", r.kind AS "room_kind!: RoomKind"
        FROM slots s
        JOIN attractions a ON a.id = s.attraction_id
        JOIN rooms r ON r.id = s.room_id
        WHERE a.convention_id = $1
          AND r.kind <> 'panel_contest'
          AND r.kind::text <> a.kind::text
        ORDER BY s.starts_at
        "#,
        convention_id,
    )
    .fetch_all(&state.pool)
    .await?;
    for row in mismatch_rows {
        conflicts.push(Conflict::RoomTypeMismatch {
            slot: SlotRef {
                id: row.slot_id,
                attraction_title: row.attraction_title,
                starts_at: row.starts_at,
                ends_at: row.ends_at,
            },
            room_name: row.room_name,
            room_kind: row.room_kind,
            attraction_kind: row.attraction_kind,
        });
    }

    Ok(Json(ConflictReport { conflicts }))
}
