import { useState } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type LeagueRecord, type Podium, type SeasonTile, type TeamHonours, type TeamView } from "../../api/types";
import { formatDate, rating } from "../../lib/format";
import { openPlayer } from "../../lib/goto";
import { t, tx } from "../../lib/i18n";
import { ClassIcon } from "../ClassIcon";
import { Country } from "../Country";
import { classLabel } from "../analysis/common";
import { MedalGlyph } from "../players/MedalGlyph";
import { DivisionBadge, medalTitles, seasonLong, seasonShort } from "../players/PlayerProfile";
import "../players/players.css";
import "./teams.css";
import { YouOnThisTeam, YourTeamsStrip } from "./YourTeams";

/**
 * Teams (Q29, and Flashy's UX pass): every ETF2L Highlander season as a
 * tile -- its banner from ETF2L's news, who won it, how you did -- then a
 * season's podiums and division tables, then a team's own page, built like
 * a player's profile.
 */

type View = { kind: "seasons" } | { kind: "season"; season: number } | { kind: "team"; teamId: number; from: View };

const PLACE = ["gold", "silver", "bronze"] as const;

export function TeamsPage() {
  const [view, setView] = useState<View>({ kind: "seasons" });
  const openTeam = (teamId: number) => setView((from) => ({ kind: "team", teamId, from: from.kind === "team" ? from.from : from }));
  if (view.kind === "team") {
    return <TeamScreen teamId={view.teamId} onBack={() => setView(view.from)} onTeam={openTeam} backLabel={view.from.kind === "season" ? t("← Season") : t("← All seasons")} />;
  }
  if (view.kind === "season") {
    return <SeasonScreen season={view.season} onBack={() => setView({ kind: "seasons" })} onTeam={openTeam} />;
  }
  return <SeasonsGrid onSeason={(season) => setView({ kind: "season", season })} onTeam={openTeam} />;
}

// ---- 1. Every season as a tile ----------------------------------------

function SeasonsGrid({ onSeason, onTeam }: { onSeason: (season: number) => void; onTeam: (teamId: number) => void }) {
  const q = useQuery({ queryKey: ["seasons_overview"], queryFn: api.getSeasonsOverview, staleTime: 5 * 60_000 });
  if (q.isPending) return <div className="teams-page"><p className="hint">{t("Loading seasons…")}</p></div>;
  if (q.isError) return <div className="teams-page"><p className="error">{errorMessage(q.error)}</p></div>;
  if (q.data.length === 0) {
    return (
      <div className="teams-page">
        <section className="panel">
          <h2>{t("Teams")}</h2>
          <p className="hint" style={{ marginTop: 6 }}>{t("No seasons yet. The next sync reads the last year of ETF2L Highlander: every division, team and result.")}</p>
        </section>
      </div>
    );
  }
  return (
    <div className="teams-page">
      <YourTeamsStrip onTeam={onTeam} />
      <header className="ts-head">
        <h2>{t("ETF2L Highlander seasons")}</h2>
        <p className="hint">{t("Pick a season for its podiums, its divisions and every team in them.")}</p>
      </header>
      <div className="ts-grid">
        {q.data.map((s, i) => (
          <SeasonCard key={s.season} s={s} live={i === 0 && !s.champion} onOpen={() => onSeason(s.season)} />
        ))}
      </div>
    </div>
  );
}

function seasonTitle(s: { season: number; seasonName: string }): string {
  if (s.season >= 100) return s.seasonName;
  return s.seasonName && !s.seasonName.startsWith("Season") ? t("Season {0} · {1}", { "0": s.season, "1": s.seasonName }) : t("Season {0}", { "0": s.season });
}

function useBanner(s: { season: number; seasonName: string }) {
  return useQuery({ queryKey: ["season_banner", s.season], queryFn: () => api.getSeasonBanner(s.season, s.seasonName), staleTime: Infinity }).data ?? null;
}

