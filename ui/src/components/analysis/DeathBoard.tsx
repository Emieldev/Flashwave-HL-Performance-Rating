import type { DeathRow } from "../../api/types";
import { t, tx } from "../../lib/i18n";

/**
 * Where the player who killed you was, seen from above.
 *
 * You are the middle, facing up the page. The angle round the circle is how
 * far you would have had to turn to look at them, so the bottom of the circle
 * is directly behind you. The distance out is how far away they were.
 *
 * A dartboard was the first attempt, but deaths almost never differ
 * vertically — everyone stands on the same floor — so every dot landed on one
 * line. Direction and distance are the two things that actually vary.
 *
 * The shaded wedge is roughly what a TF2 screen shows: inside it they were on
 * your screen when they killed you, outside it they were not.
 */

const SEEN = "#5791c8";
const SCOPED = "#d6763a";

/** Half of TF2's default field of view, near enough for a wedge. */
const HALF_FOV = 45;
/** Rings, in map units. A Sniper sightline runs about 1,500. */
const RINGS = [500, 1000, 1500, 2000];

export function DeathBoard({ deaths }: { deaths: DeathRow[] }) {
  const placed = deaths.filter((d) => d.killerDxDeg !== null);
  if (placed.length === 0) return null;

  const S = 320;
  const c = S / 2;
  const R = c - 26;
  const max = Math.max(1200, ...placed.map((d) => d.killerRange ?? 0));
  // Unknown distances sit just outside the last ring rather than vanishing.
  const radius = (units: number | null) => (units === null ? R + 6 : (Math.min(units, max) / max) * R);
  const place = (d: DeathRow) => {
    const bearing = ((d.killerDxDeg ?? 0) * Math.PI) / 180;
    const r = radius(d.killerRange);
    return [c + Math.sin(bearing) * r, c - Math.cos(bearing) * r] as const;
  };

  const behind = placed.filter((d) => Math.abs(d.killerDxDeg ?? 0) > HALF_FOV);
  const scopedBehind = behind.filter((d) => d.scoped).length;
  const unknown = placed.filter((d) => d.killerRange === null).length;
  const wedge = [
    `M ${c} ${c}`,
    `L ${c + Math.sin((-HALF_FOV * Math.PI) / 180) * R} ${c - Math.cos((-HALF_FOV * Math.PI) / 180) * R}`,
    `A ${R} ${R} 0 0 1 ${c + Math.sin((HALF_FOV * Math.PI) / 180) * R} ${c - Math.cos((HALF_FOV * Math.PI) / 180) * R}`,
    "Z",
  ].join(" ");

  return (
    <figure className="aim-fig">
      <figcaption>{tx("Where the player who killed you was{0}", { "0": <span className="aim-fig-sub">{t("seen from above: you are in the middle, facing up")}</span> })}
      </figcaption>

      <div className="board-wrap">
        <svg
          viewBox={`0 0 ${S} ${S}`}
          className="aim-board death-board"
          role="img"
          aria-label={t("{behind} of {placed} deaths came from outside your view", { behind: behind.length, placed: placed.length })}
        >
          <path d={wedge} className="board-fov" />
          {RINGS.filter((u) => u <= max).map((u) => (
            <g key={u}>
              <circle cx={c} cy={c} r={radius(u)} className="board-ring" />
              <text x={c + 3} y={c - radius(u) + 11} className="aim-axis">
                {u.toLocaleString()}
              </text>
            </g>
          ))}
          <line x1={c} x2={c} y1={c - R} y2={c + R} className="board-cross" />
          <line x1={c - R} x2={c + R} y1={c} y2={c} className="board-cross" />
          <text x={c} y={12} className="aim-axis" textAnchor="middle">{t("where you were looking")}</text>
          <text x={c} y={S - 3} className="aim-axis" textAnchor="middle">{t("behind you")}</text>

          {placed.map((d) => {
            const [x, y] = place(d);
            const turn = Math.abs(d.killerDxDeg ?? 0);
            return (
              <circle key={d.tick} cx={x} cy={y} r={5} fill={d.scoped ? SCOPED : SEEN} fillOpacity={0.85}>
                <title>
                  {turn < 1 ? t("straight ahead") : t("{0}° to your {1}", { "0": turn.toFixed(0), "1": (d.killerDxDeg ?? 0) > 0 ? t("right") : t("left") })}
                  {d.killerRange === null ? t(", distance not recorded") : t(", {0} units away", { "0": d.killerRange.toFixed(0) })}
                  {d.scoped ? t(", you were scoped") : ""}
                  {d.matesNear === 0 ? t(", nobody near you") : t(", {matesNear} teammate{1} near", { "1": d.matesNear === 1 ? "" : "s", matesNear: d.matesNear })}
                </title>
              </circle>
            );
          })}
          <circle cx={c} cy={c} r={3} className="board-you" />
        </svg>

        <dl className="board-read">
          <dt>{t("Off your screen")}</dt>
          <dd>
            {tx("{behind} of {placed} deaths came from more than {HALF_FOV}° to a side, so they were never in front of you", { behind: behind.length, placed: placed.length, HALF_FOV: HALF_FOV })}</dd>
          <dt>{t("Scoped at the time")}</dt>
          <dd>
            {tx("{length} of {placed}{2}", { "2": scopedBehind > 0 && t(", and {scopedBehind} of those came from outside your view", { scopedBehind: scopedBehind }), length: placed.filter((d) => d.scoped).length, placed: placed.length })}
          </dd>
          <dt>{t("Usual distance")}</dt>
          <dd>
            {median(placed)}
            {unknown > 0 && (
              <span className="muted">
                {tx("{0}· {unknown} on the rim: the demo never carried them, which is what a Spy behind you looks like", { "0": " ", unknown: unknown })}</span>
            )}
          </dd>
        </dl>
      </div>

      <ul className="aim-legend">
        <li>
          {tx("{0} you were scoped", { "0": <span className="aim-dot" style={{ background: SCOPED }} aria-hidden /> })}</li>
        <li>
          {tx("{0} you were not", { "0": <span className="aim-dot" style={{ background: SEEN }} aria-hidden /> })}</li>
        <li>
          {tx("{0} roughly what your screen showed", { "0": <span className="aim-dot board-dot-fov" aria-hidden /> })}</li>
      </ul>
    </figure>
  );
}

/** The middle distance of the deaths the demo could measure. */
function median(rows: DeathRow[]): string {
  const known = rows
    .map((d) => d.killerRange)
    .filter((v): v is number => v !== null)
    .sort((a, b) => a - b);
  if (known.length === 0) return "not recorded";
  return `${known[Math.floor(known.length / 2)].toFixed(0)} units`;
}
