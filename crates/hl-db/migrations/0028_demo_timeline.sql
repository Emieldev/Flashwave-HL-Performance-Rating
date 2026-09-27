-- PLAN Q3: a demo, kept.
--
-- One row per demo: everything it held, recorded once, so later passes are
-- derived from here rather than from the file -- which may since have been
-- deleted to save space (Q23). hl-demos/src/timeline.rs has the format: the
-- four blobs are deflated, `head` is JSON (people, user ids, tick seams).
--
-- DERIVED, but not rebuildable once the file is gone, which is the point of
-- keeping it. A demo with a timeline is never dropped from `demo` when its
-- file disappears; it is marked deleted instead, so its links survive.
CREATE TABLE demo_timeline (
    demo_id      INTEGER PRIMARY KEY,
    version      INTEGER NOT NULL,
    tick_rate    REAL NOT NULL,
    stride       INTEGER NOT NULL,
    head         TEXT NOT NULL,
    samples      BLOB NOT NULL,
    changes      BLOB NOT NULL,
    objects      BLOB NOT NULL,
    events       BLOB NOT NULL,
    raw_bytes    INTEGER NOT NULL,
    stored_bytes INTEGER NOT NULL,
    recorded_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    FOREIGN KEY (demo_id) REFERENCES demo (demo_id) ON DELETE CASCADE
) STRICT;
