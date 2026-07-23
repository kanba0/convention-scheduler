//! CRUD edge cases: partial (COALESCE) updates and the domain CHECK constraints.

use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;

mod common;
use common::{create_attraction, create_convention, server};

#[sqlx::test]
async fn patch_updates_only_the_given_fields(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    let attraction = create_attraction(&server, &con, "Original Title", "panel", 60).await;

    // Only duration is sent; the COALESCE update must leave title and kind alone.
    let res = server
        .patch(&format!("/attractions/{attraction}"))
        .json(&json!({ "duration_minutes": 90 }))
        .await;
    res.assert_status_ok();

    let body = res.json::<Value>();
    assert_eq!(body["duration_minutes"], 90);
    assert_eq!(body["title"], "Original Title");
    assert_eq!(body["kind"], "panel");
}

#[sqlx::test]
async fn attraction_with_negative_duration_is_rejected(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;

    let res = server
        .post(&format!("/conventions/{con}/attractions"))
        .json(&json!({
            "title": "Bad Panel",
            "kind": "panel",
            "duration_minutes": -30,
            "description": null,
        }))
        .await;
    res.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn convention_ending_before_it_starts_is_rejected(pool: PgPool) {
    let server = server(pool);

    let res = server
        .post("/conventions")
        .json(&json!({
            "name": "Backwards Con",
            "starts_on": "2026-08-03",
            "ends_on": "2026-08-01",
        }))
        .await;
    res.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}
