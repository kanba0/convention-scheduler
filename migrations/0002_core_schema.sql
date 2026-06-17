-- Core domain schema for the convention scheduler.
--
-- Entity hierarchy (everything is scoped to one convention):
--   convention
--     ├── rooms        (typed: what kind of attraction a room can host)
--     ├── panelists    (hosts)
--     └── attractions  (a panel or a contest)
--           ├── attraction_panelists  (who hosts it — many-to-many)
--           └── slots                 (placement: this attraction, this room, this time)

-- ---------------------------------------------------------------------------
-- Enumerated types
-- ---------------------------------------------------------------------------
-- Native Postgres ENUMs: the database itself rejects an invalid value (a typo
-- like 'panle' fails at write time). sqlx maps these to a Rust enum later.

-- What an attraction *is*.
CREATE TYPE attraction_kind AS ENUM ('panel', 'contest');

-- What a room can *host*. 'panel_contest' = either.
CREATE TYPE room_kind AS ENUM ('panel', 'contest', 'panel_contest');

-- ---------------------------------------------------------------------------
-- updated_at trigger
-- ---------------------------------------------------------------------------
-- Postgres does not bump updated_at on its own. This reusable trigger function
-- stamps now() into the row on every UPDATE; each editable table attaches it
-- below so we can never forget to maintain it by hand.
CREATE FUNCTION set_updated_at() RETURNS trigger AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- ---------------------------------------------------------------------------
-- Conventions — the top-level event
-- ---------------------------------------------------------------------------
CREATE TABLE conventions (
    id         uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    name       text        NOT NULL,
    starts_on  date        NOT NULL,
    ends_on    date        NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),

    -- A convention can't end before it starts.
    CONSTRAINT conventions_dates_ordered CHECK (ends_on >= starts_on)
);

CREATE TRIGGER conventions_set_updated_at
    BEFORE UPDATE ON conventions
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ---------------------------------------------------------------------------
-- Rooms — typed by what they can host, unique by name within a convention
-- ---------------------------------------------------------------------------
CREATE TABLE rooms (
    id            uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    convention_id uuid        NOT NULL REFERENCES conventions (id) ON DELETE CASCADE,
    name          text        NOT NULL,
    kind          room_kind   NOT NULL,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),

    -- Two rooms in the same convention can't share a name (but different
    -- conventions can each have a "Main").
    CONSTRAINT rooms_name_unique_per_convention UNIQUE (convention_id, name)
);

CREATE TRIGGER rooms_set_updated_at
    BEFORE UPDATE ON rooms
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ---------------------------------------------------------------------------
-- Panelists — hosts
-- ---------------------------------------------------------------------------
-- availability_note is an optional human memo only. The machine-usable
-- availability (precise time windows, converted from fuzzy notes at import) is
-- a separate table planned for the import slice; see TODO.md.
CREATE TABLE panelists (
    id                uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    convention_id     uuid        NOT NULL REFERENCES conventions (id) ON DELETE CASCADE,
    nick              text        NOT NULL,
    availability_note text,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),

    CONSTRAINT panelists_nick_unique_per_convention UNIQUE (convention_id, nick)
);

CREATE TRIGGER panelists_set_updated_at
    BEFORE UPDATE ON panelists
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ---------------------------------------------------------------------------
-- Attractions — the core unit (a panel or a contest)
-- ---------------------------------------------------------------------------
CREATE TABLE attractions (
    id               uuid            PRIMARY KEY DEFAULT gen_random_uuid(),
    convention_id    uuid            NOT NULL REFERENCES conventions (id) ON DELETE CASCADE,
    title            text            NOT NULL,
    kind             attraction_kind NOT NULL,
    -- NULL = duration not estimated yet; any actual value must be positive.
    duration_minutes integer,
    description      text,
    created_at       timestamptz     NOT NULL DEFAULT now(),
    updated_at       timestamptz     NOT NULL DEFAULT now(),

    CONSTRAINT attractions_duration_positive
        CHECK (duration_minutes IS NULL OR duration_minutes > 0)
);

CREATE TRIGGER attractions_set_updated_at
    BEFORE UPDATE ON attractions
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ---------------------------------------------------------------------------
-- attraction_panelists — many-to-many: who hosts what
-- ---------------------------------------------------------------------------
-- One attraction can have several hosts (comma-separated in the source sheets); one panelist can host
-- several attractions. The composite primary key prevents duplicate links.
-- (Insert/delete only — no updates — so no updated_at here.)
CREATE TABLE attraction_panelists (
    attraction_id uuid NOT NULL REFERENCES attractions (id) ON DELETE CASCADE,
    panelist_id   uuid NOT NULL REFERENCES panelists (id)   ON DELETE CASCADE,

    PRIMARY KEY (attraction_id, panelist_id)
);

-- ---------------------------------------------------------------------------
-- Slots — a placement of an attraction into a room for a time range
-- ---------------------------------------------------------------------------
CREATE TABLE slots (
    id            uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    attraction_id uuid        NOT NULL REFERENCES attractions (id) ON DELETE CASCADE,
    room_id       uuid        NOT NULL REFERENCES rooms (id)       ON DELETE CASCADE,
    starts_at     timestamptz NOT NULL,
    ends_at       timestamptz NOT NULL,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),

    CONSTRAINT slots_time_ordered CHECK (ends_at > starts_at),
    -- An attraction is placed at most once (no double-scheduling). Intentional
    -- repeats would be a separate event concept later; see TODO.md.
    CONSTRAINT slots_attraction_unique UNIQUE (attraction_id)
);

CREATE TRIGGER slots_set_updated_at
    BEFORE UPDATE ON slots
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ---------------------------------------------------------------------------
-- Indexes on the foreign keys we'll filter/join by most often.
-- (Primary keys and UNIQUE constraints are already indexed automatically.)
-- ---------------------------------------------------------------------------
CREATE INDEX rooms_convention_id_idx       ON rooms (convention_id);
CREATE INDEX panelists_convention_id_idx   ON panelists (convention_id);
CREATE INDEX attractions_convention_id_idx ON attractions (convention_id);
CREATE INDEX slots_room_id_idx             ON slots (room_id);
-- Note: slots.attraction_id is already indexed by the UNIQUE constraint above.