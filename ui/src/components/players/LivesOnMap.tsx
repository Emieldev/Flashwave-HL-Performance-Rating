import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type LifeOnMap } from "../../api/types";
import { capitalize, formatDate } from "../../lib/format";
import { t, tx } from "../../lib/i18n";
import { ClassIcon } from "../ClassIcon";

/**
 * A player's lives on one map (Q57, Emiel): pick a map and a class, and
 * every route the demos on this machine followed them through is drawn on
 * it -- every Spy cross on Product, split by playing RED and BLU. Only the
 * matches with an STV, or with their own recording, have routes; the
 * counts say how many that is.
 */

type Side = "both" | "Red" | "Blue";

/** A step longer than this went through a teleporter or a respawn: the line
 *  breaks rather than crossing the map. Points are about four a second, so
 *  this is well past running speed. */
const JUMP = 900;

export function LivesOnMapTab({ accountId, name }: { accountId: number; name: string }) {
  const mapsQ = useQuery({ queryKey: ["lives_maps", accountId], queryFn: () => api.livesMaps(accountId), staleTime: 5 * 60_000 });
  const maps = mapsQ.data ?? [];
  const [picked, setPicked] = useState<string | null>(null);
  const map = picked ?? maps[0]?.map ?? null;
  const fullName = maps.find((m) => m.map === map)?.name ?? map;

  const livesQ = useQuery({
    queryKey: ["lives_on_map", accountId, map],
    queryFn: () => api.livesOnMap(accountId, map!),
    enabled: map !== null,
    staleTime: 5 * 60_000,
  });
  const overviewQ = useQuery({
    queryKey: ["overview", fullName],
    queryFn: () => api.getMapOverview(fullName ?? ""),
    enabled: fullName !== null,
    staleTime: Infinity,
  });

  const [cls, setCls] = useState<string | null>(null);
  const [side, setSide] = useState<Side>("both");
  const [deathsOnly, setDeathsOnly] = useState(false);
  const [hover, setHover] = useState<LifeOnMap | null>(null);

  const all = livesQ.data?.lives ?? [];
  // The classes they played here, most lives first; the side filter applies.
  const classes = useMemo(() => {
    const n = new Map<string, number>();
    for (const l of all) if (l.class && (side === "both" || l.team === side)) n.set(l.class, (n.get(l.class) ?? 0) + 1);
    return [...n.entries()].sort((a, b) => b[1] - a[1]);
  }, [all, side]);
  const shown = all.filter((l) => (cls === null || l.class === cls) && (side === "both" || l.team === side) && (!deathsOnly || l.died));
  const sides = { Red: all.filter((l) => (cls === null || l.class === cls) && l.team === "Red").length, Blue: all.filter((l) => (cls === null || l.class === cls) && l.team === "Blue").length };

  if (mapsQ.isPending) return <p className="hint">{t("Loading…")}</p>;
  if (mapsQ.isError) return <p className="error">{errorMessage(mapsQ.error)}</p>;
  if (maps.length === 0) {
    return (
      <p className="hint lm-empty">
        {tx("No demo on this machine followed {0}. Routes come from SourceTV demos (download one from a match's Demo linking panel) and from their own recordings.", { "0": <strong>{name}</strong> })}
      </p>
    );
  }

  const v = livesQ.data;
  return (
    <div className="lm">
      <div className="lm-controls">
        <label className="an-field">
          <span className="an-label">{t("Map")}</span>
          <select
            value={map ?? ""}
            onChange={(e) => {
              setPicked(e.target.value);
              setCls(null);
              setHover(null);
            }}
          >
            {maps.map((m) => (
              <option key={m.map} value={m.map}>
                {capitalize(m.map)} ({tx("{0} match{1}", { "0": m.matches, "1": m.matches === 1 ? "" : "es" })})
              </option>
            ))}
          </select>
        </label>
        <div className="segmented" role="tablist" aria-label={t("Side")}>
          {(["both", "Red", "Blue"] as Side[]).map((s) => (
            <button key={s} role="tab" aria-selected={side === s} className={side === s ? "seg active" : "seg"} onClick={() => setSide(s)}>
              {s === "both" ? t("Both sides") : s === "Red" ? tx("RED ({0})", { "0": sides.Red }) : tx("BLU ({0})", { "0": sides.Blue })}
            </button>
          ))}
        </div>
        <label className="lm-check">
          <input type="checkbox" checked={deathsOnly} onChange={(e) => setDeathsOnly(e.target.checked)} /> {t("Only lives that ended in a death")}
        </label>
      </div>

      {classes.length > 0 && (
        <div className="class-filter" role="tablist" aria-label={t("Class")}>
          <button role="tab" aria-selected={cls === null} className={cls === null ? "cf active" : "cf"} onClick={() => setCls(null)}>
            {t("All classes")}
          </button>
          {classes.map(([c, n]) => (
            <button key={c} role="tab" aria-selected={cls === c} className={cls === c ? "cf active" : "cf"} onClick={() => setCls(cls === c ? null : c)} title={`${capitalize(c)}: ${n}`}>
              <ClassIcon cls={c} size={20} />
              <span className="cf-n">{n}</span>
            </button>
          ))}
        </div>
      )}

      {v && (
        <p className="hint lm-count">
          {tx("{0} lives drawn, from {1} match{2} on {3}: {4} with a SourceTV demo, {5} from their own recordings. Matches without either have no routes.", {
            "0": shown.length,
            "1": v.matches,
            "2": v.matches === 1 ? "" : "es",
            "3": capitalize(v.map),
            "4": v.stvMatches,
            "5": v.matches - v.stvMatches,
          })}
        </p>
      )}
      {livesQ.isPending && <p className="hint">{t("Reading the routes…")}</p>}
      {livesQ.isError && <p className="error">{errorMessage(livesQ.error)}</p>}

      {v && <Drawing lives={shown} overview={overviewQ.data ?? null} hover={hover} onHover={setHover} />}

      {hover && (
        <p className="lm-hover">
          <span className={`team-${(hover.team ?? "").toLowerCase()}`}>{hover.team === "Red" ? "RED" : hover.team === "Blue" ? "BLU" : "?"}</span>{" "}
          {hover.class ? capitalize(hover.class) : t("unknown class")} · {formatDate(hover.playedAt)}
          {hover.roundNum !== null && <> · {tx("round {0}", { "0": hover.roundNum })}</>} · {tx("{0} s", { "0": Math.round(hover.seconds) })}
          {hover.died ? <> · {t("died")}</> : null} · {hover.stv ? t("STV") : t("POV")}
        </p>
      )}
    </div>
  );
}

