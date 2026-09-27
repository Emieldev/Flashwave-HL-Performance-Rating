import { useSyncExternalStore } from "react";
import type { QueryClient } from "@tanstack/react-query";
import { api } from "../api/client";
import { noteError } from "./problems";
import { errorMessage, type Progress, type SyncDone } from "../api/types";
import { t } from "./i18n";

/**
 * A sync, tracked app-wide rather than by the strip that started it.
 *
 * A sync runs for minutes in the backend whatever the window is showing, so
 * its progress belongs beside the demo downloads in the corner rather than in
 * a bar across the top of one page. The backend's events are listened to once,
 * here; anything that wants to show them reads this store.
 */

export type SyncState =
  | { state: "idle" }
  | { state: "running"; progress: Progress | null; failures: number; notes: string[] }
  | { state: "done"; result: SyncDone; notes: string[] }
  | { state: "error"; message: string };

/** How often the match list may refresh while logs are arriving. Newest are
 *  fetched first, so last night's game shows up within seconds — but a
 *  refresh per log would re-render the list for an hour straight. */
const REFRESH_EVERY_MS = 1000;

/** Rough seconds per log: logs.tf's throttle plus its response time. */
const SECONDS_PER_LOG = 2.5;

let status: SyncState = { state: "idle" };
let listeners: Array<() => void> = [];
let started = false;
let refreshTimer: number | null = null;

function set(next: SyncState) {
  status = next;
  listeners.forEach((l) => l());
}

/** Everything a finished sync could have changed. */
function invalidateAll(qc: QueryClient) {
  for (const key of [
    "matches",
    "index_stats",
    "profile",
    "match",
    "teammates",
    "context_counts",
    "rawlog_stats",
    "played_filters",
    "all_history",
  ]) {
    void qc.invalidateQueries({ queryKey: [key] });
  }
}

/** Start listening to the backend's sync events. Safe to call twice. */
export function watchSync(qc: QueryClient) {
  if (started) return;
  started = true;

  // A sync may already be running: one started before the window reloaded.
  void api.syncBusy().then((busy) => {
    if (busy && status.state === "idle") {
      set({ state: "running", progress: null, failures: 0, notes: [] });
    }
  });

  void api.onSync({
    onProgress: (p) => {
      const was = status.state === "running" ? status : null;
      // What a source could not give us, kept for the card at the end: a
      // sync that carried on without logs.tf succeeded, but not completely.
      const note =
        p.kind === "sourceFailed"
          ? t("{source} could not be reached; the sync carried on without it.", { source: p.source })
          : p.kind === "gaveUp"
            ? t("{source} stopped answering after {1} of {2}; the rest waits for the next sync.", { "1": p.done.toLocaleString(), "2": p.total.toLocaleString(), source: p.source })
            : null;
      // Every failure is written down with its reason. The counter alone
      // ("2 failed") was all anyone ever saw, and it cannot be acted on.
      if (p.kind === "fetchFailed") {
        noteError({ what: `fetching log ${p.logId}`, message: explain(p.error), detail: p.error });
      } else if (p.kind === "sourceFailed") {
        noteError({ what: p.source, message: explain(p.error), detail: p.error });
      } else if (p.kind === "gaveUp") {
        noteError({
          what: p.source,
          message: `stopped answering after ${p.done} of ${p.total}; the rest waits for the next sync`,
        });
      }
      set({
        state: "running",
        progress: p,
        failures: (was?.failures ?? 0) + (p.kind === "fetchFailed" ? 1 : 0),
        notes: note && !was?.notes.includes(note) ? [...(was?.notes ?? []), note] : (was?.notes ?? []),
      });
      // Each log is rated as it lands, so a refresh shows finished rows.
      if (p.kind === "fetching" && p.done > 0 && refreshTimer === null) {
        refreshTimer = window.setTimeout(() => {
          refreshTimer = null;
          void qc.invalidateQueries({ queryKey: ["matches"] });
        }, REFRESH_EVERY_MS);
      }
    },
    onDone: (result) => {
      set({
        state: "done",
        result,
        notes: status.state === "running" ? status.notes : [],
      });
      invalidateAll(qc);
    },
    onError: (e) => {
      noteError({ what: "the sync", message: explain(e.message), detail: e.message });
      set({ state: "error", message: e.message });
    },
  });
}

/**
 * A server's error in words a player can act on.
 *
 * The raw chain is kept as the detail; this is the line shown first. Three
 * of these cover almost everything a sync hits, and the difference between
 * them matters: one is worth retrying, one never will be, and one is not
 * about this app at all.
 */
