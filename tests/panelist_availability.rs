//! Structured panelist availability windows: add, list, remove, and the empty-is-available state.

mod common;

use axum::http::StatusCode;
use common::{create_convention, create_panelist, server};
use serde_json::{Value, json};
use sqlx::PgPool;

const GHOST: &str = "00000000-0000-0000-0000-000000000000";

#[sqlx::test]
async fn adding_a_window_returns_it(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    let panelist = create_panelist(&server, &con, "Alice").await;

    let res = server
        .post(&format!("/panelists/{panelist}/availability"))
        .json(&json!({ "starts_at": "2026-08-01T09:00:00", "ends_at": "2026-08-01T18:00:00" }))
        .await;
    res.assert_status(StatusCode::CREATED);
    let created = res.json::<Value>();

    let listed = server
        .get(&format!("/panelists/{panelist}/availability"))
        .await;
    listed.assert_status_ok();
    let windows = listed.json::<Value>();
    let windows = windows.as_array().unwrap();
    assert_eq!(windows.len(), 1);
    // Compare against the POST response (both server-serialized) rather than the raw input.
    assert_eq!(windows[0]["id"], created["id"]);
    assert_eq!(windows[0]["starts_at"], created["starts_at"]);
    assert_eq!(windows[0]["ends_at"], created["ends_at"]);
}

#[sqlx::test]
async fn a_panelist_with_no_windows_has_an_empty_list(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    let panelist = create_panelist(&server, &con, "Alice").await;

    let res = server
        .get(&format!("/panelists/{panelist}/availability"))
        .await;
    res.assert_status_ok();
    assert!(res.json::<Value>().as_array().unwrap().is_empty());
}

#[sqlx::test]
async fn windows_come_back_earliest_first(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    let panelist = create_panelist(&server, &con, "Alice").await;

    // Added late-then-early; the list must reorder them.
    server
        .post(&format!("/panelists/{panelist}/availability"))
        .json(&json!({ "starts_at": "2026-08-01T12:00:00", "ends_at": "2026-08-01T13:00:00" }))
        .await
        .assert_status(StatusCode::CREATED);
    let early = server
        .post(&format!("/panelists/{panelist}/availability"))
        .json(&json!({ "starts_at": "2026-08-01T09:00:00", "ends_at": "2026-08-01T10:00:00" }))
        .await;
    let early = early.json::<Value>();

    let listed = server
        .get(&format!("/panelists/{panelist}/availability"))
        .await;
    let windows = listed.json::<Value>();
    let windows = windows.as_array().unwrap();
    assert_eq!(windows.len(), 2);
    assert_eq!(windows[0]["id"], early["id"]);
}

#[sqlx::test]
async fn an_end_before_start_is_rejected(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    let panelist = create_panelist(&server, &con, "Alice").await;

    let res = server
        .post(&format!("/panelists/{panelist}/availability"))
        .json(&json!({ "starts_at": "2026-08-01T18:00:00", "ends_at": "2026-08-01T09:00:00" }))
        .await;
    res.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn removing_a_window_deletes_it(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    let panelist = create_panelist(&server, &con, "Alice").await;

    let created = server
        .post(&format!("/panelists/{panelist}/availability"))
        .json(&json!({ "starts_at": "2026-08-01T09:00:00", "ends_at": "2026-08-01T18:00:00" }))
        .await;
    let window_id = created.json::<Value>()["id"].as_str().unwrap().to_string();

    server
        .delete(&format!("/panelists/{panelist}/availability/{window_id}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);

    let listed = server
        .get(&format!("/panelists/{panelist}/availability"))
        .await;
    assert!(listed.json::<Value>().as_array().unwrap().is_empty());
}

#[sqlx::test]
async fn removing_a_window_that_isnt_there_is_404(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    let panelist = create_panelist(&server, &con, "Alice").await;

    server
        .delete(&format!("/panelists/{panelist}/availability/{GHOST}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn adding_a_window_for_a_missing_panelist_is_404(pool: PgPool) {
    let server = server(pool);
    let res = server
        .post(&format!("/panelists/{GHOST}/availability"))
        .json(&json!({ "starts_at": "2026-08-01T09:00:00", "ends_at": "2026-08-01T18:00:00" }))
        .await;
    res.assert_status(StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn listing_windows_for_a_missing_panelist_is_404(pool: PgPool) {
    let server = server(pool);
    server
        .get(&format!("/panelists/{GHOST}/availability"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
}
