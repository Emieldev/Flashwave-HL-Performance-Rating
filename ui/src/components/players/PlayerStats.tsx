import { useState } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { Rank } from "../../api/types";
import { rating } from "../../lib/format";
import { ClassIcon } from "../ClassIcon";
import { classLabel } from "../analysis/common";
import { t, tx } from "../../lib/i18n";
import { DivisionBadge } from "./PlayerProfile";
import "../rating/rating.css";

/**
 * Ratings for everyone (Q36, Flashy): a player's rating per class on the
 * league scale, the HLTV-style stat bars -- each component group as a
 * percentile against the league on their class -- and their ranks.
 */

const GROUP_LABEL: Record<string, string> = {
  fragging: "Kills and damage",
  survival: "Staying alive",
  teamplay: "Playing for the team",
  objective: "The objective",
  medic: "Medic",
  speciality: "Class speciality",
};

export function PlayerStatsCard({ accountId }: { accountId: number }) {
  const q = useQuery({ queryKey: ["player_stats", accountId], queryFn: () => api.getPlayerStats(accountId) });
  const [picked, setPicked] = useState<string | null>(null);
  if (q.isPending) return <p className="hint">{t("Loading…")}</p>;
  if (q.isError || !q.data || q.data.classes.length === 0) {
    return <p className="hint">{t("None of their games are in the logs held, so there is no rating yet.")}</p>;
  }
  const s = q.data;
  const c = s.classes.find((x) => x.class === picked) ?? s.classes[0];
  const headline = c.recent ?? c.career;
  return (
    <div className="ps-card">
      <div className="ps-classes" role="tablist">
        {s.classes.slice(0, 6).map((x) => (
          <button key={x.class} role="tab" aria-selected={x.class === c.class} className={x.class === c.class ? "ps-class on" : "ps-class"} onClick={() => setPicked(x.class)} title={classLabel(x.class)}>
            <ClassIcon cls={x.class} size={18} />
            <span>{x.games}</span>
          </button>
        ))}
      </div>
      <div className="ps-rating">
        <span className="ps-rating-label">
          {classLabel(c.class)} · {c.recent !== null ? t("last 3 months played") : t("career")}
        </span>
        <span className="ps-rating-value">{rating(headline)}</span>
      </div>
      <p className="hint ps-sub">
        {c.recent !== null
          ? tx("{0} recent games · career {1} over {2} · best {3}", { "0": c.recentGames, "1": rating(c.career), "2": c.games, "3": rating(c.best) })
          : tx("{0} games · best {1}", { "0": c.games, "1": rating(c.best) })}
      </p>
      <ul className="ps-bars">
        {c.groups.map(([g, v]) => (
          <li key={g}>
            <span className="ps-bar-label">{t(GROUP_LABEL[g] ?? g)}</span>
            <span className="ps-bar-value">
              {Math.round(v)}
              <span className="muted">/100</span>
            </span>
            <span className="ps-bar-track" aria-hidden>
              <i className={`rg-g-${g}`} style={{ width: `${Math.max(2, v)}%` }} />
            </span>
          </li>
        ))}
      </ul>
      <p className="hint ps-foot">{t("Each bar: how their games compare with every game on the class in the league, 0 to 100. The parts are on the How ratings work page.")}</p>
      {s.ranks.length > 0 && (
        <div className="ps-ranks">
          <h4>{t("Ranks")}</h4>
          <ul>
            {s.ranks.slice(0, 6).map((r) => (
              <li key={`${r.season}-${r.class}`}>
                <RankChip r={r} />
                <span className="muted">
                  {tx("{0} over {1} games", { "0": rating(r.avg), "1": r.games })}
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

export function RankChip({ r }: { r: Rank }) {
  return (
    <span className="ps-rank" title={tx("{0} of {1} ranked {2}s in {3}, Season {4}, with {5} or more games", { "0": r.rank, "1": r.of, "2": classLabel(r.class), "3": r.division, "4": r.season, "5": 8 }) as string}>
      <strong>#{r.rank}</strong> <ClassIcon cls={r.class} size={16} /> {t("in")} <DivisionBadge d={{ name: r.division, tier: r.tier }} /> <span className="muted">S{r.season}</span>
    </span>
  );
}

const CLASSES = ["scout", "soldier", "pyro", "demoman", "heavy", "engineer", "medic", "sniper", "spy"];

/** A season's top players of a class in a division. */
export function TopPlayers({ onPick }: { onPick: (accountId: number) => void }) {
  const [cls, setCls] = useState("sniper");
  const [tier, setTier] = useState<number | null>(null);
  const [season, setSeason] = useState<number | null>(null);
  const q = useQuery({ queryKey: ["rankings", season, tier, cls], queryFn: () => api.getRankings(season, tier, cls), placeholderData: keepPreviousData });
  const r = q.data;
  return (
    <div className="panel top-players">
      <h2>{t("Top players")}</h2>
      <p className="hint" style={{ marginTop: 6 }}>{tx("By average rating in one season, on one class, in the division they played. {0} or more games to be ranked.", { "0": 8 })}</p>
      <div className="tp-controls">
        <div className="ps-classes">
          {CLASSES.map((c) => (
            <button key={c} className={c === cls ? "ps-class on" : "ps-class"} onClick={() => setCls(c)} title={classLabel(c)} aria-pressed={c === cls}>
              <ClassIcon cls={c} size={18} />
            </button>
          ))}
        </div>
        {r && (
          <>
            <select value={r.tier} onChange={(e) => setTier(Number(e.target.value))} aria-label={t("Division")}>
              {r.divisions.map(([tr, name]) => (
                <option key={tr} value={tr}>
                  {name}
                </option>
              ))}
            </select>
            <select value={r.season} onChange={(e) => { setSeason(Number(e.target.value)); setTier(null); }} aria-label={t("Season")}>
              {r.seasons.map(([s, name]) => (
                <option key={s} value={s}>
                  {tx("S{0} · {1}", { "0": s, "1": name })}
                </option>
              ))}
            </select>
          </>
        )}
      </div>
      {r && r.rows.length === 0 && <p className="hint">{t("Nobody on this class played enough rated games in this division and season.")}</p>}
      {r && r.rows.length > 0 && (
        <table className="pp-seasons tp-table">
          <thead>
            <tr>
              <th>#</th>
              <th>{t("Player")}</th>
              <th>{t("Team")}</th>
              <th>{t("Games")}</th>
              <th>{t("Rating")}</th>
            </tr>
          </thead>
          <tbody>
            {r.rows.slice(0, 25).map((x) => (
              <tr key={x.accountId} className="tp-row" onClick={() => onPick(x.accountId)}>
                <td>{x.rank}</td>
                <td>
                  <strong>{x.name}</strong>
                </td>
                <td className="muted">{x.team?.name}</td>
                <td>{x.games}</td>
                <td>
                  <strong>{rating(x.avg)}</strong>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
