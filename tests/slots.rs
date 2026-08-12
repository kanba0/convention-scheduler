//! Integration tests for slots — placement of an attraction into a room/time,
//! and the guards around it (cross-convention refs, single placement, time order).

use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;

mod common;
use common::{create_attraction, create_convention, create_room, create_slot, server};

#[sqlx::test]
async fn placement_requires_refs_in_the_same_convention(pool: PgPool) {
    let server = server(pool);
    let home = create_convention(&server).await;
    let other = create_convention(&server).await;
    let attraction = create_attraction(&server, &home, "Panel", "panel", 60).await;
    let foreign_room = create_room(&server, &other, "Hall", "panel").await;

    let res = server
        .post(&format!("/conventions/{home}/slots"))
        .json(&json!({
            "attraction_id": attraction,
            "room_id": foreign_room,
            "starts_at": "2026-08-01T10:00:00",
            "ends_at": "2026-08-01T11:00:00",
        }))
        .await;
    res.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn placing_an_attraction_twice_is_a_conflict(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    let room = create_room(&server, &con, "Hall", "panel").await;
    let attraction = create_attraction(&server, &con, "Panel", "panel", 60).await;
    create_slot(
        &server,
        &con,
        &attraction,
        &room,
        "2026-08-01T10:00:00",
        "2026-08-01T11:00:00",
    )
    .await;

    let res = server
        .post(&format!("/conventions/{con}/slots"))
        .json(&json!({
            "attraction_id": attraction,
            "room_id": room,
            "starts_at": "2026-08-01T12:00:00",
            "ends_at": "2026-08-01T13:00:00",
        }))
        .await;
    res.assert_status(StatusCode::CONFLICT);
}

#[sqlx::test]
async fn slot_ending_before_it_starts_is_rejected(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    let room = create_room(&server, &con, "Hall", "panel").await;
    let attraction = create_attraction(&server, &con, "Panel", "panel", 60).await;

    let res = server
        .post(&format!("/conventions/{con}/slots"))
        .json(&json!({
            "attraction_id": attraction,
            "room_id": room,
            "starts_at": "2026-08-01T11:00:00",
            "ends_at": "2026-08-01T10:00:00",
        }))
        .await;
    res.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn moving_a_slot_to_a_foreign_room_is_422_not_404(pool: PgPool) {
    let server = server(pool);
    let home = create_convention(&server).await;
    let other = create_convention(&server).await;
    let room = create_room(&server, &home, "Home Hall", "panel").await;
    let foreign_room = create_room(&server, &other, "Foreign Hall", "panel").await;
    let attraction = create_attraction(&server, &home, "Panel", "panel", 60).await;
    let slot = create_slot(
        &server,
        &home,
        &attraction,
        &room,
        "2026-08-01T10:00:00",
        "2026-08-01T11:00:00",
    )
    .await;

    // The slot exists, so a foreign room is a validation error, not a missing slot.
    let res = server
        .patch(&format!("/slots/{slot}"))
        .json(&json!({ "room_id": foreign_room }))
        .await;
    res.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn patching_a_missing_slot_is_404(pool: PgPool) {
    let server = server(pool);
    let ghost = "00000000-0000-0000-0000-000000000000";
    let res = server
        .patch(&format!("/slots/{ghost}"))
        .json(&json!({ "starts_at": "2026-08-01T10:00:00" }))
        .await;
    res.assert_status_not_found();
}

#[sqlx::test]
async fn moving_a_slot_to_a_room_in_its_convention_updates_it(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    let room1 = create_room(&server, &con, "Hall 1", "panel").await;
    let room2 = create_room(&server, &con, "Hall 2", "panel").await;
    let attraction = create_attraction(&server, &con, "Panel", "panel", 60).await;
    let slot = create_slot(
        &server,
        &con,
        &attraction,
        &room1,
        "2026-08-01T10:00:00",
        "2026-08-01T11:00:00",
    )
    .await;

    let res = server
        .patch(&format!("/slots/{slot}"))
        .json(&json!({ "room_id": room2 }))
        .await;
    res.assert_status_ok();
    assert_eq!(res.json::<Value>()["room_id"].as_str().unwrap(), room2);
}

#[sqlx::test]
async fn moving_a_slot_in_time_updates_it(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    let room = create_room(&server, &con, "Hall", "panel").await;
    let attraction = create_attraction(&server, &con, "Panel", "panel", 60).await;
    let slot = create_slot(
        &server,
        &con,
        &attraction,
        &room,
        "2026-08-01T10:00:00",
        "2026-08-01T11:00:00",
    )
    .await;

    let res = server
        .patch(&format!("/slots/{slot}"))
        .json(&json!({
            "starts_at": "2026-08-01T14:00:00",
            "ends_at": "2026-08-01T15:00:00",
        }))
        .await;
    res.assert_status_ok();

    // Compared as strings: this is also where the zone-less wire format is pinned.
    let body = res.json::<Value>();
    assert_eq!(body["starts_at"], "2026-08-01T14:00:00");
    assert_eq!(body["ends_at"], "2026-08-01T15:00:00");
}
