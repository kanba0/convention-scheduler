//! Integration tests for the CSV importer (`POST /conventions/{id}/import`).

use axum::http::StatusCode;
use serde_json::Value;
use sqlx::PgPool;

mod common;
use common::{create_convention, host_nicks, import_csv, server};

#[sqlx::test]
async fn clean_import_creates_attractions_panelists_and_links(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;

    let csv = "title,kind,duration_hours,hosts,description\n\
               Opening Panel,panel,1,\"Alice, Bob\",Welcome\n\
               Cosplay Contest,contest,1.5,Carol,\n";
    let res = import_csv(&server, &con, csv).await;
    res.assert_status_ok();

    let summary = res.json::<Value>();
    assert_eq!(summary["attractions_created"], 2);
    assert_eq!(summary["panelists_created"], 3);
    assert_eq!(summary["links_created"], 3);

    assert_eq!(
        host_nicks(&server, &con, "Opening Panel").await,
        ["Alice", "Bob"]
    );
    assert_eq!(
        host_nicks(&server, &con, "Cosplay Contest").await,
        ["Carol"]
    );
}

#[sqlx::test]
async fn absent_kind_defaults_to_panel(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;

    let csv = "title,duration_hours\n\
               Mystery Panel,1\n";
    import_csv(&server, &con, csv).await.assert_status_ok();

    let attractions = server
        .get(&format!("/conventions/{con}/attractions"))
        .await
        .json::<Value>();
    assert_eq!(attractions.as_array().unwrap()[0]["kind"], "panel");
}

#[sqlx::test]
async fn duration_hours_convert_to_minutes(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;

    let csv = "title,duration_hours\n\
               Half-Hour Panel,1.5\n";
    import_csv(&server, &con, csv).await.assert_status_ok();

    let attractions = server
        .get(&format!("/conventions/{con}/attractions"))
        .await
        .json::<Value>();
    assert_eq!(attractions.as_array().unwrap()[0]["duration_minutes"], 90);
}

#[sqlx::test]
async fn shared_host_is_created_once(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;

    let csv = "title,duration_hours,hosts\n\
               Panel A,1,Alice\n\
               Panel B,1,Alice\n";
    let res = import_csv(&server, &con, csv).await;
    res.assert_status_ok();

    let summary = res.json::<Value>();
    assert_eq!(summary["panelists_created"], 1);
    assert_eq!(summary["links_created"], 2);
}

#[sqlx::test]
async fn repeated_nick_in_one_cell_links_once(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;

    let csv = "title,duration_hours,hosts\n\
               Panel,1,\"Alice, Alice\"\n";
    let res = import_csv(&server, &con, csv).await;
    res.assert_status_ok();

    let summary = res.json::<Value>();
    assert_eq!(summary["panelists_created"], 1);
    assert_eq!(summary["links_created"], 1);
}

#[sqlx::test]
async fn row_without_title_is_rejected(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;

    let csv = "title,duration_hours\n\
               ,1\n";
    let res = import_csv(&server, &con, csv).await;
    res.assert_status(StatusCode::UNPROCESSABLE_ENTITY);

    let errors = res.json::<Value>()["errors"].as_array().unwrap().clone();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].as_str().unwrap().contains("title is required"));
}

#[sqlx::test]
async fn non_positive_duration_is_rejected(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;

    let csv = "title,duration_hours\n\
               Zero Panel,0\n";
    let res = import_csv(&server, &con, csv).await;
    res.assert_status(StatusCode::UNPROCESSABLE_ENTITY);

    let errors = res.json::<Value>()["errors"].as_array().unwrap().clone();
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0]
            .as_str()
            .unwrap()
            .contains("duration_hours must be positive")
    );
}

#[sqlx::test]
async fn duration_rounding_to_zero_is_rejected(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;

    // 0.005h is positive but rounds to 0 minutes — a different check than <= 0.
    let csv = "title,duration_hours\n\
               Blink Panel,0.005\n";
    let res = import_csv(&server, &con, csv).await;
    res.assert_status(StatusCode::UNPROCESSABLE_ENTITY);

    let errors = res.json::<Value>()["errors"].as_array().unwrap().clone();
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0]
            .as_str()
            .unwrap()
            .contains("duration rounds to 0 minutes")
    );
}

#[sqlx::test]
async fn unknown_kind_is_rejected(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;

    let csv = "title,kind,duration_hours\n\
               Weird Panel,banquet,1\n";
    let res = import_csv(&server, &con, csv).await;
    res.assert_status(StatusCode::UNPROCESSABLE_ENTITY);

    let errors = res.json::<Value>()["errors"].as_array().unwrap().clone();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].as_str().unwrap().contains("unknown variant"));
}

#[sqlx::test]
async fn all_invalid_rows_reported_in_one_pass(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;

    // Missing title on line 2, non-positive duration on line 4.
    let csv = "title,kind,duration_hours,hosts,description\n\
               ,panel,1,Alice,\n\
               Good Panel,panel,1,Bob,\n\
               Bad Duration,panel,0,Carol,\n";
    let res = import_csv(&server, &con, csv).await;
    res.assert_status(StatusCode::UNPROCESSABLE_ENTITY);

    let errors = res.json::<Value>()["errors"].as_array().unwrap().clone();
    assert_eq!(errors.len(), 2);
    assert!(
        errors
            .iter()
            .any(|e| e.as_str().unwrap().contains("line 2"))
    );
    assert!(
        errors
            .iter()
            .any(|e| e.as_str().unwrap().contains("line 4"))
    );
}

#[sqlx::test]
async fn import_is_all_or_nothing(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;

    let csv = "title,kind,duration_hours,hosts,description\n\
               ,panel,1,Alice,\n\
               Good Panel,panel,1,Bob,\n";
    import_csv(&server, &con, csv)
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY);

    let attractions = server
        .get(&format!("/conventions/{con}/attractions"))
        .await
        .json::<Value>();
    assert!(attractions.as_array().unwrap().is_empty());
}

#[sqlx::test]
async fn missing_convention_is_404(pool: PgPool) {
    let server = server(pool);
    let ghost = "00000000-0000-0000-0000-000000000000";
    let csv = "title,kind,duration_hours,hosts,description\n\
               Panel,panel,1,Alice,\n";
    import_csv(&server, ghost, csv)
        .await
        .assert_status_not_found();
}
