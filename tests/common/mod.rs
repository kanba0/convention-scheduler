//! Helpers shared across the integration suites (`mod common;`).
#![allow(dead_code)] // each suite uses only a subset

use axum::http::StatusCode;
use axum_test::TestServer;
use serde_json::{Value, json};
use sqlx::PgPool;

use convention_scheduler::app;
use convention_scheduler::state::AppState;

/// Build the app under test on the given pool.
pub fn server(pool: PgPool) -> TestServer {
    TestServer::new(app(AppState { pool }))
}

/// Create a convention; returns its id.
pub async fn create_convention(server: &TestServer) -> String {
    let res = server
        .post("/conventions")
        .json(&json!({
            "name": "Test Con",
            "starts_on": "2026-08-01",
            "ends_on": "2026-08-03",
        }))
        .await;
    res.assert_status(StatusCode::CREATED);
    id_of(&res)
}

/// Create a room of the given kind (`panel`, `contest`, `panel_contest`); returns its id.
pub async fn create_room(
    server: &TestServer,
    convention_id: &str,
    name: &str,
    kind: &str,
) -> String {
    let res = server
        .post(&format!("/conventions/{convention_id}/rooms"))
        .json(&json!({ "name": name, "kind": kind }))
        .await;
    res.assert_status(StatusCode::CREATED);
    id_of(&res)
}

/// Create an attraction of the given kind (`panel`, `contest`); returns its id.
pub async fn create_attraction(
    server: &TestServer,
    convention_id: &str,
    title: &str,
    kind: &str,
    duration_minutes: i64,
) -> String {
    let res = server
        .post(&format!("/conventions/{convention_id}/attractions"))
        .json(&json!({
            "title": title,
            "kind": kind,
            "duration_minutes": duration_minutes,
            "description": null,
        }))
        .await;
    res.assert_status(StatusCode::CREATED);
    id_of(&res)
}

/// Create a panelist; returns its id.
pub async fn create_panelist(server: &TestServer, convention_id: &str, nick: &str) -> String {
    let res = server
        .post(&format!("/conventions/{convention_id}/panelists"))
        .json(&json!({ "nick": nick, "availability_note": null }))
        .await;
    res.assert_status(StatusCode::CREATED);
    id_of(&res)
}

/// Link a panelist to an attraction as a host.
pub async fn link_host(server: &TestServer, attraction_id: &str, panelist_id: &str) {
    server
        .put(&format!(
            "/attractions/{attraction_id}/panelists/{panelist_id}"
        ))
        .await
        .assert_status(StatusCode::NO_CONTENT);
}

/// Place an attraction into a room for a time range (`starts`/`ends` RFC3339); returns the slot id.
pub async fn create_slot(
    server: &TestServer,
    convention_id: &str,
    attraction_id: &str,
    room_id: &str,
    starts: &str,
    ends: &str,
) -> String {
    let res = server
        .post(&format!("/conventions/{convention_id}/slots"))
        .json(&json!({
            "attraction_id": attraction_id,
            "room_id": room_id,
            "starts_at": starts,
            "ends_at": ends,
        }))
        .await;
    res.assert_status(StatusCode::CREATED);
    id_of(&res)
}

/// POST a raw CSV body; returns the response so the caller can assert on it.
pub async fn import_csv(
    server: &TestServer,
    convention_id: &str,
    csv: &str,
) -> axum_test::TestResponse {
    server
        .post(&format!("/conventions/{convention_id}/import"))
        .text(csv)
        .await
}

/// The `conflicts` array from `GET /conventions/{id}/conflicts`.
pub async fn conflicts(server: &TestServer, convention_id: &str) -> Vec<Value> {
    let res = server
        .get(&format!("/conventions/{convention_id}/conflicts"))
        .await;
    res.assert_status_ok();
    res.json::<Value>()["conflicts"].as_array().unwrap().clone()
}

/// Look up the nicks hosting an attraction (by title), sorted.
pub async fn host_nicks(server: &TestServer, convention_id: &str, title: &str) -> Vec<String> {
    let id = attraction_id_by_title(server, convention_id, title).await;
    let res = server.get(&format!("/attractions/{id}/panelists")).await;
    res.assert_status_ok();
    let mut nicks: Vec<String> = res
        .json::<Value>()
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["nick"].as_str().unwrap().to_string())
        .collect();
    nicks.sort();
    nicks
}

/// Look up an attraction's id by its title.
pub async fn attraction_id_by_title(
    server: &TestServer,
    convention_id: &str,
    title: &str,
) -> String {
    let res = server
        .get(&format!("/conventions/{convention_id}/attractions"))
        .await;
    res.assert_status_ok();
    res.json::<Value>()
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["title"] == title)
        .unwrap_or_else(|| panic!("no attraction titled {title:?}"))["id"]
        .as_str()
        .unwrap()
        .to_string()
}

/// Pull the `id` field out of a JSON response body.
fn id_of(res: &axum_test::TestResponse) -> String {
    res.json::<Value>()["id"].as_str().unwrap().to_string()
}
