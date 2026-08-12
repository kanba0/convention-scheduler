//! Greedy schedule generation: hardest-to-place attractions first, each taking
//! the first candidate slot that breaks no hard constraint. Pure — no database
//! access, and nothing here writes; the caller decides what to do with the plan.

use std::collections::HashMap;

use serde::Serialize;
use time::{Date, Duration, PrimitiveDateTime, Time};
use uuid::Uuid;

use crate::attractions::AttractionKind;
use crate::rooms::RoomKind;
use crate::wall_clock::local_datetime;

/// A program day. Days with unset hours can't be scheduled into, so they never reach here.
pub struct Day {
    pub date: Date,
    pub opens_at: Time,
    pub closes_at: Time,
}

pub struct Room {
    pub id: Uuid,
    pub kind: RoomKind,
}

pub struct ToPlace {
    pub id: Uuid,
    pub kind: AttractionKind,
    pub duration_minutes: i32,
    pub host_ids: Vec<Uuid>,
}

pub struct Window {
    pub starts_at: PrimitiveDateTime,
    pub ends_at: PrimitiveDateTime,
}

/// An occupied span — an existing slot, or one this run just placed. Blocks its
/// room *and* its hosts.
pub struct Busy {
    pub room_id: Uuid,
    pub host_ids: Vec<Uuid>,
    pub starts_at: PrimitiveDateTime,
    pub ends_at: PrimitiveDateTime,
}

pub struct GenerateInput {
    pub days: Vec<Day>,
    pub rooms: Vec<Room>,
    pub to_place: Vec<ToPlace>,
    /// A panelist absent from the map stated no restrictions — free whenever the program runs.
    pub availability: HashMap<Uuid, Vec<Window>>,
    pub busy: Vec<Busy>,
    pub step_minutes: i64,
}

