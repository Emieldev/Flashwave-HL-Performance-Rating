/**
 * Asking the app to open a page from somewhere deep inside another one --
 * the "How ratings work" link on a match's class matchups -- without passing
 * a navigation callback down through every component in between.
 */
const EVENT = "hl:goto";

export type Destination = "rating" | "players" | "teams";

const PLAYER_EVENT = "hl:player";
let pendingPlayer: number | null = null;

/** The Players tab, on one player's profile (Q40's "Open profile"). */
export function openPlayer(accountId: number) {
  pendingPlayer = accountId;
  goTo("players");
  window.dispatchEvent(new CustomEvent<number>(PLAYER_EVENT, { detail: accountId }));
}

/** For the Players tab: the player asked for before it was first shown. */
export function takePendingPlayer(): number | null {
  const p = pendingPlayer;
  pendingPlayer = null;
  return p;
}

/** For the Players tab: a player asked for while it is mounted. */
export function onOpenPlayer(open: (accountId: number) => void): () => void {
  const handler = (e: Event) => {
    pendingPlayer = null;
    open((e as CustomEvent<number>).detail);
  };
  window.addEventListener(PLAYER_EVENT, handler);
  return () => window.removeEventListener(PLAYER_EVENT, handler);
}

const TEAM_EVENT = "hl:team";
let pendingTeam: number | null = null;

/** The Teams tab, on one team's page (from a match header). */
export function openTeam(teamId: number) {
  pendingTeam = teamId;
  goTo("teams");
  window.dispatchEvent(new CustomEvent<number>(TEAM_EVENT, { detail: teamId }));
}

/** For the Teams tab: the team asked for before it was first shown. */
export function takePendingTeam(): number | null {
  const id = pendingTeam;
  pendingTeam = null;
  return id;
}

/** For the Teams tab: a team asked for while it is mounted. */
export function onOpenTeam(open: (teamId: number) => void): () => void {
  const handler = (e: Event) => {
    pendingTeam = null;
    open((e as CustomEvent<number>).detail);
  };
  window.addEventListener(TEAM_EVENT, handler);
  return () => window.removeEventListener(TEAM_EVENT, handler);
}

export function goTo(page: Destination) {
  window.dispatchEvent(new CustomEvent<Destination>(EVENT, { detail: page }));
}

/** For the app shell: call `open` whenever something asks for a page. */
export function onGoTo(open: (page: Destination) => void): () => void {
  const handler = (e: Event) => open((e as CustomEvent<Destination>).detail);
  window.addEventListener(EVENT, handler);
  return () => window.removeEventListener(EVENT, handler);
}
