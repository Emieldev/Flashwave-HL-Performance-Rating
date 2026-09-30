-- Ratings for everyone (Q36, Flashy; PLAN §26).
--
-- The league sample's games are rated in every full rating pass (Q34), and
-- until now those ratings were only used to set the scale. Kept here --
-- never in `rating`, which is the owner's matches -- they give every
-- player a rating, stat bars and a rank, not only the players the owner met.
--
-- `groups` is each component group's weighted percentile, 0-100, as JSON
-- ({"fragging": 71.2, "survival": 55.0, ...}): the stat bars of a profile.
--
-- DERIVED: rewritten by every full rating pass, per model version.

CREATE TABLE league_rating (
    model_version TEXT    NOT NULL,
    log_id        INTEGER NOT NULL,
    account_id    INTEGER NOT NULL,
    class         TEXT    NOT NULL,
    score         REAL    NOT NULL,
    minutes       REAL    NOT NULL,
    groups        TEXT    NOT NULL,
    PRIMARY KEY (model_version, log_id, account_id)
) STRICT;

CREATE INDEX league_rating_account ON league_rating (model_version, account_id);
