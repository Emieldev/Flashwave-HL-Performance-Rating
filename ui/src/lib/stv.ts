import { useState } from "react";
import { api } from "../api/client";
import { errorMessage, type MatchDetail } from "../api/types";
import { beginDownload, failDownload, useDownload } from "./downloads";

/**
 * Whether a match has an STV demo to read, and how to get one: shared by
 * every panel that needs it, so a download started from any of them shows
 * everywhere (the download itself is followed app-wide).
 *
 * A deleted STV still counts: what was read from it -- the timeline --
 * is kept, and every STV-only panel reads that, not the file.
 */
export function useStv(d: MatchDetail) {
  const has = d.demos.some((x) => x.kind === "stv");
  const canFetch = !has && d.demosTfId !== null;
  const download = useDownload(d.logId);
  const [error, setError] = useState<string | null>(null);
  async function fetch() {
    setError(null);
    beginDownload(d.logId, `${d.map ?? "this match"}, log ${d.logId}`);
    try {
      await api.fetchStv(d.logId);
    } catch (e) {
      setError(errorMessage(e));
      failDownload(d.logId, errorMessage(e));
    }
  }
  return { has, canFetch, download, error, fetch };
}
