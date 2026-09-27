import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type MatchDetail } from "../../api/types";
import { t } from "../../lib/i18n";
import { ClassIcon } from "../ClassIcon";

const CLASSES = ["", "scout", "sniper", "soldier", "demoman", "medic", "heavyweapons", "pyro", "spy", "engineer"];

/**
 * Q28 (Flashy): where each player spent the match, by callout -- the
 * anchors and the rotators. From the STV's every-second positions, so only
 * for a match with one, and only on a map whose callouts are drawn.
 */
export function PositionsPanel({ d }: { d: MatchDetail }) {
  const map = d.map ?? "";
  const q = useQuery({ queryKey: ["positions", d.logId, map], queryFn: () => api.getPositions(d.logId, map), enabled: map !== "" });
  if (q.isError) {
    return (
      <section className="panel">
        <h2>{t("Positions")}</h2>
        <p className="error">{errorMessage(q.error)}</p>
      </section>
    );
  }
  const v = q.data;
  if (!v) return null;
  return (
    <section className="panel positions-panel">
      <h2>{t("Positions")}</h2>
      <p className="hint">
        {t("Where each player spent their time alive, by callout, from the STV demo: one position a second.")}
        {v.draft && t(" The callouts are a draft; fix them with Edit callouts on the kill map.")}
      </p>
      <div className="pos-teams">
        {[2, 3].map((team) => (
          <div key={team} className="pos-team">
            <h3 className={team === 2 ? "team-red" : "team-blue"}>{team === 2 ? "RED" : "BLU"}</h3>
            {v.players
              .filter((p) => p.team === team)
              .map((p) => {
                const zoned = p.zones.reduce((a, z) => a + z.seconds, 0);
                return (
                  <div key={p.accountId} className="pos-row">
                    <span className="pos-who">
                      {p.class > 0 && <ClassIcon cls={CLASSES[p.class]} size={18} />}
                      {p.name}
                    </span>
                    <div className="pos-bar" title={t("{0}% of their time inside a drawn zone", { "0": Math.round((100 * zoned) / Math.max(1, p.aliveS)) })}>
                      {p.zones.slice(0, 4).map((z, i) => (
                        <div key={z.zone} className={`pos-seg pos-seg-${i}`} style={{ flex: z.seconds }} title={`${z.zone}: ${Math.round((100 * z.seconds) / Math.max(1, p.aliveS))}%`}>
                          <span>{z.zone}</span>
                        </div>
                      ))}
                      <div className="pos-seg pos-seg-rest" style={{ flex: Math.max(0, p.aliveS - p.zones.slice(0, 4).reduce((a, z) => a + z.seconds, 0)) }} />
                    </div>
                  </div>
                );
              })}
          </div>
        ))}
      </div>
    </section>
  );
}
