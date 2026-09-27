# Flashwave.tf 0.6.0

Windows only. **On 0.5.0 already?** The app offers this update itself: take
it from the card, or Settings › Updates › Check for updates.

New install: download the `-setup.exe` and run it. **Windows will say
"Windows protected your PC"** — the installer is not code-signed.
Right-click the download → **Properties** → tick **Unblock** → **OK**. Check
it is my build first:

```powershell
Get-FileHash .\Flashwave.tf_0.6.0_x64-setup.exe -Algorithm SHA256
```

The hash is at the bottom. Updating keeps everything.

---

Callouts on the map and where everyone played, spychecks, the cart, a Teams
tab for all of ETF2L Highlander, matches from a demo alone, and a sync
that is two to three times faster.

## Callouts and positions

- **Callouts on the kill map** for the whole Highlander pool — Product,
  Proot, Ashville, Vigil, Upward and Swiftwater — traced from Java59's
  callout maps, coloured by whose side they are on. Tick **Callouts**; the
  busiest zones are the brightest.
- **Fix them yourself.** **Edit** on the kill map: click a zone, drag its
  corners, drag the dot on an edge to add a corner, right-click one to
  remove it, or draw a new zone. Your version is kept and never
  overwritten by an update. Where zones overlap, the smaller one wins.
- **Positions** (match page, with an STV): where each player spent the
  match, by callout — who anchors, who rotates.
- **Zoom and pan** the map: scroll to zoom, drag to move. Zone opacity and
  names can be turned down.

## New on the match page

- **Spychecks** (ivg): every hit on a fully cloaked Spy — not fading in, not
  blinking, not on fire or jarated — one per attacker per two seconds, so
  a held minigun counts once. Who checked, who was found, and a
  `demo_gototick` for each.
- **The cart** (payload): seconds BLU was three or more players up and the
  cart stood still — split into nobody on it and a defender blocking — each
  stall with its jump, and how much the cart moved after the first, second
  and third won fight of a round.
- **Demo linking**: drag a `.dem` onto the match page (or click to pick one)
  to link it to that match. It is checked first — the same map, the same
  players, the kills lining up — and placed on the log's clock exactly.
- **Without an STV demo**, the panels that need one stay on the page,
  blurred, and say so, with a way to the download.

## Teams

A new **Teams** tab: every ETF2L Highlander team of the last year. Each
season's divisions and map pool, and a page per team — record, win rate on
every map, results, and who played, with their rating where they appear in
your matches. It fills in over your next few syncs.

## A match with no log

Settings › Import › **A match with no log** (beowulf): for a server with no
logs.tf, pick the match's demo and it becomes the match — scoreboard,
fights, rating and all. Checked against a real log: every kill, all ubers,
drops, headshots and backstabs the same, damage within 2%.

## Switch player

Click your name top right (vilden): switch to another SteamID, or to one of
the last few this PC has used. Nothing is deleted, and switching back is
instant.

## The rating

**Delaying your own team's spawn now costs you** (boSe, ivg). Measured the way
they asked — the extra wait each dead teammate actually had, from the log —
not a head count: a cap that cost a teammate a whole respawn wave (8 s or
more) goes with losing the round, one costing a couple of seconds does not.
In the rating for Scout, Soldier, Pyro, Medic and Sniper, where it made the
rating better at picking the winner; not for the other four, where it did not.

## Faster

- A sync's fights pass: **21 s → 7 s**. A full rebuild: **33 s → 21 s**.
- Reading a demo is about 13% faster and uses less memory.

## After updating

The first sync recomputes every match once (for the spawn delays) and reads
the ETF2L seasons, which takes a few minutes the first time. Ratings for
the five classes above move slightly.

**SHA-256**

```
04D793BAB10988AFC7EA98D815A13FE1A6C777B722C130B5E5986DE48A503663  Flashwave.tf_0.6.0_x64-setup.exe
06511037FE0898733EBEA7A4EFE37525D857534019E3C4D6AA90E57E83F5F043  Flashwave.tf_0.6.0_x64_en-US.msi
```
