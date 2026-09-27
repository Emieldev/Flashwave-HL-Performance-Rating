import { useMemo, useState } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type TeamEra, type Teammate } from "../../api/types";
import { capitalize, formatDate, signed } from "../../lib/format";
import "./teammates.css";
import { t as tr, tx, k } from "../../lib/i18n";

type SortKey = "games" | "officials" | "winRate" | "lastPlayed" | "delta";

const SORTS: Array<{ key: SortKey; label: string; title: string; num?: boolean }> = [
  { key: "games", label: k("Games"), title: k("Games on the same side"), num: true },
  { key: "officials", label: k("Officials"), title: k("ETF2L officials together"), num: true },
  { key: "winRate", label: k("Record"), title: k("Wins and losses together; ties left out"), num: true },
  { key: "lastPlayed", label: k("Together"), title: k("First and last game together") },
  { key: "delta", label: k("Your rating with them"), title: k("Your average rating in games with them, and the difference from your other games"), num: true },
];

const winRate = (m: { wins: number; losses: number }) =>
  m.wins + m.losses === 0 ? null : (m.wins / (m.wins + m.losses)) * 100;

/** "Jul 2023". */
function month(unix: number): string {
  return new Date(unix * 1000).toLocaleDateString(undefined, { month: "short", year: "numeric" });
}