/** The routes over the map's top-down picture, or over their own extent. */
function Drawing({
  lives,
  overview,
  hover,
  onHover,
}: {
  lives: LifeOnMap[];
  overview: { minX: number; maxY: number; size: number; aspect: number; image: string } | null;
  hover: LifeOnMap | null;
  onHover: (l: LifeOnMap | null) => void;
}) {
  // The frame in game units: the picture's, else the routes' own bounds.
  const frame = useMemo(() => {
    if (overview) return { minX: overview.minX, maxY: overview.maxY, w: overview.size, h: overview.size * overview.aspect };
    let [x0, x1, y0, y1] = [Infinity, -Infinity, Infinity, -Infinity];
    for (const l of lives)
      for (const [x, y] of l.points) {
        x0 = Math.min(x0, x);
        x1 = Math.max(x1, x);
        y0 = Math.min(y0, y);
        y1 = Math.max(y1, y);
      }
    if (!Number.isFinite(x0)) return null;
    const pad = 200;
    const size = Math.max(x1 - x0, y1 - y0) + pad * 2;
    return { minX: x0 - pad, maxY: y1 + pad, w: size, h: size };
  }, [lives, overview]);
  if (!frame) return <p className="hint">{t("Nothing to draw with these filters.")}</p>;

  const W = 1000;
  const H = (W * frame.h) / frame.w;
  const px = (x: number) => ((x - frame.minX) / frame.w) * W;
  const py = (y: number) => ((frame.maxY - y) / frame.h) * H;
  // One SVG path per life, broken where it jumped.
  const d = (l: LifeOnMap) => {
    let s = "";
    let prev: [number, number] | null = null;
    for (const p of l.points) {
      const far = prev !== null && Math.hypot(p[0] - prev[0], p[1] - prev[1]) > JUMP;
      s += `${prev === null || far ? "M" : "L"}${px(p[0]).toFixed(1)} ${py(p[1]).toFixed(1)}`;
      prev = p;
    }
    return s;
  };
  // Fainter the more there are, so a hundred routes still read as a pattern.
  const opacity = Math.max(0.12, Math.min(0.7, 9 / Math.sqrt(lives.length + 1)));

  return (
    <div className="lm-stage">
      <svg viewBox={`0 0 ${W} ${H.toFixed(0)}`} role="img" aria-label={t("Routes on the map")} onMouseLeave={() => onHover(null)}>
        {overview && <image href={overview.image} x={0} y={0} width={W} height={H} preserveAspectRatio="none" opacity={0.55} />}
        {lives.map((l, i) => {
          const last = l.points[l.points.length - 1];
          const on = hover === l;
          return (
            <g key={i} className={`lm-life ${l.team === "Red" ? "red" : l.team === "Blue" ? "blu" : "none"}${on ? " on" : ""}`} onMouseEnter={() => onHover(l)}>
              <path d={d(l)} style={{ opacity: on ? 1 : opacity }} />
              {/* A wider invisible twin, so a thin line is easy to hover. */}
              <path d={d(l)} className="lm-hit" />
              {l.died && last && <circle cx={px(last[0])} cy={py(last[1])} r={on ? 7 : 4} style={{ opacity: on ? 1 : Math.min(1, opacity * 1.6) }} />}
            </g>
          );
        })}
      </svg>
    </div>
  );
}
