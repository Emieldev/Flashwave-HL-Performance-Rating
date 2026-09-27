import type { ContextKind, MatchContext } from "../api/types";
import { t } from "../lib/i18n";

export const KIND_LABEL: Record<ContextKind, string> = {
  official: "Official",
  scrim: "Scrim",
  pug: "Pug",
};

export const KIND_PLURAL: Record<ContextKind, string> = {
  official: "Officials",
  scrim: "Scrims",
  pug: "Pugs",
};

/** Why a match got its kind, for a hover title. */
export function kindReason(c: MatchContext): string {
  switch (c.kind) {
    case "official":
      return c.linkMethod === "roster"
        ? t("ETF2L official, found by matching both teams' rosters (trends.tf had not tagged it)")
        : t("ETF2L official, tagged by trends.tf");
    case "scrim":
      return c.regulars >= 5
        ? t("Team game: {n} of your teammates played with you regularly around then", { n: c.regulars })
        : t("Team game: most of your side was on your ETF2L roster");
    case "pug":
      return c.regulars === 1
        ? t("Pug or lobby: 1 regular teammate on your side")
        : t("Pug or lobby: {n} regular teammates on your side", { n: c.regulars });
  }
}

/** Short division label: "High", "Div 2", "Open". */
export function divisionLabel(division: string | null): string | null {
  if (!division) return null;
  return division.replace(/^Division\s+/i, "Div ");
}

export function ContextBadge({ c }: { c: MatchContext }) {
  const div = c.kind === "official" ? divisionLabel(c.official?.division ?? null) : null;
  return (
    <span className={`badge badge-${c.kind}`} title={kindReason(c)}>
      {c.kind === "official" ? t("ETF2L") : KIND_LABEL[c.kind].toUpperCase()}
      {div && <span className="badge-sub"> · {div}</span>}
    </span>
  );
}
