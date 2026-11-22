# Fishy Client

A lightweight, open-source launcher for your Epic Games library, written in Rust with a fish-themed pixel-art UI.

It signs you in, shows your games, installs them, updates them (downloading only what changed) and starts them. Windows 10/11 only.

## How light is it?

Measured on one Windows 11 PC (16 cores) on 2026-11-22, each launcher started from fully closed and left idle:

| | Fishy Client 1.0.0 | Epic Games Launcher |
|---|---|---|
| Program on disk | **9 MB** | 869 MB |
| Memory in use | **429 MB** | 1,449 MB |
| Processor use while idle | **0.3 %** | 1.7 % |
| Processes | **7** | 10 |
| Windows services added | **none** | 2 |

Most of Fishy Client's memory is Microsoft WebView2, which draws the window. Numbers vary a little between PCs and runs.

## Install

Download `Fishy Client_<version>_x64_en-US.msi` and double-click it. It installs to `C:\Program Files\Fishy Client\` with a Start menu shortcut.

The installer isn't code-signed yet, so Windows may say "Windows protected your PC". Click **More info → Run anyway**.

Uninstalling (Settings → Apps → Installed apps) removes the program but keeps your games, settings and sign-in. To remove those too, use **Settings → Delete all my data** in Fishy Client first.

### Games you already installed with Epic's launcher
If Epic's launcher already installed some of your games, the Library shows "3 games installed by Epic's launcher can be added." Press **Add them**: each game's files are checked (a few seconds) and the game shows as Installed, with nothing downloaded. If some files are missing or damaged, press **Download them** to fetch only those.

- The game stays in Epic's folder, so both launchers see it. Uninstalling it in Fishy Client deletes its files, and Epic's launcher then shows it as broken. Updating it in one launcher can make the other ask for an update or a repair.
- Games Epic installed with only some optional parts (Fortnite, for example) can't be added yet. Keep using Epic's launcher for those.
- New games can't go into `C:\Program Files\Epic Games`: Windows only lets programs with administrator rights create folders there. Use your own Games folder (the default) or something like `D:\Games`.

## Warnings: please read before using Fishy Client

**Use it at your own risk.** Fishy Client is a free hobby project. It is provided "as is", with no warranty (see the GPL licence).

### Your Epic account could be restricted
- **Fishy Client is not made, approved or supported by Epic Games.** It is not affiliated with Epic Games in any way. "Epic Games" and "Epic Games Store" are Epic's trademarks.
- **Epic doesn't officially allow third-party launchers.** To work, Fishy Client identifies itself to Epic as the Epic Games Launcher, the same way the open-source launchers [Legendary](https://github.com/derrod/legendary) and [Heroic](https://heroicgameslauncher.com/) do.
- Those launchers have been used for years, and we know of **no bans for normal use** (signing in, installing and playing games you own). But **the risk is not zero**: Epic could decide to suspend or restrict accounts that use unofficial launchers, at any time and without warning.
- **If losing your Epic account would hurt** (many games, money spent, Fortnite items), think twice. The zero-risk option is a second Epic account that only holds free games.
- Only use it with **your own account** and **games you own**. Don't share your sign-in.

### Some games won't work
- **Fortnite: not tested with Fishy Client.** Tools built on Legendary report that it starts on Windows (the "doesn't work" reports are mostly from Linux, where its anti-cheat can't run). It's the game Epic watches most closely, so if you want to try it, use a second account first.
- **Online games with anti-cheat** (Easy Anti-Cheat, BattlEye) may refuse to start or go online without Epic's own launcher. **Don't try to force them** (for example by changing their files): that's the kind of thing anti-cheat systems ban for.
- Games that start through **Ubisoft Connect, the EA app or Rockstar's launcher** aren't supported yet.
- There's **no Epic overlay**: no in-game friends list or achievement pop-ups.
- **No store:** buying games, refunds and gifts happen on Epic's website or in Epic's launcher.

### It can stop working
- Epic can change how its sign-in or downloads work at any time. Signing in, downloading or starting games may then **break until Fishy Client is updated**, and there's no promise when that will be.

### Your files and PC
- **Uninstall** on a game's card, and **Delete all my data** when you also tick your games, **permanently delete those game folders.** Double-check before confirming. (Uninstalling Fishy Client itself keeps your games.)
- The installer **isn't code-signed**, so Windows shows "Windows protected your PC". Only download Fishy Client from this GitHub page: a copy from anywhere else could be changed to steal your account.
- While a game starts, **other programs on your PC can read the one-time sign-in code** Fishy Client gives it (Epic's own launcher and Legendary have the same limit). Only use Fishy Client on a PC you trust.

This page is not legal advice. If you're unsure, read [Epic's terms](https://www.epicgames.com/site/en-US/tos) or don't use unofficial launchers.

## Privacy

Fishy Client has **no telemetry, analytics or tracking**, and no server of its own. Nothing about you is sent to the people who make it.

### Signing in
- You sign in on **Epic's own login page**, in a private app window. Your password goes to Epic only; Fishy Client never sees it.
- Fishy Client never reads or copies browser cookies, and the login window has no access to the app.
- The sign-in (Epic's access and refresh tokens, your display name and account ID) is stored in **Windows Credential Manager**, in entries named `FishyClient`. Never in a file. **Log out** removes it and ends the session at Epic.

### What is sent, and to whom
Only to Epic, always over HTTPS with normal certificate checks:

| Address | Why |
|---|---|
| `www.epicgames.com` | the login page |
| `account-public-service-prod03.ol.epicgames.com` | signing in, keeping you signed in, logging out, the one-time code a game needs to start |
| `library-service.live.use1a.on.epicgames.com`, `catalog-public-service-prod06.ol.epicgames.com` | your list of games, their titles and cover art |
| `launcher-public-service-prod06.ol.epicgames.com` | where to download a game and its newest version |
| `ecommerceintegration-public-service-ecomprod02.ol.epicgames.com` | an ownership token, only for the few games that need one to start |
| Epic's download servers (addresses Epic gives for each game) | the game files themselves |

Each downloaded piece is checked against Epic's file list, so a changed file is never installed. Fishy Client doesn't update itself, so there's no update server that could be attacked.

When a game starts, Fishy Client passes it your Epic display name, account ID, language and a one-time code, the same as Epic's launcher does. While the game is starting, other programs on your PC could read these from its command line (the same limit Epic's launcher and Legendary have).

### What is stored on your PC
| Where | What |
|---|---|
| Windows Credential Manager, `FishyClient` | your sign-in (see above) |
| `%APPDATA%\FishyClient\settings.json` | your settings |
| `%APPDATA%\FishyClient\library.json` | your list of games, so the Library shows at once |
| `%APPDATA%\FishyClient\installed.json` | your installed games: folder, version, size, install date and when you last pressed Play (for the "Recently played" sort) |
| `%APPDATA%\FishyClient\manifests\`, `downloads\` | file lists for installed games, and downloads in progress |
| `%APPDATA%\FishyClient\logs\` | logs, kept for 7 days |
| `%TEMP%\FishyClient\` | an ownership token file for the few games that need one; deleted when Fishy Client starts |
| `%LOCALAPPDATA%\com.fishyclient.app\` | the app window's own browser cache (for example cover images), made by Microsoft WebView2 |
| the Registry `Run` key | only if you turn on **Start with Windows** |

**Logs** never contain your tokens, sign-in codes, Epic's error messages, account ID, email or name, and a filter masks anything secret-looking as a second safety net. They do contain file paths, which can include your Windows user name (for example `C:\Users\you\Games\…`). Logs never leave your PC.

**Running games:** after you press Play, Fishy Client checks every 5 seconds which programs are open, so it can show "Running". It only compares their file paths with the game's folder, in memory. They are never logged, saved or sent. The check stops when the game closes.

**Games from Epic's launcher:** when the Library opens, Fishy Client **reads** (never changes) Epic's list of installed games in `C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests\`, and, when you press **Add them**, each game's own `.egstore` file list and file sizes. Those files hold no account details. The log only says how many games were found and which game IDs were added, never folder paths.

**Your files:** Fishy Client only reads and writes the folders you pick for games, plus the places above. It doesn't scan your drives, read save files or browser profiles, or collect hardware details.

### Deleting everything
**Settings → Delete all my data** logs you out, removes your sign-in from Credential Manager, deletes everything in `%APPDATA%\FishyClient`, empties the app window's browser cache and turns off Start with Windows. You choose whether your installed games are deleted too. (The empty WebView2 folder `%LOCALAPPDATA%\com.fishyclient.app\` stays while Fishy Client is installed; uninstalling doesn't remove it, so delete it by hand afterwards if you like.)

## Develop

Requirements: Rust (rustup), Microsoft C++ Build Tools, Node.js. In PowerShell, use `npm.cmd` (PowerShell blocks plain `npm` on some PCs).

```powershell
cd app
npm.cmd install
npm.cmd run tauri dev     # opens the app window
npm.cmd test              # UI unit tests
npm.cmd run dev           # UI only, in a browser: add ?mock=signedout, signedin or offline
npm.cmd run tauri build   # builds the .msi into target\release\bundle\msi\
```

```powershell
cargo test --workspace   # Rust tests (from the repo root)
```

The full design and progress log is in [docs/superpowers/specs/2026-11-20-fishy-client-design.md](docs/superpowers/specs/2026-11-20-fishy-client-design.md).

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
