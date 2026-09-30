-- Player profiles (Q35, Flashy): who everyone in the league is.
--
-- league_log_player: each league-sample log's players, their team and the
-- class they played most, so a player's main class can be read from the
-- league and not only from the owner's matches. DERIVED from
-- league_log_json; rebuilt from it whenever it is missing a log.
--
-- etf2l_player: ETF2L's page for a player -- country, the classes they
-- signed up as, their avatar -- fetched when a profile is first opened and
-- kept a week. SOURCE data from the ETF2L API, verbatim fields only.

CREATE TABLE league_log_player (
    log_id     INTEGER NOT NULL,
    account_id INTEGER NOT NULL,
    team       TEXT,
    class      TEXT,
    seconds    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (log_id, account_id)
) STRICT;

CREATE INDEX league_log_player_account ON league_log_player (account_id);

CREATE TABLE etf2l_player (
    account_id INTEGER PRIMARY KEY,
    etf2l_id   INTEGER,
    name       TEXT,
    country    TEXT,
    -- JSON array of class names, as ETF2L lists them.
    classes    TEXT,
    avatar     TEXT,
    registered INTEGER,
    fetched_at INTEGER NOT NULL
) STRICT;
