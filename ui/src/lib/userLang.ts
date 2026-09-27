import { api } from "../api/client";
import type { LanguageFiles } from "../api/types";
import { applyUserFiles } from "./i18n";

/**
 * Read the user's lang folder and lay its `.lang` files over the built-in
 * translations. Kept apart from i18n.ts so that module never pulls in the
 * API: it has to be importable from anywhere, the API client included.
 *
 * Runs once at start and again from Settings' Reload. A failure leaves the
 * built-in translations exactly as they were.
 */
export async function loadUserLanguages(): Promise<LanguageFiles | null> {
  try {
    const found = await api.languageFiles();
    applyUserFiles(found.files);
    return found;
  } catch (e) {
    console.warn("reading the language folder failed", e);
    return null;
  }
}
