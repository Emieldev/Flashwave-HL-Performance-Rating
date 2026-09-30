import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type CatDivision, type Medal, type PlayerProfile as Profile } from "../../api/types";
import { formatDate } from "../../lib/format";
import { ClassIcon } from "../ClassIcon";
import { classLabel } from "../analysis/common";
import { t, tx } from "../../lib/i18n";
import { PlayerStatsCard, RankChip } from "./PlayerStats";
import { MedalGlyph } from "./MedalGlyph";

/**
 * A player's profile, HLTV-style (Q35, Flashy; PLAN §26): who they are,
 * their teams season by season, their divisions and medals, their
 * officials -- from the ETF2L officials the league job downloaded and who
 * played them, and ETF2L's own page for the country and declared classes.
 * The medals are worked out from playoff finals and final tables; ETF2L
 * has no awards to fetch.
 */

type Tab = "overview" | "teams" | "achievements" | "yours";

const PLACE = ["gold", "silver", "bronze"] as const;

export function PlayerProfile({ accountId, yours }: { accountId: number; yours: React.ReactNode }) {
  const q = useQuery({ queryKey: ["player_profile", accountId], queryFn: () => api.getPlayerProfile(accountId) });
  const [tab, setTab] = useState<Tab>("overview");
  if (q.isPending) return <div className="panel"><p className="hint">{t("Loading…")}</p></div>;
  if (q.isError) return <div className="panel"><p className="error">{errorMessage(q.error)}</p></div>;
  const p = q.data;
  return (
    <>
      <Header p={p} />
      <div className="panel pp-body">
        <nav className="pp-tabs" role="tablist">
          {(
            [
              ["overview", t("Overview")],
              ["teams", t("Teams")],
              ["achievements", tx("Achievements ({0})", { "0": p.medals.length })],
              ["yours", t("In your matches")],
            ] as [Tab, React.ReactNode][]
          ).map(([id, label]) => (
            <button key={id} role="tab" aria-selected={tab === id} className={tab === id ? "pp-tab on" : "pp-tab"} onClick={() => setTab(id)}>
              {label}
            </button>
          ))}
        </nav>
        {tab === "overview" && <Overview p={p} />}
        {tab === "teams" && <Teams p={p} />}
        {tab === "achievements" && <Achievements medals={p.medals} />}
        {tab === "yours" && yours}
      </div>
    </>
  );
}

