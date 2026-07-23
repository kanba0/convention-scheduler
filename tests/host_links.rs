//! Integration tests for host links — the attraction↔panelist assignment, its
//! idempotency, cross-convention rejection, and unlink behaviour.

use axum::http::StatusCode;
use serde_json::Value;
use sqlx::PgPool;

mod common;
use common::{create_attraction, create_convention, create_panelist, link_host, server};

#[sqlx::test]
async fn linking_a_host_is_idempotent(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    let attraction = create_attraction(&server, &con, "Panel", "panel", 60).await;
    let alice = create_panelist(&server, &con, "Alice").await;

    // link_host asserts 204, so linking twice asserts the repeat is a no-op 204.
    link_host(&server, &attraction, &alice).await;
    link_host(&server, &attraction, &alice).await;

    let hosts = server
        .get(&format!("/attractions/{attraction}/panelists"))
        .await
        .json::<Value>();
    assert_eq!(hosts.as_array().unwrap().len(), 1);
}

#[sqlx::test]
async fn cross_convention_link_is_rejected(pool: PgPool) {
    let server = server(pool);
    let home = create_convention(&server).await;
    let other = create_convention(&server).await;
    let attraction = create_attraction(&server, &home, "Panel", "panel", 60).await;
    let foreign_panelist = create_panelist(&server, &other, "Bob").await;

    let res = server
        .put(&format!(
            "/attractions/{attraction}/panelists/{foreign_panelist}"
        ))
        .await;
    res.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn unlinking_a_host_that_isnt_linked_is_404(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    let attraction = create_attraction(&server, &con, "Panel", "panel", 60).await;
    let alice = create_panelist(&server, &con, "Alice").await;

    let res = server
        .delete(&format!("/attractions/{attraction}/panelists/{alice}"))
        .await;
    res.assert_status_not_found();
}

#[sqlx::test]
async fn unlinking_removes_the_host(pool: PgPool) {
    let server = server(pool);
    let con = create_convention(&server).await;
    let attraction = create_attraction(&server, &con, "Panel", "panel", 60).await;
    let alice = create_panelist(&server, &con, "Alice").await;
    link_host(&server, &attraction, &alice).await;

    server
        .delete(&format!("/attractions/{attraction}/panelists/{alice}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);

    let hosts = server
        .get(&format!("/attractions/{attraction}/panelists"))
        .await
        .json::<Value>();
    assert!(hosts.as_array().unwrap().is_empty());
}
