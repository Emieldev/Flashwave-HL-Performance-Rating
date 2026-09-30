-- A sample of ETF2L Highlander officials from every division (Flashy,
-- September 2026): up to 300 matches a division, the last three years first,
-- every map in the pool, so ratings can be read against the league rather
-- than only against the owner's own matches, and a player can be placed in
-- the division they play in.
--
-- Kept apart from log_index / log_raw / rawlog on purpose: those are the
-- owner's matches, and every pass over them -- the match list, the rating
-- pool, the fights -- would otherwise pick these up before anyone has
-- decided how they should count.
--
-- The division comes from etf2l_season_match (joined on etf2l_match_id).
-- SOURCE data from trends.tf (which log belongs to which ETF2L match),
-- logs.tf and more.tf; `picked` is the one decision made here.

CREATE TABLE league_log (
    log_id         INTEGER PRIMARY KEY,
    etf2l_match_id INTEGER NOT NULL,
    map            TEXT    NOT NULL,
    played_at      INTEGER NOT NULL,
    duration_s     INTEGER,
    title          TEXT,
    -- In the sample: this log's match was chosen for its division.
    picked         INTEGER NOT NULL DEFAULT 0,
    -- Where its logs.tf JSON came from, once it has one: 'logs.tf' or
    -- 'more.tf' (a stand-in, asked of logs.tf again later).
    json_source    TEXT,
    json_attempts  INTEGER NOT NULL DEFAULT 0,
    -- The raw server log: NULL not tried yet, 'ok', or 'missing'.
    raw_state      TEXT
) STRICT;

CREATE INDEX league_log_match ON league_log (etf2l_match_id);
CREATE INDEX league_log_picked ON league_log (picked, json_source);

CREATE TABLE league_log_json (
    log_id INTEGER PRIMARY KEY,
    json   TEXT NOT NULL
) STRICT;

CREATE TABLE league_rawlog (
    log_id INTEGER PRIMARY KEY,
    zip    BLOB NOT NULL
) STRICT;
