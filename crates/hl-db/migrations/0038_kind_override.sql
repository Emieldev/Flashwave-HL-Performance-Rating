-- Q63 (Flashy): a match's kind set by hand -- official, scrim or pug --
-- which the context pass applies over its own every time it runs, so a
-- sync never undoes it. `was_kind` / `was_method` are what the pass itself
-- decided, so "Automatic" can put them back at once.
CREATE TABLE match_kind_override (
    log_id     INTEGER PRIMARY KEY,
    kind       TEXT NOT NULL,           -- official | scrim | pug
    was_kind   TEXT NOT NULL,
    was_method TEXT,
    set_at     INTEGER NOT NULL
) STRICT;
