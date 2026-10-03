# Flashwave.tf 0.8.0 alpha

**On 0.5.0 or later (Windows)?** The app offers this update itself: take it
from the card, or Settings › Updates › Check for updates.

**New install on Windows:** download the `-setup.exe` and run it. **Windows
will say "Windows protected your PC"** — the installer is not code-signed.
Right-click the download → **Properties** → tick **Unblock** → **OK**. Check
it is my build first:

```powershell
Get-FileHash .\Flashwave.tf_0.8.0_x64-setup.exe -Algorithm SHA256
```

**New: Linux.** The `.AppImage` runs on most distributions
(`chmod +x Flashwave.tf_*.AppImage && ./Flashwave.tf_*.AppImage`); the `.deb`
is for Debian, Ubuntu and Mint (`sudo apt install ./Flashwave.tf_*.deb`).
They are built on GitHub a few minutes after this release appears, so give
it a quarter of an hour. This is the first Linux build: please say in Issues
or on Discord how it runs.

The hashes are at the bottom. Updating keeps everything.

---

Every ETF2L Highlander player has a profile now — medals, divisions, ranks
and ratings against the whole league — and it works from the first start:
the league comes built in. Plus a Linux build, and ratings refitted on the
league.

## The league, built in

- **Every official since 2020**, from every division: who played, for which
  team, and how they rated. It ships inside the app (14 MB), so the player
  search, profiles, medals and ranks work at once — no download first.
- **Ratings are against the league now.** 1.00 is a typical ETF2L
  Highlander game, not a typical game of yours. Your own ratings move to the
  new scale on the first start.
- **Settings › League sample** still downloads the match logs themselves,
  slowly, for anyone who wants every official's kill-by-kill detail.

## Players

- **The Players tab** searches everyone: every official of the last six
  years and everyone in your matches. A **profile** has their avatar,
  country and flag, aliases, main class, current team, highest division,
  **medals**, a season-by-season team timeline, recent officials, and
  **career numbers from trends.tf**.
- **Medals** worked out from ETF2L's playoffs and tables, every season and
  division checked: gold and silver from the Grand Final, bronze from the
  3rd place match.
- **Ranks** per season, division and class, and a **Top players** table.
- **MVPs** for every Grand Final, per class, HLTV-style.
- **Click a name on the scoreboard** for a card: medals, division, main
  class rating and best rank.
- **Divisions beside every name** on a match page, as they were that
  season, and each side's average.

## Your games

- **Where you play**: on your profile, per map, where your kills and deaths
  happen by callout, where you stand and the moves you make most.
- **Momentum** on payload: how far the cart got, second by second, every
  fight on it, and where it got stuck.
- **"Who you played" by division**: your rating against each division's
  players.

## Ratings

- **Model v9**: seven classes refitted on ~4,650 matchups each, most of
  them ETF2L officials, each kept only because it picks the winner more
  often. Heavy 74.9 → 79.0%, Spy 71.6 → 74.4%, Engineer 74.9 → 77.2%.

## Fixes

- **After a match**: the app now waits for the demo to have something in it
  before looking for the log; it gave up too early before.
- **Map images and callouts**: an image that is the app's own counts as
  the app default, and the Maps section has proper buttons.

## After updating

The first start moves your ratings to the league's scale (about a minute),
and model v9 changes most classes' numbers a little.

**SHA-256**

```
16AA8326D1580E7A3B0F6A11057CF147AE349ED63944A40EE05B673107AF8097  Flashwave.tf_0.8.0_x64-setup.exe
7FE4EA20D793D1D127C7B479F68E11196AA39FFFDA2C9335E56EE8715265C9CA  Flashwave.tf_0.8.0_x64_en-US.msi
```

The Linux files are built on GitHub; their hashes are on the release page
beside each file.
