import type { FightLine, FightsCard } from "../../api/types";
import { t, tx } from "../../lib/i18n";
import { classLabel } from "../analysis/common";

/**
 * Your kills in context against the players you face on the class: who opens
 * fights, whose kills get traded straight back, who dies right after their
 * own kill. Same filters as the rest of the profile.
 */
export function FightsPanel({ card, cls }: { card: FightsCard; cls: string }) {
  return (
    <section className="panel fights-panel">
      <header>
        <h2>{t("Fights")}</h2>
        <p className="hint">{tx("Your {games} games against {1} by the {2}s you have faced.", { "1": card.poolGames.toLocaleString(), "2": classLabel(cls), games: card.games })}</p>
      </header>
      <div className="table-wrap">
        <table className="match-table fights-card">
          <thead>
            <tr>
              <th />
              <th className="num">{t("You")}</th>
              <th className="num">{t("Players you face")}</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {card.lines.map((l) => (
              <tr key={l.label} title={t(l.hint)}>
                <td>
                  {t(l.label)} <span className="muted">· {t(l.unit)}</span>
                </td>
                <td className="num">{fmt(l, l.you)}</td>
                <td className="num muted">{fmt(l, l.pool)}</td>
                <td>
                  <Verdict l={l} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function fmt(l: FightLine, v: number | null): string {
  if (v === null) return "–";
  // Small shares keep a decimal, or 4.6% and 5.2% would both read "5%".
  if (l.unit.startsWith("%")) return `${v.toFixed(v < 10 ? 1 : 0)}%`;
  return v.toFixed(2);
}

/** Better or worse than the pool, in words and an arrow, never colour alone. */
function Verdict({ l }: { l: FightLine }) {
  if (l.you === null || l.pool === null || l.better === 0 || l.pool === 0) return null;
  const rel = (l.you - l.pool) / Math.abs(l.pool);
  if (Math.abs(rel) < 0.05) return <span className="muted">{t("about the same")}</span>;
  const good = rel * l.better > 0;
  return (
    <span className={good ? "verdict good" : "verdict bad"}>
      {tx("{0} by {1}%", { "0": good ? t("▲ better") : t("▼ worse"), "1": Math.round(Math.abs(rel) * 100) })}</span>
  );
}
