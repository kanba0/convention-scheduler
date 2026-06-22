# convention_scheduler

A REST API for scheduling hobby conventions — assigning panels and contests to
typed rooms across a multi-day program, and automatically flagging the scheduling
conflicts that organizers currently catch by hand.

The domain comes from real convention-running artifacts (PII-scrubbed spreadsheets):
typed rooms, panels vs. contests, multiple hosts per attraction, fuzzy free-text
panelist availability, and the colored grid cells that encode "this slot fits / this
slot clashes" — which is exactly the conflict detection this API automates.

This repo is full-stack — a Rust backend plus an in-repo frontend (not Rust) added
later. The core product is the operator's workflow: **auto-generate** a schedule from
the constraints, then **hand-adjust** it by dragging attractions around a room×time
grid with live conflict highlighting.

## Status

Early development. Phases 1–3 are complete — the CRUD spine (conventions, rooms,
panelists, attractions, host links, slots), the assembled `GET /schedule` view,
bulk attraction import, and `GET /conventions/:id/conflicts` conflict detection.
Hardening (Phase 4) is next. See the [Roadmap](#roadmap) below.

## Tech stack

- **Rust** (edition 2024) with **axum** 0.8 (async HTTP)
- **PostgreSQL** 17 via **sqlx** 0.8 (compile-time-checked queries, migrations)
- **tokio** async runtime, **serde** for JSON, **tracing** for structured logs
- **Docker Compose** for the local database
- Integration tests + GitHub Actions CI (planned, phase 4)

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

### Development

Queries are checked at compile time against the committed `.sqlx/` cache, so builds
need no database (`.cargo/config.toml` sets `SQLX_OFFLINE`). After changing any query,
refresh the cache:

```sh
cargo sqlx prepare   # needs the dev Postgres up; commit the updated .sqlx/
```

Enable the pre-commit hook once per clone to catch a stale cache before it lands:

```sh
git config core.hooksPath .githooks
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

## Conflict detection (phase 3, the core feature)

Organizers currently color grid cells by hand to mark clashes. The API automates
the three real checks:

1. A room double-booked (two overlapping slots in the same room).
2. A panelist double-booked (hosting two overlapping attractions).
3. An attraction placed in a room whose type doesn't match its kind.

`GET /conventions/:id/conflicts` *reports* these — it does not forbid them. A clash
is a state you can sit in and then resolve, mirroring the manual grid. An earlier
plan to add a database `EXCLUDE` constraint preventing room overlaps was dropped (see
[TODO.md](TODO.md)): the editor already enforces one attraction per cell through its
drag/swap interaction, so the constraint would be redundant and only make edits harder.

## Roadmap

- [x] **Phase 0** — scaffold: axum server, `/health` DB probe, Docker Compose
  Postgres, migrations wired up.
- [x] **Phase 1** — CRUD spine: conventions, rooms, panelists, attractions,
  host links, slots, and an assembled `GET /schedule` view.
- [x] **Phase 2** — CSV / spreadsheet import for the attraction list.
- [x] **Phase 3** — conflict detection (`GET /conventions/:id/conflicts`): room
  double-booking, panelist double-booking, and room-type mismatch, reported (not
  forbidden — the editor's drag/swap handles room occupancy; see TODO.md).
- [ ] **Phase 4** — hardening: integration tests, Dockerfile, GitHub Actions CI
  (`cargo test` + `cargo clippy -D warnings`).
- [ ] **Phase 5** — schedule generator: auto-place attractions into slots from the
  constraints (hard rules: room type, availability, no double-booking; soft prefs
  as bias).
- [ ] **Phase 6** — operator GUI (in-repo, non-Rust frontend): view the schedule
  grid, drag attractions between room×time cells, live conflict colours — the core
  product surface. Intertwined with Phase 5 (a viewer is how you test the generator).

## License

Not yet decided.
