//! `POST /conventions/{id}/import` — bulk-load a convention's attraction list from a
//! CSV (the organizer's spreadsheet, exported).
//!
//! Two phases. **Validate** parses and checks every row with no database access,
//! collecting *all* problems so the operator can fix the whole sheet in one pass; any
//! error means nothing is imported. **Write** runs only on clean rows, in a single
//! transaction that's all-or-nothing as a backstop.
//!
//! Expected columns: `title`, `kind`, `duration_hours`, `hosts`, `description`. `kind`
//! blank defaults to `panel`; `hosts` is a comma-separated nick list inside one cell;
//! durations are hours in the sheet, converted to the minutes the DB stores.

use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::attractions::AttractionKind;
use crate::error::AppError;
use crate::state::AppState;

/// One raw CSV row. Typed fields let serde+csv reject malformed cells (an unknown kind,
/// a non-numeric duration) with the offending line number attached.
#[derive(Deserialize)]
struct ImportRow {
    title: String,
    // Blank cell -> None -> defaults to panel; `default` also tolerates the column being absent.
    #[serde(default)]
    kind: Option<AttractionKind>,
    // Sheets record durations in hours (often fractional, e.g. 1.5); the DB stores minutes.
    duration_hours: f64,
    // Comma-separated nicks in a single cell, the format the source sheets use.
    #[serde(default)]
    hosts: String,
    #[serde(default)]
    description: Option<String>,
}

/// A row that passed validation, converted to DB-ready values. The write phase only ever
/// sees these — by here, every user-fixable problem has already been caught.
struct ValidRow {
    title: String,
    kind: AttractionKind,
    duration_minutes: i32,
    hosts: Vec<String>,
    description: Option<String>,
}

/// What the import did, returned for the operator's confirmation.
#[derive(Serialize)]
struct ImportSummary {
    attractions_created: usize,
    panelists_created: usize,
    links_created: usize,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/conventions/{convention_id}/import", post(import))
}

/// `POST /conventions/{convention_id}/import` — raw CSV body in, summary out.
async fn import(
    State(state): State<AppState>,
    Path(convention_id): Path<Uuid>,
    body: String,
) -> Result<Json<ImportSummary>, AppError> {
    // 404 up front: importing into a missing convention is a not-found, not a buried FK error.
    let exists = sqlx::query_scalar!(
        "SELECT EXISTS(SELECT 1 FROM conventions WHERE id = $1)",
        convention_id,
    )
    .fetch_one(&state.pool)
    .await?;
    if exists != Some(true) {
        return Err(AppError::NotFound);
    }

    // Phase 1: validate everything before touching the DB.
    let rows = parse_and_validate(&body)?;

    // Phase 2: write. Nothing here is visible until the final commit; an early return
    // drops `tx` and Postgres rolls the whole import back.
    let mut tx = state.pool.begin().await?;
    let mut panelist_ids: HashMap<String, Uuid> = HashMap::new();
    let mut summary = ImportSummary {
        attractions_created: 0,
        panelists_created: 0,
        links_created: 0,
    };

    for row in rows {
        let attraction_id = sqlx::query_scalar!(
            r#"
            INSERT INTO attractions (convention_id, title, kind, duration_minutes, description)
            VALUES ($1, $2, $3::attraction_kind, $4, $5)
            RETURNING id
            "#,
            convention_id,
            row.title,
            row.kind as AttractionKind,
            row.duration_minutes,
            row.description,
        )
        .fetch_one(&mut *tx)
        .await?;
        summary.attractions_created += 1;

        for nick in row.hosts {
            let panelist_id = match panelist_ids.get(&nick) {
                Some(id) => *id,
                None => {
                    // Find-or-create: look up first, insert only when missing. Clearer than an
                    // upsert, and it avoids bumping an existing panelist's updated_at.
                    let id = match sqlx::query_scalar!(
                        "SELECT id FROM panelists WHERE convention_id = $1 AND nick = $2",
                        convention_id,
                        nick,
                    )
                    .fetch_optional(&mut *tx)
                    .await?
                    {
                        Some(id) => id,
                        None => {
                            let id = sqlx::query_scalar!(
                                "INSERT INTO panelists (convention_id, nick) VALUES ($1, $2) RETURNING id",
                                convention_id,
                                nick,
                            )
                            .fetch_one(&mut *tx)
                            .await?;
                            summary.panelists_created += 1;
                            id
                        }
                    };
                    panelist_ids.insert(nick, id);
                    id
                }
            };

            // ON CONFLICT DO NOTHING: a nick repeated in one cell links once;
            // rows_affected() reports whether this insert was actually a new link.
            let linked = sqlx::query!(
                "INSERT INTO attraction_panelists (attraction_id, panelist_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
                attraction_id,
                panelist_id,
            )
            .execute(&mut *tx)
            .await?;
            summary.links_created += linked.rows_affected() as usize;
        }
    }

    tx.commit().await?;
    Ok(Json(summary))
}

