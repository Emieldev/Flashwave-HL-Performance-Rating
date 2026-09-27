-- PLAN Q29 (Flashy): every Highlander team of the last year, not only the
-- owner's. Kept apart from etf2l_match / etf2l_roster on purpose: those are
-- the owner's officials, and the context pass links the owner's logs to
-- them by time -- a table full of other teams' matches would give it the
-- wrong ones to pick from.
--
-- SOURCE data from the ETF2L API; nothing here is derived.

-- One ETF2L competition: a division of a season, or its playoffs.
CREATE TABLE etf2l_competition (
    competition_id INTEGER PRIMARY KEY,
    season         INTEGER NOT NULL,        -- 36 for "Highlander Season 36 (...)"
    season_name    TEXT NOT NULL,           -- "Autumn 2026"
    division       TEXT NOT NULL,           -- "High", "Open", "Low", ...
    stage          TEXT NOT NULL,           -- "regular", "Playoffs", "3rd Place", ...
    name           TEXT NOT NULL,
    archived       INTEGER NOT NULL DEFAULT 0,
    pool           TEXT,                    -- JSON array of maps
    fetched_at     INTEGER NOT NULL
) STRICT;

CREATE INDEX etf2l_competition_season ON etf2l_competition (season);

CREATE TABLE etf2l_team (
    team_id  INTEGER PRIMARY KEY,
    name     TEXT NOT NULL,
    country  TEXT,
    avatar   TEXT
) STRICT;

-- One official of any team, from a competition's results.
CREATE TABLE etf2l_season_match (
    match_id       INTEGER PRIMARY KEY,
    competition_id INTEGER NOT NULL,
    -- A season's main competition holds several divisions; each match
    -- says which it was in, and the tier ranks them (1 is the top).
    division       TEXT,
    tier           INTEGER,
    week           INTEGER,
    round          TEXT,
    time           INTEGER,
    clan1_id       INTEGER NOT NULL,
    clan2_id       INTEGER NOT NULL,
    r1             INTEGER,
    r2             INTEGER,
    default_win    INTEGER NOT NULL DEFAULT 0,
    maps           TEXT,                    -- JSON array
    detail_fetched INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE INDEX etf2l_season_match_comp ON etf2l_season_match (competition_id);
CREATE INDEX etf2l_season_match_c1 ON etf2l_season_match (clan1_id);
CREATE INDEX etf2l_season_match_c2 ON etf2l_season_match (clan2_id);

-- Each map of a match, from the match's own page: rounds each side took.
CREATE TABLE etf2l_season_map (
    match_id    INTEGER NOT NULL,
    match_order INTEGER NOT NULL,
    map         TEXT NOT NULL,
    clan1       INTEGER NOT NULL,
    clan2       INTEGER NOT NULL,
    golden_cap  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (match_id, match_order)
) STRICT;

-- Who played each match, and for which side.
CREATE TABLE etf2l_season_player (
    match_id   INTEGER NOT NULL,
    account_id INTEGER NOT NULL,
    team_id    INTEGER,
    name       TEXT,
    PRIMARY KEY (match_id, account_id)
) STRICT;

CREATE INDEX etf2l_season_player_team ON etf2l_season_player (team_id);
