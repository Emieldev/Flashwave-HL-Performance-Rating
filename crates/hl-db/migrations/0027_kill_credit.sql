-- PLAN Q6b: who shares in each kill.
--
-- The fight swing credited a kill's whole value to whoever landed the last
-- shot. HLTV shares it with everyone who damaged the victim in the five
-- seconds before; this table holds those shares, one row per contributor,
-- summing to one per kill. `seq` is the kill's index in the raw log, the
-- same key kill_situation uses.
--
-- DERIVED by the fights pass (version 6). Independent of any weight, like
-- kill_situation, so the swing table can be retuned without re-reading logs.
CREATE TABLE kill_credit (
    log_id     INTEGER NOT NULL,
    seq        INTEGER NOT NULL,
    account_id INTEGER NOT NULL,
    share      REAL NOT NULL,
    PRIMARY KEY (log_id, seq, account_id)
) STRICT;
