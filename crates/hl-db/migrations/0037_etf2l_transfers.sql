-- ETF2L transfers (Q48, Flashy; PLAN §27).
--
-- Every join and leave ETF2L records, from a team's transfer list
-- (/team/{id}/transfers) or a player's (/player/{id}/transfers): who, which
-- team, joined or left, when, and who made the change (the player, or a
-- leader adding or removing them). They date a roster: a medal goes to the
-- players still on the team at its last match of the season, not to one who
-- left in week two; a team page shows its roster's history and a profile
-- its teams with dates.
--
-- `account_id` is the Steam account when ETF2L gives one; `kind` is as ETF2L
-- has it ('joined', 'left'). Public data, shipped in the league snapshot.

CREATE TABLE etf2l_transfer (
    team_id     INTEGER NOT NULL,
    player_id   INTEGER NOT NULL,
    time        INTEGER NOT NULL,
    kind        TEXT    NOT NULL,
    account_id  INTEGER,
    player_name TEXT    NOT NULL,
    team_name   TEXT    NOT NULL,
    team_type   TEXT,
    by_id       INTEGER,
    by_name     TEXT,
    PRIMARY KEY (team_id, player_id, time, kind)
) STRICT;

CREATE INDEX etf2l_transfer_account ON etf2l_transfer (account_id);

-- When each list was last read, `kind` 'team' or 'player' and ETF2L's id
-- (a Steam account for a player). CACHE: safe to delete; lists are read again.
CREATE TABLE etf2l_transfer_fetch (
    kind       TEXT    NOT NULL,
    id         INTEGER NOT NULL,
    fetched_at INTEGER NOT NULL,
    PRIMARY KEY (kind, id)
) STRICT;
