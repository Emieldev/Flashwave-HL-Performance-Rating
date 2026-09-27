/**
 * Every release's notes, for Settings › Changelog.
 *
 * Built in from `docs/release-<version>.md` -- the same file each GitHub
 * release is published from -- so the changelog works offline and can never
 * say something different from what shipped. Adding a release's notes file
 * is all it takes for the next build to list it.
 */

const FILES = import.meta.glob("../../../docs/release-*.md", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

export type Release = { version: string; body: string };

/** "0.4.0" -> [0, 4, 0], for ordering; "0.3" counts as "0.3.0". */
const parts = (v: string) => v.split(".").map((n) => Number.parseInt(n, 10) || 0);

function newer(a: string, b: string): number {
  const [x, y] = [parts(a), parts(b)];
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    const d = (y[i] ?? 0) - (x[i] ?? 0);
    if (d !== 0) return d;
  }
  return 0;
}

/**
 * The notes without what only matters on the download page: the title (the
 * version is shown already), the install instructions above the first rule,
 * and the checksums at the end.
 */
function tidy(text: string): string {
  let s = text.replace(/\r\n/g, "\n").replace(/^# .*\n+/, "");
  const rule = s.search(/^---$/m);
  if (rule !== -1 && rule < s.length / 2) s = s.slice(rule + 3);
  const sums = s.search(/^\*\*SHA-256\*\*/m);
  if (sums !== -1) s = s.slice(0, sums);
  return s.trim();
}

export const RELEASES: Release[] = Object.entries(FILES)
  .map(([path, text]) => ({ version: path.match(/release-([\d.]+)\.md$/)?.[1] ?? "?", body: tidy(text) }))
  .sort((a, b) => newer(a.version, b.version));
