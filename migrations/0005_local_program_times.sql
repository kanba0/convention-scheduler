-- Program times are venue wall clock, not instants: "Saturday 14:00" is the whole
-- fact, with no zone to resolve it against. Record stamps (created_at/updated_at)
-- stay timestamptz -- those really are points on a global timeline.
--
-- The USING clause reads each stored instant back as its UTC wall clock, which is
-- how every existing row was written.

ALTER TABLE slots
    ALTER COLUMN starts_at TYPE timestamp USING starts_at AT TIME ZONE 'UTC',
    ALTER COLUMN ends_at   TYPE timestamp USING ends_at   AT TIME ZONE 'UTC';

ALTER TABLE panelist_availability
    ALTER COLUMN starts_at TYPE timestamp USING starts_at AT TIME ZONE 'UTC',
    ALTER COLUMN ends_at   TYPE timestamp USING ends_at   AT TIME ZONE 'UTC';