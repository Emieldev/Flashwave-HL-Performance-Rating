import { useEffect, useState } from "react";

/**
 * Back and forward, the way a browser does it.
 *
 * Where you are is a tab, plus the match open on top of it if there is one.
 * Every move to somewhere new is added after the current place; going back
 * walks down the list and forward walks up it; and moving somewhere new
 * after going back drops what was ahead, so forward can never land on a place
 * that no longer follows from here.
 *
 * The mouse's back and forward buttons (4 and 5) and Alt+Left / Alt+Right
 * drive it. This is kept in React rather than in `window.history`: the app is
 * one page with no URLs, and the WebView's own history would have nothing to
 * go back to -- or, in development, a page reload.
 */
export type Place<Tab extends string> = { tab: Tab; log: number | null };

type History<Tab extends string> = { stack: Place<Tab>[]; at: number };

// Enough for any session; the oldest fall off rather than growing forever.
const LIMIT = 100;

const same = <Tab extends string>(a: Place<Tab>, b: Place<Tab>) => a.tab === b.tab && a.log === b.log;

export function useNavigation<Tab extends string>(start: Tab) {
  const [h, setH] = useState<History<Tab>>({ stack: [{ tab: start, log: null }], at: 0 });
  const here = h.stack[h.at];

  const visit = (p: Place<Tab>) =>
    setH((h) => {
      if (same(h.stack[h.at], p)) return h;
      const stack = [...h.stack.slice(0, h.at + 1), p].slice(-LIMIT);
      return { stack, at: stack.length - 1 };
    });
  const back = () => setH((h) => (h.at > 0 ? { ...h, at: h.at - 1 } : h));
  const forward = () => setH((h) => (h.at < h.stack.length - 1 ? { ...h, at: h.at + 1 } : h));

  useEffect(() => {
    // Buttons 3 and 4 in the DOM's numbering are the mouse's back and forward.
    // Handled on the way down *and* up: WebView2 acts on one or the other
    // depending on the version, and either left alone can navigate the page.
    const onMouse = (e: MouseEvent) => {
      if (e.button !== 3 && e.button !== 4) return;
      e.preventDefault();
      if (e.type === "mouseup") (e.button === 3 ? back : forward)();
    };
    const onKey = (e: KeyboardEvent) => {
      if (!e.altKey || (e.key !== "ArrowLeft" && e.key !== "ArrowRight")) return;
      e.preventDefault();
      (e.key === "ArrowLeft" ? back : forward)();
    };
    window.addEventListener("mousedown", onMouse);
    window.addEventListener("mouseup", onMouse);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onMouse);
      window.removeEventListener("mouseup", onMouse);
      window.removeEventListener("keydown", onKey);
    };
    // `back` and `forward` only ever call `setH` with an updater, so the
    // first ones stay correct for the life of the page.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return {
    tab: here.tab,
    log: here.log,
    /** Go to a tab, closing any open match. */
    go: (tab: Tab) => visit({ tab, log: null }),
    /** Open a match over the current tab. */
    open: (log: number | null) => visit({ tab: here.tab, log }),
    /**
     * Leave the open match: back through history when that is where it was
     * opened from, so the mouse's forward button can return to it; otherwise
     * straight to the tab underneath.
     */
    close: () => {
      const prev = h.stack[h.at - 1];
      if (prev && prev.tab === here.tab && prev.log === null) back();
      else visit({ tab: here.tab, log: null });
    },
    back,
    forward,
  };
}
