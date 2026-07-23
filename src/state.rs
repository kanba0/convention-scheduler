//! Shared application state, handed by axum to every request handler.

use sqlx::PgPool;

/// Shared state for every handler. Cheap to `Clone`: `PgPool` is `Arc`-backed.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
}
