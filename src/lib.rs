//! The library crate: all the resource modules plus `app`, which assembles them
//! into the router. `main.rs` is a thin binary that connects the DB and serves
//! `app`; integration tests build the same `app` and fire requests at it directly.

mod attractions;
mod conflicts;
mod conventions;
mod error;
mod host_links;
mod import;
mod panelists;
mod rooms;
mod schedule;
mod slots;
pub mod state;

use axum::{Json, Router, extract::State, routing::get};
use serde_json::{Value, json};
use sqlx::Row;

use crate::state::AppState;

/// Build the full application router from shared state. Both the real server
/// (`main`) and the integration tests go through here, so they exercise the
/// exact same routing, handlers, and middleware.
pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .merge(conventions::router())
        .merge(rooms::router())
        .merge(panelists::router())
        .merge(attractions::router())
        .merge(import::router())
        .merge(host_links::router())
        .merge(slots::router())
        .merge(schedule::router())
        .merge(conflicts::router())
        .with_state(state)
        .layer(tower_http::trace::TraceLayer::new_for_http())
}

/// Liveness + DB connectivity probe. Always returns 200 so the process reports
/// "alive"; the body reports whether Postgres is actually reachable.
async fn health(State(state): State<AppState>) -> Json<Value> {
    let db_up = sqlx::query("SELECT 1")
        .fetch_one(&state.pool)
        .await
        .and_then(|row| row.try_get::<i32, _>(0))
        .is_ok();

    Json(json!({
        "status": "ok",
        "db": if db_up { "up" } else { "down" },
    }))
}