#[derive(Debug, Serialize)]
pub struct PlacedSlot {
    pub attraction_id: Uuid,
    pub room_id: Uuid,
    #[serde(with = "local_datetime")]
    pub starts_at: PrimitiveDateTime,
    #[serde(with = "local_datetime")]
    pub ends_at: PrimitiveDateTime,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnplacedReason {
    NoCompatibleRoom,
    DoesNotFitAnyDay,
    HostsNeverAvailable,
    NoFreeSlot,
}

#[derive(Debug, Serialize)]
pub struct Unplaced {
    pub attraction_id: Uuid,
    pub reason: UnplacedReason,
}

#[derive(Debug, Serialize)]
pub struct Plan {
    pub placed: Vec<PlacedSlot>,
    pub unplaced: Vec<Unplaced>,
}

/// Propose a placement for each attraction in `to_place`, scheduling around `busy`.
pub fn generate(input: GenerateInput) -> Plan {
    let GenerateInput {
        days,
        rooms,
        mut to_place,
        availability,
        mut busy,
        step_minutes,
    } = input;

    to_place.sort_by(|a, b| {
        kind_rank(a.kind)
            .cmp(&kind_rank(b.kind))
            .then(b.host_ids.len().cmp(&a.host_ids.len()))
            .then(b.duration_minutes.cmp(&a.duration_minutes))
            // Ids last, so equally-constrained attractions place in a stable order.
            .then(a.id.cmp(&b.id))
    });

    let step = Duration::minutes(step_minutes.max(1));
    let mut placed = Vec::new();
    let mut unplaced = Vec::new();

    for attraction in &to_place {
        match place(attraction, &days, &rooms, &availability, &busy, step) {
            Ok(slot) => {
                busy.push(Busy {
                    room_id: slot.room_id,
                    host_ids: attraction.host_ids.clone(),
                    starts_at: slot.starts_at,
                    ends_at: slot.ends_at,
                });
                placed.push(slot);
            }
            Err(reason) => unplaced.push(Unplaced {
                attraction_id: attraction.id,
                reason,
            }),
        }
    }

    Plan { placed, unplaced }
}

/// First candidate slot breaking no hard constraint. The flags record how far the
/// scan got, so a failure names its cause instead of a blanket "didn't fit".
fn place(
    attraction: &ToPlace,
    days: &[Day],
    rooms: &[Room],
    availability: &HashMap<Uuid, Vec<Window>>,
    busy: &[Busy],
    step: Duration,
) -> Result<PlacedSlot, UnplacedReason> {
    let compatible: Vec<&Room> = rooms
        .iter()
        .filter(|room| room_can_host(room.kind, attraction.kind))
        .collect();
    if compatible.is_empty() {
        return Err(UnplacedReason::NoCompatibleRoom);
    }

    let duration = Duration::minutes(i64::from(attraction.duration_minutes));
    let mut fits_a_day = false;
    let mut hosts_ever_available = false;

    for day in days {
        for start in candidate_starts(day, duration, step) {
            fits_a_day = true;
            let end = start + duration;

            if !hosts_available(&attraction.host_ids, availability, start, end) {
                continue;
            }
            hosts_ever_available = true;

            if !hosts_idle(&attraction.host_ids, busy, start, end) {
                continue;
            }

            if let Some(room) = compatible
                .iter()
                .find(|room| room_free(room.id, busy, start, end))
            {
                return Ok(PlacedSlot {
                    attraction_id: attraction.id,
                    room_id: room.id,
                    starts_at: start,
                    ends_at: end,
                });
            }
        }
    }

    if !fits_a_day {
        Err(UnplacedReason::DoesNotFitAnyDay)
    } else if !hosts_ever_available {
        Err(UnplacedReason::HostsNeverAvailable)
    } else {
        Err(UnplacedReason::NoFreeSlot)
    }
}

/// Every start leaving room for the full duration before closing.
fn candidate_starts(day: &Day, duration: Duration, step: Duration) -> Vec<PrimitiveDateTime> {
    let opens = PrimitiveDateTime::new(day.date, day.opens_at);
    let closes = PrimitiveDateTime::new(day.date, day.closes_at);

    let mut starts = Vec::new();
    let mut start = opens;
    while start + duration <= closes {
        starts.push(start);
        start += step;
    }
    starts
}

/// Contests rank first: fewer room kinds can host them.
fn kind_rank(kind: AttractionKind) -> u8 {
    match kind {
        AttractionKind::Contest => 0,
        AttractionKind::Panel => 1,
    }
}

fn room_can_host(room: RoomKind, attraction: AttractionKind) -> bool {
    matches!(
        (room, attraction),
        (RoomKind::PanelContest, _)
            | (RoomKind::Panel, AttractionKind::Panel)
            | (RoomKind::Contest, AttractionKind::Contest)
    )
}

/// One window must cover the whole span — v1 doesn't stitch adjacent windows together.
fn hosts_available(
    host_ids: &[Uuid],
    availability: &HashMap<Uuid, Vec<Window>>,
    start: PrimitiveDateTime,
    end: PrimitiveDateTime,
) -> bool {
    host_ids
        .iter()
        .all(|host_id| match availability.get(host_id) {
            Some(windows) if !windows.is_empty() => windows
                .iter()
                .any(|window| window.starts_at <= start && window.ends_at >= end),
            _ => true,
        })
}

fn hosts_idle(
    host_ids: &[Uuid],
    busy: &[Busy],
    start: PrimitiveDateTime,
    end: PrimitiveDateTime,
) -> bool {
    !busy.iter().any(|occupied| {
        overlaps(occupied, start, end)
            && occupied
                .host_ids
                .iter()
                .any(|host_id| host_ids.contains(host_id))
    })
}

fn room_free(
    room_id: Uuid,
    busy: &[Busy],
    start: PrimitiveDateTime,
    end: PrimitiveDateTime,
) -> bool {
    !busy
        .iter()
        .any(|occupied| occupied.room_id == room_id && overlaps(occupied, start, end))
}

/// Half-open `[start, end)`, so a slot may begin exactly when another ends.
fn overlaps(occupied: &Busy, start: PrimitiveDateTime, end: PrimitiveDateTime) -> bool {
    occupied.starts_at < end && start < occupied.ends_at
}
