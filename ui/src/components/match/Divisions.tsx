import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { MatchDetail, MatchDivisions, Team } from "../../api/types";
import { teamLabel } from "../../lib/format";
import { t } from "../../lib/i18n";

/**
 * The division of the people you play (Q38, Flashy): every player's ETF2L
 * division at the time of the match -- the season it was in, or their
 * nearest season within a year, shown faded -- and each side's average.
 */
export function useMatchDivisions(logId: number) {
  return useQuery({ queryKey: ["match_divisions", logId], queryFn: () => api.getMatchDivisions(logId), staleTime: 5 * 60_000 }).data;
}

const SHORT: Record<string, string> = { Premiership: "Prem" };
const short = (d: string) => SHORT[d] ?? d.replace("Division ", "Div ");

/** A small division tag beside a name. */
export function DivTag({ divisions, accountId }: { divisions: MatchDivisions | undefined; accountId: number }) {
  const p = divisions?.players[accountId];
  if (!p) return null;
  return (
    <span
      className={`div-badge div-t${Math.min(p.tier, 4)} div-small${p.exact ? "" : " div-near"}`}
      title={(() => {
        // "Season 34 (Summer 2025)", or a season between two numbered ones
        // by its name alone ("AFA 2025", kept as 134 inside).
        const when = p.season >= 100 ? (p.seasonName ?? "AFA") : p.seasonName ? `${t("Season {0}", { "0": p.season })} (${p.seasonName})` : t("Season {0}", { "0": p.season });
        return p.exact ? t("{0} in {1}, when this was played", { "0": p.division, "1": when }) : t("{0} in {1}: the nearest season they played, not the one this was in", { "0": p.division, "1": when });
      })()}
    >
      {short(p.division)}
    </span>
  );
}

/** Each side's average division: "RED ~High (1.4) · BLU ~Mid (2.1)". */
export function SideDivisions({ d, divisions }: { d: MatchDetail; divisions: MatchDivisions | undefined }) {
  if (!divisions) return null;
  const sides: Team[] = [d.leftTeam, d.leftTeam === "Red" ? "Blue" : "Red"];
  const parts = sides
    .map((team) => {
      const tiers = d.players.filter((p) => p.team === team).map((p) => divisions.players[p.accountId]?.tier).filter((x): x is number => x !== undefined);
      if (tiers.length < 3) return null;
      const avg = tiers.reduce((a, b) => a + b, 0) / tiers.length;
      const name = divisions.tierNames[Math.round(avg)] ?? "";
      return { team, avg, name, known: tiers.length, of: d.players.filter((p) => p.team === team).length };
    })
    .filter((x) => x !== null);
  if (parts.length === 0) return null;
  return (
    <p className="hint side-divisions" title={t("Tiers: 0 Premiership, 1 High, 2 Mid, 3 Low, 4 Open. From the ETF2L officials read; players with none are left out.")}>
      {t("Divisions")}:{" "}
      {parts.map((x, i) => (
        <span key={x.team}>
          {i > 0 && " · "}
          <span className={`team-${x.team.toLowerCase()}`}>{teamLabel(x.team)}</span> ~{short(x.name)}{" "}
          <span className="muted">
            ({x.avg.toFixed(1)}, {x.known}/{x.of})
          </span>
        </span>
      ))}
    </p>
  );
}
