//! Integration tests for `GET /conventions/{id}/schedule/generate` — the loading
//! and the dry-run guarantee. The packing itself is covered in `engine.rs`.

use axum_test::TestServer;
use serde_json::{Value, json};
use sqlx::PgPool;

mod common;
use common::{
    create_attraction, create_convention, create_panelist, create_room, create_slot, link_host,
    server,
};

async fn set_hours(server: &TestServer, con: &str, day: &str, opens: &str, closes: &str) {
    server
        .patch(&format!("/conventions/{con}/days/{day}"))
        .json(&json!({ "opens_at": opens, "closes_at": closes }))
        .await
        .assert_status_ok();
}

async fn generate(server: &TestServer, con: &str) -> Value {
    let res = server
        .get(&format!("/conventions/{con}/schedule/generate"))
        .await;
    res.assert_status_ok();
    res.json::<Value>()
}

async fn slot_count(server: &TestServer, con: &str) -> usize {
    let res = server.get(&format!("/conventions/{con}/slots")).await;
    res.assert_status_ok();
    res.json::<Value>().as_array().unwrap().len()
}

#[sqlx::test]
async fn an_unplaced_attraction_is_proposed_a_slot(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    set_hours(&server, &con, "2026-08-01", "10:00", "14:00").await;
    let hall = create_room(&server, &con, "Hall", "panel").await;
    let attraction = create_attraction(&server, &con, "Panel", "panel", 60).await;

    let plan = generate(&server, &con).await;

    assert_eq!(plan["unplaced"].as_array().unwrap().len(), 0);
    let placed = plan["placed"].as_array().unwrap();
    assert_eq!(placed.len(), 1);
    assert_eq!(placed[0]["attraction_id"], attraction);
    assert_eq!(placed[0]["room_id"], hall);
    assert_eq!(placed[0]["starts_at"], "2026-08-01T10:00:00");
    assert_eq!(placed[0]["ends_at"], "2026-08-01T11:00:00");
}

#[sqlx::test]
async fn generating_writes_nothing(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    set_hours(&server, &con, "2026-08-01", "10:00", "14:00").await;
    create_room(&server, &con, "Hall", "panel").await;
    create_attraction(&server, &con, "Panel", "panel", 60).await;

    let plan = generate(&server, &con).await;

    assert_eq!(plan["placed"].as_array().unwrap().len(), 1);
    assert_eq!(slot_count(&server, &con).await, 0);
}

#[sqlx::test]
async fn an_already_placed_attraction_is_left_out(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    set_hours(&server, &con, "2026-08-01", "10:00", "14:00").await;
    let hall = create_room(&server, &con, "Hall", "panel").await;
    let placed_already = create_attraction(&server, &con, "Placed", "panel", 60).await;
    let waiting = create_attraction(&server, &con, "Waiting", "panel", 60).await;
    create_slot(
        &server,
        &con,
        &placed_already,
        &hall,
        "2026-08-01T10:00:00",
        "2026-08-01T11:00:00",
    )
    .await;

    let plan = generate(&server, &con).await;

    let placed = plan["placed"].as_array().unwrap();
    assert_eq!(placed.len(), 1);
    assert_eq!(placed[0]["attraction_id"], waiting);
    assert_eq!(plan["unplaced"].as_array().unwrap().len(), 0);
}

#[sqlx::test]
async fn an_existing_slot_blocks_its_room(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    set_hours(&server, &con, "2026-08-01", "10:00", "14:00").await;
    let hall = create_room(&server, &con, "Hall", "panel").await;
    let occupant = create_attraction(&server, &con, "Occupant", "panel", 60).await;
    create_attraction(&server, &con, "Waiting", "panel", 60).await;
    create_slot(
        &server,
        &con,
        &occupant,
        &hall,
        "2026-08-01T10:00:00",
        "2026-08-01T11:00:00",
    )
    .await;

    let plan = generate(&server, &con).await;

    assert_eq!(plan["placed"][0]["starts_at"], "2026-08-01T11:00:00");
}

#[sqlx::test]
async fn an_existing_slot_blocks_its_host_elsewhere(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    set_hours(&server, &con, "2026-08-01", "10:00", "14:00").await;
    let hall = create_room(&server, &con, "Hall 1", "panel").await;
    create_room(&server, &con, "Hall 2", "panel").await;
    let host = create_panelist(&server, &con, "Ala").await;
    let occupant = create_attraction(&server, &con, "Occupant", "panel", 60).await;
    let waiting = create_attraction(&server, &con, "Waiting", "panel", 60).await;
    link_host(&server, &occupant, &host).await;
    link_host(&server, &waiting, &host).await;
    create_slot(
        &server,
        &con,
        &occupant,
        &hall,
        "2026-08-01T10:00:00",
        "2026-08-01T11:00:00",
    )
    .await;

    let plan = generate(&server, &con).await;

    assert_eq!(plan["placed"][0]["starts_at"], "2026-08-01T11:00:00");
}

#[sqlx::test]
async fn a_hosts_window_moves_the_placement(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    set_hours(&server, &con, "2026-08-01", "10:00", "14:00").await;
    create_room(&server, &con, "Hall", "panel").await;
    let host = create_panelist(&server, &con, "Ala").await;
    let attraction = create_attraction(&server, &con, "Panel", "panel", 60).await;
    link_host(&server, &attraction, &host).await;
    server
        .post(&format!("/panelists/{host}/availability"))
        .json(&json!({
            "starts_at": "2026-08-01T12:00:00",
            "ends_at": "2026-08-01T14:00:00",
        }))
        .await
        .assert_status(axum::http::StatusCode::CREATED);

    let plan = generate(&server, &con).await;

    assert_eq!(plan["placed"][0]["starts_at"], "2026-08-01T12:00:00");
}

#[sqlx::test]
async fn a_day_without_hours_is_not_scheduled_into(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    create_room(&server, &con, "Hall", "panel").await;
    let attraction = create_attraction(&server, &con, "Panel", "panel", 60).await;

    let plan = generate(&server, &con).await;

    assert_eq!(plan["placed"].as_array().unwrap().len(), 0);
    let unplaced = plan["unplaced"].as_array().unwrap();
    assert_eq!(unplaced.len(), 1);
    assert_eq!(unplaced[0]["attraction_id"], attraction);
    assert_eq!(unplaced[0]["reason"], "does_not_fit_any_day");
}

#[sqlx::test]
async fn an_attraction_with_no_room_for_its_kind_says_so(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    set_hours(&server, &con, "2026-08-01", "10:00", "14:00").await;
    create_room(&server, &con, "Hall", "panel").await;
    create_attraction(&server, &con, "Contest", "contest", 60).await;

    let plan = generate(&server, &con).await;

    assert_eq!(
        plan["unplaced"][0]["reason"].as_str().unwrap(),
        "no_compatible_room"
    );
}

#[sqlx::test]
async fn generating_for_a_missing_convention_is_404(pool: PgPool) {
    let server = server(pool);
    let ghost = "00000000-0000-0000-0000-000000000000";
    server
        .get(&format!("/conventions/{ghost}/schedule/generate"))
        .await
        .assert_status_not_found();
}
