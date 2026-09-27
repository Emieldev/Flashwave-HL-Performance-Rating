import { useState } from "react";
import type { AimRow } from "../../api/types";
import { t, tx } from "../../lib/i18n";

/**
 * The kills on a target, seen down your own scope (PLAN §14).
 *
 * The centre is the head you killed. A dot is where your crosshair sat
 * relative to it: right of centre means you were aiming to their right, above
 * centre means high. Rings are degrees off, on a square-root scale so the
 * middle — where most kills live — stays readable.
 *
 * Two moments can be shown: the shot itself, and one second earlier, which is
 * the same kills with the crosshair still on its way.
 */

const CALM = "#5791c8";
const BUSY = "#d6763a";

/** Rings, in degrees off the head. */
const RINGS = [1, 3, 10, 30];
const MAX_DEG = RINGS[RINGS.length - 1];
/** A head is about this wide in map units, which sets the bullseye. */
const HEAD_UNITS = 16;

export function AimBoard({ kills }: { kills: AimRow[] }) {
  const [when, setWhen] = useState<"shot" | "before">("shot");
  const [trails, setTrails] = useState(true);
  const seen = kills.filter((k) => k.victimSeen);
  if (seen.length === 0) return null;

  const at = (k: AimRow) => (when === "shot" ? { x: k.dxDeg, y: k.dyDeg, d: k.errorDeg } : { x: k.beforeDxDeg, y: k.beforeDyDeg, d: k.beforeDeg });

  const S = 420;
  const c = S / 2;
  const R = c - 26;
  // Square root of the angle, so one degree is not a pixel at the centre.
  const radius = (deg: number) => R * Math.sqrt(Math.min(deg, MAX_DEG) / MAX_DEG);
  const place = (x: number, y: number, d: number) => {
    const len = Math.hypot(x, y) || 1;
    const r = radius(d);
    return [c + (x / len) * r, c - (y / len) * r] as const;
  };

  const inside = seen.filter((k) => at(k).d <= MAX_DEG).length;
  const bias = {
    x: seen.reduce((n, k) => n + at(k).x, 0) / seen.length,
    y: seen.reduce((n, k) => n + at(k).y, 0) / seen.length,
  };
  const medianRange = [...seen].map((k) => k.rangeUnits).sort((a, b) => a - b)[Math.floor(seen.length / 2)] ?? 1000;
  // The head's own angular size at that range: what "on target" actually is.
  const headDeg = (2 * Math.atan(HEAD_UNITS / 2 / medianRange) * 180) / Math.PI;

  return (
    <figure className="aim-fig">
      <figcaption>{t("Where your crosshair sat")}<span className="aim-fig-sub">{t("centre is the head you killed; right of centre means you were aiming to their right")}</span>
        <label className="check board-trails" title={t("Draw the second before each kill as a line into the dot")}>
          <input type="checkbox" checked={trails} onChange={(e) => setTrails(e.target.checked)} />{" "}{t("trails")}</label>
        <span className="board-when segmented" role="tablist" aria-label={t("Moment")}>
          <button role="tab" aria-selected={when === "shot"} className={when === "shot" ? "seg active" : "seg"} onClick={() => setWhen("shot")}>{t("At the shot")}</button>
          <button role="tab" aria-selected={when === "before"} className={when === "before" ? "seg active" : "seg"} onClick={() => setWhen("before")}>{t("A second before")}</button>
        </span>
      </figcaption>

      <div className="board-wrap">
        <svg viewBox={`0 0 ${S} ${S}`} className="aim-board" role="img" aria-label={t("{inside} of {seen} kills within {MAX_DEG} degrees of the head", { inside: inside, seen: seen.length, MAX_DEG: MAX_DEG })}>
          {RINGS.map((deg) => (
            <g key={deg}>
              <circle cx={c} cy={c} r={radius(deg)} className="board-ring" />
              <text x={c} y={c - radius(deg) - 4} className="aim-axis" textAnchor="middle">
                {deg}°
              </text>
            </g>
          ))}
          <line x1={c - R} x2={c + R} y1={c} y2={c} className="board-cross" />
          <line x1={c} x2={c} y1={c - R} y2={c + R} className="board-cross" />
          {/* The head itself, to scale: inside this circle the shot was on it. */}
          <circle cx={c} cy={c} r={Math.max(2, radius(headDeg))} className="board-head" />

          {trails &&
            seen.map((k) => {
              // The path is stored from the head's point of view, so it needs
              // the same placing as a dot: the line ends where the dot sits.
              if (k.path.length < 2) return null;
              const d = k.path
                .map(([px, py], i) => {
                  const [x, y] = place(px, py, Math.hypot(px, py));
                  return `${i === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`;
                })
                .join(" ");
              return <path key={`t${k.tick}`} d={d} className="board-trail" />;
            })}

          {seen.map((k) => {
            const p = at(k);
            const [x, y] = place(p.x, p.y, p.d);
            return (
              <circle key={k.tick} cx={x} cy={y} r={4.5} fill={k.headshot ? BUSY : "none"} stroke={k.headshot ? BUSY : CALM} strokeWidth={1.8} opacity={0.9}>
                <title>
                  {tx("{0}° off ({1}, {2}), {3} units away{4}", { "0": p.d.toFixed(1), "1": fmt(p.x), "2": fmtY(p.y), "3": k.rangeUnits.toFixed(0), "4": k.headshot ? t(", headshot") : "" })}
                </title>
              </circle>
            );
          })}

          {/* Where the crosshair usually sat: the habit, not the kill. */}
          {(() => {
            const d = Math.hypot(bias.x, bias.y);
            const [x, y] = place(bias.x, bias.y, d);
            return (
              <g className="board-bias">
                <line x1={x - 7} x2={x + 7} y1={y} y2={y} />
                <line x1={x} x2={x} y1={y - 7} y2={y + 7} />
                <title>{tx("On average {0}° off: {1}, {2}", { "0": d.toFixed(1), "1": fmt(bias.x), "2": fmtY(bias.y) })}
                </title>
              </g>
            );
          })()}
        </svg>

        <dl className="board-read">
          <dt>{t("Usually off by")}</dt>
          <dd>
            {Math.hypot(bias.x, bias.y).toFixed(1)}° — {fmt(bias.x)}, {fmtY(bias.y)}
          </dd>
          <dt>{t("On the head")}</dt>
          <dd>
            {tx("{length} of {seen} kills, inside the {2}° the head covers at {3} units", { "2": headDeg.toFixed(1), "3": medianRange.toFixed(0), length: seen.filter((k) => at(k).d <= headDeg).length, seen: seen.length })}</dd>
          <dt>{t("Off the board")}</dt>
          <dd>
            {tx("{0} kills were more than {MAX_DEG}° away and sit on the rim", { "0": seen.length - inside, MAX_DEG: MAX_DEG })}</dd>
        </dl>
      </div>

      <ul className="aim-legend">
        <li>
          {tx("{0} headshot", { "0": <span className="aim-dot" style={{ background: BUSY }} aria-hidden /> })}</li>
        <li>
          {tx("{0} body shot", { "0": <span className="aim-dot aim-dot-open" style={{ borderColor: CALM }} aria-hidden /> })}</li>
        <li>
          {tx("{0} where you usually were", { "0": <span className="aim-dot board-dot-bias" aria-hidden /> })}</li>
      </ul>
    </figure>
  );
}

/** A sideways angle, said the way a player would say it. */
function fmt(deg: number): string {
  if (Math.abs(deg) < 0.05) return "dead centre";
  return `${Math.abs(deg).toFixed(1)}° ${deg > 0 ? "right" : "left"}`;
}

/** The same, up and down. */
function fmtY(deg: number): string {
  if (Math.abs(deg) < 0.05) return "level";
  return `${Math.abs(deg).toFixed(1)}° ${deg > 0 ? "high" : "low"}`;
}
