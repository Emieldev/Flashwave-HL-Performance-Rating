# Releasing, and how the in-app updater works

The app checks GitHub on startup and offers the new version in a card. It
downloads the installer itself and runs it, so nobody has to go through the
"Windows protected your PC → Unblock" dance for an update — only for their
first install.

## The one thing you cannot lose

```
%USERPROFILE%\.flashwave-keys\flashwave.key
```

Every update is signed with this, and the matching public key is compiled
into the app. An installed copy will refuse anything not signed by it.

**If you lose this key, you cannot update anyone.** Not "it gets harder" —
every existing install becomes a dead end that has to be replaced by hand.
Back it up somewhere that is not this machine. It has no password, so treat
the file itself as the secret: anyone holding it can push an update to
every install.

That signature is *not* code signing and does nothing for SmartScreen. It
answers a different question — not "does Microsoft trust this publisher"
but "was this built by whoever holds the key". Which is the question that
matters when the download comes from a GitHub release page anyone can open
a PR against.

## Picking the number

Versions follow [Semantic Versioning 2.0.0](https://semver.org/): `MAJOR.MINOR.PATCH`,
no leading zeroes, and a published version is never changed -- a fix is a new
version. What counts as the "public API" here is what other people and older
installs depend on: the database (migrations must keep upgrading every older
one), `config.json` and `weights.toml`, the `.lang` format, backups, and the
updater's `latest.json`.

- **PATCH** (0.5.0 → 0.5.1): only fixes and speed-ups, nothing new to see.
- **MINOR** (0.5.1 → 0.6.0, patch back to 0): anything new -- a panel, a stat,
  a setting. While the app is 0.y.z (SemVer's initial development), a release
  that removes or breaks something is a minor bump too, and its notes say so.
- **MAJOR**: from 1.0.0 on, anything that breaks the list above.
- Test builds are pre-releases: `0.6.0-beta.1`, which sorts before `0.6.0`.

## Cutting a release

1. **Bump the version in three places** — the workspace `Cargo.toml`,
   `package.json`, and `src-tauri/tauri.conf.json`. The window title and
   Settings both derive from the first, so there is no fourth place any
   more (there used to be, and 0.4.0 shipped calling itself 0.3).

2. **Write `docs/release-<version>.md`.** The updater card shows its first
   few lines, so lead with what changed rather than with a heading. If the
   rating model changed, regenerate the browser mock of "How ratings work"
   (`hl guide --json > ui/src/api/fixtures/rating_guide.json`); the app's
   own page reads the live model and needs nothing.

3. **Build it signed:**

   ```bash
   npm run release
   ```

   This refuses to start without the key, builds, and writes `latest.json`
   next to the installer using the signature of the file it just made.

4. **Check the version is really in the binary** before publishing:

   ```powershell
   (Get-Item target\release\hl-app.exe).VersionInfo.FileVersion
   ```

5. **Publish, with both files:**

   ```bash
   gh release create v<version> \
     "target/release/bundle/nsis/Flashwave.tf_<version>_x64-setup.exe" \
     "target/release/bundle/msi/Flashwave.tf_<version>_x64_en-US.msi" \
     "target/release/bundle/nsis/latest.json" \
     --title "Flashwave.tf <version> beta" \
     --notes-file docs/release-<version>.md \
     --latest
   ```

   **`latest.json` must be attached to the release.** Without it the
   updater has nothing to read and every client silently stays put.

   **And the release must be marked Latest -- not a pre-release.** "beta"
   (before 0.7.0, "alpha") lives in the title and the app's badge only.

6. **Check what the updater will actually see** -- give GitHub's cache a
   minute:

   ```bash
   curl -sL https://github.com/bartflk/Flashwave-HL-Performance-Rating/releases/latest/download/latest.json
   ```

   It must say the new version. If it says the old one, the release is not
   marked Latest.

## How the client finds it

The app asks for:

```
https://github.com/bartflk/Flashwave-HL-Performance-Rating/releases/latest/download/latest.json
```

GitHub keeps `/releases/latest/` pointed at the newest non-draft release,
so publishing is all it takes — there is no separate manifest to host and
nothing to keep in step by hand.

One consequence worth knowing: **a pre-release is never "latest"** while
any full release exists. 0.4.1 went out as a full release, so when 0.5.0
was first published as a pre-release this URL kept serving 0.4.1's manifest
and nobody would have been offered the update. Publish every release as
Latest (`--latest`), and check the URL afterwards (step 6).

## Why the restart is a button

The app holds an exclusive lock on the database while its window is open.
An installer replacing files under a running process is the shape of the
thing that corrupted the database twice in September 2026, so the update
downloads, installs, and then *asks*. Closing cleanly first is the safe
order, and it is worth the extra click.

## If an update fails

It lands in **Settings › Problems** with the reason, like everything else,
and **Copy report** puts it somewhere it can be sent. The usual causes:

- **`latest.json` was not uploaded** — the check finds nothing, silently.
- **The URL in the manifest does not match the tag.** `npm run release`
  builds it from the version, so a tag that is not `v<version>` produces a
  404 at download time.
- **Built without the key.** Then there is no `.sig`, the script stops
  before writing a manifest, and you find out at build time rather than
  from a tester.
- **The manifest starts with a byte-order mark.** A JSON reader rejects it
  and the check finds nothing. `release.ps1` writes it without one; if you
  ever write `latest.json` by hand, check its first byte is `{`.
- **The release is a pre-release.** See above: `/releases/latest/` skips it.
