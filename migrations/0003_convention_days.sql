-- Per-day program hours for a convention.
--
-- A convention's start/end dates say *which days* the program runs; this table
-- says *when* each of those days is open. One row per program date, seeded from
-- the date span when a convention is created (and additively as the span grows).
-- Hours start NULL ("not set yet"); the operator fills them in, and the schedule
-- generator skips any day whose hours are still unset.

CREATE TABLE convention_days (
    convention_id uuid NOT NULL REFERENCES conventions (id) ON DELETE CASCADE,
    day           date NOT NULL,
    opens_at      time,
    closes_at     time,

    PRIMARY KEY (convention_id, day),

    -- A day can't close before it opens. NULLs pass the CHECK, so an
    -- hours-not-set-yet day (both NULL) is legal.
    CONSTRAINT convention_days_hours_ordered CHECK (closes_at > opens_at)
);