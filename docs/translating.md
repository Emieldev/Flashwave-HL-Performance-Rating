# Translating Flashwave.tf

Thank you for helping. You do not need to be a programmer for this.

## The files

One `.lang` file per language, in `lang/` at the top of the repository:

| File | Language |
|---|---|
| `fr.lang` | Français |
| `pt.lang` | Português |
| `es.lang` | Español |
| `ru.lang` | Русский |
| `template.lang` | blank, for starting a new language |

**Every file already has a draft.** They were written without a native
speaker, so the most useful thing you can do is read yours through and fix
whatever reads wrong or sounds unnatural -- that is faster than starting from
nothing, and you know how your community actually talks about the game.
Portuguese is drafted as Brazilian Portuguese.

Each line is the English on the left and your translation on the right:

```
language = "Français"
locale = "fr-FR"

"Matches" = "Matchs"
"Sync finished" = ""
"Update available" = "Mise à jour disponible"
```

**An empty `""` means "not translated yet".** The app shows the English
there instead, so you can translate as much or as little as you like and
send it in. Nothing breaks if a line is empty. Lines starting with `#` are
comments.

`language` is your language's own name, as it appears on the button in
Settings. `locale` decides how dates are written (`fr-FR`, `pt-BR`, `pl-PL`).

## Trying your changes in the app

You do not need to build anything.

1. Settings › Language. Pick your language and press **Edit this language**.
   The app saves the whole language as a `.lang` file in its lang folder and
   shows it to you.
2. Edit the right-hand sides in any text editor (Notepad is fine; save as
   UTF-8).
3. Press **Reload**. Your lines show straight away.
4. Send the file on Discord, and it goes into the next release for everyone.

A file in that folder beats the built-in translation line by line, so you can
also keep only the lines you changed. It is never overwritten: pressing Edit
again leaves an existing file alone.

**A new language:** pick English and press **Start a new language**. That
saves `new.lang` with every line empty. Rename it to your language's code
(`pl.lang`, `de.lang`), set `language` and `locale` at the top, translate, and
press Reload: it appears as a new button.

Settings lists every file it read, and any line it could not understand, with
its line number.

## Rules

- **Never change the English on the left.** It is how the app finds your
  line. Change it and the app stops finding your translation.
- **Keep both sides in "double quotes".** A `"` inside a translation is
  written `\"`.
- **Keep anything in `{braces}` exactly as it is**, but move it to wherever
  your grammar wants it. `"{n} matches"` can become `"matchs : {n}"`. A brace
  you leave out is simply not shown, which is how English-only plural endings
  like the `{1}` in `"{0} match{1}"` are dropped.
- **Keep it short.** These are buttons and headings, often in a narrow
  space. If the English is two words, aim for two or three.
- TF2 terms — Medic, Uber, Sniper, cart, KOTH, stopwatch — use whatever your
  community actually says, even if that is the English word.

## For the developer: keeping the files in step

```bash
npm run i18n
```

Lists, per language, how many lines are translated, how many are missing,
and any that are stale — the app no longer says that English, usually
because it was reworded. `npm run i18n -- --write` adds the missing lines
(empty), removes the stale ones, sorts, and refreshes `template.lang`. Run it
after wrapping new text, before sending the files to translators. A fix
someone sends back is copied over the file in `lang/` and checked with the
same command.

The files are bundled into the app at build time; the user's lang folder
(beside the database) is read at start and on Reload.

To make a piece of text translatable, wrap it where it is:

```tsx
import { t } from "../lib/i18n";

<h2>{t("Downloaded demos")}</h2>
<p>{t("Deleted {n} demos", { n: count })}</p>
```

Only double-quoted literals are found by the tool. A label that reaches
`t()` through a variable has to be listed in `LITERALS` in
`scripts/i18n.mjs`, or it will never appear in the files.
