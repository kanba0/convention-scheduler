//! Per-day program hours: seeding from the date span, additive re-seed on edit, and setting hours.

mod common;

use axum::http::StatusCode;
use common::*;
use serde_json::{Value, json};
use sqlx::PgPool;

/// GET the program days of a convention as a JSON array (earliest first).
async fn days(server: &axum_test::TestServer, convention_id: &str) -> Vec<Value> {
    let res = server
        .get(&format!("/conventions/{convention_id}/days"))
        .await;
    res.assert_status_ok();
    res.json::<Value>().as_array().unwrap().clone()
}

#[sqlx::test]
async fn creating_a_convention_seeds_a_day_per_program_date(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await; // spans 2026-08-01 .. 2026-08-03

    let days = days(&server, &con).await;

    let dates: Vec<&str> = days.iter().map(|d| d["day"].as_str().unwrap()).collect();
    assert_eq!(dates, ["2026-08-01", "2026-08-02", "2026-08-03"]);
    // Seeded days have no hours yet.
    assert!(
        days.iter()
            .all(|d| d["opens_at"].is_null() && d["closes_at"].is_null())
    );
}

#[sqlx::test]
async fn setting_a_days_hours_persists_them(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;

    let res = server
        .patch(&format!("/conventions/{con}/days/2026-08-01"))
        .json(&json!({ "opens_at": "14:00", "closes_at": "20:00" }))
        .await;
    res.assert_status_ok();

    let days = days(&server, &con).await;
    let first = &days[0];
    assert_eq!(first["day"], "2026-08-01");
    assert_eq!(first["opens_at"], "14:00");
    assert_eq!(first["closes_at"], "20:00");
}

#[sqlx::test]
async fn closing_before_opening_is_rejected(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;

    let res = server
        .patch(&format!("/conventions/{con}/days/2026-08-01"))
        .json(&json!({ "opens_at": "20:00", "closes_at": "14:00" }))
        .await;
    res.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn setting_hours_on_a_day_outside_the_span_is_404(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;

    let res = server
        .patch(&format!("/conventions/{con}/days/2026-09-09"))
        .json(&json!({ "opens_at": "14:00", "closes_at": "20:00" }))
        .await;
    res.assert_status(StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn extending_the_span_adds_the_new_days(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;

    server
        .patch(&format!("/conventions/{con}"))
        .json(&json!({ "ends_on": "2026-08-05" }))
        .await
        .assert_status_ok();

    let dates: Vec<String> = days(&server, &con)
        .await
        .iter()
        .map(|d| d["day"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        dates,
        [
            "2026-08-01",
            "2026-08-02",
            "2026-08-03",
            "2026-08-04",
            "2026-08-05"
        ]
    );
}

#[sqlx::test]
async fn extending_the_span_preserves_existing_hours(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;

    server
        .patch(&format!("/conventions/{con}/days/2026-08-01"))
        .json(&json!({ "opens_at": "14:00", "closes_at": "20:00" }))
        .await
        .assert_status_ok();

    server
        .patch(&format!("/conventions/{con}"))
        .json(&json!({ "ends_on": "2026-08-05" }))
        .await
        .assert_status_ok();

    let days = days(&server, &con).await;
    let first = days.iter().find(|d| d["day"] == "2026-08-01").unwrap();
    assert_eq!(first["opens_at"], "14:00");
    assert_eq!(first["closes_at"], "20:00");
}

#[sqlx::test]
async fn shrinking_the_span_keeps_the_now_orphaned_days(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;

    server
        .patch(&format!("/conventions/{con}"))
        .json(&json!({ "ends_on": "2026-08-02" }))
        .await
        .assert_status_ok();

    // The now out-of-range 2026-08-03 is kept, not deleted.
    let dates: Vec<String> = days(&server, &con)
        .await
        .iter()
        .map(|d| d["day"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(dates, ["2026-08-01", "2026-08-02", "2026-08-03"]);
}

#[sqlx::test]
async fn listing_days_for_a_missing_convention_is_404(pool: PgPool) {
    let server = server(pool);
    let res = server
        .get("/conventions/00000000-0000-0000-0000-000000000000/days")
        .await;
    res.assert_status(StatusCode::NOT_FOUND);
}
