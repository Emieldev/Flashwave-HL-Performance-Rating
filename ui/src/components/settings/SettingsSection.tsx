import { createContext, useContext, useEffect, useId, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { t } from "../../lib/i18n";
import { SettingsIcon, type IconName } from "./icons";

/**
 * One section of Settings (Flashy's UX pass): an icon, a title, a line
 * saying what it is for, an (i) with the longer story, and a fold that is
 * remembered. The panel inside keeps its own controls; its old heading and,
 * with `hideIntro`, its old first paragraph are hidden, since this header
 * now says both.
 */

const KEY = (id: string) => `hl.settings.closed.${id}`;

function storedClosed(id: string): boolean {
  try {
    return localStorage.getItem(KEY(id)) === "1";
  } catch {
    return false;
  }
}

function storeClosed(id: string, closed: boolean) {
  try {
    localStorage.setItem(KEY(id), closed ? "1" : "0");
  } catch {
    // Blocked storage: the fold still works for this session.
  }
}

/** What the page shares with its sections: the search, folding all, and who matched. */
export interface SettingsShared {
  query: string;
  /** Bumped by "Collapse all" / "Expand all"; the value says which. */
  foldAll: { n: number; closed: boolean };
  report: (id: string, matches: boolean) => void;
  /** A section asked to open from the side menu. */
  focus: { id: string; n: number } | null;
}

export const SettingsContext = createContext<SettingsShared>({ query: "", foldAll: { n: 0, closed: false }, report: () => {}, focus: null });

export function SettingsSection({
  id,
  icon,
  title,
  summary,
  info,
  keywords = "",
  hideIntro = false,
  children,
}: {
  id: string;
  icon: IconName;
  title: string;
  summary: ReactNode;
  info?: ReactNode;
  /** Extra words to be found by, English ones included whatever the language. */
  keywords?: string;
  /** Hide the panel's own first paragraph: this header says it now. */
  hideIntro?: boolean;
  children: ReactNode;
}) {
  const shared = useContext(SettingsContext);
  const [closed, setClosed] = useState(() => storedClosed(id));
  const [infoOpen, setInfoOpen] = useState(false);
  const body = useRef<HTMLDivElement>(null);
  const head = useRef<HTMLElement>(null);
  const [bodyText, setBodyText] = useState("");
  const infoId = useId();

  // What the section says, for the search: read after each render, so it
  // follows the language and whatever the panel is showing now.
  useLayoutEffect(() => {
    const text = body.current?.textContent ?? "";
    if (text !== bodyText) setBodyText(text);
  });

  const q = shared.query.trim().toLowerCase();
  const infoText = typeof info === "string" ? info : "";
  const summaryText = typeof summary === "string" ? summary : "";
  const matches = q === "" || `${title} ${summaryText} ${infoText} ${keywords} ${bodyText}`.toLowerCase().includes(q);
  const report = shared.report;
  useEffect(() => report(id, matches), [id, matches, report]);

  // "Collapse all" and "Expand all".
  const lastFold = useRef(shared.foldAll.n);
  useEffect(() => {
    if (shared.foldAll.n === lastFold.current) return;
    lastFold.current = shared.foldAll.n;
    setClosed(shared.foldAll.closed);
    storeClosed(id, shared.foldAll.closed);
  }, [shared.foldAll, id]);

  // Picked in the side menu: open it and bring it into view.
  useEffect(() => {
    if (shared.focus?.id !== id) return;
    setClosed(false);
    storeClosed(id, false);
    head.current?.scrollIntoView({ behavior: "smooth", block: "start" });
  }, [shared.focus, id]);

  const toggle = () => {
    setClosed((c) => {
      storeClosed(id, !c);
      return !c;
    });
  };
  // A search opens what it finds.
  const open = q !== "" ? true : !closed;

  return (
    <section ref={head} id={`settings-${id}`} className={`set-section${hideIntro ? " hide-intro" : ""}${open ? "" : " closed"}`} hidden={!matches}>
      <header className="set-head">
        <button className="set-toggle" onClick={toggle} aria-expanded={open} aria-controls={`settings-body-${id}`}>
          <span className="set-icon" aria-hidden>
            <SettingsIcon name={icon} />
          </span>
          <span className="set-titles">
            <span className="set-title">{title}</span>
            <span className="set-summary">{summary}</span>
          </span>
          <span className="set-chevron" aria-hidden>
            <SettingsIcon name="chevron" />
          </span>
        </button>
        {info && (
          <span className="set-info" onMouseEnter={() => setInfoOpen(true)} onMouseLeave={() => setInfoOpen(false)}>
            <button
              className="set-info-btn"
              aria-label={t("About {0}", { "0": title })}
              aria-describedby={infoOpen ? infoId : undefined}
              aria-expanded={infoOpen}
              onClick={() => setInfoOpen((o) => !o)}
              onBlur={() => setInfoOpen(false)}
            >
              <SettingsIcon name="info" />
            </button>
            {infoOpen && (
              <span className="set-info-pop" role="tooltip" id={infoId}>
                {info}
              </span>
            )}
          </span>
        )}
      </header>
      <div className="set-body" id={`settings-body-${id}`} ref={body} hidden={!open}>
        {children}
      </div>
    </section>
  );
}
