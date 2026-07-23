//! Integration tests for conflict detection (`GET /conventions/{id}/conflicts`).

use sqlx::PgPool;

mod common;
use common::{
    conflicts, create_attraction, create_convention, create_panelist, create_room, create_slot,
    link_host, server,
};

#[sqlx::test]
async fn clean_schedule_has_no_conflicts(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    let room = create_room(&server, &con, "Main Hall", "panel").await;

    let alice = create_panelist(&server, &con, "Alice").await;
    let a = create_attraction(&server, &con, "Panel A", "panel", 60).await;
    let b = create_attraction(&server, &con, "Panel B", "panel", 60).await;
    link_host(&server, &a, &alice).await;
    link_host(&server, &b, &alice).await;
    create_slot(
        &server,
        &con,
        &a,
        &room,
        "2026-08-01T10:00:00Z",
        "2026-08-01T11:00:00Z",
    )
    .await;
    create_slot(
        &server,
        &con,
        &b,
        &room,
        "2026-08-01T12:00:00Z",
        "2026-08-01T13:00:00Z",
    )
    .await;

    assert!(conflicts(&server, &con).await.is_empty());
}

#[sqlx::test]
async fn room_double_booked_is_reported(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    let room = create_room(&server, &con, "Main Hall", "panel").await;

    let early = create_attraction(&server, &con, "Early Panel", "panel", 60).await;
    let late = create_attraction(&server, &con, "Overlapping Panel", "panel", 60).await;
    create_slot(
        &server,
        &con,
        &early,
        &room,
        "2026-08-01T10:00:00Z",
        "2026-08-01T11:00:00Z",
    )
    .await;
    create_slot(
        &server,
        &con,
        &late,
        &room,
        "2026-08-01T10:30:00Z",
        "2026-08-01T11:30:00Z",
    )
    .await;

    let conflicts = conflicts(&server, &con).await;
    assert_eq!(conflicts.len(), 1);
    let c = &conflicts[0];
    assert_eq!(c["type"], "room_double_booked");
    assert_eq!(c["room_name"], "Main Hall");
    // The two slots in a pair aren't ordered by time (the query pairs them by id),
    // so compare the set.
    let mut titles = [
        c["slots"][0]["attraction_title"].as_str().unwrap(),
        c["slots"][1]["attraction_title"].as_str().unwrap(),
    ];
    titles.sort();
    assert_eq!(titles, ["Early Panel", "Overlapping Panel"]);
}

#[sqlx::test]
async fn touching_slots_do_not_conflict(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    let room = create_room(&server, &con, "Main Hall", "panel").await;

    // The ranges are half-open [start, end), so one ending exactly as the next
    // begins is not an overlap.
    let a = create_attraction(&server, &con, "Panel A", "panel", 60).await;
    let b = create_attraction(&server, &con, "Panel B", "panel", 60).await;
    create_slot(
        &server,
        &con,
        &a,
        &room,
        "2026-08-01T10:00:00Z",
        "2026-08-01T11:00:00Z",
    )
    .await;
    create_slot(
        &server,
        &con,
        &b,
        &room,
        "2026-08-01T11:00:00Z",
        "2026-08-01T12:00:00Z",
    )
    .await;

    assert!(conflicts(&server, &con).await.is_empty());
}

#[sqlx::test]
async fn panelist_double_booked_is_reported(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;

    let alice = create_panelist(&server, &con, "Alice").await;
    let alpha = create_attraction(&server, &con, "Alpha", "panel", 60).await;
    let beta = create_attraction(&server, &con, "Beta", "panel", 60).await;
    link_host(&server, &alpha, &alice).await;
    link_host(&server, &beta, &alice).await;

    // Different rooms, so the only clash is the person.
    let room1 = create_room(&server, &con, "Room 1", "panel").await;
    let room2 = create_room(&server, &con, "Room 2", "panel").await;
    create_slot(
        &server,
        &con,
        &alpha,
        &room1,
        "2026-08-01T10:00:00Z",
        "2026-08-01T11:00:00Z",
    )
    .await;
    create_slot(
        &server,
        &con,
        &beta,
        &room2,
        "2026-08-01T10:30:00Z",
        "2026-08-01T11:30:00Z",
    )
    .await;

    let conflicts = conflicts(&server, &con).await;
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0]["type"], "panelist_double_booked");
    assert_eq!(conflicts[0]["panelist_nick"], "Alice");
}

#[sqlx::test]
async fn room_type_mismatch_flags_only_incompatible_rooms(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;

    let panel_room = create_room(&server, &con, "Panel Room", "panel").await;
    let flex_room = create_room(&server, &con, "Flex Room", "panel_contest").await;

    let cosplay = create_attraction(&server, &con, "Cosplay", "contest", 60).await;
    create_slot(
        &server,
        &con,
        &cosplay,
        &panel_room,
        "2026-08-01T10:00:00Z",
        "2026-08-01T11:00:00Z",
    )
    .await;
    // A panel_contest room hosts any kind, so this contest is fine there.
    let karaoke = create_attraction(&server, &con, "Karaoke", "contest", 60).await;
    create_slot(
        &server,
        &con,
        &karaoke,
        &flex_room,
        "2026-08-01T10:00:00Z",
        "2026-08-01T11:00:00Z",
    )
    .await;

    let conflicts = conflicts(&server, &con).await;
    assert_eq!(conflicts.len(), 1);
    let c = &conflicts[0];
    assert_eq!(c["type"], "room_type_mismatch");
    assert_eq!(c["slot"]["attraction_title"], "Cosplay");
    assert_eq!(c["room_name"], "Panel Room");
}

#[sqlx::test]
async fn missing_convention_is_404(pool: PgPool) {
    let server = server(pool);
    let ghost = "00000000-0000-0000-0000-000000000000";
    server
        .get(&format!("/conventions/{ghost}/conflicts"))
        .await
        .assert_status_not_found();
}
