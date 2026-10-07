# Flashwave.tf 0.10.0 alpha

**On 0.5.0 or later (Windows)?** The app offers this update itself: take it
from the card, or Settings › Updates › Check for updates.

**New install on Windows:** download the `-setup.exe` and run it. **Windows
will say "Windows protected your PC"** — the installer is not code-signed.
Right-click the download → **Properties** → tick **Unblock** → **OK**. Check
it is my build first:

```powershell
Get-FileHash .\Flashwave.tf_0.10.0_x64-setup.exe -Algorithm SHA256
```

**Linux:** the `.AppImage` runs on most distributions
(`chmod +x Flashwave.tf_*.AppImage && ./Flashwave.tf_*.AppImage`); the `.deb`
is for Debian, Ubuntu and Mint (`sudo apt install ./Flashwave.tf_*.deb`).
They are built on GitHub a few minutes after this release appears.

The hashes are at the bottom. Updating keeps everything.

---

Logs now come from **drops.tf**, so a sync is several times faster and no
longer stalls on logs.tf's limits. Demos link properly on combined logs, a
player's lives on a map can be drawn across every match, a match's kind can
be set by hand, and you can bookmark pages. Ratings are unchanged (model v11).

## Logs from drops.tf

- **drops.tf, by Icewind, is now where logs come from first**: logs.tf's own
  logs and raw server logs, mirrored, with no rate limit. logs.tf is only
  asked for what drops.tf has not got yet (a game from the last hour).
  Thank you, Icewind.
- **A first sync is about 4–6 times faster per log**, and fetches up to 300
  raw server logs a sync instead of 30, so a long history fills in after a
  few syncs, not dozens.
- The sync card says where logs come from, and its time left follows the
  sync's own pace.
- **ETF2L's API moves** to `api.etf2l.org` on 1 November. The app switches
  by itself, and falls back to the old address if the new one does not
  answer.

## Demos

- **Combined logs**: a demo of one map in a combined log is accepted ("on
  koth_product_final, and this match is on product + proot" no more), and
  downloading a combined log's STV fetches every part's demo (ivg, Emiel).
- **demos.tf demos further back** are found: the search now walks your whole
  demos.tf history over a few syncs instead of stopping 1,000 demos back
  (Emiel).
- **Add a demo by its demos.tf link** in Demo linking: paste
  `https://demos.tf/990239`, and it is downloaded and checked against the
  match (Emiel).
- **"None of its kills line up"** on an old match was its server log not
  being downloaded yet. Linking a demo now fetches it first (Emiel).
- **The movement map** breaks a route at a teleporter instead of drawing a
  line across the map, and shows sentries, dispensers and teleporter
  entrances and exits (Emiel).

## Players

- **Lives on a map**, a new tab on every player's page: pick a map, a class
  and a side, and every route the demos on your PC followed them through is
  drawn on the map — every Spy cross on Product, RED and BLU apart. It says
  how many matches that is (Emiel).
- **Bookmarks**: a ☆ beside a match, player, team or season keeps it; the ★
  in the top bar lists them (Clark).

## Matches

- **Change what kind of match it is**: click the Official / Scrim / Pug
  badge, on the list or the match page, and pick the right one when the app
  got it wrong. It stays through every sync, shows a ✎, and *Automatic*
  gives it back to the app.
- **Officials show ETF2L's result** as the score, with the logs' rounds
  beneath when they differ (Clark).
- **Import** beside Refresh: add a log by its id or logs.tf link without
  going to Settings.
- **With STV**: only the matches with their SourceTV demo on your PC (Emiel).
- **Sort by a class**: Kills on Soldier, Deaths to Sniper and so on, counted
  from the server logs (Clark).
- **The class counts follow the filters**: Officials, Scrims, Pugs, the
  period and the map (Emiel).

## Fixes

- A damaged database is set aside and the app offers a backup, instead of
  failing to start because Windows still held the file.

## Thanks

Icewind for drops.tf and demos.tf, and ivg, Emiel and Clark for the tickets
behind most of this release.

**SHA-256**

```
HASHES
```

The Linux files are built on GitHub; their hashes are on the release page
beside each file.