function Header({ p }: { p: Profile }) {
  const titles = medalTitles(p.medals);
  const played = p.mainClass;
  const declared = p.declaredClasses.map((c) => c.toLowerCase()).filter((c) => c !== played);
  const shownAliases = p.aliases.slice(0, 6);
  return (
    <div className="panel pp-head">
      <div className="pp-top">
        <div className="pp-avatar">{p.avatar ? <img src={p.avatar} alt="" /> : <span>{p.name.slice(0, 1).toUpperCase()}</span>}</div>
        <div className="pp-who">
          <div className="pp-name-row">
            <h2 className="pp-name">{p.name}</h2>
            {p.highest && <DivisionBadge d={p.highest} />}
          </div>
          {shownAliases.length > 0 && (
            <p className="hint pp-aka">
              {tx("Also as {0}", { "0": shownAliases.join(", ") })}
              {p.aliases.length > shownAliases.length && ` ${t("and {0} more", { "0": p.aliases.length - shownAliases.length })}`}
            </p>
          )}
          <dl className="pp-facts">
            {p.country && (
              <>
                <dt>{t("Country")}</dt>
                <dd>{p.country}</dd>
              </>
            )}
            <dt>{t("Main class")}</dt>
            <dd className="pp-classes">
              {played ? (
                <span title={t("Most played in the logs held: your matches and the league sample")}>
                  <ClassIcon cls={played} size={18} /> {classLabel(played)}
                </span>
              ) : (
                <span className="muted">–</span>
              )}
              {declared.length > 0 && (
                <span className="muted" title={t("Signed up as on ETF2L")}>
                  {" "}· {t("signed up as")} {declared.map((c) => classLabel(c)).join(", ")}
                </span>
              )}
            </dd>
            <dt>{t("Current team")}</dt>
            <dd>
              {p.current ? (
                <span className="pp-team">
                  <TeamAvatar src={p.current.team.avatar} />
                  {p.current.team.name}
                  <span className="muted">
                    {" "}· {p.current.division} · {seasonShort(p.current.season, p.current.seasonName)}
                  </span>
                </span>
              ) : (
                <span className="muted">{t("No ETF2L official in the seasons read")}</span>
              )}
            </dd>
            <dt>{t("Officials")}</dt>
            <dd>{tx("{0} in {1} season{2}", { "0": p.officials.length, "1": new Set(p.seasons.map((s) => s.season)).size, "2": new Set(p.seasons.map((s) => s.season)).size === 1 ? "" : "s" })}</dd>
          </dl>
          <HeaderRanks accountId={p.accountId} />
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
        <div className="pp-links">
          {p.etf2lId !== null && (
            <a href={`https://etf2l.org/forum/user/${p.etf2lId}/`} target="_blank" rel="noreferrer">
              {t("ETF2L ↗")}
            </a>
          )}
          <a href={`https://trends.tf/player/${p.steamid64}/`} target="_blank" rel="noreferrer">{t("trends.tf ↗")}</a>
          <a href={`https://logs.tf/profile/${p.steamid64}`} target="_blank" rel="noreferrer">{t("logs.tf ↗")}</a>
          <a href={`https://steamcommunity.com/profiles/${p.steamid64}`} target="_blank" rel="noreferrer">{t("Steam ↗")}</a>
        </div>
      </div>
      {p.medals.length > 0 && (
        <div className="pp-trophies" aria-label={t("Medals")}>
          {p.medals.map((m, i) => (
            <MedalIcon key={i} m={m} />
          ))}
        </div>
      )}
    </div>
  );
}

/** The newest season's ranks, best first, beside the name like HLTV's Top 20. */
function HeaderRanks({ accountId }: { accountId: number }) {
  const q = useQuery({ queryKey: ["player_stats", accountId], queryFn: () => api.getPlayerStats(accountId) });
  const ranks = q.data?.ranks ?? [];
  if (ranks.length === 0) return null;
  const newest = ranks[0].season;
  return (
    <div className="pp-ranks">
      {ranks
        .filter((r) => r.season === newest)
        .slice(0, 3)
        .map((r) => (
          <RankChip key={r.class} r={r} accountId={accountId} />
        ))}
    </div>
  );
}

/** "Low winner" x2, "High runner-up" x1, ... best first. */
function medalTitles(medals: Medal[]): [string, number, number][] {
  const count = new Map<string, [number, number, number]>();
  for (const m of medals) {
    const what = m.place === 1 ? t("{0} winner", { "0": m.division }) : m.place === 2 ? t("{0} runner-up", { "0": m.division }) : t("{0} third", { "0": m.division });
    const e = count.get(what) ?? [0, m.place, m.tier ?? 9];
    e[0] += 1;
    count.set(what, e);
  }
  return [...count.entries()]
    .sort((a, b) => a[1][2] - b[1][2] || a[1][1] - b[1][1])
    .map(([label, [n, place]]) => [label, n, place]);
}

function MedalIcon({ m }: { m: Medal }) {
  const kind = PLACE[m.place - 1];
  return (
    <span className={`pp-medal pp-${kind}`} title={`${m.place === 1 ? t("Winner") : m.place === 2 ? t("Runner-up") : t("Third")} · ${m.division} · ${seasonLong(m.season, m.seasonName)} · ${m.team.name} · ${m.how}`}>
      <span className="pp-medal-tile">
        <MedalGlyph size={38} place={m.place} />
      </span>
      <span className="pp-medal-label">
        {seasonShort(m.season, m.seasonName)} {shortDivision(m.division)}
      </span>
    </span>
  );
}

/** "S34", or a season between two numbered ones by its name ("AFA 2025"). */
export function seasonShort(season: number, name?: string): string {
  return season >= 100 ? (name ?? "AFA") : `S${season}`;
}

