# Fishy Client

A lightweight, open-source launcher for your Epic Games library, written in Rust with a fish-themed pixel-art UI.

> Work in progress. The current build shows the UI foundation (ocean background, fish loader, fish download bar, app icon) with fake data. Epic login, library, install and launch come next.

## Signing in

Fishy Client signs in through Epic's own login page, shown in a small app window. Your password goes to Epic, never to Fishy Client. The session is stored in Windows Credential Manager (entries named `FishyClient`), and **Log out** removes it.

Epic doesn't offer an official login for third-party launchers. Like Legendary and Heroic, Fishy Client signs in by identifying itself as the Epic Games Launcher. Epic could change this at any time, which would break signing in until Fishy Client is updated.

Fishy Client has no analytics or tracking. It only talks to Epic.

## Develop

Requirements: Rust (rustup), Microsoft C++ Build Tools, Node.js.

```powershell
cd app
npm install
npm run tauri dev   # opens the app window
npm test            # runs the UI unit tests
npm run dev         # UI only, in a browser: add ?mock=signedout, signedin or offline
```

```powershell
cargo test -p fishy-core   # Rust core tests (from the repo root)
```

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
