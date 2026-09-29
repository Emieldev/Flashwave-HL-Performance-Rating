import { useSyncExternalStore } from "react";

/**
 * Which name a player goes by on screen: the one in the log, which is
 * whatever they called themselves in-game that day, or their ETF2L name,
 * which is the one the community knows them by and stays put.
 *
 * ETF2L names come from official rosters the app has already read (see
 * `etf2l_names` in the backend), so choosing them costs no requests. A
 * player who never appeared in one keeps the name from the log.
 *
 * The swap happens once, where API responses arrive (./../api/client.ts):
 * any object carrying a player's `accountId` (or `steamid64`) next to a
 * `name` gets the ETF2L one. Team names carry no account id and are left
 * alone. Changing the choice refetches, so every page follows.
 *
 * Per machine, like the theme and the language.
 */

export type NameSource = "log" | "etf2l";

const KEY = "hl.names";
const STEAM_BASE = 76561197960265728n;

function load(): NameSource {
  try {
    return localStorage.getItem(KEY) === "etf2l" ? "etf2l" : "log";
  } catch {
    // Storage can be unavailable; the log's names are always there.
    return "log";
  }
}

let source: NameSource = load();
let names = new Map<number, string>();
let snapshot = { source, count: 0 };
let listeners: Array<() => void> = [];
// Told only when what the pages show would change, so they refetch.
let refetchers: Array<() => void> = [];

function emit() {
  snapshot = { source, count: names.size };
  listeners.forEach((f) => f());
}

export function setNameSource(s: NameSource) {
  if (s === source) return;
  source = s;
  try {
    localStorage.setItem(KEY, s);
  } catch {
    // Not remembered across restarts, but the pages still change now.
  }
  emit();
  refetchers.forEach((f) => f());
}

/** The ETF2L names, by account id, as the backend sends them (string keys). */
export function setEtf2lNames(byAccount: Record<string, string>) {
  const next = new Map(Object.entries(byAccount).map(([k, v]) => [Number(k), v] as const));
  const same = next.size === names.size && [...next].every(([k, v]) => names.get(k) === v);
  names = next;
  emit();
  if (!same && source === "etf2l") refetchers.forEach((f) => f());
}

/** Re-render on a change of source or of the names known. */
export function useNameSource(): { source: NameSource; count: number } {
  return useSyncExternalStore(
    (l) => {
      listeners.push(l);
      return () => {
        listeners = listeners.filter((x) => x !== l);
      };
    },
    () => snapshot,
  );
}

/** Call `f` whenever the names on screen would change; returns the unsubscribe. */
export function onNamesChanged(f: () => void): () => void {
  refetchers.push(f);
  return () => {
    refetchers = refetchers.filter((x) => x !== f);
  };
}

function accountOf(o: Record<string, unknown>): number | null {
  if (typeof o.accountId === "number") return o.accountId;
  if (typeof o.steamid64 === "string" && /^\d{17}$/.test(o.steamid64)) {
    return Number(BigInt(o.steamid64) - STEAM_BASE);
  }
  return null;
}

function swap(v: unknown): unknown {
  if (Array.isArray(v)) return v.map(swap);
  if (v === null || typeof v !== "object") return v;
  const o = v as Record<string, unknown>;
  const out: Record<string, unknown> = {};
  for (const [k, x] of Object.entries(o)) out[k] = swap(x);
  if (typeof o.name === "string") {
    const id = accountOf(o);
    const etf2l = id === null ? undefined : names.get(id);
    if (etf2l) out.name = etf2l;
  }
  return out;
}

/**
 * A response with players' names swapped for their ETF2L ones, when that is
 * the choice. A copy: the input is left as it was, so a fixture or a cached
 * object never changes underneath its owner.
 */
export function withChosenNames<T>(v: T): T {
  if (source !== "etf2l" || names.size === 0) return v;
  return swap(v) as T;
}
