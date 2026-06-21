mod attractions;
mod conventions;
mod error;
mod host_links;
mod panelists;
mod rooms;
mod slots;
mod state;

use axum::{Json, Router, extract::State, routing::get};
use serde_json::{Value, json};
use sqlx::Row;
use sqlx::postgres::PgPoolOptions;
use std::env;

use crate::state::AppState;

#[tokio::main]
async fn main() {
    // Load .env if present; ignore if it isn't (e.g. in production the env is set for real).
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "convention_scheduler=debug,tower_http=debug,info".into()),
        )
        .init();

    let database_url =
        env::var("DATABASE_URL").expect("DATABASE_URL must be set (see .env / .env.example)");
    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string());

    // connect_lazy lets the server boot even if Postgres isn't up yet — the
    // first query (e.g. /health) is what actually opens a connection.
    let pool = PgPoolOptions::new()
        .max_connections(5)
        // Keep /health snappy: if Postgres is unreachable, fail fast instead of
        // blocking on the default 30s acquire timeout.
        .acquire_timeout(std::time::Duration::from_secs(2))
        .connect_lazy(&database_url)
        .expect("failed to build Postgres connection pool");

    // Run migrations, but don't crash the server if the DB isn't reachable yet.
    match sqlx::migrate!("./migrations").run(&pool).await {
        Ok(()) => tracing::info!("migrations applied"),
        Err(e) => tracing::warn!("could not run migrations (is Postgres up?): {e}"),
    }

    let state = AppState { pool };

    let app = Router::new()
        .route("/health", get(health))
        .merge(conventions::router())
        .merge(rooms::router())
        .merge(panelists::router())
        .merge(attractions::router())
        .merge(host_links::router())
        .merge(slots::router())
        .with_state(state)
        .layer(tower_http::trace::TraceLayer::new_for_http());

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind listener");
    tracing::info!("listening on http://{addr}");

    axum::serve(listener, app).await.expect("server error");
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
