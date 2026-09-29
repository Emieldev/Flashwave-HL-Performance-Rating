/**
 * Asking the app to open a page from somewhere deep inside another one --
 * the "How ratings work" link on a match's class matchups -- without passing
 * a navigation callback down through every component in between.
 */
const EVENT = "hl:goto";

export type Destination = "rating";

export function goTo(page: Destination) {
  window.dispatchEvent(new CustomEvent<Destination>(EVENT, { detail: page }));
}

/** For the app shell: call `open` whenever something asks for a page. */
export function onGoTo(open: (page: Destination) => void): () => void {
  const handler = (e: Event) => open((e as CustomEvent<Destination>).detail);
  window.addEventListener(EVENT, handler);
  return () => window.removeEventListener(EVENT, handler);
}
