import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type DemoView, type MatchDetail } from "../../api/types";
import { copy } from "../../lib/toast";
import { beginDownload, failDownload, useDownload } from "../../lib/downloads";
import { locale, t, tx } from "../../lib/i18n";

/**
 * The demos behind this match, and how to jump into them.
 *
 * TF2 cannot open a demo and seek in one console line (`demo_gototick` runs
 * before the demo has loaded), so the flow is two steps: open the demo once
 * with `playdemo`, then paste a `demo_gototick` for each moment. Every
 * timeline marker below copies its own tick.
 */
export function DemoPanel({ d }: { d: MatchDetail }) {
  const qc = useQueryClient();
  const [error, setError] = useState<string | null>(null);

  // The download is followed app-wide: it keeps going, and keeps reporting,
  // while you read another match.
  const download = useDownload(d.logId);
  useEffect(() => {
    if (download?.state !== "done") return;
    void qc.invalidateQueries({ queryKey: ["match", d.logId] });
    void qc.invalidateQueries({ queryKey: ["matches"] });
    void qc.invalidateQueries({ queryKey: ["spychecks", d.logId] });
    void qc.invalidateQueries({ queryKey: ["cart", d.logId] });
  }, [download?.state, d.logId, qc]);

  async function fetchStv() {
    setError(null);
    beginDownload(d.logId, `${d.map ?? "this match"}, log ${d.logId}`);
    try {
      await api.fetchStv(d.logId);
    } catch (e) {
      setError(errorMessage(e));
      failDownload(d.logId, errorMessage(e));
    }
  }

  // A deleted STV is not one this machine has: offering the download
  // again is how it comes back, with the same progress bar as the first
  // time, rather than a second button that does the same thing.
  const hasStv = d.demos.some((x) => x.kind === "stv" && !x.deleted);
  const canFetch = d.demosTfId !== null && !hasStv;

  if (d.demos.length === 0 && !canFetch) {
    return (
      <section className="panel demo-panel">
        <h2>{t("Demo")}</h2>
        <p className="hint" style={{ marginTop: 6 }}>{t("No recording of this match on this machine, and demos.tf has none either.")}</p>
      </section>
    );
  }

  return (
    <section className="panel demo-panel">
      <header className="demo-head">
        <div>
          <h2>{t("Demo")}</h2>
          {d.demos.length > 0 && (
            <p className="hint">{tx("Open the demo once, then click any marker on the round timeline to copy its{0}{1}. Jumps land 5 seconds early, so you see the lead-up.", { "0": " ", "1": <code>{t("demo_gototick")}</code> })}</p>
          )}
        </div>
      </header>

      {d.demos.map((demo) => (
        <DemoRow key={demo.demoId} demo={demo} />
      ))}

      {canFetch && (
        <div className="stv-fetch">
          <div>
            <strong>{t("SourceTV demo on demos.tf")}</strong>
            <p className="hint">{t("All 18 players, not just your view. Stopwatch matches are usually split into one demo per half, and demos.tf links one of them, so this may cover only part of the match.")}</p>
          </div>
          {download ? (
            <div className="dl-progress">
              <div className="progress-track">
                <div
                  className={download.total ? "progress-fill" : "progress-fill indeterminate"}
                  style={download.total ? { width: `${Math.round((download.bytes / download.total) * 100)}%` } : undefined}
                />
              </div>
              <span className="hint">
                {mb(download.bytes)}
                {download.total ? t(" of {0}", { "0": mb(download.total) }) : ""}
              </span>
            </div>
          ) : (
            <button onClick={() => void fetchStv()}>{t("Download to tf/demos/stv")}</button>
          )}
        </div>
      )}
      {error && <p className="error">{error}</p>}
    </section>
  );
}

function DemoRow({ demo }: { demo: DemoView }) {
  const cmd = `playdemo ${demo.playdemoArg}`;
  return (
    <div className="demo-row">
      <div className="demo-info">
        <div className="demo-name">
          <span className={demo.kind === "stv" ? "badge badge-demo" : "badge badge-pov"}>
            {demo.kind === "stv" ? t("STV") : t("POV")}
          </span>
          <code>{demo.fileName}</code>
        </div>
        <span className="hint">
          {tx("{0} min{1}{2}{3}{4}", { "0": Math.round(demo.durationS / 60), "1": demo.recordedAt !== null && t(" · recorded {0}", { "0": new Date(demo.recordedAt * 1000).toLocaleString(locale()) }), "2": t(" · {0}% of this match inside", { "0": Math.round(demo.logShare * 100) }), "3": demo.markers > 0 && t(" · {markers} killstreak marker{1}", { "1": demo.markers === 1 ? "" : "s", markers: demo.markers }), "4": demo.approximate && t(" · jump positions estimated") })}
        </span>
      </div>
      {demo.deleted ? (
        // Handing over a playdemo command for a file that is not there
        // sends someone into TF2 to be told so by the console.
        <span className="hint" title={t("Deleted to save space. Everything read from it is still here.")}>{t("deleted · download again below")}</span>
      ) : (
        <button className="primary" onClick={() => void copy(cmd, "playdemo command")} title={cmd}>{t("Copy playdemo")}</button>
      )}
    </div>
  );
}

function mb(bytes: number): string {
  return `${(bytes / 1_000_000).toFixed(1)} MB`;
}
