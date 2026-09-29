# Flashwave.tf 0.7.0 alpha

Windows only. **On 0.5.0 or later?** The app offers this update itself: take
it from the card, or Settings › Updates › Check for updates.

New install: download the `-setup.exe` and run it. **Windows will say
"Windows protected your PC"** — the installer is not code-signed.
Right-click the download → **Properties** → tick **Unblock** → **OK**. Check
it is my build first:

```powershell
Get-FileHash .\Flashwave.tf_0.7.0_x64-setup.exe -Algorithm SHA256
```

The hash is at the bottom. Updating keeps everything.

---

Your game shows up after the match even when logs.tf is
refusing you, every class's rating is explained on its own page, and the
kill map ships with map images for everyone.

## After a match

- **Your game, without pressing Sync.** When TF2 finishes the demo the app
  waits 10 seconds, then checks every 5 seconds for two minutes and fetches
  the log the moment it is up (ivg).
- **Refresh** on the Matches page does the same by hand.
- **When logs.tf blocks you** (too many requests), the app stops asking at
  once and leaves it alone for 10 minutes instead of keeping the ban going.
  New matches come from **more.tf** meanwhile — same scoreboard and kills —
  and are swapped for the real log once logs.tf answers again.

## How ratings work

A new page, from **How ratings work →** on the class matchups: how a game
becomes a number, and for each of the nine classes what it is rated on and
how much each part counts. It is drawn from the live model, so it is always
the one rating your games.

## Maps

- **Map images built in** for Ashville, Bagel, Cascade, Gullywash,
  Process, Product, Proot, Proplant, Steel, Swiftwater, Upward and Vigil —
  thanks to more.tf.
- **Settings › Maps**: every map you played, where its image and callouts
  come from, **import your own top-down image** and line it up with your
  kills.
- **Callout presets**: export a map's callouts and share them; import one
  (or drop the file on the window) with a check and an Undo.
- **Proot's callouts** as Flashy drew them are the new default.
- **The map read from the STV demo** when logs.tf does not say which map a
  round was on, and a **"which map was this?"** picker when nothing can tell.

## Fixes

- **Positions**: combined logs no longer put one team on both sides; the
  columns now read **Your team** / **Enemy team**.
- **"Spawns delayed by your caps"** is gone from payload and attack/defend,
  where capping moves the attackers' spawn up — it mostly flattered BLU.
- **Teammates** shows ratings with two decimals, like everywhere else.

## Also

- **ETF2L names** instead of the names in the log: Settings › Player names.
- An **English language file**, for correcting the English itself.

## After updating

The first sync recomputes every match once (a few seconds), for the payload
change. Ratings on payload and attack/defend move slightly.

**SHA-256**

```
6392BF16CA1C98C2D4394367CF844D271FC8863BBE711A8EC007C0684E6F13AF  Flashwave.tf_0.7.0_x64-setup.exe
FDA6B78FEE049A0E3F3A98C12CACCD42171BF3C604FF69A7AC08FA9C37B4A5AF  Flashwave.tf_0.7.0_x64_en-US.msi
```
