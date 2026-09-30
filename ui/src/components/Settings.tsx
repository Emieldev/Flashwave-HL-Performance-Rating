import { Fragment, useEffect, useState } from "react";
import { LeagueSamplePanel } from "./LeagueSamplePanel";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api, inTauri } from "../api/client";
import { errorMessage, type AppStatus, type Cleaned, type DemoIndexSummary } from "../api/types";
import { formatDate } from "../lib/format";
import { HistoryPanel } from "./HistoryPanel";
import { MapsPanel } from "./MapsPanel";
import { ImportPanel } from "./ImportPanel";
import { startRebuild, useSyncStatus } from "../lib/sync";
import { setTheme, THEMES, useTheme } from "../lib/theme";
import { allLanguages, languageFile, setLanguage, t, t as tr, useLanguage, userLanguageFiles, tx } from "../lib/i18n";
import { loadUserLanguages } from "../lib/userLang";
import { setNameSource, useNameSource } from "../lib/names";
import { checkForUpdate, installUpdate, restartNow, useUpdate } from "../lib/update";
import { RELEASES } from "../lib/changelog";
import { Markdown } from "./Markdown";
import { clearProblems, markProblemsSeen, report, useProblems } from "../lib/problems";

export function Settings({
  status,
  onReconfigure,
}: {
  status: AppStatus;
  onReconfigure: () => void;
}) {
  // Rebuilding reports from the corner like a sync, because it is one: the
  // same events, the same minutes of work.
  const sync = useSyncStatus();
  const busy = sync.state === "running";

  return (
    <div className="content">
      <UpdatesPanel version={status.version} />
      <ProblemsPanel version={status.version} />
      <LanguagePanel />
      <NamesPanel />
      <MapsPanel />
      <ThemePanel />
      <HistoryPanel />
      <ImportPanel />
      <Etf2lPanel />
      <LeagueSamplePanel />
      <RawlogPanel />
      <DemosPanel />
      <DownloadedDemosPanel />
      <div className="panel">
        <h2>{t("Setup")}</h2>
        <dl className="kv" style={{ marginTop: 14 }}>
          <dt>{tr("SteamID")}</dt>
          <dd>
            <code>{status.config.steamid}</code>
          </dd>
          <dt>{tr("TF2 folder")}</dt>
          <dd>
            {status.config.tfPath ? (
              <code>{status.config.tfPath}</code>
            ) : (
              <span className="muted">{tr("Not set: demo jumps and downloads are off. Everything else works.")}</span>
            )}
          </dd>
        </dl>
        <button className="linkish" style={{ marginTop: 14 }} onClick={onReconfigure}>{tr("Change these")}</button>
      </div>

      <div className="panel">
        <h2>{t("Data")}</h2>
        <p className="hint" style={{ marginTop: 6 }}>{tr("Rebuilds every match from stored logs. No downloading.")}</p>
        <div className="row" style={{ marginTop: 14 }}>
          <button onClick={() => void startRebuild()} disabled={busy}>
            {busy ? tr("Working…") : tr("Rebuild from stored data")}
          </button>
        </div>
        <dl className="kv" style={{ marginTop: 16 }}>
          <dt>{tr("Database")}</dt>
          <dd className="path-row">
            <code>{status.dbPath}</code>
            {/* Every log, rating and demo link is in this one file, and it is
                the only thing here that cannot be fetched again. */}
            <button className="linkish" onClick={() => void api.revealPath(status.dbPath)}>{tr("Show in Explorer")}</button>
          </dd>
          <dt>{tr("Version")}</dt>
          <dd>{status.version}</dd>
        </dl>
      </div>

      <BackupsPanel />
      <ChangelogPanel />
    </div>
  );
}

/**
 * Which version this is, and a way to ask for a newer one. The app looks
 * once at startup on its own; this is for when you have heard there is a new
 * release and do not want to restart to find out.
 */
