-- Two more ways to know a round's map (Q30, Flashy, September 2026).
--
-- demos_tf_map: the map demos.tf lists for the demo matched to a log. A log
-- whose map logs.tf never gave can be matched to its demo by time alone
-- (the demo was recording when the log began), and the listing already
-- names the map: enough to draw the kill map before the STV is downloaded.
-- SOURCE bookkeeping, filled by the demos.tf search.
--
-- round_map_manual: "this was on ___", set by the player on a match page
-- when nothing else could tell. SOURCE: the player's word, never derived,
-- and it outranks everything except the log's own map field.

ALTER TABLE log_index ADD COLUMN demos_tf_map TEXT;

CREATE TABLE round_map_manual (
    log_id    INTEGER NOT NULL,
    round_num INTEGER NOT NULL,
    map       TEXT    NOT NULL,
    set_at    INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (log_id, round_num)
) STRICT;
