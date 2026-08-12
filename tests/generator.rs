//! Tests for the schedule generator. It's a pure function, so unlike the rest of
//! the suite these need no database.

use std::collections::HashMap;

use convention_scheduler::generator::{
    AttractionKind, Busy, Day, GenerateInput, Plan, Room, RoomKind, ToPlace, UnplacedReason,
    Window, generate,
};
use time::macros::{date, datetime, time};
use uuid::Uuid;

/// One day, 10:00–14:00.
fn one_day() -> Vec<Day> {
    vec![Day {
        date: date!(2026 - 08 - 14),
        opens_at: time!(10:00),
        closes_at: time!(14:00),
    }]
}

fn room(kind: RoomKind) -> Room {
    Room {
        id: Uuid::new_v4(),
        kind,
    }
}

fn attraction(kind: AttractionKind, minutes: i32, hosts: &[Uuid]) -> ToPlace {
    ToPlace {
        id: Uuid::new_v4(),
        kind,
        duration_minutes: minutes,
        host_ids: hosts.to_vec(),
    }
}

fn input(days: Vec<Day>, rooms: Vec<Room>, to_place: Vec<ToPlace>) -> GenerateInput {
    GenerateInput {
        days,
        rooms,
        to_place,
        availability: HashMap::new(),
        busy: Vec::new(),
        step_minutes: 60,
    }
}

fn only_reason(plan: &Plan) -> &UnplacedReason {
    assert_eq!(plan.unplaced.len(), 1, "expected exactly one unplaced");
    &plan.unplaced[0].reason
}

#[test]
fn an_attraction_lands_at_the_first_hour_of_the_day() {
    let hall = room(RoomKind::Panel);
    let hall_id = hall.id;

    let plan = generate(input(
        one_day(),
        vec![hall],
        vec![attraction(AttractionKind::Panel, 60, &[])],
    ));

    assert!(plan.unplaced.is_empty());
    assert_eq!(plan.placed.len(), 1);
    assert_eq!(plan.placed[0].room_id, hall_id);
    assert_eq!(plan.placed[0].starts_at, datetime!(2026-08-14 10:00));
    assert_eq!(plan.placed[0].ends_at, datetime!(2026-08-14 11:00));
}

#[test]
fn a_contest_skips_a_panel_only_room() {
    let contest_room = room(RoomKind::Contest);
    let contest_room_id = contest_room.id;

    let plan = generate(input(
        one_day(),
        vec![room(RoomKind::Panel), contest_room],
        vec![attraction(AttractionKind::Contest, 60, &[])],
    ));

    assert_eq!(plan.placed[0].room_id, contest_room_id);
}

#[test]
fn a_panel_contest_room_hosts_either_kind() {
    let plan = generate(input(
        one_day(),
        vec![room(RoomKind::PanelContest)],
        vec![
            attraction(AttractionKind::Contest, 60, &[]),
            attraction(AttractionKind::Panel, 60, &[]),
        ],
    ));

    assert_eq!(plan.placed.len(), 2);
    assert!(plan.unplaced.is_empty());
}

#[test]
fn with_no_room_of_the_right_kind_nothing_is_placed() {
    let plan = generate(input(
        one_day(),
        vec![room(RoomKind::Panel)],
        vec![attraction(AttractionKind::Contest, 60, &[])],
    ));

    assert!(plan.placed.is_empty());
    assert_eq!(only_reason(&plan), &UnplacedReason::NoCompatibleRoom);
}

#[test]
fn an_attraction_longer_than_the_day_is_unplaceable() {
    let plan = generate(input(
        one_day(),
        vec![room(RoomKind::Panel)],
        vec![attraction(AttractionKind::Panel, 300, &[])],
    ));

    assert_eq!(only_reason(&plan), &UnplacedReason::DoesNotFitAnyDay);
}

