# When Epic changes something

Fishy Client talks to Epic the way Epic's own launcher does, and Epic can change that at any time without warning. This page is the checklist for when sign-in, the Library, downloads or Play suddenly stop working for everyone.

## What users see

When Epic replies with something Fishy Client doesn't understand, the app says:

> Epic changed something. Update Fishy Client, or check back later.

It shows in the sign-in screen, the Library banner, a game's achievements, the Downloads list, the Play error (with **Play offline** still offered for games that allow it) and the Store's free games list. Fishy Client never treats a changed reply as "you have no games" or "this game can't be installed". Logs only say `unexpected reply from Epic`, never Epic's text.

A connection problem is different: it says "Check your internet". If many people report the "Epic changed something" message at the same time, it's almost certainly Epic.

## 1. Check Legendary and Heroic first

Fishy Client uses the same Epic calls as [Legendary](https://github.com/derrod/legendary) (a command-line launcher) and [Heroic](https://github.com/Heroic-Games-Launcher/HeroicGamesLauncher) (built on Legendary). They have many more users, so a change on Epic's side usually shows up there first.

- Look at their **open and recent issues** for the broken feature (search "login", "auth", "library", "manifest", "download", "launch").
- Look at their **latest commits and releases**. Legendary's Epic code is in `legendary/api/egs.py` (calls and constants) and `legendary/models/` (replies and manifests).
- If they fixed it, copy the idea (both are GPL-3.0, like Fishy Client).

## 2. Find what changed

Everything Fishy Client relies on from Epic is in one file: **`crates/fishy-core/src/epic.rs`**:

- the client ID and secret, and the three user agents
- every server address
- the login and redirect pages
- the Store pages
- where Epic's own launcher keeps its files

The common changes are:

| Symptom | Usually | Where to look |
|---|---|---|
| Sign-in fails for everyone | new client ID, user agent or login page | `epic.rs`; Legendary's `egs.py` |
| Everything fails with HTTP 4xx | a server address or user-agent change | `epic.rs` |
| "Epic changed something" in the Library | a field renamed in a reply | `library/client.rs`, `library/build.rs` |
| Downloads fail with it | download info or manifest format changed | `install/client.rs`, `install/manifest/` |
| Play fails with it | the one-time code reply changed | `launch/client.rs` |

The log (`%APPDATA%\FishyClient\logs`, **Settings → Open logs folder**) says which step failed, without Epic's text.

## 3. Prove it with a test

`crates/fishy-core/testdata/epic/` holds replies shaped like Epic's real ones, and `crates/fishy-core/tests/epic_replies.rs` reads each through the real code.

1. Capture the new reply. For example, run Legendary with `-v` (verbose), or look at your browser's network tab for the Store.
2. **Remove every token, code, account ID, email and name** and replace them with fake text. Never commit real ones.
3. Save it over the matching file (the table in `testdata/epic/README.md` says which).
4. Run:

   ```powershell
   cargo test -p fishy-core --test epic_replies
   ```

   The failing test names the part Fishy Client can no longer read.
5. Fix the code until it passes, then run everything: `cargo test --workspace`, `npm.cmd --prefix app test`, `npm.cmd --prefix app run check`.

The `changed_…` tests in the same file make sure a broken reply always becomes "Epic changed something". Keep them passing.

## 4. Release

A fix like this is worth a release on its own ([RELEASING.md](RELEASING.md)). Installed copies offer the update at their next start, because updates come from GitHub, not Epic.

## Things to never do

- Don't copy code from Epic's launcher, and don't put Epic's files in the repository.
- Don't log or commit tokens, codes, account IDs or Epic's error text (spec §12).
- Don't try to get around anti-cheat or Epic's security checks. If Epic blocks third-party launchers on purpose, say so in the README rather than working around it.
