-- Logs stored from more.tf while logs.tf refused us (Flashy, September 2026).
--
-- logs.tf turns an address away for a while after too many requests, and a
-- game just played then never shows up. more.tf keeps its own parsed copy of
-- every log, so the log is built from that instead: its JSON goes into
-- log_raw and a server log written from its kills into rawlog, like any
-- other log. A row here says both are stand-ins: the sync asks logs.tf for
-- the real ones again, and the row goes once the real JSON is in.
--
-- SOURCE bookkeeping; nothing here is derived.

CREATE TABLE log_stand_in (
    log_id    INTEGER PRIMARY KEY,
    -- Where the stand-in came from: 'more.tf'.
    source    TEXT    NOT NULL,
    stored_at INTEGER NOT NULL DEFAULT (unixepoch())
);
