# Flashwave.tf 0.9.0 alpha

**On 0.5.0 or later (Windows)?** The app offers this update itself: take it
from the card, or Settings › Updates › Check for updates.

**New install on Windows:** download the `-setup.exe` and run it. **Windows
will say "Windows protected your PC"** — the installer is not code-signed.
Right-click the download → **Properties** → tick **Unblock** → **OK**. Check
it is my build first:

```powershell
Get-FileHash .\Flashwave.tf_0.9.0_x64-setup.exe -Algorithm SHA256
```

**Linux:** the `.AppImage` runs on most distributions
(`chmod +x Flashwave.tf_*.AppImage && ./Flashwave.tf_*.AppImage`); the `.deb`
is for Debian, Ubuntu and Mint (`sudo apt install ./Flashwave.tf_*.deb`).
They are built on GitHub a few minutes after this release appears.

The hashes are at the bottom. Updating keeps everything.

---

Teams get pages of their own, your profile becomes one page about you, and a
match now shows the two teams facing each other. Ratings are model v11: no
more points for where a respawn wave fell.

## Teams

- **Every season as a tile**, with ETF2L's banner, its podiums and MVPs, and
  every division's table.
- **A team page like a player's profile**: tag, links, former names, ETF2L
  roles, cups entered, lineups season by season, fixtures and head-to-head.
- **ETF2L transfers**: who joined and left when, so a team's history and a
  player's teams have dates. A medal goes to the roster that won it — not to
  a merc, and not to someone who left before the final (twatter).
- **Your teams** sit on top, with your record and rating for each.

## Your profile

- **You at the top**: your ETF2L avatar, division, rosters with their tags,
  "on ETF2L since", medals, and your rating beside your name.
- **One page**: Rating, then Career (teams by season, officials, trends.tf,
  achievements, team history), then Teammates.
- **The Teammates tab is gone.** Your regulars are on your profile, your
  record with a team is on its page, and another player's page says how you
  rated in games with them.

## Matches

- **A new header**: both teams across the score with their ETF2L logos,
  countries and records that season, the map's overview behind them. Click a
  team for its page.
- **Ping and Pyro reflects** from demos: who reflected what, whether it hit,
  and whether it was headed at the team (ivg, ImABush).
- **ETF2L's SourceTV uploads** are fetched when demos.tf has none, and each
  demo is linked to the right log.
- **Read again**, in Demo linking: reads the match's demos and server log
  again with this version, so a match catches up after an update.
- **RSP on the scoreboard**: the seconds of respawn your caps cost your own
  team. Shown, not rated.

## Ratings

- **Model v11**, checked class by class against who won:
  - The cost of a cap to your team's respawns is out of every rating. It only
    existed on KOTH and 5CP, so a rating meant one thing on payload and
    another off it.
  - Medic ubers, Heavy "kills in context" and Spy deaths are out: each added
    nothing the rest did not already say. Medic drops stay.
  - The Spy, reworked with the Spy mains (Suomipe, Taiga): medic picks up,
    caps and backstabs out.
- Ratings move about 0.01–0.05 a game; the Medic model picks the winner
  1.2 points more often on recent games.

## Sync

- **The sync card** says which of its nine steps it is on, which match it is
  reading, and has a **Cancel** button. What landed before cancelling is kept.
- **A sync at startup** when new Highlander logs are up.
- **League officials** that trends.tf never linked a log to are found on
  logs.tf by a player of each team.

## Faster and friendlier

- **Offline**, a profile or season screen says so in about 2 seconds, not
  20 to 40.
- **Fewer requests** to logs.tf, trends.tf and ETF2L: bigger pages, answers
  reused when nothing changed, and the app names itself to the sites it
  asks.

## Fixes

- Season names in team headers and medal tooltips read "Season 33 (Spring
  2025)", not "[object Object]".
- Versions of one map count as one map on team pages (pl_upward, not
  pl_upward_f12).

## After updating

The first start rates your matches with model v11 (a minute or two). The
league that ships inside the app is rated with v11 too.

**SHA-256**

```
TODO-fill-from-the-build  Flashwave.tf_0.9.0_x64-setup.exe
TODO-fill-from-the-build  Flashwave.tf_0.9.0_x64_en-US.msi
```

The Linux files are built on GitHub; their hashes are on the release page
beside each file.
