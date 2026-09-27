import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type CartFight, type MatchDetail } from "../../api/types";
import { clock } from "../../lib/format";
import { copy } from "../../lib/toast";
import { t, tx } from "../../lib/i18n";

/**
 * Q11 (Flashy): the cart in a numbers advantage. Seconds BLU was up three
 * or more and the cart stood still -- nobody on it, or a defender alive on
 * it that nobody killed -- and how much it moved after each won fight.
 * Payload only, and only where an STV was read.
 */
export function CartPanel({ d }: { d: MatchDetail }) {
  const q = useQuery({ queryKey: ["cart", d.logId], queryFn: () => api.getCart(d.logId) });
  if (q.isError) {
    return (
      <section className="panel cart-panel">
        <h2>{t("The cart")}</h2>
        <p className="error">{errorMessage(q.error)}</p>
      </section>
    );
  }
  const v = q.data;
  if (!v) return null;
  const wasted = v.rounds.reduce((a, r) => a + r.upStillS, 0);
  const empty = v.rounds.reduce((a, r) => a + r.upStillEmptyS, 0);
  const jump = (tick: number, what: string) => void copy(`demo_gototick ${tick}`, what);

  return (
    <section className="panel cart-panel">
      <h2>{t("The cart")}</h2>
      <p className="hint">
        {tx("Seconds BLU was up {0} or more players and the cart stood still: {1} in this match, {2} of them with nobody on the cart and the rest with a defender blocking it.", {
          "0": v.up,
          "1": <strong>{wasted}s</strong>,
          "2": <strong>{empty}s</strong>,
        })}
      </p>

      <div className="cart-rounds">
        {v.rounds.map((r, i) => (
          <div key={i} className="cart-round">
            <div className="cart-round-head">
              <strong>{t("Round {0}", { "0": i + 1 })}</strong>
              <span className="hint">
                {t("{0} live · cart moving {1}%", { "0": clock(r.seconds), "1": Math.round((100 * r.movingS) / Math.max(1, r.seconds)) })}
              </span>
            </div>
            <div
              className="cart-bar"
              title={t("Up {0}+: {1}s moving, {2}s still with nobody on it, {3}s blocked", {
                "0": v.up,
                "1": r.upS - r.upStillS,
                "2": r.upStillEmptyS,
                "3": r.upStillS - r.upStillEmptyS,
              })}
            >
              <div className="cart-seg cart-seg-moving" style={{ flex: Math.max(0, r.upS - r.upStillS) }} />
              <div className="cart-seg cart-seg-empty" style={{ flex: r.upStillEmptyS }} />
              <div className="cart-seg cart-seg-blocked" style={{ flex: r.upStillS - r.upStillEmptyS }} />
            </div>
            <span className="hint">{t("{0}s up {1}+ · {2}s of it still", { "0": r.upS, "1": v.up, "2": r.upStillS })}</span>
          </div>
        ))}
      </div>
      <div className="cart-legend hint">
        <span><i className="cart-seg-moving" /> {t("moving")}</span>
        <span><i className="cart-seg-empty" /> {t("still, nobody on it")}</span>
        <span><i className="cart-seg-blocked" /> {t("still, blocked")}</span>
      </div>

      {v.stalls.length > 0 && (
        <>
          <h3>{t("Stalls while up")}</h3>
          <div className="table-wrap">
            <table className="match-table cart-table">
              <thead>
                <tr>
                  <th>{t("Round")}</th>
                  <th className="num">{t("At")}</th>
                  <th className="num">{t("Still for")}</th>
                  <th className="num">{t("Up")}</th>
                  <th>{t("Why")}</th>
                </tr>
              </thead>
              <tbody>
                {v.stalls.map((s, i) => (
                  <tr
                    key={i}
                    className="spy-row"
                    title={t("Copy demo_gototick {tick}", { tick: s.jumpTick })}
                    onClick={() => jump(s.jumpTick, t("Stall at {0}", { "0": clock(s.atS) }))}
                  >
                    <td>{s.round + 1}</td>
                    <td className="num">{clock(s.atS)}</td>
                    <td className="num">{s.seconds}s</td>
                    <td className="num">+{s.mostUp}</td>
                    <td>{s.emptyS * 2 >= s.seconds ? t("nobody on the cart") : t("a defender blocking")}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}

      {v.fights.length > 0 && (
        <>
          <h3>{tx("After a won fight: seconds of the next {0} the cart moved", { "0": v.afterS })}</h3>
          <div className="cart-after">
            {[1, 2, 3, 0].map((n) => (
              <After key={n} n={n} fights={v.fights.filter((f) => n === 0 || f.nth === n)} />
            ))}
          </div>
        </>
      )}
    </section>
  );
}

function After({ n, fights }: { n: number; fights: CartFight[] }) {
  const label = n === 0 ? t("Every fight won") : n === 1 ? t("1st fight won") : n === 2 ? t("2nd fight won") : t("3rd fight won");
  if (fights.length === 0) {
    return (
      <div className="cart-after-cell">
        <span className="hint">{label}</span>
        <strong className="muted">–</strong>
      </div>
    );
  }
  const moving = fights.reduce((a, f) => a + f.movingS, 0) / fights.length;
  const window = fights.reduce((a, f) => a + f.windowS, 0) / fights.length;
  return (
    <div className="cart-after-cell">
      <span className="hint">{label}</span>
      <strong>{moving.toFixed(1)}s</strong>
      <span className="hint">
        {fights.length === 1
          ? t("of {0}s · 1 fight", { "0": window.toFixed(0) })
          : t("of {0}s · {1} fights", { "0": window.toFixed(0), "1": fights.length })}
      </span>
    </div>
  );
}
