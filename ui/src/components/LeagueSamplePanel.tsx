import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../api/client";
import { errorMessage } from "../api/types";
import { locale, t } from "../lib/i18n";

/**
 * The league sample (Flashy): officials from every ETF2L division, fetched
 * slowly in the background so ratings can be read against the league and a
 * player can be placed in the division they play in. Off until switched on:
 * it is hours of requests against community servers.
 */
export function LeagueSamplePanel() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["league_sample"], queryFn: api.getLeagueSample, refetchInterval: 10_000 });
  const s = q.data;

  const toggle = async (on: boolean) => {
    await api.setLeagueSample(on);
    void qc.invalidateQueries({ queryKey: ["league_sample"] });
  };

  const totals = s?.tiers.reduce(
    (a, x) => ({ logs: a.logs + x.logs, json: a.json + x.jsonLogstf + x.jsonMoretf, raw: a.raw + x.raw + x.rawMissing, matches: a.matches + x.matches, rosters: a.rosters + x.rosters }),
    { logs: 0, json: 0, raw: 0, matches: 0, rosters: 0 },
  );
  const phase = !s
    ? ""
    : !s.enabled
      ? t("Paused.")
      : s.discoveredAt === null
        ? t("Listing ETF2L's officials and choosing the matches (about ten minutes)…")
        : totals && totals.json < totals.logs
          ? s.logstfResting
            ? t("logs.tf asked us to slow down: resting for a few minutes, with more.tf filling in.")
            : t("Downloading the logs…")
          : totals && totals.raw < totals.logs
            ? t("Downloading the server logs…")
            : t("Everything chosen is here. New officials are looked for once a day.");
  const year = (at: number | null) => (at ? new Date(at * 1000).toLocaleDateString(locale(), { year: "numeric", month: "short" }) : "–");

  return (
    <div className="panel league-panel">
      <h2>{t("League sample")}</h2>
      <p className="hint" style={{ marginTop: 6 }}>
        {t("Up to 300 ETF2L officials from each division, the last three years first and every map in the pool, downloaded slowly in the background while the app is open. Kept apart from your own matches: it will let ratings be read against the whole league, and tell you which division the players you face come from.")}
      </p>
      {q.isError && <p className="error">{errorMessage(q.error)}</p>}
      {s && (
        <>
          <div className="row" style={{ marginTop: 12, gap: 10, alignItems: "center", flexWrap: "wrap" }}>
            <button className={s.enabled ? undefined : "primary"} onClick={() => void toggle(!s.enabled)}>
              {s.enabled ? t("Pause") : s.discoveredAt === null ? t("Start") : t("Resume")}
            </button>
            <span className="hint">{phase}</span>
          </div>
          {s.tiers.length > 0 && (
            <>
              <table className="league-table">
                <thead>
                  <tr>
                    <th>{t("Division")}</th>
                    <th>{t("Matches")}</th>
                    <th>{t("Logs")}</th>
                    <th>{t("Server logs")}</th>
                    <th>{t("Rosters")}</th>
                    <th>{t("Maps")}</th>
                    <th>{t("From")}</th>
                  </tr>
                </thead>
                <tbody>
                  {s.tiers.map((x) => (
                    <tr key={x.tier}>
                      <th scope="row">{x.division}</th>
                      <td title={t("Chosen, of the {0} wanted", { "0": s.targetPerTier })}>
                        {x.matches}
                        <span className="muted"> / {s.targetPerTier}</span>
                      </td>
                      <td>
                        <Bar done={x.jsonLogstf + x.jsonMoretf} of={x.logs} />
                        {x.jsonMoretf > 0 && <span className="muted" title={t("From more.tf while logs.tf was resting; asked of logs.tf again later")}> ({x.jsonMoretf} more.tf)</span>}
                      </td>
                      <td>
                        <Bar done={x.raw + x.rawMissing} of={x.logs} />
                      </td>
                      <td>
                        <Bar done={x.rosters} of={x.matches} />
                      </td>
                      <td>{x.maps}</td>
                      <td className="muted">
                        {year(x.oldest)} – {year(x.newest)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <p className="hint" style={{ marginTop: 8 }}>
                {t("{0} ETF2L logs listed; {1} MB held so far.", { "0": s.logsListed.toLocaleString(), "1": (s.bytes / 1e6).toFixed(0) })}{" "}
                {s.tiers.some((x) => x.matches < s.targetPerTier) && t("Where a division is short of 300, ETF2L did not play that many officials in the seasons read.")}
              </p>
            </>
          )}
        </>
      )}
    </div>
  );
}

function Bar({ done, of }: { done: number; of: number }) {
  const pct = of > 0 ? Math.min(100, (done / of) * 100) : 0;
  return (
    <span className="league-bar" title={`${done} / ${of}`}>
      <span className="progress-track">
        <span className="progress-fill" style={{ width: `${pct}%` }} />
      </span>
      <span className="league-bar-n">
        {done}/{of}
      </span>
    </span>
  );
}
