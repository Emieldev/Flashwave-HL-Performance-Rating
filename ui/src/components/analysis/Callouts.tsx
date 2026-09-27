import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type CalloutFile, type CalloutZone } from "../../api/types";
import { t, tx } from "../../lib/i18n";

/**
 * Q28 (Flashy): callouts as zones on the map -- the jigsaw -- and an editor
 * to draw and fix them, because seeded callouts are drafts until someone
 * who plays the map has checked them. Zones are in game units.
 */

type Pt = [number, number];

export function useCallouts(map: string | null) {
  return useQuery({
    queryKey: ["callouts", map],
    queryFn: () => api.getCallouts(map ?? ""),
    enabled: map !== null,
    staleTime: Infinity,
  });
}

function inside([x, y]: Pt, poly: Pt[]): boolean {
  let c = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [x1, y1] = poly[i];
    const [x2, y2] = poly[j];
    if (y1 > y !== y2 > y && x < ((x2 - x1) * (y - y1)) / (y2 - y1) + x1) c = !c;
  }
  return c;
}

/** The first zone holding a point, as the Rust side decides it. */
export function zoneAt(zones: CalloutZone[], p: Pt): number | null {
  const i = zones.findIndex((z) => z.points.length >= 3 && inside(p, z.points));
  return i < 0 ? null : i;
}

/** Twelve hues, cycled: neighbours differ, and the names carry identity. */
const HUES = [205, 25, 140, 285, 50, 330, 170, 0, 95, 245, 310, 70];

/** The zones, drawn in SVG under the marks. `px` maps game units to the canvas. */
export function ZoneShapes(props: {
  zones: CalloutZone[];
  px: (p: Pt) => Pt;
  selected: number | null;
  counts?: Map<number, number>;
  drawing?: Pt[];
  /** 0 is outlines only, 1 as solid as the zones go. */
  opacity?: number;
  labels?: boolean;
}) {
  const { zones, px, selected, counts, drawing, opacity = 0.5, labels = true } = props;
  const most = counts ? Math.max(1, ...counts.values()) : 1;
  // On a mirrored map a zone takes its side's colour -- RED's ground red,
  // BLU's blue, the shared middle grey -- so a glance says whose half a
  // fight was in. One-way maps keep a hue per zone.
  const names = new Set(zones.map((z) => z.name));
  const paired = (n: string) => {
    const m = /^(RED|BLU) (.+)$/.exec(n);
    return m !== null && names.has(`${m[1] === "RED" ? "BLU" : "RED"} ${m[2]}`) ? m[1] : null;
  };
  const mirrored = zones.some((z) => paired(z.name) !== null);
  return (
    <g className="co-layer">
      {zones.map((z, i) => {
        if (z.points.length < 3) return null;
        const pts = z.points.map(px);
        const d = pts.map(([x, y], j) => `${j ? "L" : "M"}${x.toFixed(1)},${y.toFixed(1)}`).join("") + "Z";
        const cx = pts.reduce((a, p) => a + p[0], 0) / pts.length;
        const cy = pts.reduce((a, p) => a + p[1], 0) / pts.length;
        const side = mirrored ? paired(z.name) : null;
        const hue = side === "RED" ? 2 : side === "BLU" ? 207 : HUES[i % HUES.length];
        const sat = mirrored && side === null ? 8 : 60;
        // With counts, the fill says how busy a zone was; without, every
        // zone is the same faint wash and the outline does the work.
        const n = counts?.get(i) ?? 0;
        const fill = (counts ? 0.06 + 0.4 * (n / most) : 0.2) * opacity * 2;
        return (
          <g key={i} className={selected === i ? "co-zone selected" : "co-zone"}>
            <path
              d={d}
              fill={`hsla(${hue}, ${sat}%, 55%, ${Math.min(0.85, fill).toFixed(3)})`}
              stroke={`hsla(${hue}, ${sat + 10}%, 70%, ${(0.25 + 0.6 * Math.min(1, opacity * 1.5)).toFixed(2)})`}
              strokeWidth={selected === i ? 2.5 : 1.2}
            />
            {(labels || selected === i) && (
              <text x={cx} y={cy} className="co-label" textAnchor="middle" dominantBaseline="middle">
                {side ? z.name.slice(4) : z.name}
                {counts && n > 0 ? ` ${n}` : ""}
              </text>
            )}
          </g>
        );
      })}
      {drawing && drawing.length > 0 && (
        <g className="co-drawing">
          <polyline points={drawing.map(px).map(([x, y]) => `${x},${y}`).join(" ")} fill="none" strokeWidth={2} />
          {drawing.map(px).map(([x, y], i) => (
            <circle key={i} cx={x} cy={y} r={3.5} />
          ))}
        </g>
      )}
    </g>
  );
}

