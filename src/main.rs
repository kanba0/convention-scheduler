use axum::{Json, Router, extract::State, routing::get};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use std::env;

/// Shared application state handed to every request handler.
#[derive(Clone)]
struct AppState {
    pool: PgPool,
}

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

    // connect_lazy means the server boots even if Postgres isn't up yet — handy in dev.
    // The first query (e.g. /health) is what actually opens a connection.
    let pool = PgPoolOptions::new()
        .max_connections(5)
        // Keep /health snappy: if Postgres is unreachable, fail fast instead of
        // blocking on the default 30s acquire timeout.
        .acquire_timeout(std::time::Duration::from_secs(2))
        .connect_lazy(&database_url)
        .expect("failed to build Postgres connection pool");

    // Try to run migrations, but don't crash the server if the DB isn't reachable yet.
    match sqlx::migrate!("./migrations").run(&pool).await {
        Ok(()) => tracing::info!("migrations applied"),
        Err(e) => tracing::warn!("could not run migrations (is Postgres up?): {e}"),
    }

    let state = AppState { pool };

    let app = Router::new()
        .route("/health", get(health))
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
/// "alive"; the body tells you whether Postgres is actually reachable.
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