/// Phase 1: parse the CSV and validate every row with no database access. Returns the
/// DB-ready rows, or *all* the row-level errors at once (so one re-import fixes the sheet).
fn parse_and_validate(body: &str) -> Result<Vec<ValidRow>, AppError> {
    // Spreadsheet exports often prepend a UTF-8 BOM; left in, it glues onto the first
    // header so `title` stops matching. Strip it before parsing.
    let csv_text = body.strip_prefix('\u{feff}').unwrap_or(body);

    // `Trim::All` strips surrounding whitespace from every cell, so " panel " and
    // "Alice, Bob " parse cleanly.
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(csv_text.as_bytes());
    let headers = reader
        .headers()
        .map_err(|e| AppError::Validation(format!("could not read CSV header: {e}")))?
        .clone();

    let mut valid = Vec::new();
    let mut errors = Vec::new();

    for record in reader.records() {
        let record = match record {
            Ok(r) => r,
            Err(e) => {
                errors.push(format!("could not parse CSV: {e}"));
                continue;
            }
        };
        // Line in the original file (header is line 1), so the error points at the real row.
        let line = record.position().map_or(0, |p| p.line());
        let row: ImportRow = match record.deserialize(Some(&headers)) {
            Ok(r) => r,
            Err(e) => {
                errors.push(format!("row at line {line}: {}", describe_csv_error(&e, &headers)));
                continue;
            }
        };

        // Collect every problem in this row, not just the first, so one fix-pass clears it.
        let before = errors.len();
        if row.title.is_empty() {
            errors.push(format!("row at line {line}: title is required"));
        }
        // is_finite rejects NaN and infinities; the round guards a tiny value vanishing to 0.
        let duration_minutes = if !row.duration_hours.is_finite() || row.duration_hours <= 0.0 {
            errors.push(format!("row at line {line}: duration_hours must be positive"));
            0
        } else {
            let minutes = (row.duration_hours * 60.0).round() as i32;
            if minutes == 0 {
                errors.push(format!("row at line {line}: duration rounds to 0 minutes"));
            }
            minutes
        };

        if errors.len() == before {
            valid.push(ValidRow {
                title: row.title,
                kind: row.kind.unwrap_or(AttractionKind::Panel),
                duration_minutes,
                hosts: row
                    .hosts
                    .split(',')
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                    .map(String::from)
                    .collect(),
                description: row.description,
            });
        }
    }

    if errors.is_empty() {
        Ok(valid)
    } else {
        Err(AppError::ValidationList(errors))
    }
}

/// Turn a csv deserialize failure into a clean, frontend-friendly message. csv's own
/// `Display` appends a `(line: N, byte: M)` position that duplicates the line number we
/// already prepend; this keeps just the underlying problem and names the offending column.
fn describe_csv_error(err: &csv::Error, headers: &csv::StringRecord) -> String {
    let csv::ErrorKind::Deserialize { err, .. } = err.kind() else {
        return err.to_string();
    };
    let problem = err.kind().to_string();
    match err.field().and_then(|i| headers.get(i as usize)) {
        Some(column) => format!("column '{column}': {problem}"),
        None => problem,
    }
}
