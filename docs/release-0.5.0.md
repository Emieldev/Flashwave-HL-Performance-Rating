# Flashwave.tf 0.5.0

Windows only. **On 0.4.1 already?** The app offers this update itself: take
it from the card, or Settings › Updates › Check for updates.

New install: download the `-setup.exe` and run it. **Windows will say
"Windows protected your PC"** — the installer is not code-signed.
Right-click the download → **Properties** → tick **Unblock** → **OK**. Check
it is my build first:

```powershell
Get-FileHash .\Flashwave.tf_0.5.0_x64-setup.exe -Algorithm SHA256
```

The hash is at the bottom. Updating keeps everything.

---

The biggest release yet: every player's aim, demos kept after they are
deleted, four languages, and the app telling you what it is doing.

## Aim for everyone, not just you

Pick any player on the match page and the Aim tab shows *their* crosshair
error, flicks and range, read from the match's SourceTV demo. Your own
recording still answers for you, since it has your real view angles; the
server's demo answers for the other seventeen.

## Demos are kept, even after you delete them

Every demo the app reads is now also kept inside the database as a compact
timeline — everyone's position and view every tick, health, ubers, cloak,
the cart, buildings, every hit and death — at 1-7 MB instead of 70-130 MB.
Future versions can learn new things from matches whose demo files are long
gone. Settings › Demos shows how many are kept.

**Clean up downloaded demos.** Settings › Downloaded demos lists the STV
demos the app fetched, deletes the ones it has finished reading, and can do
it automatically after every sync. Your own recordings are never touched.

## Four languages

Settings › Language: **Français, Español, Português, Русский**, alongside
English. These are drafts and will read slightly off in places. Every
language is a plain `.lang` file you can fix yourself — *Edit this
language*, change a line, *Reload* — and send back on Discord to be included
for everyone. A new language works the same way.

## It tells you what it is doing

Syncs, rebuilds and demo reads show what they are on and how far along. A
downloaded demo no longer sits at 100% in silence: the card walks through
linking it, reading it (with a percentage, and "1 of 2" when your recording
is read too), keeping it, and saving.

## The rating

**Captures are weighed by what they cost** (zaag). A point taken against
defenders counts for more than one walked onto, for Scout, Pyro, Engineer
and Medic. **Demoman's fight swing is shared** with everyone who damaged the
victim in the five seconds before, HLTV-style, instead of all going to the
last hit. Both were measured against match results before going in (model
v8).

## On the match page

- **Rounds show the game state**: under each round's lanes, players up or
  down, both teams' uber building, ready and in use, and who held the
  advantage — on the same time axis, so an uber sits under its marker.
- **Teamfights**: which fights your side went into together, and who
  arrived late.
- **Mouse back and forward buttons work**, with a proper history (Alt+← and
  Alt+→ too).

## Settings

- **Updates**: your version, and a button to check for a new one.
- **Changelog**: every release's notes, this one included.

## Fixed

- **Opening one half of a combined log broke the match page** (Flashy).
- **Scoped time and cloaked Spies were undercounted.** The demo library
  reads a condition as off whenever a higher one in the same byte is on —
  scoped and just teleported read as not scoped. Conditions are now read
  directly.
- **"A second before the kill" in your own recordings was more than a
  second**: a client recording skips ticks, and the count was in frames.
- A recording holding two matches no longer crashes the demo reader.
- A checkbox in Settings sat in the middle of nowhere; the selected language
  button no longer shouts in capitals.

## After updating

The first sync re-reads every demo you have once, to keep it and to apply
the aim fixes — a few seconds a demo, with the progress shown. Aim numbers
shift slightly; that is the fixes.
