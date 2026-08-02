-- Structured availability windows for a panelist: the machine-readable source of
-- truth for scheduling, distinct from the free-text availability_note (a memo).
-- Only restrictions are stored -- no windows means "available whenever the
-- program runs", which the schedule generator reads accordingly.

CREATE TABLE panelist_availability (
    id          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    panelist_id uuid        NOT NULL REFERENCES panelists (id) ON DELETE CASCADE,
    starts_at   timestamptz NOT NULL,
    ends_at     timestamptz NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),

    CONSTRAINT panelist_availability_time_ordered CHECK (ends_at > starts_at)
);

CREATE INDEX panelist_availability_panelist_id_idx ON panelist_availability (panelist_id);