function SeasonCard({ s, live, onOpen }: { s: SeasonTile; live: boolean; onOpen: () => void }) {
  const banner = useBanner(s);
  const top = s.divisions.slice(0, 7);
  return (
    <button className="ts-card" onClick={onOpen} title={t("Open {0}", { "0": seasonTitle(s) })}>
      <span className={banner ? "ts-banner" : "ts-banner ts-banner-none"} style={banner ? { backgroundImage: `url("${banner}")` } : undefined}>
        {!banner && <span className="ts-banner-label">{s.season >= 100 ? s.seasonName : `S${s.season}`}</span>}
        {live && <span className="ts-live">{t("Live")}</span>}
      </span>
      <span className="ts-body">
        <span className="ts-title">{seasonTitle(s)}</span>
        <span className="hint">
          {formatDate(s.from)} – {formatDate(s.to)} · {tx("{0} teams", { "0": s.teams })} · {tx("{0} officials", { "0": s.matches })}
        </span>
        <span className="ts-divs">
          {top.map((d) => (
            <DivisionBadge key={d.name} d={d} />
          ))}
        </span>
        {s.champion && (
          <span className="ts-champ">
            <MedalGlyph size={18} place={1} />
            <TeamAvatar src={s.champion.avatar} />
            <strong>{s.champion.name}</strong>
            <span className="muted">· {s.championDivision}</span>
          </span>
        )}
        {s.you && (
          <span className="ts-you">
            <span className="muted">{t("You")}:</span> {s.you.team.name} <DivisionBadge d={{ name: s.you.division, tier: s.you.tier ?? 4 }} />
            {s.you.place && <MedalGlyph size={16} place={s.you.place} />}
            {s.you.merc && <span className="muted"> · {t("merc")}</span>}
          </span>
        )}
      </span>
    </button>
  );
}

// ---- 2. One season: podiums, then the division tables ----------------

function SeasonScreen({ season, onBack, onTeam }: { season: number; onBack: () => void; onTeam: (id: number) => void }) {
  const tiles = useQuery({ queryKey: ["seasons_overview"], queryFn: api.getSeasonsOverview, staleTime: 5 * 60_000 });
  const tile = tiles.data?.find((s) => s.season === season);
  const tables = useQuery({ queryKey: ["leagues", season], queryFn: () => api.getLeagues(season), placeholderData: keepPreviousData });
  const podiums = useQuery({ queryKey: ["season_podiums", season], queryFn: () => api.getSeasonPodiums(season), staleTime: 5 * 60_000 });
  const banner = useBanner(tile ?? { season, seasonName: "" });
  const v = tables.data;
  return (
    <div className="teams-page">
      <button className="linkish back" onClick={onBack}>
        {t("← All seasons")}
      </button>
      <section className="panel ts-hero" style={banner ? { backgroundImage: `linear-gradient(90deg, var(--panel) 35%, transparent), url("${banner}")` } : undefined}>
        <h2>{tile ? seasonTitle(tile) : t("Season {0}", { "0": season })}</h2>
        {tile && (
          <p className="hint">
            {formatDate(tile.from)} – {formatDate(tile.to)} · {tx("{0} teams", { "0": tile.teams })} · {tx("{0} officials", { "0": tile.matches })}
          </p>
        )}
        {v?.season && v.season.pool.length > 0 && <p className="hint teams-pool">{tx("Map pool: {0}", { "0": v.season.pool.map((m) => <code key={m}>{m}</code>) })}</p>}
      </section>

      {podiums.isPending && <p className="hint">{t("Loading podiums…")}</p>}
      {podiums.data && podiums.data.length > 0 && (
        <section className="ts-podiums">
          {podiums.data.map((p) => (
            <PodiumCard key={p.division} p={p} onTeam={onTeam} />
          ))}
        </section>
      )}

      {tables.isError && <p className="error">{errorMessage(tables.error)}</p>}
      {v && v.divisions.length > 0 && (
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
                      <tr key={row.teamId} className="teams-row" onClick={() => onTeam(row.teamId)} title={t("Open {0}", { "0": row.name })}>
                        <td className="num muted">{i + 1}</td>
                        <td>
                          <span className="teams-name">
                            <TeamAvatar src={row.avatar} />
                            {row.name}
                          </span>
                        </td>
                        <td className="num">{row.record.played}</td>
                        <td className="num">{wl(row.record)}</td>
                        <td className="num">
                          {row.scoreFor}:{row.scoreAgainst}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </section>
          ))}
        </div>
      )}
      {v && v.pendingDetails > 0 && <p className="hint">{t("{0} matches still to be read in detail; the per-map numbers fill in over the next syncs.", { "0": v.pendingDetails })}</p>}
    </div>
  );
}

