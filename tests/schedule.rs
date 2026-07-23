//! Integration tests for the assembled schedule view (`GET /conventions/{id}/schedule`).

use serde_json::Value;
use sqlx::PgPool;

mod common;
use common::{
    create_attraction, create_convention, create_panelist, create_room, create_slot, link_host,
    server,
};

#[sqlx::test]
async fn schedule_stitches_slot_with_room_attraction_and_hosts(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    let room = create_room(&server, &con, "Main Hall", "panel").await;
    let attraction = create_attraction(&server, &con, "Opening Panel", "panel", 60).await;
    let alice = create_panelist(&server, &con, "Alice").await;
    link_host(&server, &attraction, &alice).await;
    create_slot(
        &server,
        &con,
        &attraction,
        &room,
        "2026-08-01T10:00:00Z",
        "2026-08-01T11:00:00Z",
    )
    .await;

    let res = server.get(&format!("/conventions/{con}/schedule")).await;
    res.assert_status_ok();
    let body = res.json::<Value>();

    assert_eq!(body["convention"]["name"], "Test Con");
    let slots = body["slots"].as_array().unwrap();
    assert_eq!(slots.len(), 1);
    let slot = &slots[0];
    assert_eq!(slot["room"]["name"], "Main Hall");
    assert_eq!(slot["attraction"]["title"], "Opening Panel");
    assert_eq!(slot["attraction"]["duration_minutes"], 60);
    let hosts = slot["hosts"].as_array().unwrap();
    assert_eq!(hosts.len(), 1);
    assert_eq!(hosts[0]["nick"], "Alice");
}

#[sqlx::test]
async fn schedule_for_a_missing_convention_is_404(pool: PgPool) {
    let server = server(pool);
    let ghost = "00000000-0000-0000-0000-000000000000";
    server
        .get(&format!("/conventions/{ghost}/schedule"))
        .await
        .assert_status_not_found();
}
