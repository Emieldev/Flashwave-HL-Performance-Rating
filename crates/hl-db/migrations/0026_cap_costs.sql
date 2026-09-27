-- PLAN Q17 and Q25 (ivg): what each capture cost.
--
-- logs.tf's `cpc` counts captures flat, so walking onto a point nobody is
-- alive to defend pays the same as taking one into a full defence. Both
-- columns are read from the raw log at the moment before the point went in,
-- per capper:
--
--   caps_contested   enemies alive to stop it, summed over the player's caps
--   caps_mates_dead  their own team's dead, summed the same way
--
-- DERIVED by the fights pass, which moves to version 5 and recomputes every
-- stored log; the defaults only exist so the ALTER can run.
ALTER TABLE fight_stat ADD COLUMN caps_contested INTEGER NOT NULL DEFAULT 0;
ALTER TABLE fight_stat ADD COLUMN caps_mates_dead INTEGER NOT NULL DEFAULT 0;