export function explain(raw: string): string {
  const m = raw.toLowerCase();
  if (m.includes("10060") || m.includes("timed out") || m.includes("error sending request")) {
    return t("could not reach the server — it may be down, or the connection dropped");
  }
  if (m.includes("404") || m.includes("not found")) {
    return t("the server does not have this log");
  }
  if (m.includes("429") || m.includes("too many")) {
    return t("asked for too much too quickly; it will be retried");
  }
  if (m.includes("500") || m.includes("502") || m.includes("503")) {
    return t("the server answered with an error of its own");
  }
  if (m.includes("json") || m.includes("expected")) {
    return t("the answer was not in the shape we expect");
  }
  // Long chains read badly in a list; the whole thing is in the detail.
  return raw.length > 160 ? `${raw.slice(0, 157)}…` : raw;
}

/** Ask for a sync, and show it as running from the click rather than from the
 *  first event — indexing takes a few seconds before anything is reported. */
export async function startSync(full = false) {
  set({ state: "running", progress: null, failures: 0, notes: [] });
  try {
    await api.syncStart(full);
  } catch (e) {
    set({ state: "error", message: errorMessage(e) });
  }
}

/** The same, for rebuilding every match from stored data. */
export async function startRebuild() {
  set({ state: "running", progress: null, failures: 0, notes: [] });
  try {
    await api.reprocessStart();
  } catch (e) {
    set({ state: "error", message: errorMessage(e) });
  }
}

export function dismissSync() {
  if (status.state === "done" || status.state === "error") set({ state: "idle" });
}

export function useSyncStatus(): SyncState {
  return useSyncExternalStore(
    (l) => {
      listeners.push(l);
      return () => {
        listeners = listeners.filter((x) => x !== l);
      };
    },
    () => status,
  );
}

/** How far along, 0 to 1, or null when a phase cannot say. */
export function fractionOf(p: Progress | null): number | null {
  if (!p) return null;
  switch (p.kind) {
    case "fetching":
    case "reprocessing":
    case "rating":
    case "rawLogs":
    case "parts":
    case "fights":
    case "readingDemos":
      return p.total > 0 ? p.done / p.total : 1;
    case "etf2l":
      return p.total > 0 ? p.done / p.total : null;
    default:
      return null;
  }
}

/** One line saying what the sync is doing now. */
export function labelOf(p: Progress | null): string {
  if (!p) return t("Starting…");
  const n = (x: number) => x.toLocaleString();
  switch (p.kind) {
    case "indexing":
      return p.rows > 0 ? t("Indexing {source} — {rows} rows", { source: p.source, rows: n(p.rows) }) : t("Indexing {source}…", { source: p.source });
    case "indexed":
      return t("Indexed. {superseded} per-round logs folded into their match.", { superseded: p.superseded });
    case "fetching":
      if (p.total === 0) return t("Nothing new to fetch.");
      return (
        t("Matches {done} of {total}", { done: n(p.done), total: n(p.total) }) +
        (p.done < p.total ? t(" — about {0} left", { "0": eta(p.total - p.done) }) : "")
      );
    case "fetchFailed":
      return t("Log {logId} failed; continuing.", { logId: p.logId });
    case "reprocessing":
      // The card's title already says what this is.
      return t("{done} of {total} matches", { done: n(p.done), total: n(p.total) });
    case "rating":
      return t("Rating {done} of {total} matches", { done: n(p.done), total: n(p.total) });
    case "rawLogs":
      if (p.total === 0) return t("Server logs up to date.");
      return (
        t("Server logs {done} of {total}", { done: n(p.done), total: n(p.total) }) +
        (p.done < p.total ? t(" — about {0} left", { "0": eta(p.total - p.done) }) : "")
      );
    case "parts":
      return t("Per-map logs {done} of {total}", { done: n(p.done), total: n(p.total) });
    case "stage":
      return `${t(p.what)}…`;
    case "fights":
      return p.total === 0 ? t("Fights up to date.") : t("Reading fights {done} of {total}", { done: n(p.done), total: n(p.total) });
    case "readingDemos":
      // Around 16 s each, so the count moves slowly and the match it is on
      // is the part worth saying.
      if (p.total === 0) return t("Demos already read.");
      return t("Reading demos {done} of {total}", { done: n(p.done), total: n(p.total) }) + (p.logId !== null ? t(" — log {logId}", { logId: p.logId }) : "");
    case "etf2l":
      return p.total === 0 ? t("Checking ETF2L…") : t("ETF2L officials {done} of {total}", { done: p.done, total: p.total });
    case "sourceFailed":
      return t("{source} could not be reached; carrying on without it.", { source: p.source });
    case "gaveUp":
      return t("{source} stopped answering; the rest waits for the next sync.", { source: p.source });
  }
}

export function eta(logs: number): string {
  const mins = Math.ceil((logs * SECONDS_PER_LOG) / 60);
  return mins <= 1 ? t("1 min") : t("{mins} min", { mins: mins });
}