function UpdatesPanel({ version }: { version: string }) {
  const u = useUpdate();
  return (
    <div className="panel">
      <h2>{tr("Updates")}</h2>
      <div className="row" style={{ marginTop: 10, gap: 12, flexWrap: "wrap", alignItems: "center" }}>
        <span>{tx("You have {0}", { "0": <strong>{tr("Version {version}", { version: version })}</strong> })}</span>
        <button onClick={() => void checkForUpdate(true)} disabled={u.state === "checking" || u.state === "downloading"}>
          {u.state === "checking" ? tr("Checking…") : tr("Check for updates")}
        </button>
      </div>
      <div className="update-status" aria-live="polite">
        {u.state === "checking" && (
          <p className="hint"><span className="spin" aria-hidden /> {tr("Asking GitHub for the newest release…")}</p>
        )}
        {u.state === "current" && <p className="hint">{tr("Up to date: this is the newest release.")}</p>}
        {u.state === "available" && (
          <>
            <p>{tr("Version {version} is available.", { version: u.version })}</p>
            <button className="primary" style={{ marginTop: 8 }} onClick={() => void installUpdate()}>{tr("Download and install")}</button>
          </>
        )}
        {u.state === "downloading" && (
          <>
            <p className="hint">
              <span className="spin" aria-hidden /> {tr("Downloading {version}…", { version: u.version })}
              {u.total ? ` ${Math.round((u.got / u.total) * 100)}%` : ""}
            </p>
            <span className="dl-bar" aria-hidden>
              <span className={u.total ? "dl-fill" : "dl-fill dl-unknown"} style={u.total ? { width: `${(u.got / u.total) * 100}%` } : undefined} />
            </span>
          </>
        )}
        {u.state === "ready" && (
          <>
            <p>{tr("Version {version} is installed. Restart to use it.", { version: u.version })}</p>
            <button className="primary" style={{ marginTop: 8 }} onClick={() => void restartNow()}>{tr("Restart now")}</button>
          </>
        )}
        {u.state === "failed" && <p className="error">{u.message}</p>}
        {!inTauri && <p className="hint">{tr("Updates are checked in the installed app, not in a browser preview.")}</p>}
      </div>
    </div>
  );
}

/** What changed in each release, newest first; the same notes as on GitHub. */
function ChangelogPanel() {
  return (
    <div className="panel">
      <h2>{tr("Changelog")}</h2>
      <p className="hint" style={{ marginTop: 6 }}>{tr("What changed in each release, newest first. The notes are in English.")}</p>
      <div className="changelog">
        {RELEASES.map((r, i) => (
          <details key={r.version} open={i === 0} className="changelog-entry">
            <summary>{tr("Version {version}", { version: r.version })}</summary>
            <Markdown text={r.body} />
          </details>
        ))}
      </div>
    </div>
  );
}

/**
 * Everything that went wrong, and a button that turns it into a message.
 *
 * A tester with a problem had nothing to send but a screenshot of a black
 * window. This is the thing to paste instead.
 */