function PodiumCard({ p, onTeam }: { p: Podium; onTeam: (id: number) => void }) {
  return (
    <div className="panel ts-podium">
      <div className="ts-podium-head">
        <DivisionBadge d={{ name: p.division, tier: p.tier ?? 4 }} />
      </div>
      <ol className="ts-places">
        {p.medals.map((m, i) => (
          <li key={i} className={`pp-${PLACE[m.place - 1]}`} title={m.how}>
            <span className="pp-medal-tile ts-place-tile">
              <MedalGlyph size={20} place={m.place} />
            </span>
            <button className="linkish ts-team" onClick={() => onTeam(m.team.id)}>
              <TeamAvatar src={m.team.avatar} />
              {m.team.name}
            </button>
          </li>
        ))}
      </ol>
      {p.mvp && (
        <button className="linkish ts-mvp" onClick={() => openPlayer(p.mvp!.accountId)} title={t("Grand Final {0} over {1} logs", { "0": p.mvp.finalRating.toFixed(2), "1": p.mvp.finalMaps })}>
          <span className="pp-mvp-star" aria-hidden>
            ★
          </span>
          <span className="muted">{t("MVP")}</span> <ClassIcon cls={p.mvp.class} size={16} /> <strong>{p.mvpName ?? "?"}</strong>
          <span className="muted">{rating(p.mvp.score)}</span>
        </button>
      )}
    </div>
  );
}

// ---- 3. One team, like a player's profile ------------------------------

type TeamTab = "overview" | "roster" | "seasons" | "results";

function TeamScreen({ teamId, onBack, onTeam, backLabel }: { teamId: number; onBack: () => void; onTeam: (id: number) => void; backLabel: string }) {
  const q = useQuery({ queryKey: ["team", teamId], queryFn: () => api.getTeam(teamId) });
  const h = useQuery({ queryKey: ["team_honours", teamId], queryFn: () => api.getTeamHonours(teamId) });
  const [tab, setTab] = useState<TeamTab>("overview");
  return (
    <div className="teams-page">
      <button className="linkish back" onClick={onBack}>
        {backLabel}
      </button>
      {q.isPending && <p className="hint">{t("Loading team…")}</p>}
      {q.isError && <p className="error">{errorMessage(q.error)}</p>}
      {q.data === null && (
        <>
          <p className="hint">{t("This team's seasons are older than the ones read, so only your own games with it are here.")}</p>
          <YouOnThisTeam teamId={teamId} named />
        </>
      )}
      {q.data && (
        <>
          <TeamHeader v={q.data} honours={h.data} />
          <YouOnThisTeam teamId={teamId} />
          <div className="panel pp-body">
            <nav className="pp-tabs" role="tablist">
              {(
                [
                  ["overview", t("Overview")],
                  ["roster", tx("Roster ({0})", { "0": q.data.roster.length })],
                  ["seasons", t("Seasons")],
                  ["results", tx("Results ({0})", { "0": q.data.results.length })],
                ] as [TeamTab, React.ReactNode][]
              ).map(([id, label]) => (
                <button key={id} role="tab" aria-selected={tab === id} className={tab === id ? "pp-tab on" : "pp-tab"} onClick={() => setTab(id)}>
                  {label}
                </button>
              ))}
            </nav>
            {tab === "overview" && <TeamOverview v={q.data} onTeam={onTeam} onRoster={() => setTab("roster")} onResults={() => setTab("results")} />}
            {tab === "roster" && <Roster v={q.data} />}
            {tab === "seasons" && <Seasons honours={h.data} />}
            {tab === "results" && <Results v={q.data} onTeam={onTeam} />}
          </div>
        </>
      )}
    </div>
  );
}

