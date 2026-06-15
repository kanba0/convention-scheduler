-- Infrastructure-level extensions we'll lean on later.
-- btree_gist lets us mix scalar columns (room_id) with a range column inside an
-- EXCLUDE constraint in phase 2 (room/panelist overlap prevention at the DB level).
CREATE EXTENSION IF NOT EXISTS btree_gist;