function ProblemsPanel({ version }: { version: string }) {
  const problems = useProblems();
  const [copied, setCopied] = useState(false);
  useEffect(() => markProblemsSeen(), [problems.length]);

  if (problems.length === 0) {
    return (
      <div className="panel">
        <h2>{t("Problems")}</h2>
        <p className="hint" style={{ marginTop: 6 }}>{tr("Nothing has gone wrong since the app started.")}</p>
      </div>
    );
  }
  return (
    <div className="panel">
      <h2>{t("Problems")}</h2>
      <p className="hint" style={{ marginTop: 6 }}>
        {tx("{problems} since the app started. Copy this into Discord if you are reporting something.", { problems: problems.length })}</p>
      <div className="row" style={{ marginTop: 12 }}>
        <button
          onClick={() => {
            void navigator.clipboard?.writeText(report({ version }));
            setCopied(true);
            window.setTimeout(() => setCopied(false), 2000);
          }}
        >
          {copied ? tr("Copied") : tr("Copy report")}
        </button>
        <button className="linkish" onClick={() => clearProblems()}>{tr("Clear")}</button>
      </div>
      <div className="table-wrap" style={{ marginTop: 14 }}>
        <table className="match-table">
          <thead>
            <tr>
              <th>{tr("When")}</th>
              <th>{tr("What")}</th>
              <th>{tr("Why")}</th>
            </tr>
          </thead>
          <tbody>
            {problems.slice(0, 50).map((p) => (
              <tr key={p.id}>
                <td className="muted nowrap">{new Date(p.at).toLocaleTimeString()}</td>
                <td className="nowrap">
                  {tr(p.what)}
                  {p.count > 1 && <span className="muted"> ×{p.count}</span>}
                </td>
                <td className="muted">{p.message}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

/**
 * Pick a palette (Q10).
 *
 * Every theme is dark. The app is built from translucent light tints over
 * dark surfaces, so a light theme is not a swap of this list — it is its
 * own piece of work, and half-doing it would look worse than not offering
 * it. RED and BLU are never themed: they mean something in TF2, and a
 * scoreboard that recolours them is lying about which team is which.
 */
/**
 * Which language the app speaks (Q13). A missing translation shows the
 * English rather than a blank, so a partly translated language is usable
 * from the first entry a volunteer adds.
 *
 * Below the choice, the way in for volunteers: save a language as a `.lang`
 * file, edit it, press Reload, send it back. Files in that folder beat the
 * built-in text, so a fix shows the moment it is made, not at the next
 * release.
 */
function LanguagePanel() {
  const lang = useLanguage();
  const files = userLanguageFiles();
  const [note, setNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const run = (what: () => Promise<void>) => async () => {
    setBusy(true);
    setNote(null);
    try {
      await what();
    } catch (e) {
      setNote(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  // English saves the blank template as new.lang: the start of a language
  // the app does not have yet.
  const edit = run(async () => {
    const saved = await api.saveLanguageFile(lang === "en" ? "new" : lang, languageFile(lang));
    setNote(
      !saved.created
        ? tr("{0} is in the language folder. Edit it, then press Reload.", { "0": saved.path })
        : lang === "en"
          ? tr("Saved {0}. Rename it to your language's code, like pl.lang, fill in the right-hand side, then press Reload.", { "0": saved.path })
          : tr("Saved {0}. Edit it, then press Reload.", { "0": saved.path }),
    );
    await api.revealPath(saved.path);
  });
  const open = run(async () => {
    const found = await loadUserLanguages();
    if (found) await api.revealPath(found.dir);
  });
  const reload = run(async () => {
    const found = await loadUserLanguages();
    setNote(found ? tr("Language files read: {n}.", { n: found.files.length }) : tr("The language folder could not be read."));
  });

  return (
    <div className="panel">
      <h2>{t("Language")}</h2>
      <div className="row" style={{ marginTop: 10, gap: 8, flexWrap: "wrap" }}>
        {allLanguages().map((l) => (
          <button
            key={l.id}
            className={lang === l.id ? "primary lang-choice" : "lang-choice"}
            aria-pressed={lang === l.id}
            onClick={() => setLanguage(l.id)}
          >
            {l.name}
          </button>
        ))}
      </div>
      {lang !== "en" && (
        <p className="hint" style={{ marginTop: 8 }}>
          {t("Draft translations, not yet checked by a native speaker. Anything untranslated is shown in English.")}
        </p>
      )}

      <h3 style={{ marginTop: 18 }}>{tr("Translation files")}</h3>
      <p className="hint" style={{ marginTop: 6 }}>
        {tr("Every language is a plain .lang file. Fix a line or translate a new one, press Reload to see it here, and send the file on Discord to have it included for everyone.")}
      </p>
      <div className="row" style={{ marginTop: 10, gap: 8, flexWrap: "wrap" }}>
        <button onClick={() => void edit()} disabled={busy}>
          {lang === "en" ? tr("Start a new language") : tr("Edit this language")}
        </button>
        <button onClick={() => void open()} disabled={busy}>{tr("Show the language folder")}</button>
        <button onClick={() => void reload()} disabled={busy}>{tr("Reload")}</button>
      </div>
      {note && <p className="hint" style={{ marginTop: 8 }}>{note}</p>}
      {files.length > 0 && (
        <ul className="hint" style={{ marginTop: 8 }}>
          {files.map((f) => (
            <li key={f.id}>
              <code>{f.id}.lang</code>{" "}
              {f.added
                ? tr("adds {name}: {n} lines", { name: f.name, n: f.lines })
                : f.lines === 0
                  ? tr("as shipped, nothing changed")
                  : tr("{n} lines over the built-in {name}", { name: f.name, n: f.lines })}
              {f.errors.length > 0 && (
                <span className="error">
                  {" · "}
                  {tr("{n} lines could not be read, first {first}", { n: f.errors.length, first: f.errors[0] })}
                </span>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/**
 * The name a player goes by on every page: the one from the log, or their
 * ETF2L name. The ETF2L names come from official rosters the app has
 * already read, so the choice costs no requests to ETF2L.
 */
function NamesPanel() {
  const { source, count } = useNameSource();
  return (
    <div className="panel">
      <h2>{tr("Player names")}</h2>
      <div className="row" style={{ marginTop: 10, gap: 8, flexWrap: "wrap" }}>
        <button className={source === "log" ? "primary" : undefined} aria-pressed={source === "log"} onClick={() => setNameSource("log")}>
          {tr("As in the log")}
        </button>
        <button className={source === "etf2l" ? "primary" : undefined} aria-pressed={source === "etf2l"} onClick={() => setNameSource("etf2l")}>
          {tr("ETF2L names")}
        </button>
      </div>
      <p className="hint" style={{ marginTop: 8 }}>
        {tr("ETF2L names come from the official rosters the app has already read, so they cost no extra requests. Players who never played an ETF2L official keep the name from the log.")}{" "}
        {tr("{n} players have an ETF2L name.", { n: count.toLocaleString() })}
      </p>
    </div>
  );
}

function ThemePanel() {
  const theme = useTheme();
  return (
    <div className="panel">
      <h2>{t("Theme")}</h2>
      <div className="theme-grid">
        {THEMES.map((t) => (
          <button
            key={t.id}
            className={theme === t.id ? "theme-card active" : "theme-card"}
            onClick={() => setTheme(t.id)}
            aria-pressed={theme === t.id}
          >
            <span className={`theme-swatch theme-${t.id}`} aria-hidden>
              <i className="sw-bg" />
              <i className="sw-panel" />
              <i className="sw-accent" />
            </span>
            <span className="theme-name">{t.name}</span>
            <span className="theme-hint muted">{tr(t.hint)}</span>
          </button>
        ))}
      </div>
    </div>
  );
}

/**
 * Copies of the database. One is taken before every sync and rebuild, five
 * are kept, and this is where to check they exist — the file holds every log,
 * demo index and rating, and nothing else here can rebuild it from nothing.
 */
function BackupsPanel() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["backups"], queryFn: api.listBackups });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);

  /** A name that says what it is and when, so a folder of them sorts. */
  function suggestedName() {
    const d = new Date();
    const pad = (n: number) => String(n).padStart(2, "0");
    return `flashwave-${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}.sqlite3`;
  }

  async function saveElsewhere() {
    setBusy(true);
    setError(null);
    setSaved(null);
    try {
      const b = await api.saveBackupAs(suggestedName());
      if (b) setSaved(b.path);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function backupNow() {
    setBusy(true);
    setError(null);
    try {
      await api.backupNow();
      await qc.invalidateQueries({ queryKey: ["backups"] });
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  const items = q.data?.items ?? [];
  return (
    <div className="panel">
      <h2>{t("Backups")}</h2>
      <p className="hint">{tx("Taken before every sync and rebuild; the newest five are kept. To restore one, close the app and rename it over {0}.", { "0": <code>{tr("hl.sqlite3")}</code> })}</p>
      <p className="hint">
        {tx("{0} Keep a copy on another drive.", { "0": <strong>{tr("Uninstalling can delete these.")}</strong> })}</p>
      <div className="row" style={{ marginTop: 14 }}>
        <button onClick={() => void backupNow()} disabled={busy}>
          {busy ? tr("Copying…") : tr("Back up now")}
        </button>
        <button onClick={() => void saveElsewhere()} disabled={busy}>
          {busy ? tr("Copying…") : tr("Save a copy elsewhere…")}
        </button>
      </div>
      {saved && (
        <p className="hint" style={{ marginTop: 10 }}>{tx("Written to {0}.", { "0": <code>{saved}</code> })}</p>
      )}
      {error && <p className="error" style={{ marginTop: 10 }}>{error}</p>}
      {items.length === 0 ? (
        <p className="hint" style={{ marginTop: 14 }}>{tr("No copies yet. The next sync makes one.")}</p>
      ) : (
        <dl className="kv" style={{ marginTop: 16 }}>
          <dt>{tr("Folder")}</dt>
          <dd className="path-row">
            <code>{q.data?.dir}</code>
            <button className="linkish" onClick={() => void api.revealPath(q.data!.dir)}>{tr("Show in Explorer")}</button>
          </dd>
          {items.map((b) => (
            <Fragment key={b.path}>
              <dt>{formatDate(b.madeAt, true)}</dt>
              <dd>{tx("{0} MB", { "0": (b.bytes / 1_000_000).toFixed(0) })}</dd>
            </Fragment>
          ))}
        </dl>
      )}
    </div>
  );
}

/** Demo index counts, and a rescan for when you have just recorded. */
function DemosPanel() {
  const qc = useQueryClient();
  const stats = useQuery({ queryKey: ["demo_stats"], queryFn: api.demoStats });
  const [scan, setScan] = useState<{ busy: boolean; result: DemoIndexSummary | null; error: string | null }>({
    busy: false,
    result: null,
    error: null,
  });

  async function rescan() {
    setScan({ busy: true, result: null, error: null });
    try {
      const result = await api.scanDemos();
      setScan({ busy: false, result, error: null });
      void qc.invalidateQueries({ queryKey: ["demo_stats"] });
      void qc.invalidateQueries({ queryKey: ["matches"] });
      void qc.invalidateQueries({ queryKey: ["match"] });
    } catch (e) {
      setScan({ busy: false, result: null, error: errorMessage(e) });
    }
  }

  const s = stats.data;
  return (
    <div className="panel">
      <h2>{t("Demos")}</h2>
      <p className="hint" style={{ marginTop: 6 }}>{tx("Demos in {0}, {1} and {2}, matched to logs by map and time.", { "0": <code>{tr("tf")}</code>, "1": <code>{tr("tf/demos")}</code>, "2": <code>{tr("tf/demos/stv")}</code> })}</p>
      {s && (
        <dl className="kv" style={{ marginTop: 14 }}>
          <dt>{tr("Demos found")}</dt>
          <dd>
            {s.demos}
            {s.stv > 0 && tr(" ({stv} STV)", { stv: s.stv })}
          </dd>
          <dt>{tr("Linked")}</dt>
          <dd>
            {tx("{linked} demos, covering {matchesWithDemo} matches", { linked: s.linked, matchesWithDemo: s.matchesWithDemo })}</dd>
          <dt>{tr("Markers")}</dt>
          <dd>{tx("{markers} killstreak markers from Demo Support", { markers: s.markers })}</dd>
          <dt title={tr("Every demo the app reads is kept as a compact timeline, so later versions can still learn from it once the file is gone.")}>{tr("Kept")}</dt>
          <dd>
            {s.timelines === 0
              ? <span className="muted">{tr("None yet: the next sync keeps each demo it reads.")}</span>
              : tr("{timelines} demos, {size}", { timelines: s.timelines, size: `${Math.max(1, Math.round(s.timelineBytes / 1e6))} MB` })}
            {s.timelinesFileGone > 0 && <span className="muted">{tr(" · {gone} no longer on disk", { gone: s.timelinesFileGone })}</span>}
          </dd>
        </dl>
      )}
      <div className="row" style={{ marginTop: 14 }}>
        <button onClick={() => void rescan()} disabled={scan.busy}>
          {scan.busy ? tr("Scanning…") : tr("Rescan demos")}
        </button>
        {scan.result && (
          <span className="hint">{tx("Scanned {scanned}, linked {demosLinked}.", { scanned: scan.result.scanned, demosLinked: scan.result.demosLinked })}</span>
        )}
      </div>
      {scan.error && <p className="error" style={{ marginTop: 10 }}>{scan.error}</p>}
    </div>
  );
}

/**
 * The STV demos the app downloaded, and getting rid of them (PLAN Q23).
 *
 * A demo is read once and never needed again, and each is around 80 MB.
 * Deleting one removes the file and nothing else: the row stays, with the
 * demos.tf id it can be fetched back from, and everything already derived
 * from it -- aim, deaths, routes -- was derived and stored while the file
 * was here. The match page does not change.
 *
 * Two things this panel never offers, and says so, because the reasonable
 * guess is that it would:
 *
 * - **Your own POV demos.** TF2 wrote those, in your folder, and some are
 *   the only copy of a match older than this app. They are also the only
 *   source of your own view angles as you made them. 3.5 GB of
 *   irreplaceable against 600 MB of replaceable is the wrong trade.
 * - **An STV you already had.** No demos.tf id means no way back, which
 *   makes it your file too.
 */
function DownloadedDemosPanel() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["downloaded_demos"], queryFn: api.downloadedDemos });
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<Cleaned | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);

  async function remove(only: number | null) {
    setBusy(true);
    setError(null);
    setDone(null);
    try {
      const r = await api.deleteDownloadedDemos(only, false);
      setDone(r);
      await qc.invalidateQueries({ queryKey: ["downloaded_demos"] });
      await qc.invalidateQueries({ queryKey: ["demo_stats"] });
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
      setConfirming(false);
    }
  }

  const items = q.data ?? [];
  const total = items.reduce((n, d) => n + d.sizeBytes, 0);
  const ready = items.filter((d) => d.read);
  const readySize = ready.reduce((n, d) => n + d.sizeBytes, 0);
  const gb = (b: number) => (b >= 1e9 ? `${(b / 1e9).toFixed(1)} GB` : `${Math.round(b / 1e6)} MB`);

  return (
    <div className="panel">
      <h2>{t("Downloaded demos")}</h2>
      <p className="hint" style={{ marginTop: 6 }}>{tr("STV demos the app fetched. Each is read once; after that it is only taking up space, and it can be downloaded again whenever it is wanted. Your own recordings are never touched.")}</p>

      {items.length === 0 ? (
        <p className="hint" style={{ marginTop: 14 }}>
          {q.isPending ? tr("Looking…") : tr("None downloaded yet.")}
        </p>
      ) : (
        <>
          <dl className="kv" style={{ marginTop: 14 }}>
            <dt>{tr("Held")}</dt>
            <dd>
              {tx("{items} demo{1}, {2}", { "1": items.length === 1 ? "" : "s", "2": gb(total), items: items.length })}
            </dd>
            <dt>{tr("Finished with")}</dt>
            <dd>
              {tx("{ready} of {items}, {2}", { "2": gb(readySize), ready: ready.length, items: items.length })}
            </dd>
          </dl>

          <div className="table-wrap" style={{ marginTop: 12 }}>
            <table className="match-table">
              <thead>
                <tr>
                  <th>{tr("Demo")}</th>
                  <th className="num">{tr("Size")}</th>
                  <th>{tr("State")}</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {items.map((d) => (
                  <tr key={d.demoId}>
                    <td className="nowrap">{d.map ?? d.fileName}</td>
                    <td className="num">{gb(d.sizeBytes)}</td>
                    <td>
                      {d.read ? (
                        <span className="muted">{tr("read")}</span>
                      ) : (
                        <span className="muted" title={tr("The app has not finished reading this one, so it is kept.")}>{tr("still needed")}</span>
                      )}
                    </td>
                    <td>
                      <button className="linkish" disabled={busy || !d.read} onClick={() => void remove(d.demoId)}>{tr("Delete")}</button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          <div className="row" style={{ marginTop: 14 }}>
            {confirming ? (
              <>
                <span className="hint">{tx("Delete {ready} demo{1} ({2})?", { "1": ready.length === 1 ? "" : "s", "2": gb(readySize), ready: ready.length })}</span>
                <button className="primary" disabled={busy} onClick={() => void remove(null)}>
                  {busy ? tr("Deleting…") : tr("Yes, delete")}
                </button>
                <button disabled={busy} onClick={() => setConfirming(false)}>{tr("Cancel")}</button>
              </>
            ) : (
              <button disabled={busy || ready.length === 0} onClick={() => setConfirming(true)}>{tr("Delete all read demos")}</button>
            )}
          </div>
        </>
      )}

      <AutoDelete />

      {done && (
        <p className="hint" style={{ marginTop: 10 }}>{tx("Deleted {deleted}, freeing {1}.{2}", { "1": gb(done.bytes), "2": done.skipped > 0 && tr(" {skipped} kept: not finished reading them yet.", { skipped: done.skipped }), deleted: done.deleted })}
        </p>
      )}
      {error && <p className="error" style={{ marginTop: 10 }}>{error}</p>}
    </div>
  );
}

/**
 * Delete each downloaded demo as soon as the app is finished with it.
 *
 * Off by default, and that is the considered choice rather than caution.
 * The aim pass has had eleven versions, and every one of them re-read every
 * demo to store something better than the version before. A deleted demo
 * cannot take part in the twelfth: its rows stay frozen at whatever the
 * pass knew when the file was still there. That is a fine trade if disk is
 * the thing you are short of, and a bad one otherwise, so it is asked
 * rather than assumed.
 */
function AutoDelete() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["auto_delete_demos"], queryFn: api.autoDeleteDemos });
  const [error, setError] = useState<string | null>(null);

  async function set(on: boolean) {
    setError(null);
    try {
      await api.setAutoDeleteDemos(on);
      await qc.invalidateQueries({ queryKey: ["auto_delete_demos"] });
    } catch (e) {
      setError(errorMessage(e));
    }
  }

  return (
    <div style={{ marginTop: 14 }}>
      <label className="row" style={{ gap: 8, cursor: "pointer", fontSize: "0.9rem" }}>
        <input
          type="checkbox"
          checked={q.data ?? false}
          disabled={q.isPending}
          onChange={(e) => void set(e.target.checked)}
        />
        <span>{tr("Delete each demo once it has been read")}</span>
      </label>
      <p className="hint" style={{ marginTop: 6 }}>{tr("Runs after every sync. A later version of the rating re-reads demos to improve what it stores, and a deleted one cannot take part until it is downloaded again.")}</p>
      {error && <p className="error">{error}</p>}
    </div>
  );
}

/** What ETF2L added: officials, and how every match was classified. */
function Etf2lPanel() {
  const q = useQuery({ queryKey: ["context_counts"], queryFn: api.contextCounts });
  const c = q.data;
  return (
    <div className="panel">
      <h2>{t("ETF2L and match types")}</h2>
      <p className="hint" style={{ marginTop: 6 }}>{tr("Your ETF2L results are fetched on every sync. Each Highlander match you played is then sorted into an official, a scrim (most of your side are regular teammates or your ETF2L roster) or a pug.")}</p>
      {c && (
        <dl className="kv" style={{ marginTop: 14 }}>
          <dt>{tr("ETF2L player")}</dt>
          <dd>
            {c.etf2lPlayer ? (
              <button className="linkish" onClick={() => void api.openExternal(`https://etf2l.org/forum/user/${c.etf2lPlayer}/`)}>
                #{c.etf2lPlayer} ↗
              </button>
            ) : (
              <span className="muted">{tr("not found yet — sync to look it up")}</span>
            )}
          </dd>
          <dt>{tr("Officials")}</dt>
          <dd>
            {tx("{officials} logs{1}", { "1": c.rosterOfficials > 0 && (
              <span className="muted">{" "}{tx("· {rosterOfficials} found by roster that trends.tf had not tagged", { rosterOfficials: c.rosterOfficials })}</span>
            ), officials: c.officials })}
          </dd>
          <dt>{tr("Scrims")}</dt>
          <dd>{c.scrims}</dd>
          <dt>{tr("Pugs")}</dt>
          <dd>{c.pugs}</dd>
          <dt>{tr("Last fetched")}</dt>
          <dd>
            {c.lastFetch ? (
              <>
                {new Date(c.lastFetch * 1000).toLocaleString()}{" "}
                <span className="muted">{tx("({etf2lMatches} ETF2L matches stored)", { etf2lMatches: c.etf2lMatches })}</span>
              </>
            ) : (
              <span className="muted">{tr("never")}</span>
            )}
          </dd>
        </dl>
      )}
    </div>
  );
}

/** Raw server logs: where every kill, with its time and position, comes from. */
function RawlogPanel() {
  const q = useQuery({ queryKey: ["rawlog_stats"], queryFn: api.rawlogStats });
  const s = q.data;
  return (
    <div className="panel">
      <h2>{t("Raw logs")}</h2>
      <p className="hint" style={{ marginTop: 6 }}>{tr("The server log behind each logs.tf page. It has every kill with its time, both classes and where both players stood, so kills are valued one by one: by the victim's class, the map, and whether they were defending. Fetched on every sync.")}</p>
      {s && (
        <dl className="kv" style={{ marginTop: 14 }}>
          <dt>{tr("Stored")}</dt>
          <dd>
            {tx("{0} matches {1}", { "0": s.stored.toLocaleString(), "1": <span className="muted">{tx("({0} MB)", { "0": (s.bytes / 1e6).toFixed(0) })}</span> })}
          </dd>
          <dt>{tr("Kills")}</dt>
          <dd>{s.kills.toLocaleString()}</dd>
          <dt>{tr("Still to fetch")}</dt>
          <dd>
            {s.pending}
            {s.pending > 0 && <span className="muted">{" "}{tr("· retried on the next sync")}</span>}
          </dd>
          {s.missing > 0 && (
            <>
              <dt>{tr("Not on logs.tf")}</dt>
              <dd>
                {s.missing} <span className="muted">{tr("· these use logs.tf's totals instead")}</span>
              </dd>
            </>
          )}
        </dl>
      )}
    </div>
  );
}
