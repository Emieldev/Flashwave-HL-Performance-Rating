/**
 * The `.lang` translation file: one line per piece of text, the English on
 * the left and the translation on the right, both as JSON strings so quotes,
 * backslashes and any script survive without a special rule.
 *
 *     # a comment
 *     language = "Français"
 *     locale = "fr-FR"
 *
 *     "Matches" = "Matchs"
 *     "Loading…" = ""
 *
 * An empty right-hand side means "not translated yet" and shows the English.
 * The bare `language` and `locale` lines name the language and pick how its
 * dates are written.
 *
 * scripts/i18n.mjs reads and writes the same format; keep the two in step.
 */

export type LangFile = {
  /** The language's own name, from `language = "..."`. */
  name: string | null;
  /** BCP 47 tag for dates and numbers, from `locale = "..."`. */
  locale: string | null;
  entries: Record<string, string>;
  /** Lines that could not be read, as "line 12: ..." — shown in Settings. */
  errors: string[];
};

const STR = String.raw`"(?:[^"\\]|\\.)*"`;
const ENTRY = new RegExp(`^(${STR})\\s*=\\s*(${STR})$`);
const HEADER = new RegExp(`^(language|locale)\\s*=\\s*(${STR})$`);

export function parseLang(text: string): LangFile {
  const out: LangFile = { name: null, locale: null, entries: {}, errors: [] };
  const lines = text.replace(/^\uFEFF/, "").split(/\r?\n/);
  lines.forEach((raw, i) => {
    const line = raw.trim();
    if (!line || line.startsWith("#")) return;
    try {
      const h = HEADER.exec(line);
      if (h) {
        const v = JSON.parse(h[2]) as string;
        if (h[1] === "language") out.name = v;
        else out.locale = v;
        return;
      }
      const m = ENTRY.exec(line);
      if (m) {
        out.entries[JSON.parse(m[1]) as string] = JSON.parse(m[2]) as string;
        return;
      }
    } catch {
      // A bad escape inside the quotes: reported below like any other line.
    }
    out.errors.push(`line ${i + 1}: ${line.length > 60 ? line.slice(0, 60) + "…" : line}`);
  });
  return out;
}

const INTRO = `# One line per piece of text: the English on the left, this language on the
# right. Change only the right-hand side. An empty right side ("") shows the
# English. Keep {placeholders} like {0} or {name}: the app fills them with
# numbers and names, and they can move to wherever the sentence needs them.
#
# To try a change: put this file in the app's lang folder (Settings, Language,
# "Open the language folder") and press Reload. Send the file on Discord to
# have it included for everyone.`;

export function writeLang(name: string, locale: string, entries: Record<string, string>): string {
  const keys = Object.keys(entries).sort((a, b) => a.localeCompare(b));
  const body = keys.map((k) => `${JSON.stringify(k)} = ${JSON.stringify(entries[k] ?? "")}`);
  return [`# Flashwave.tf: ${name}`, "#", INTRO, "", `language = ${JSON.stringify(name)}`, `locale = ${JSON.stringify(locale)}`, "", ...body, ""].join("\n");
}