/** The editing panel beside the map. Drawing itself happens on the map. */
export function CalloutEditor(props: {
  map: string;
  file: CalloutFile;
  drawing: Pt[];
  onDrawing: (p: Pt[]) => void;
  selected: number | null;
  onSelect: (i: number | null) => void;
  onClose: () => void;
}) {
  const { map, file, drawing, onDrawing, selected, onSelect, onClose } = props;
  const qc = useQueryClient();
  const [zones, setZones] = useState<CalloutZone[]>(file.zones);
  const [names, setNames] = useState<string[]>(file.names);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dirty, setDirty] = useState(false);

  // The map draws from the query's data; the editor's copy is written
  // through to it so every change shows at once.
  const show = (z: CalloutZone[], n: string[] = names) => {
    setZones(z);
    setNames(n);
    setDirty(true);
    qc.setQueryData<CalloutFile>(["callouts", map], (f) => (f ? { ...f, zones: z, names: n } : f));
  };

  function finish() {
    const n = name.trim();
    if (drawing.length < 3 || !n) return;
    show([{ name: n, points: drawing }, ...zones], names.filter((x) => x !== n));
    onDrawing([]);
    setName("");
    onSelect(0);
  }

  function remove(i: number) {
    const z = zones[i];
    show(
      zones.filter((_, j) => j !== i),
      names.includes(z.name) ? names : [...names, z.name],
    );
    onSelect(null);
  }

  function rename(i: number, n: string) {
    show(zones.map((z, j) => (j === i ? { ...z, name: n } : z)));
  }

  /** Earlier in the list wins where zones overlap. */
  function move(i: number, by: number) {
    const j = i + by;
    if (j < 0 || j >= zones.length) return;
    const next = [...zones];
    [next[i], next[j]] = [next[j], next[i]];
    show(next);
    onSelect(j);
  }

  async function save() {
    setBusy(true);
    setError(null);
    try {
      const saved = await api.saveCallouts(map, { ...file, zones, names, draft: false });
      qc.setQueryData(["callouts", map], saved);
      void qc.invalidateQueries({ queryKey: ["positions"] });
      setDirty(false);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function reset() {
    setBusy(true);
    try {
      const back = await api.resetCallouts(map);
      qc.setQueryData(["callouts", map], back);
      setZones(back.zones);
      setNames(back.names);
      setDirty(false);
      onSelect(null);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  function discard() {
    if (dirty) void qc.invalidateQueries({ queryKey: ["callouts", map] });
    onDrawing([]);
    onClose();
  }

  return (
    <aside className="co-editor">
      <h3>{t("Callouts on {0}", { "0": map })}</h3>
      <p className="hint">
        {drawing.length === 0
          ? t("Click the map to draw a zone, corner by corner. Where zones overlap, the one higher in the list wins.")
          : tx("{0} corners. Name it and finish, or keep clicking.", { "0": drawing.length })}
      </p>
      <div className="co-draw">
        <input list="co-names" placeholder={t("Name, e.g. Cliff")} value={name} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && finish()} />
        <datalist id="co-names">
          {names.map((n) => (
            <option key={n} value={n} />
          ))}
        </datalist>
        <button className="primary" disabled={drawing.length < 3 || !name.trim()} onClick={finish}>{t("Finish zone")}</button>
        {drawing.length > 0 && (
          <button className="linkish" onClick={() => onDrawing(drawing.slice(0, -1))}>{t("Undo corner")}</button>
        )}
      </div>
      {names.length > 0 && (
        <div className="co-names">
          <span className="hint">{t("Known, not drawn yet:")}</span>
          {names.map((n) => (
            <button key={n} className="co-chip" onClick={() => setName(n)}>{n}</button>
          ))}
        </div>
      )}
      <ol className="co-list">
        {zones.map((z, i) => (
          <li key={i} className={selected === i ? "selected" : undefined} onClick={() => onSelect(i)}>
            <input value={z.name} onChange={(e) => rename(i, e.target.value)} aria-label={t("Zone name")} />
            <button className="linkish" title={t("Earlier wins where zones overlap")} onClick={() => move(i, -1)}>↑</button>
            <button className="linkish" onClick={() => move(i, 1)}>↓</button>
            <button className="linkish" onClick={() => remove(i)}>{t("Delete")}</button>
          </li>
        ))}
      </ol>
      <div className="co-actions">
        <button className="primary" disabled={busy || !dirty} onClick={() => void save()}>{t("Save")}</button>
        <button disabled={busy} onClick={discard}>{dirty ? t("Discard") : t("Done")}</button>
        {file.origin === "yours" && (
          <button className="linkish" disabled={busy} onClick={() => void reset()}>{t("Back to the built-in callouts")}</button>
        )}
      </div>
      {error && <p className="error">{error}</p>}
      <p className="hint co-where">{t("Saved to the callouts folder in the app's data folder, one file per map, never overwritten by an update.")}</p>
    </aside>
  );
}
