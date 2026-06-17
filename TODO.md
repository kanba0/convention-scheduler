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

## Frontend (out of scope for this repo)

- [ ] A GUI for editing the schedule (drag time-blocks, snap to 30-min grid,
  colour conflicts). This would be a **separate frontend app** (TS/JS) talking
  to this API — not Rust. The block-editing UX is why fixed time-blocks may beat
  free ranges at the presentation layer, even though the DB stores flexible
  `starts_at`/`ends_at`.