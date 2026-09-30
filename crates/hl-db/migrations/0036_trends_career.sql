-- Career numbers from trends.tf (Q37, Flashy; PLAN §26).
--
-- trends.tf's player page holds what the app cannot work out without every
-- log a player ever played: their Highlander W-L, winrate per class, damage
-- per minute, hours, aliases and teams. It has no JSON API, so the page is
-- read when a profile is opened and kept here for a day.
--
-- `career` is the parsed page as JSON (hl_ingest::trends_career::Career).
-- CACHE: safe to delete; the next profile opened reads the page again.

CREATE TABLE trends_career (
    account_id INTEGER PRIMARY KEY,
    career     TEXT    NOT NULL,
    fetched_at INTEGER NOT NULL
) STRICT;
