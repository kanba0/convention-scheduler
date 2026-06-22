# TODO / design backlog

Phase-sequenced work lives in the [README roadmap](README.md#roadmap). This file
is the *design backlog*: bigger ideas we deliberately deferred so the basic
version can prove itself first. Nothing here is committed to a phase yet.

## Scheduling intelligence

- [ ] **Constraint system for placement.** The heart of a real scheduler:
  rules that decide whether an attraction *may* (or *should*) go in a given
  room/time. Two flavours:
  - **Hard constraints** (must hold): room type matches attraction kind;
    room is big enough; panelist is available; no double-booking.
  - **Soft constraints / preferences** (bias, not law): keep popular panels in
    "prime time"; spread a panelist's sessions out; etc. Violating a soft
    constraint should be *allowed but discouraged* (a cost), so the scheduler
    can break it when there's no other fit.
- [ ] **Same-time exclusions independent of hosts.** "These two panels must not
  run at the same time" even when they share no panelists — e.g. similar theme,
  same target audience. A constraint between attractions, not via people.
- [ ] **Prime-time / time-of-day preferences** per attraction (soft).

## Rooms

- [ ] **Room size / capacity** as a first-class attribute, and as a placement
  constraint (a popular panel needs a bigger room).
- [ ] **Room class beyond kind** — for example, "main stage". A stage hosts its own category of things
  (cosplay contest, concert, big-guest meetup) and seats far more people. Needs
  its own modelling, not an enum value.

## Panelists & availability

- [ ] **Structured availability windows.** A `panelist_availability` table of
  precise `(panelist_id, starts_at, ends_at)` windows — the machine-usable
  source of truth for scheduling. The free-text `availability_note` stays only
  as a human memo.
- [ ] **Importer forces fuzzy → precise.** When importing a CSV/sheet, the user
  must convert fuzzy notes ("only Saturday till 18:00") into concrete windows.
  Precise availability makes conflict detection and auto-scheduling far easier.

## Convention structure

- [ ] **Per-day program hours** (e.g. Fri 14–20, Sat 9–20, Sun 9–14) — likely a
  `convention_days` table (date + open/close time).
- [ ] **Category hour budgets** (total hours for attractions / panels /
  contests), as planning aids.

## Plans & versioning

- [ ] **Plan versioning.** Keep history of a schedule so organizers can compare
  versions. `updated_at` (added in migration 0002) is the first breadcrumb;
  real versioning is bigger.
- [ ] **Change-diff highlighting.** Show what moved between two plan versions
  (colour the changed slots), echoing the manual green/orange grid workflow.

## Event types

- [ ] **Intentional repeats.** Currently an attraction is placed at most once
  (`slots_attraction_unique`). If an organizer genuinely wants the same thing
  twice, model it as a distinct "repeatable" event concept rather than relaxing
  the constraint.

## Import

CSV stays the interchange format — universal, and conventions on oddball tools
can still export or convert to it. No multi-format (xlsx, …) parsing server-side;
the CSV boundary is the API's contract. Phase 2 built the basic importer; these
are the operator-facing extensions for when the GUI lands.

- [ ] **Preview-then-confirm (dry-run).** A validate-only pass reports what
  *would* happen ("23 attractions, 4 new panelists: Alice, Bob, …; 2 rows have
  errors") with nothing written, then a confirm commits it. The Phase 2 importer
  already separates validation from the write, so the preview summary is
  computable without touching the DB — design the endpoint for it (a dry-run
  flag, or a preview call paired with a commit call).
- [ ] **Operator selects what to import.** Sheets are not a fixed shape — extra
  notes columns, section headers, differing orders. The UI lets the operator map
  / pick which columns feed which fields, rather than forcing a rigid header row.
  More than a 1:1 column rename.
- [ ] **Error preview grid.** Render the sheet as a table, paint the bad rows /
  cells red with the message on hover. The Phase 2 `{"errors":[...]}` response
  already carries `line N` + `column 'x'` — exactly what this needs.
- [ ] **Re-import / replace semantics.** Attractions have no title uniqueness, so
  a naive re-import duplicates them. Options to design: warn which titles would
  double; a "replace" mode; a "wipe the schedule and re-import" reset. Today's
  importer is append-only (documented limitation).
- [ ] **Separate panelist-availability importer (later).** Hourly availability
  constraints likely need their own import path, distinct from the attraction
  list, feeding the structured windows under *Panelists & availability* above.
  Defer until that need actually lands — don't build it ahead of the table.

## Frontend / GUI

The operator GUI is a committed phase, not out of scope: see
[Phase 6](README.md#roadmap) — in-repo, non-Rust (TS/JS), talking to this API.
The deferred design questions are about its UX, not whether to build it:

- [ ] **Block-editing UX.** Drag time-blocks, snap to a 30-min grid, colour
  conflicts live. This is why fixed time-blocks may beat free ranges at the
  presentation layer, even though the DB stores flexible `starts_at`/`ends_at`.