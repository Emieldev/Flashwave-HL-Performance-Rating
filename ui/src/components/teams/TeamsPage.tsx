import { useState } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type LeagueRecord, type TeamView } from "../../api/types";
import { capitalize, formatDate, rating } from "../../lib/format";
import { t, tx } from "../../lib/i18n";
import "./teams.css";

/**
 * Q29 (Flashy): ETF2L Highlander, every team, a year back. The season's
 * tables, and a page per team: record, win rate on each map of the pool,
 * results and who played. All from ETF2L; ratings are the pool's.
 */
export function TeamsPage() {
  const [season, setSeason] = useState<number | undefined>(undefined);
  const [team, setTeam] = useState<number | null>(null);
  const q = useQuery({ queryKey: ["leagues", season ?? null], queryFn: () => api.getLeagues(season), placeholderData: keepPreviousData });

  if (team !== null) return <TeamPage teamId={team} onBack={() => setTeam(null)} onTeam={setTeam} />;
  if (q.isPending) return <div className="teams-page"><p className="hint">{t("Loading teams…")}</p></div>;
  if (q.isError) return <div className="teams-page"><p className="error">{errorMessage(q.error)}</p></div>;
  const v = q.data;

  if (!v.season) {
    return (
      <div className="teams-page">
        <section className="panel">
          <h2>{t("Teams")}</h2>
          <p className="hint" style={{ marginTop: 6 }}>{t("No seasons yet. The next sync reads the last year of ETF2L Highlander: every division, team and result.")}</p>
        </section>
      </div>
    );
  }
  const s = v.season;

  return (
    <div className="teams-page">
      <section className="panel">
        <header className="teams-head">
          <div>
            <h2>{t("Highlander Season {0}", { "0": s.season })}</h2>
            <p className="hint">{s.name}</p>
          </div>
          <select value={s.season} onChange={(e) => setSeason(Number(e.target.value))} aria-label={t("Season")}>
            {v.seasons.map((x) => (
              <option key={x.season} value={x.season}>
                {t("Season {0} ({1})", { "0": x.season, "1": x.name })}
              </option>
            ))}
          </select>
        </header>
        {s.pool.length > 0 && (
          <p className="hint teams-pool">
            {tx("Map pool: {0}", { "0": s.pool.map((m) => <code key={m}>{m}</code>) })}
          </p>
        )}
        {v.pendingDetails > 0 && (
          <p className="hint">{t("{0} matches still to be read in detail; the per-map numbers fill in over the next syncs.", { "0": v.pendingDetails })}</p>
        )}
      </section>

      <div className="teams-divisions">
        {v.divisions.map((d) => (
          <section key={d.division} className="panel">
            <h2>{d.division}</h2>
            <div className="table-wrap">
              <table className="match-table teams-table">
                <thead>
                  <tr>
                    <th className="num">#</th>
                    <th>{t("Team")}</th>
                    <th className="num">{t("Played")}</th>
                    <th className="num">{t("W-L")}</th>
                    <th className="num">{t("Score")}</th>
                  </tr>
                </thead>
                <tbody>
                  {d.teams.map((row, i) => (
                    <tr key={row.teamId} className="teams-row" onClick={() => setTeam(row.teamId)} title={t("Open {0}", { "0": row.name })}>
                      <td className="num muted">{i + 1}</td>
                      <td>
                        <span className="teams-name">
                          {row.avatar ? <img src={row.avatar} alt="" className="teams-avatar" /> : <span className="teams-avatar" />}
                          {row.name}
                        </span>
                      </td>
                      <td className="num">{row.record.played}</td>
                      <td className="num">{wl(row.record)}</td>
                      <td className="num">{row.scoreFor}:{row.scoreAgainst}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>
        ))}
      </div>
    </div>
  );
}

function wl(r: LeagueRecord): string {
  return r.drawn > 0 ? `${r.won}-${r.lost}-${r.drawn}` : `${r.won}-${r.lost}`;
}

function pct(r: LeagueRecord): number | null {
  return r.played > 0 ? Math.round((100 * r.won) / r.played) : null;
}

function TeamPage({ teamId, onBack, onTeam }: { teamId: number; onBack: () => void; onTeam: (id: number) => void }) {
  const q = useQuery({ queryKey: ["team", teamId], queryFn: () => api.getTeam(teamId) });
  return (
    <div className="teams-page">
      <button className="linkish back" onClick={onBack}>{t("← All teams")}</button>
      {q.isPending && <p className="hint">{t("Loading team…")}</p>}
      {q.isError && <p className="error">{errorMessage(q.error)}</p>}
      {q.data === null && <p className="hint">{t("This team is not stored.")}</p>}
      {q.data && <Team v={q.data} onTeam={onTeam} />}
    </div>
  );
}

function Team({ v, onTeam }: { v: TeamView; onTeam: (id: number) => void }) {
  const inPool = v.maps.filter((m) => m.inPool);
  const other = v.maps.filter((m) => !m.inPool);
  const rated = v.roster.filter((r) => r.rating !== null);
  return (
    <>
      <section className="panel team-head">
        {v.avatar ? <img src={v.avatar} alt="" className="team-avatar" /> : <span className="team-avatar" />}
        <div>
          <h2>{v.name}</h2>
          <p className="hint">
            {[v.country, v.seasons.map(([s, d]) => t("S{0} {1}", { "0": s, "1": d })).join(" · ")].filter(Boolean).join(" · ")}
          </p>
        </div>
        <div className="team-record">
          <strong>{wl(v.record)}</strong>
          <span className="hint">{pct(v.record) === null ? "" : t("{0}% won", { "0": pct(v.record) ?? 0 })}</span>
        </div>
      </section>

      <section className="panel">
        <h2>{t("By map")}</h2>
        <p className="hint">{t("Maps won and lost in officials. Maps of the current pool first.")}</p>
        <div className="team-maps">
          {[...inPool, ...other].map((m) => {
            const p = pct(m.record);
            return (
              <div key={m.map} className={m.inPool ? "team-map" : "team-map muted"}>
                <div className="team-map-head">
                  <code>{m.map}</code>
                  <span className="num">{wl(m.record)}</span>
                </div>
                <div className="team-map-bar" title={t("{0}% of {1} maps won", { "0": p ?? 0, "1": m.record.played })}>
                  <div className="team-map-fill" style={{ width: `${p ?? 0}%` }} />
                </div>
                <span className="hint">
                  {p === null ? "–" : t("{0}% won", { "0": p })}
                  {m.roundsFor + m.roundsAgainst > 0 && t(" · rounds {0}:{1}", { "0": m.roundsFor, "1": m.roundsAgainst })}
                </span>
              </div>
            );
          })}
        </div>
      </section>

      <div className="team-cols">
        <section className="panel">
          <h2>{t("Players")}</h2>
          <p className="hint">
            {rated.length > 0
              ? t("Ratings are from your match pool: the games of theirs you have played in or against.")
              : t("None of them is in your match pool yet, so there are no ratings to show.")}
          </p>
          <div className="table-wrap">
            <table className="match-table">
              <thead>
                <tr>
                  <th>{t("Player")}</th>
                  <th className="num">{t("Officials")}</th>
                  <th>{t("Class")}</th>
                  <th className="num">{t("Rating")}</th>
                </tr>
              </thead>
              <tbody>
                {v.roster.map((r) => (
                  <tr key={r.accountId}>
                    <td className="nowrap">{r.name}</td>
                    <td className="num">{r.matches}</td>
                    <td>{r.class ? capitalize(r.class) : <span className="muted">–</span>}</td>
                    <td className="num" title={r.rating !== null ? t("{0} rated games", { "0": r.games }) : undefined}>
                      {r.rating !== null ? rating(r.rating) : <span className="muted">–</span>}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>

        <section className="panel">
          <h2>{t("Results")}</h2>
          <div className="table-wrap">
            <table className="match-table">
              <thead>
                <tr>
                  <th>{t("Date")}</th>
                  <th>{t("Against")}</th>
                  <th>{t("Map")}</th>
                  <th className="num">{t("Score")}</th>
                </tr>
              </thead>
              <tbody>
                {v.results.map((r) => {
                  const won = r.scoreFor !== null && r.scoreAgainst !== null && r.scoreFor > r.scoreAgainst;
                  const lost = r.scoreFor !== null && r.scoreAgainst !== null && r.scoreFor < r.scoreAgainst;
                  return (
                    <tr key={r.matchId}>
                      <td className="nowrap muted">{r.time ? formatDate(r.time) : "–"}</td>
                      <td>
                        <button className="linkish" onClick={() => onTeam(r.opponentId)}>{r.opponent}</button>
                        {r.stage !== "regular" && <span className="badge">{r.stage}</span>}
                      </td>
                      <td className="nowrap">{r.maps[0] ?? "–"}</td>
                      <td className={won ? "num team-won" : lost ? "num team-lost" : "num"}>
                        {r.defaultWin ? t("default") : `${r.scoreFor ?? "–"}:${r.scoreAgainst ?? "–"}`}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </section>
      </div>
    </>
  );
}
