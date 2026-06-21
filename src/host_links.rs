//! Host assignments — the `attraction_panelists` many-to-many. A panelist hosts
//! an attraction; managed as a sub-resource under `/attractions/{id}/panelists`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use uuid::Uuid;

use crate::error::AppError;
use crate::panelists::Panelist;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/attractions/{attraction_id}/panelists", get(list_hosts))
        .route(
            "/attractions/{attraction_id}/panelists/{panelist_id}",
            put(link).delete(unlink),
        )
}

/// `GET /attractions/{attraction_id}/panelists` — the panelists hosting it, by nick.
async fn list_hosts(
    State(state): State<AppState>,
    Path(attraction_id): Path<Uuid>,
) -> Result<Json<Vec<Panelist>>, AppError> {
    let hosts = sqlx::query_as!(
        Panelist,
        r#"
        SELECT p.id, p.convention_id, p.nick, p.availability_note, p.created_at, p.updated_at
        FROM panelists p
        JOIN attraction_panelists ap ON ap.panelist_id = p.id
        WHERE ap.attraction_id = $1
        ORDER BY p.nick
        "#,
        attraction_id,
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(hosts))
}

/// `PUT /attractions/{attraction_id}/panelists/{panelist_id}` — link a host.
/// Idempotent (re-linking is a no-op 204); 422 if the two aren't in one convention.
async fn link(
    State(state): State<AppState>,
    Path((attraction_id, panelist_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    // The SELECT only yields a row when both exist and share a convention, so a
    // cross-convention (or missing) pair inserts nothing. ON CONFLICT makes a
    // repeat link a no-op.
    let inserted = sqlx::query!(
        r#"
        INSERT INTO attraction_panelists (attraction_id, panelist_id)
        SELECT a.id, p.id
        FROM attractions a, panelists p
        WHERE a.id = $1 AND p.id = $2 AND a.convention_id = p.convention_id
        ON CONFLICT DO NOTHING
        "#,
        attraction_id,
        panelist_id,
    )
    .execute(&state.pool)
    .await?
    .rows_affected();

    if inserted == 1 {
        return Ok(StatusCode::NO_CONTENT);
    }

    // Zero rows is ambiguous: already linked (fine), or an invalid pair (reject).
    let already_linked = sqlx::query_scalar!(
        r#"SELECT EXISTS(
            SELECT 1 FROM attraction_panelists WHERE attraction_id = $1 AND panelist_id = $2
        )"#,
        attraction_id,
        panelist_id,
    )
    .fetch_one(&state.pool)
    .await?
    .unwrap_or(false);

    if already_linked {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::Validation(
            "attraction and panelist must belong to the same convention".to_string(),
        ))
    }
}

/// `DELETE /attractions/{attraction_id}/panelists/{panelist_id}` — unlink a host.
async fn unlink(
    State(state): State<AppState>,
    Path((attraction_id, panelist_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    let deleted = sqlx::query!(
        "DELETE FROM attraction_panelists WHERE attraction_id = $1 AND panelist_id = $2",
        attraction_id,
        panelist_id,
    )
    .execute(&state.pool)
    .await?
    .rows_affected();

    if deleted == 0 {
        Err(AppError::NotFound)
    } else {
        Ok(StatusCode::NO_CONTENT)
    }
}