#[test]
fn contests_are_placed_before_panels() {
    let contest = attraction(AttractionKind::Contest, 60, &[]);
    let contest_id = contest.id;

    // Listed panel-first, so only the sort can win the 10:00 slot for the contest.
    let plan = generate(input(
        one_day(),
        vec![room(RoomKind::PanelContest)],
        vec![attraction(AttractionKind::Panel, 60, &[]), contest],
    ));

    assert_eq!(plan.placed[0].attraction_id, contest_id);
    assert_eq!(plan.placed[0].starts_at, datetime!(2026-08-14 10:00));
}

#[test]
fn the_attraction_with_more_hosts_is_placed_first() {
    let crowded = attraction(
        AttractionKind::Panel,
        60,
        &[Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()],
    );
    let crowded_id = crowded.id;

    let plan = generate(input(
        one_day(),
        vec![room(RoomKind::Panel)],
        vec![
            attraction(AttractionKind::Panel, 60, &[Uuid::new_v4()]),
            crowded,
        ],
    ));

    assert_eq!(plan.placed[0].attraction_id, crowded_id);
}

#[test]
fn two_attractions_sharing_a_host_run_back_to_back() {
    let host = Uuid::new_v4();

    let plan = generate(input(
        one_day(),
        vec![room(RoomKind::PanelContest)],
        vec![
            attraction(AttractionKind::Panel, 60, &[host]),
            attraction(AttractionKind::Panel, 60, &[host]),
        ],
    ));

    assert_eq!(plan.placed.len(), 2);
    assert_eq!(plan.placed[0].starts_at, datetime!(2026-08-14 10:00));
    assert_eq!(plan.placed[1].starts_at, datetime!(2026-08-14 11:00));
}

#[test]
fn attractions_without_a_shared_host_run_in_parallel_rooms() {
    let plan = generate(input(
        one_day(),
        vec![room(RoomKind::Panel), room(RoomKind::Panel)],
        vec![
            attraction(AttractionKind::Panel, 60, &[Uuid::new_v4()]),
            attraction(AttractionKind::Panel, 60, &[Uuid::new_v4()]),
        ],
    ));

    assert_eq!(plan.placed.len(), 2);
    assert_eq!(plan.placed[0].starts_at, plan.placed[1].starts_at);
    assert_ne!(plan.placed[0].room_id, plan.placed[1].room_id);
}

#[test]
fn a_host_who_stated_no_windows_is_free_whenever() {
    let plan = generate(input(
        one_day(),
        vec![room(RoomKind::Panel)],
        vec![attraction(AttractionKind::Panel, 60, &[Uuid::new_v4()])],
    ));

    assert_eq!(plan.placed[0].starts_at, datetime!(2026-08-14 10:00));
}

#[test]
fn a_hosts_window_pushes_the_slot_later() {
    let host = Uuid::new_v4();
    let mut request = input(
        one_day(),
        vec![room(RoomKind::Panel)],
        vec![attraction(AttractionKind::Panel, 60, &[host])],
    );
    request.availability.insert(
        host,
        vec![Window {
            starts_at: datetime!(2026-08-14 12:00),
            ends_at: datetime!(2026-08-14 14:00),
        }],
    );

    let plan = generate(request);

    assert_eq!(plan.placed[0].starts_at, datetime!(2026-08-14 12:00));
}

#[test]
fn a_window_that_never_meets_the_program_leaves_the_host_unplaceable() {
    let host = Uuid::new_v4();
    let mut request = input(
        one_day(),
        vec![room(RoomKind::Panel)],
        vec![attraction(AttractionKind::Panel, 60, &[host])],
    );
    request.availability.insert(
        host,
        vec![Window {
            starts_at: datetime!(2026-08-20 10:00),
            ends_at: datetime!(2026-08-20 14:00),
        }],
    );

    let plan = generate(request);

    assert_eq!(only_reason(&plan), &UnplacedReason::HostsNeverAvailable);
}

