import { useSyncExternalStore } from "react";
import { api } from "../api/client";
import type { NewLogs } from "../api/types";
import { startSync } from "./sync";

/**
 * The look at startup (Flashy): one small request for your recent logs, and
 * a sync only when Highlander logs you are in are not stored yet -- the
 * matches played since the app was last open. Nothing happens, and nothing
 * shows, when there is nothing new.
 */

let found: NewLogs | null = null;
let looked = false;
const listeners: Array<() => void> = [];

function set(v: NewLogs | null) {
  found = v;
  for (const l of listeners) l();
}

export async function lookForNewLogs() {
  if (looked) return;
  looked = true;
  try {
    const n = await api.checkNewLogs();
    if (!n || n.count === 0) return;
    set(n);
    if (!(await api.syncBusy())) void startSync(false);
    // The sync card takes over from here; this one says why it started.
    window.setTimeout(() => set(null), 20_000);
  } catch {
    // Quiet: the Sync button is still there, and the next start looks again.
  }
}

export function dismissNewLogs() {
  set(null);
}

export function useNewLogs(): NewLogs | null {
  return useSyncExternalStore(
    (l) => {
      listeners.push(l);
      return () => {
        const i = listeners.indexOf(l);
        if (i >= 0) listeners.splice(i, 1);
      };
    },
    () => found,
  );
}
