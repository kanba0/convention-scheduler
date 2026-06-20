//! Shared application state, handed by axum to every request handler.

use sqlx::PgPool;

/// Shared state for every handler — just the DB pool for now; config and
/// clients would join it later. Cheap to `Clone`: `PgPool` is `Arc`-backed.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
}
