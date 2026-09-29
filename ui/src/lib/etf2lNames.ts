import { api } from "../api/client";
import { setEtf2lNames } from "./names";

/**
 * Read the ETF2L names the database holds. Kept apart from names.ts, which
 * the API client itself imports, the same way userLang.ts is.
 *
 * Runs at start and after every sync, which is when new rosters arrive. A
 * failure keeps whatever names were known, and the log's names still show.
 */
export async function loadEtf2lNames(): Promise<void> {
  try {
    setEtf2lNames(await api.etf2lNames());
  } catch (e) {
    console.warn("reading ETF2L names failed", e);
  }
}