export function TeammatesPage() {
  const [all, setAll] = useState(false);
  const [currentOnly, setCurrentOnly] = useState(false);
  const [sort, setSort] = useState<SortKey>("games");
  const q = useQuery({
    queryKey: ["teammates", all],
    queryFn: () => api.getTeammates(all),
    placeholderData: keepPreviousData,
  });

  const rows = useMemo(() => {
    const list = (q.data?.teammates ?? []).filter((m) => !currentOnly || m.current);
    const key = (m: Teammate): number => {
      switch (sort) {
        case "games":
          return m.games;
        case "officials":
          return m.officials;
        case "winRate":
          return winRate(m) ?? -1;
        case "lastPlayed":
          return m.lastPlayed;
        case "delta":
          return m.myAvgDelta ?? -Infinity;
      }
    };
    return [...list].sort((a, b) => key(b) - key(a) || b.games - a.games);
  }, [q.data, currentOnly, sort]);

  if (q.isPending) return <div className="mates-page"><p className="hint">{tr("Loading teammates…")}</p></div>;
  if (q.isError) return <div className="mates-page"><p className="error">{errorMessage(q.error)}</p></div>;
  const d = q.data;

  return (
    <div className={q.isPlaceholderData ? "mates-page refetching" : "mates-page"}>
      <header className="mates-head">
        <div>
          <h2>{tr("Teams and teammates")}</h2>
          <p className="hint">{tx("From {0} {1}.", { "0": d.games.toLocaleString(), "1": all ? tr("games, pugs included") : tr("officials and scrims") })}</p>
        </div>
        <div className="segmented" role="tablist" aria-label={tr("Which games")}>
          <button role="tab" aria-selected={!all} className={!all ? "seg active" : "seg"} onClick={() => setAll(false)}>{tr("Officials and scrims")}</button>
          <button role="tab" aria-selected={all} className={all ? "seg active" : "seg"} onClick={() => setAll(true)}>{tr("Including pugs")}</button>
        </div>
      </header>

      {d.teams.length > 0 && (
        <section className="team-grid" aria-label={tr("Your ETF2L teams")}>
          {d.teams.map((t) => (
            <TeamCard key={t.teamId} t={t} />
          ))}
        </section>
      )}

      <section className="panel">
        <header className="mates-table-head">
          <div>
            <h2>{tr("Regular teammates")}</h2>
            <p className="hint">{tx("Everyone with {minGames} or more games on your side. \"Your rating with them\" compares your games together with your other games: it shows who you played well alongside, not who made you play well.", { minGames: d.minGames })}</p>
          </div>
          <label className="check">
            <input type="checkbox" checked={currentOnly} onChange={(e) => setCurrentOnly(e.target.checked)} />{tr("Played together in the last two months")}</label>
        </header>

        {rows.length === 0 ? (
          <p className="hint" style={{ marginTop: 12 }}>
            {currentOnly ? tr("Nobody recent yet.") : tr("No regular teammates yet. Sync to pull your history.")}
          </p>
        ) : (
          <div className="table-wrap" style={{ marginTop: 14 }}>
            <table className="match-table mates-table">
              <thead>
                <tr>
                  <th>{tr("Player")}</th>
                  <th>{tr("Class")}</th>
                  {SORTS.map((s) => (
                    <th key={s.key} className={s.num ? "num" : undefined} title={tr(s.title)}>
                      <button
                        className={sort === s.key ? "sort active" : "sort"}
                        onClick={() => setSort(s.key)}
                        aria-pressed={sort === s.key}
                      >
                        {tr(s.label)}
                        {sort === s.key && " ↓"}
                      </button>
                    </th>
                  ))}
                  <th>{tr("Teams")}</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((m) => (
                  <MateRow key={m.accountId} m={m} />
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </div>
  );
}

function TeamCard({ t }: { t: TeamEra }) {
  const wr = winRate(t);
  const span = t.firstPlayed === t.lastPlayed || month(t.firstPlayed) === month(t.lastPlayed)
    ? month(t.lastPlayed)
    : `${month(t.firstPlayed)} – ${month(t.lastPlayed)}`;
  return (
    <article className="panel team-card">
      <header>
        <h3>{t.name || tr("Unnamed team")}</h3>
        <span className="muted">{span}</span>
      </header>
      <dl className="team-stats">
        <div>
          <dt>{tr("Games")}</dt>
          <dd>
            {t.games}
            {t.officials > 0 && <span className="muted">{" "}{tx("· {officials} official", { officials: t.officials })}</span>}
          </dd>
        </div>
        <div>
          <dt>{tr("Record")}</dt>
          <dd>
            {t.wins}–{t.losses}
            {wr !== null && <span className="muted"> · {wr.toFixed(0)}%</span>}
          </dd>
        </div>
        <div>
          <dt>{tr("Your rating")}</dt>
          <dd>{t.myAvg === null ? <span className="muted">—</span> : t.myAvg.toFixed(0)}</dd>
        </div>
      </dl>
      <ul className="core" aria-label={tr("Most frequent teammates")}>
        {t.core.map((c) => (
          <li key={c.accountId} title={tr("{games} games together", { games: c.games })}>
            <span className="core-name">{c.name}</span>
            {c.mainClass && <span className="muted"> {c.mainClass}</span>}
          </li>
        ))}
      </ul>
    </article>
  );
}

function MateRow({ m }: { m: Teammate }) {
  const wr = winRate(m);
  const open = () => void api.openExternal(`https://logs.tf/profile/${m.steamid64}`);
  return (
    <tr
      className="clickable"
      tabIndex={0}
      title={tr("Open their logs.tf profile")}
      onClick={open}
      onKeyDown={(e) => e.key === "Enter" && open()}
    >
      <td className="nowrap mate-name">
        {m.current && <span className="dot ok" title={tr("Played together in the last two months")} />}
        {m.name}
      </td>
      <td className="nowrap">{m.mainClass ? capitalize(m.mainClass) : <span className="muted">—</span>}</td>
      <td className="num">{m.games}</td>
      <td className="num">{m.officials || <span className="muted">–</span>}</td>
      <td className="num nowrap">
        {m.wins}–{m.losses}
        {wr !== null && <span className="muted"> {wr.toFixed(0)}%</span>}
      </td>
      <td className="muted nowrap">
        {formatDate(m.firstPlayed, true)} – {formatDate(m.lastPlayed, true)}
      </td>
      <td className="num nowrap">
        {m.myAvgWith === null ? (
          <span className="muted" title={tr("Too few rated games to compare")}>—</span>
        ) : (
          <>
            {m.myAvgWith.toFixed(0)}{" "}
            <span className={m.myAvgDelta! >= 0 ? "delta up" : "delta down"}>({signed(m.myAvgDelta!)})</span>
          </>
        )}
      </td>
      <td className="muted mate-teams">{m.teams.join(", ")}</td>
    </tr>
  );
}
