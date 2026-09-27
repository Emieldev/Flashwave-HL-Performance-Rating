-- PLAN Q16b: aim, deaths and living time belong to a player, not to "the
-- owner". The pass used to read one SteamID out of a demo that carried
-- eighteen, so the match page's player select changed the name above the aim
-- cards and nothing else -- it showed your crosshair error under a
-- teammate's name.
--
-- Three tables gain the player they are about. A tick is no longer unique on
-- its own (two people can die on the same one), so each primary key grows and
-- the tables are rebuilt rather than altered: SQLite cannot change a primary
-- key in place, and all of this is DERIVED from demos that are still on disk.
-- Clearing aim_log makes the next pass read every demo again.

DROP TABLE demo_aim;
DROP TABLE demo_death;
DROP TABLE demo_life;
DELETE FROM aim_log;

CREATE TABLE demo_aim (
    log_id       INTEGER NOT NULL,
    demo_id      INTEGER NOT NULL,
    tick         INTEGER NOT NULL,     -- the demo's own kill tick
    shooter      INTEGER NOT NULL,     -- account id of whoever fired
    at_raw       INTEGER,              -- the matching kill in the log's clock
    victim       INTEGER,              -- account id, where the SteamID parsed
    error_deg    REAL NOT NULL,        -- view to the victim's head, at the shot
    before_deg   REAL NOT NULL,        -- the same, one second earlier
    flick_deg    REAL NOT NULL,        -- how far the view turned in the last half second
    range_units  REAL NOT NULL,
    height       REAL NOT NULL,        -- how far above the shooter the victim stood
    victim_seen  INTEGER NOT NULL,     -- the demo carried the victim throughout
    shooter_seen INTEGER NOT NULL,     -- and the shooter, whose angles these are
    headshot     INTEGER NOT NULL,
    dx_deg        REAL NOT NULL DEFAULT 0,  -- the miss split in two, at the shot
    dy_deg        REAL NOT NULL DEFAULT 0,
    before_dx_deg REAL NOT NULL DEFAULT 0,  -- and a second before
    before_dy_deg REAL NOT NULL DEFAULT 0,
    path          TEXT,                     -- the crosshair's path, as JSON
    PRIMARY KEY (log_id, demo_id, tick, shooter)
) STRICT;

-- Every player's aim in one match, which is how the match page reads it.
CREATE INDEX demo_aim_shooter ON demo_aim (log_id, shooter);

CREATE TABLE demo_death (
    log_id        INTEGER NOT NULL,
    demo_id       INTEGER NOT NULL,
    tick          INTEGER NOT NULL,
    who           INTEGER NOT NULL,    -- account id of whoever died
    at_raw        INTEGER,             -- the matching death in the log's clock
    killer        INTEGER,             -- account id, where the SteamID parsed
    killer_range  REAL,                -- NULL when the demo never carried them
    killer_dx_deg REAL,                -- where the killer was, relative to their view
    killer_dy_deg REAL,
    nearest_mate  REAL,                -- distance to the closest living teammate
    mates_near    INTEGER NOT NULL,    -- teammates within MATE_NEAR_UNITS
    scoped        INTEGER NOT NULL,    -- scoped in when it happened
    seen          INTEGER NOT NULL,    -- the demo carried them across the window
    PRIMARY KEY (log_id, demo_id, tick, who)
) STRICT;

CREATE INDEX demo_death_who ON demo_death (log_id, who);

-- One row per player per match read: how their time was spent.
CREATE TABLE demo_life (
    log_id       INTEGER NOT NULL,
    demo_id      INTEGER NOT NULL,
    account_id   INTEGER NOT NULL,
    alive_ticks  INTEGER NOT NULL,
    scoped_ticks INTEGER NOT NULL,
    PRIMARY KEY (log_id, demo_id, account_id)
) STRICT;