#[test]
fn a_window_shorter_than_the_attraction_never_covers_it() {
    let host = Uuid::new_v4();
    let mut request = input(
        one_day(),
        vec![room(RoomKind::Panel)],
        vec![attraction(AttractionKind::Panel, 120, &[host])],
    );
    request.availability.insert(
        host,
        vec![Window {
            starts_at: datetime!(2026-08-14 10:00),
            ends_at: datetime!(2026-08-14 11:00),
        }],
    );

    let plan = generate(request);

    assert_eq!(only_reason(&plan), &UnplacedReason::HostsNeverAvailable);
}

#[test]
fn every_host_must_be_available_not_just_one() {
    let free_host = Uuid::new_v4();
    let busy_host = Uuid::new_v4();
    let mut request = input(
        one_day(),
        vec![room(RoomKind::Panel)],
        vec![attraction(
            AttractionKind::Panel,
            60,
            &[free_host, busy_host],
        )],
    );
    request.availability.insert(
        busy_host,
        vec![Window {
            starts_at: datetime!(2026-08-14 13:00),
            ends_at: datetime!(2026-08-14 14:00),
        }],
    );

    let plan = generate(request);

    assert_eq!(plan.placed[0].starts_at, datetime!(2026-08-14 13:00));
}

#[test]
fn an_existing_slot_blocks_its_room() {
    let hall = room(RoomKind::Panel);
    let hall_id = hall.id;
    let mut request = input(
        one_day(),
        vec![hall],
        vec![attraction(AttractionKind::Panel, 60, &[])],
    );
    request.busy.push(Busy {
        room_id: hall_id,
        host_ids: Vec::new(),
        starts_at: datetime!(2026-08-14 10:00),
        ends_at: datetime!(2026-08-14 11:00),
    });

    let plan = generate(request);

    assert_eq!(plan.placed[0].starts_at, datetime!(2026-08-14 11:00));
}

#[test]
fn an_existing_slot_blocks_its_host_in_every_room() {
    let host = Uuid::new_v4();
    let mut request = input(
        one_day(),
        vec![room(RoomKind::Panel), room(RoomKind::Panel)],
        vec![attraction(AttractionKind::Panel, 60, &[host])],
    );
    request.busy.push(Busy {
        room_id: Uuid::new_v4(),
        host_ids: vec![host],
        starts_at: datetime!(2026-08-14 10:00),
        ends_at: datetime!(2026-08-14 11:00),
    });

    let plan = generate(request);

    assert_eq!(plan.placed[0].starts_at, datetime!(2026-08-14 11:00));
}

#[test]
fn a_full_program_reports_what_didnt_fit() {
    let days = vec![Day {
        date: date!(2026 - 08 - 14),
        opens_at: time!(10:00),
        closes_at: time!(11:00),
    }];

    let plan = generate(input(
        days,
        vec![room(RoomKind::Panel)],
        vec![
            attraction(AttractionKind::Panel, 60, &[]),
            attraction(AttractionKind::Panel, 60, &[]),
        ],
    ));

    assert_eq!(plan.placed.len(), 1);
    assert_eq!(only_reason(&plan), &UnplacedReason::NoFreeSlot);
}

#[test]
fn placement_spills_onto_the_next_day() {
    let days = vec![
        Day {
            date: date!(2026 - 08 - 14),
            opens_at: time!(10:00),
            closes_at: time!(11:00),
        },
        Day {
            date: date!(2026 - 08 - 15),
            opens_at: time!(10:00),
            closes_at: time!(14:00),
        },
    ];

    let plan = generate(input(
        days,
        vec![room(RoomKind::Panel)],
        vec![
            attraction(AttractionKind::Panel, 60, &[]),
            attraction(AttractionKind::Panel, 60, &[]),
        ],
    ));

    assert_eq!(plan.placed.len(), 2);
    assert_eq!(plan.placed[1].starts_at, datetime!(2026-08-15 10:00));
}
