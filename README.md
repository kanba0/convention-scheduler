# convention_scheduler

A REST API for scheduling hobby conventions — assigning panels and contests to
typed rooms across a multi-day program, and automatically flagging the scheduling
conflicts that organizers currently catch by hand.

The domain comes from real convention-running artifacts (PII-scrubbed spreadsheets):
typed rooms, panels vs. contests, multiple hosts per attraction, fuzzy free-text
panelist availability, and the colored grid cells that encode "this slot fits / this
slot clashes" — which is exactly the conflict detection this API automates.

## Status

Early development. Phase 0 (scaffold) is complete; the CRUD spine is next. See the
[Roadmap](#roadmap) below.

## Tech stack

- **Rust** (edition 2024) with **axum** 0.8 (async HTTP)
- **PostgreSQL** 17 via **sqlx** 0.8 (compile-time-checked queries, migrations)
- **tokio** async runtime, **serde** for JSON, **tracing** for structured logs
- **Docker Compose** for the local database
- Integration tests + GitHub Actions CI (planned, phase 3)

## Getting started

You need Rust (1.96+) and Docker.

```sh
# 1. Start Postgres
docker compose up -d

# 2. Configure the app (copy the template, defaults match docker-compose.yml)
cp .env.example .env

# 3. Run the server (migrations apply automatically on boot)
cargo run

# 4. Check it's alive
curl localhost:8080/health
# -> {"db":"up","status":"ok"}
```

## Domain model

- **Convention** — the event; has a multi-day program (start/end dates).
- **Room** — *typed* by what it can host: `panel`, `contest`, or `panel_contest`
  (either). A room's type must match the attractions placed in it.
- **Attraction** — the core unit, either a `panel` or a `contest`. Has a title,
  duration, description, and one or more hosting panelists. Placed at most once.
- **Panelist** — a host; has a nick and availability. Availability is being
  modelled as precise time windows (converted from organizers' fuzzy notes at
  import time), with an optional free-text note kept only as a human memo; see
  [TODO.md](TODO.md).
- **Slot** — a placement of an attraction into a room for a time range.

## Conflict detection (phase 2, the core feature)

Organizers currently color grid cells by hand to mark clashes. The API will
automate the three real checks:

1. A room double-booked (two overlapping slots in the same room).
2. A panelist double-booked (hosting two overlapping attractions).
3. An attraction placed in a room whose type doesn't match its kind.

## Roadmap

- [x] **Phase 0** — scaffold: axum server, `/health` DB probe, Docker Compose
  Postgres, migrations wired up.
- [ ] **Phase 1** — CRUD spine: conventions, rooms, panelists, attractions,
  slot assignment, and an assembled `GET /schedule` view.
- [ ] **Phase 1.5** — CSV / spreadsheet import for the attraction list.
- [ ] **Phase 2** — conflict detection (`GET /conventions/:id/conflicts`),
  with a database-level `EXCLUDE` constraint preventing room overlaps.
- [ ] **Phase 3** — integration tests, Dockerfile, GitHub Actions CI
  (`cargo test` + `cargo clippy -D warnings`).
- [ ] **Later** — a thin web UI so non-technical organizers can use it without
  touching the API directly.

## License

Not yet decided.