/** "Season 34 (Summer 2025)", or just "AFA 2025". */
export function seasonLong(season: number, name: string): string {
  return season >= 100 ? name : `${tx("Season {0}", { "0": season })} (${name})`;
}

function shortDivision(d: string): string {
  return d === "Premiership" ? "Prem" : d.replace("Division ", "Div ");
}

export function DivisionBadge({ d }: { d: CatDivision }) {
  return (
    <span className={`div-badge div-t${Math.min(d.tier, 4)}`} title={t("Highest division with three or more officials played")}>
      {shortDivision(d.name)}
    </span>
  );
}

function TeamAvatar({ src }: { src: string | null }) {
  return src ? <img className="pp-team-avatar" src={src} alt="" /> : <span className="pp-team-avatar" />;
}

function Overview({ p }: { p: Profile }) {
  const recent = p.officials.slice(0, 10);
  return (
    <div className="pp-overview">
      <section>
        <h3>{t("Recent officials")}</h3>
        {recent.length === 0 && <p className="hint">{t("No ETF2L official in the seasons read.")}</p>}
        <ul className="pp-matches">
          {recent.map((o) => (
            <li key={o.matchId} className={o.won === true ? "won" : o.won === false ? "lost" : undefined}>
              <span className="pp-vs">
                {t("vs")} <TeamAvatar src={o.opponent.avatar} /> <strong>{o.opponent.name}</strong>
              </span>
              <span className="muted pp-where">
                {seasonShort(o.season)} {o.division}
                {o.stage !== "regular" ? ` · ${o.round ?? o.stage}` : ""}
                {o.time ? ` · ${formatDate(o.time, true)}` : ""}
              </span>
              <span className="pp-score">
                {o.scoreFor ?? "–"} : {o.scoreAgainst ?? "–"}
              </span>
            </li>
          ))}
        </ul>
      </section>
      <section>
        <h3>{t("Rating")}</h3>
        <PlayerStatsCard accountId={p.accountId} />
      </section>
    </div>
  );
}

function Teams({ p }: { p: Profile }) {
  if (p.seasons.length === 0) return <p className="hint">{t("No ETF2L official in the seasons read.")}</p>;
  return (
    <table className="pp-seasons">
      <thead>
        <tr>
          <th>{t("Season")}</th>
          <th>{t("Team")}</th>
          <th>{t("Division")}</th>
          <th>{t("Record")}</th>
          <th>{t("Placing")}</th>
        </tr>
      </thead>
      <tbody>
        {p.seasons.map((s) => (
          <tr key={`${s.season}-${s.team.id}`}>
            <td>
              {s.season >= 100 ? s.seasonName : <>S{s.season} <span className="muted">{s.seasonName}</span></>}
            </td>
            <td>
              <span className="pp-team">
                <TeamAvatar src={s.team.avatar} />
                {s.team.name}
              </span>
            </td>
            <td>{s.tier !== null ? <DivisionBadge d={{ name: s.division, tier: s.tier }} /> : s.division}</td>
            <td>
              {s.won}–{s.lost}
              <span className="muted"> {tx("of {0}", { "0": s.played })}</span>
            </td>
            <td>{s.place ? <span className={`pp-place pp-${PLACE[s.place - 1]}`}>{s.place === 1 ? t("Winner") : s.place === 2 ? t("Runner-up") : t("Third")}</span> : <span className="muted">–</span>}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function Achievements({ medals }: { medals: Medal[] }) {
  if (medals.length === 0) {
    return <p className="hint">{t("No medals in the seasons read. Medals come from ETF2L playoff finals and, where a division has no playoffs, its final table.")}</p>;
  }
  return (
    <ul className="pp-achievements">
      {medals.map((m, i) => (
        <li key={i}>
          <MedalIcon m={m} />
          <span>
            <strong>{m.place === 1 ? t("Winner") : m.place === 2 ? t("Runner-up") : t("Third")}</strong> · {m.division} · {seasonLong(m.season, m.seasonName)}
          </span>
          <span className="pp-team">
            <TeamAvatar src={m.team.avatar} />
            {m.team.name}
          </span>
          <span className="muted">{m.how}</span>
        </li>
      ))}
    </ul>
  );
}
