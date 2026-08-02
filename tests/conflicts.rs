//! Integration tests for conflict detection (`GET /conventions/{id}/conflicts`).

use serde_json::json;
use sqlx::PgPool;

mod common;
use common::{
    conflicts, create_attraction, create_convention, create_panelist, create_room, create_slot,
    link_host, server,
};

/// Set one day's program hours (helper for the outside-hours checks).
async fn set_hours(
    server: &axum_test::TestServer,
    con: &str,
    day: &str,
    opens: &str,
    closes: &str,
) {
    server
        .patch(&format!("/conventions/{con}/days/{day}"))
        .json(&json!({ "opens_at": opens, "closes_at": closes }))
        .await
        .assert_status_ok();
}

/// The `slot_outside_hours` conflicts from the report.
async fn outside_hours(server: &axum_test::TestServer, con: &str) -> Vec<serde_json::Value> {
    conflicts(server, con)
        .await
        .into_iter()
        .filter(|c| c["type"] == "slot_outside_hours")
        .collect()
}

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
async fn slot_ending_after_closing_is_flagged(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    set_hours(&server, &con, "2026-08-01", "14:00", "20:00").await;
    let room = create_room(&server, &con, "Main Hall", "panel").await;
    let late = create_attraction(&server, &con, "Late Panel", "panel", 30).await;
    create_slot(
        &server,
        &con,
        &late,
        &room,
        "2026-08-01T20:00:00Z",
        "2026-08-01T21:00:00Z",
    )
    .await;

    let outside = outside_hours(&server, &con).await;
    assert_eq!(outside.len(), 1);
    let c = &outside[0];
    assert_eq!(c["slot"]["attraction_title"], "Late Panel");
    assert_eq!(c["day"], "2026-08-01");
    assert_eq!(c["opens_at"], "14:00");
    assert_eq!(c["closes_at"], "20:00");
}

#[sqlx::test]
async fn slot_starting_before_opening_is_flagged(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    set_hours(&server, &con, "2026-08-01", "14:00", "20:00").await;
    let room = create_room(&server, &con, "Main Hall", "panel").await;
    let early = create_attraction(&server, &con, "Early Panel", "panel", 30).await;
    create_slot(
        &server,
        &con,
        &early,
        &room,
        "2026-08-01T13:00:00Z",
        "2026-08-01T13:30:00Z",
    )
    .await;

    let outside = outside_hours(&server, &con).await;
    assert_eq!(outside.len(), 1);
    assert_eq!(outside[0]["slot"]["attraction_title"], "Early Panel");
}

#[sqlx::test]
async fn slot_straddling_the_closing_edge_is_flagged(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    set_hours(&server, &con, "2026-08-01", "14:00", "20:00").await;
    let room = create_room(&server, &con, "Main Hall", "panel").await;
    // Starts inside hours (19:30) but runs past close (20:30) — only the tail spills over.
    let over = create_attraction(&server, &con, "Overrun Panel", "panel", 60).await;
    create_slot(
        &server,
        &con,
        &over,
        &room,
        "2026-08-01T19:30:00Z",
        "2026-08-01T20:30:00Z",
    )
    .await;

    let outside = outside_hours(&server, &con).await;
    assert_eq!(outside.len(), 1);
    assert_eq!(outside[0]["slot"]["attraction_title"], "Overrun Panel");
}

#[sqlx::test]
async fn slot_within_program_hours_is_not_flagged(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    set_hours(&server, &con, "2026-08-01", "14:00", "20:00").await;
    let room = create_room(&server, &con, "Main Hall", "panel").await;
    let panel = create_attraction(&server, &con, "On-Time Panel", "panel", 60).await;
    create_slot(
        &server,
        &con,
        &panel,
        &room,
        "2026-08-01T15:00:00Z",
        "2026-08-01T16:00:00Z",
    )
    .await;

    assert!(outside_hours(&server, &con).await.is_empty());
}

#[sqlx::test]
async fn slot_on_a_day_with_unset_hours_is_not_flagged(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    // 2026-08-01's hours are set, but the slot lands on 2026-08-02, whose hours aren't.
    set_hours(&server, &con, "2026-08-01", "14:00", "20:00").await;
    let room = create_room(&server, &con, "Main Hall", "panel").await;
    let panel = create_attraction(&server, &con, "Late Night", "panel", 30).await;
    create_slot(
        &server,
        &con,
        &panel,
        &room,
        "2026-08-02T23:00:00Z",
        "2026-08-02T23:30:00Z",
    )
    .await;

    assert!(outside_hours(&server, &con).await.is_empty());
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