function TeamHeader({ v, honours }: { v: TeamView; honours: TeamHonours | undefined }) {
  const latest = honours?.seasons[0];
  const titles = medalTitles(honours?.medals ?? []);
  const p = pct(v.record);
  return (
    <div className="panel pp-head">
      <div className="pp-top">
        <div className="pp-avatar">{v.avatar ? <img src={v.avatar} alt="" /> : <span>{v.name.slice(0, 1).toUpperCase()}</span>}</div>
        <div className="pp-who">
          <div className="pp-name-row">
            <h2 className="pp-name">{v.name}</h2>
            {latest && latest.tier !== null && <DivisionBadge d={{ name: latest.division, tier: latest.tier }} />}
          </div>
          <dl className="pp-facts">
            {v.country && (
              <>
                <dt>{t("Country")}</dt>
                <dd>
                  <Country raw={v.country} />
                </dd>
              </>
            )}
            {latest && (
              <>
                <dt>{t("Latest season")}</dt>
                <dd>
                  {seasonLong(latest.season, latest.seasonName)} · {latest.division} · {latest.won}–{latest.lost}
                </dd>
              </>
            )}
            <dt>{t("Seasons")}</dt>
            <dd>{honours ? honours.seasons.length : v.seasons.length}</dd>
          </dl>
          {titles.length > 0 && (
            <div className="pp-titles">
              {titles.map(([label, n, place]) => (
                <span key={label} className={`pp-title pp-${PLACE[place - 1]}`}>
                  <MedalGlyph size={18} place={place} />
                  {n}× {label}
                </span>
              ))}
            </div>
          )}
        </div>
        <div className="team-record">
          <strong>{wl(v.record)}</strong>
          <span className="hint">{p === null ? "" : t("{0}% won", { "0": p })}</span>
          <a href={`https://etf2l.org/teams/${v.teamId}/`} target="_blank" rel="noreferrer" className="ts-etf2l">
            {t("ETF2L ↗")}
          </a>
        </div>
      </div>
      {honours && honours.medals.length > 0 && (
        <div className="pp-trophies" aria-label={t("Medals")}>
          {honours.medals.map((m, i) => (
            <span key={i} className={`pp-medal pp-${PLACE[m.place - 1]}`} title={`${m.division} · ${seasonLong(m.season, m.seasonName)} · ${m.how}`}>
              <span className="pp-medal-tile">
                <MedalGlyph size={38} place={m.place} />
              </span>
              <span className="pp-medal-label">
                {seasonShort(m.season, m.seasonName)} {m.division.replace("Division ", "Div ")}
              </span>
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

function TeamOverview({ v, onTeam, onRoster, onResults }: { v: TeamView; onTeam: (id: number) => void; onRoster: () => void; onResults: () => void }) {
  const regulars = v.roster.filter((r) => r.class).slice(0, 9);
  const maps = v.maps.filter((m) => m.inPool);
  return (
    <div className="pp-overview">
      <section>
        <h3>{t("Recent officials")}</h3>
        <ul className="pp-matches">
          {v.results.slice(0, 8).map((r) => {
            const won = r.scoreFor !== null && r.scoreAgainst !== null && r.scoreFor > r.scoreAgainst;
            const lost = r.scoreFor !== null && r.scoreAgainst !== null && r.scoreFor < r.scoreAgainst;
            return (
              <li key={r.matchId} className={won ? "won" : lost ? "lost" : undefined}>
                <span className="pp-vs">
                  {t("vs")}{" "}
                  <button className="linkish" onClick={() => onTeam(r.opponentId)}>
                    <strong>{r.opponent}</strong>
                  </button>
                </span>
                <span className="muted pp-where">
                  {seasonShort(r.season)} {r.division}
                  {r.stage !== "regular" ? ` · ${r.round ?? r.stage}` : ""}
                  {r.time ? ` · ${formatDate(r.time)}` : ""}
                  {r.maps[0] ? ` · ${r.maps[0]}` : ""}
                </span>
                <span className="pp-score">{r.defaultWin ? t("default") : `${r.scoreFor ?? "–"} : ${r.scoreAgainst ?? "–"}`}</span>
              </li>
            );
          })}
        </ul>
        {v.results.length > 8 && (
          <button className="linkish ts-more" onClick={onResults}>
            {tx("All {0} results →", { "0": v.results.length })}
          </button>
        )}
      </section>
      <section>
        <h3>{t("Lineup")}</h3>
        {regulars.length === 0 && <p className="hint">{t("None of this team's officials have been downloaded yet, so there are no classes or ratings to show.")}</p>}
        <ul className="ts-lineup">
          {regulars.map((r) => (
            <li key={r.accountId}>
              {r.class && <ClassIcon cls={r.class} size={20} />}
              <button className="linkish" onClick={() => openPlayer(r.accountId)}>
                {r.name}
              </button>
              <span className="muted">{tx("{0} officials", { "0": r.matches })}</span>
              <span className="ts-lineup-rating">{r.rating !== null ? rating(r.rating) : "–"}</span>
            </li>
          ))}
        </ul>
        {v.roster.length > regulars.length && (
          <button className="linkish ts-more" onClick={onRoster}>
            {tx("Whole roster ({0}) →", { "0": v.roster.length })}
          </button>
        )}
        {maps.length > 0 && (
          <>
            <h3 className="ts-maps-title">{t("Current map pool")}</h3>
            <MapBars maps={maps} />
          </>
        )}
      </section>
    </div>
  );
}

function MapBars({ maps }: { maps: TeamView["maps"] }) {
  return (
    <div className="team-maps">
      {maps.map((m) => {
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
  );
}

function Roster({ v }: { v: TeamView }) {
  return (
    <>
      <p className="hint">{t("Class: what they played for this team in its officials. Rating: their average on it in those officials.")}</p>
      <div className="table-wrap">
        <table className="match-table">
          <thead>
            <tr>
              <th>{t("Player")}</th>
              <th>{t("Class")}</th>
              <th className="num">{t("Officials")}</th>
              <th className="num">{t("Last played")}</th>
              <th className="num">{t("Rating")}</th>
            </tr>
          </thead>
          <tbody>
            {v.roster.map((r) => (
              <tr key={r.accountId} className="teams-row" onClick={() => openPlayer(r.accountId)} title={t("Open {0}'s profile", { "0": r.name })}>
                <td className="nowrap">{r.name}</td>
                <td className="nowrap">
                  {r.class ? (
                    <span className="ts-class">
                      <ClassIcon cls={r.class} size={16} /> {classLabel(r.class)}
                    </span>
                  ) : (
                    <span className="muted">–</span>
                  )}
                </td>
                <td className="num">{r.matches}</td>
                <td className="num muted">{r.lastPlayed ? formatDate(r.lastPlayed) : "–"}</td>
                <td className="num" title={r.rating !== null ? t("{0} rated officials", { "0": r.games }) : undefined}>
                  {r.rating !== null ? rating(r.rating) : <span className="muted">–</span>}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <h3 className="ts-maps-title">{t("By map")}</h3>
      <p className="hint">{t("Maps won and lost in officials. Maps of the current pool first.")}</p>
      <MapBars maps={[...v.maps.filter((m) => m.inPool), ...v.maps.filter((m) => !m.inPool)]} />
    </>
  );
}

function Seasons({ honours }: { honours: TeamHonours | undefined }) {
  if (!honours) return <p className="hint">{t("Loading…")}</p>;
  return (
    <table className="pp-seasons">
      <thead>
        <tr>
          <th>{t("Season")}</th>
          <th>{t("Division")}</th>
          <th className="num">{t("W–L")}</th>
          <th>{t("Finish")}</th>
        </tr>
      </thead>
      <tbody>
        {honours.seasons.map((s) => (
          <tr key={s.season}>
            <td>{s.season >= 100 ? s.seasonName : <>S{s.season} <span className="muted">{s.seasonName}</span></>}</td>
            <td>{s.tier !== null ? <DivisionBadge d={{ name: s.division, tier: s.tier }} /> : s.division}</td>
            <td className="num">
              {s.won}–{s.lost}
            </td>
            <td>
              {s.place ? (
                <span className={`pp-place pp-${PLACE[s.place - 1]}`}>
                  <MedalGlyph size={16} place={s.place} /> {s.place === 1 ? t("Winner") : s.place === 2 ? t("Runner-up") : t("Third")}
                </span>
              ) : (
                <span className="muted">–</span>
              )}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function Results({ v, onTeam }: { v: TeamView; onTeam: (id: number) => void }) {
  return (
    <div className="table-wrap">
      <table className="match-table">
        <thead>
          <tr>
            <th>{t("Date")}</th>
            <th>{t("Season")}</th>
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
                <td className="nowrap muted">
                  {seasonShort(r.season)} {r.division}
                </td>
                <td>
                  <button className="linkish" onClick={() => onTeam(r.opponentId)}>
                    {r.opponent}
                  </button>
                  {r.stage !== "regular" && <span className="badge">{r.round ?? r.stage}</span>}
                </td>
                <td className="nowrap">{r.maps[0] ?? "–"}</td>
                <td className={won ? "num team-won" : lost ? "num team-lost" : "num"}>{r.defaultWin ? t("default") : `${r.scoreFor ?? "–"}:${r.scoreAgainst ?? "–"}`}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function TeamAvatar({ src }: { src: string | null }) {
  return src ? <img src={src} alt="" className="teams-avatar" /> : <span className="teams-avatar" />;
}

function wl(r: LeagueRecord): string {
  return r.drawn > 0 ? `${r.won}-${r.lost}-${r.drawn}` : `${r.won}-${r.lost}`;
}

function pct(r: LeagueRecord): number | null {
  return r.played > 0 ? Math.round((100 * r.won) / r.played) : null;
}
