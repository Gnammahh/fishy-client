# Fishy Client — Design Spec

**Date:** 2026-11-20
**Status:** Draft, waiting for review

Fishy Client is a lightweight, open-source replacement for the Epic Games Launcher, written in Rust. It has a fish-themed pixel-art UI.

---

## 1. Goals

| Goal | What it means in practice |
|---|---|
| **Daily use** | You can log in, see your library, install, launch and update your Epic games without opening Epic's launcher. |
| **Learning** | You learn Rust and how launcher protocols work. The code is split into small parts you can follow. |
| **Public release** | It's open source on GitHub, polished enough for other people to use. |

### Version 1 is done when
1. You can log in with your Epic account, and it stays logged in after a restart.
2. Your owned games show up in a grid with their cover art.
3. You can install a game to a folder you choose, with the fish download bar showing progress.
4. Pressing **Play** launches the game logged in as you.
5. The app shows when an update is available and downloads only the changed parts.
6. All of the above works on Windows 11.

### Not in version 1 (later)
- Cloud saves
- DLC management, beyond whatever installs with the base game
- Store or shop pages, buying games, claiming free games
- Friends list and the in-game overlay
- Linux and Proton support. The design keeps this possible, but it isn't built yet.
- Importing games already installed by Epic's launcher (added in version 1.1, §6g)

### Known limits, the same ones Heroic and Legendary have
- **Fortnite: not tested with Fishy Client.** Tools built on Legendary report that it starts on Windows (the "doesn't work" reports are mostly from Linux, where its anti-cheat can't run). It's the game Epic watches most closely, so if you want to try it, use a second account first.
- **Some anti-cheat games** (Easy Anti-Cheat, BattlEye) may refuse to start or go online without the official launcher.
- **Ubisoft, EA and Rockstar games** also need that company's own launcher installed.
- No Epic overlay, so no in-game friends or achievement pop-ups.

---

## 2. Branding and visual identity

### Name
**Fishy Client.** App window title: `Fishy Client`. Executable: `fishy-client.exe`.

### Colors
| Role | Color | Hex |
|---|---|---|
| Background | Pure black | `#000000` |
| Panels and cards | Near-black | `#0B0F14` |
| Fish outline and main accent | Aqua | `#4FE3D1` |
| Food pellets and highlights | Warm orange | `#FF9F43` |
| Text | Off-white | `#E8F1F2` |
| Secondary text | Gray-blue | `#7A8A99` |
| Error | Coral red | `#FF5C5C` |
| Success | Sea green | `#3DDC84` |

### App background: "deep water"
The background behind every screen is black that fades into blue waves, so the app feels like looking into deep water.

- **Gradient:** pure black `#000000` across the top half, fading through deep navy `#03122B` to ocean blue `#0A3A7A` at the bottom edge.
- **Waves:** 3 layered wave bands along the bottom ~35% of the window, each a slightly lighter, semi-transparent blue (`#0A3A7A`, `#0E5AA8`, `#1B7FD1` at 25–45% opacity). Wave edges use the **pixel-stepped** style, so they match the pixel fish instead of smooth curves.
- **Motion:** the layers drift sideways slowly at different speeds (parallax), with one loop every ~20–40 s. It's subtle enough not to distract and uses almost no CPU, because it's drawn with CSS transforms only.
- **Readability:** cards and panels sit on top as near-black `#0B0F14` at ~85% opacity with a light blur, so text never sits directly on the waves.
- **Reduced motion:** the waves stay still.
- Implemented as one reusable component, `<OceanBackground>`, placed once behind the whole app.
- The **app icon keeps its plain black background**. The waves are only for the app window.

### Fonts
- **Headings and buttons:** a pixel font. Planned: *Pixelify Sans* (free OFL license, bundled with the app so it works offline).
- **Body text, game titles, settings:** *Inter*, a clean sans-serif, so long text stays easy to read.

### App icon
- A **pixel-art fish, outline only**: aqua pixels on a pure black tile with **pixel-stepped rounded corners** (corner cut of 3/2/1 cells on the 16-cell tile), with the body hollow inside.
- The icon uses its own **14×10 fish** (`ICON_FISH` in `design/mockup.html`), placed at cell (1,3) on the 16×16 tile. That leaves exactly 1 cell of margin left/right and 3 top/bottom, so it's perfectly centered and fills ~87% of the width. Every size uses the same drawing scaled by a whole number (16 px = ×1, 32 = ×2, 64 = ×4, 128 = ×8, 256 = ×16). The design reference is the App icon section of `design/mockup.html`. The draft grid below is replaced by this one.
- Drawn on a **16×16 pixel grid** and scaled up with hard pixel edges (no blurring) to 32, 64, 128 and 256 px, plus a `.ico` file.
- It's our own design. It uses the chunky, blocky look of item icons in block-building games, but it isn't a copy of any existing game's sprite.
- **Draft grid** (`#` = aqua pixel, `.` = black). To be refined in a visual mockup:

```
 0 ................
 1 ................
 2 ................
 3 .......#####....
 4 .....##.....##..
 5 .#..#.........#.
 6 .##.#......#...#
 7 .#.#...........#
 8 .#.#...........#
 9 .##.#..........#
10 .#..#.........#.
11 .....##.....##..
12 .......#####....
13 ................
14 ................
15 ................
```
(The tail is on the left, the eye is at row 6 column 11, and the fish faces right.)

### Rules for pixel art
- Every fish and food sprite sits on a pixel grid and is scaled by whole numbers only (×2, ×3, ×4).
- CSS uses `image-rendering: pixelated` so pixels stay sharp.
- Animations step frame by frame (CSS `steps()` or a frame counter) rather than gliding smoothly, so they feel like pixel art.

---

## 3. Fish animations

All of these are reusable UI components. Each one has a **reduced-motion** version that's used when Windows "Show animations" is turned off.

### 3.1 Loading animation: `<FishLoader>`
Used for: app startup, the library loading, any wait longer than about 300 ms.

1. The fish idles in the middle of a small black "tank", bobbing slowly with an occasional bubble.
2. An orange **food pellet** drops from the top at a random horizontal position.
3. The fish turns to face it, swims over, and **eats it** with a 2-frame mouth open/close and a small pixel "crunch" puff.
4. The fish swims back to the middle.
5. This repeats until loading finishes. The fish then does a little happy flip and the loader fades out.

Details:
- Sprite frames: swim (2 frames), turn (1), mouth open (1), mouth closed (1), happy flip (3).
- Only one pellet falls at a time, about 1.2 s per cycle.
- Size options: `small` (inline, 32 px fish), `medium` (card, 64 px) and `large` (full screen, 128 px).
- Reduced motion: a static fish with three pellets fading in and out.

### 3.2 Download bar: `<FishProgressBar>`
Used for: game installs and updates.

- A long horizontal track **filled with small orange dots**.
- The fish sits at the current progress point. **Dots to its left have been eaten** (gone). Dots to its right are still waiting.
- As progress moves forward, the fish swims right and chomps each dot it reaches (2-frame mouth animation).
- Text under the bar: `42% · 3.1 GB / 7.4 GB · 18.2 MB/s · ~4 min left`.
- **States:**
  - *Downloading:* the fish chomps and swims.
  - *Paused:* the fish stops, faces the viewer and blows bubbles.
  - *Verifying:* the fish swims back over the track slowly with a small "magnifier" bubble.
  - *Error:* the fish turns gray with an "x" eye, and the bar turns coral red with a **Retry** button.
  - *Done:* the fish jumps out of the bar with a splash, then the bar becomes a **Play** button.
- Reduced motion: the fish moves along the bar without chomping or extra frames.

### 3.3 Small touches (after version 1, nice to have)
- Hovering a game card makes a small bubble rise from it.
- Empty library: "No fish in this pond yet", with a sleeping fish.
- Successful login: the fish waves its tail.

---

## 4. Screens

| Screen | What's on it |
|---|---|
| **Login** | Large pixel fish logo, "Fishy Client" title, and a **Sign in with Epic** button that opens Epic's real login page inside the app. |
| **Library** (home) | A grid of game cover art. Search box, filters (All / Installed / Updates), and a sort option. Each card shows the cover, title and a status badge (Installed, Update, Not installed). |
| **Game details** | Large banner image, description, size, install folder, and an **Install / Play / Update** button. Uninstall and Verify are under a "⋯" menu. |
| **Downloads** | The current download with the `FishProgressBar`, a queue, and pause/resume/cancel buttons. |
| **Settings** | Default install folder, max download speed, language, start with Windows (off by default), reduced motion, log out. |

Layout: a slim left sidebar with pixel icons (Library, Downloads, Settings, Account) and the main content on the right. The window can be resized, with a minimum size of 960×600.

---

## 5. Technology

| Part | Choice | Why |
|---|---|---|
| Language (engine) | **Rust** | Fast and safe, and it's your goal to learn it. |
| Desktop shell | **Tauri 2** | Small, fast apps with a web-based UI, which makes a polished design much easier than pure-Rust UI toolkits. |
| UI framework | **Svelte 5 + TypeScript + Vite** | Simple to learn, very fast, and good for animations. |
| HTTP | `reqwest` + `tokio` | Standard async Rust networking. |
| JSON | `serde` / `serde_json` | Standard. |
| Saving login tokens | `keyring` crate → Windows Credential Manager | Tokens are never stored in plain files. |
| Decompression and checks | `flate2` (zlib), `sha1` | Needed for Epic's download chunks. |
| Errors and logs | `thiserror`, `tracing` | Clear error messages and log files for debugging. |

### Project layout
```
fishy-client/
├─ crates/
│  ├─ fishy-core/     ← all Epic logic, no UI (auth, library, download, install, launch, update)
│  └─ fishy-cli/      ← small command-line tool for testing the core (`fishy login`, `fishy list`, ...)
├─ app/
│  ├─ src-tauri/      ← Tauri app: connects the core to the UI
│  └─ src/            ← Svelte UI: screens + fish components
├─ assets/
│  └─ sprites/        ← fish/pellet pixel sprites, icon source
└─ docs/
```
**Why split it up:** `fishy-core` can be tested without any UI, the CLI makes debugging easy, and a later Linux version or other front end can reuse the same core.

---

## 6. How it talks to Epic

We follow the protocol that the open-source **Legendary** project already worked out, using its source code as the reference. **REA** is used only when something isn't covered there or has to be checked against the real launcher.

| Step | What happens |
|---|---|
| **Login** | Epic's web login page opens in an in-app window. After you sign in, Epic returns an *authorization code*, which is exchanged for an **access token** and a **refresh token**. These are saved in Windows Credential Manager. Expired access tokens are renewed automatically with the refresh token. |
| **Library** | Ask Epic's library service which games you own, then fetch each game's title, description and images from the catalog service. Cache the results on disk so the library opens instantly next time. |
| **Install** | Get the game's *manifest*, which lists every file and the chunks it's built from. Download the chunks from Epic's CDN in parallel, check each one's hash, decompress them and assemble the files. Downloads are resumable, so closing the app mid-download is safe. |
| **Launch** | Get a short-lived *exchange code* from Epic and start the game's `.exe` with the launch arguments Epic's launcher uses (auth type, exchange code, app name, user ID, locale and so on, exactly as in Legendary). |
| **Update** | Compare the installed version with the latest one. Download a new manifest, work out which chunks changed, and fetch only those. |

**Where data lives:** settings and the cache go in `%APPDATA%\FishyClient\`, and logs in `%APPDATA%\FishyClient\logs\`. The installed-games list is a JSON file in the settings folder.

**Note on licensing:** Legendary is licensed under GPL-3.0. Because we're porting its logic closely, Fishy Client will also be **GPL-3.0**. That's fine for an open-source project and avoids any licensing problems.

**Note on branding:** we don't use Epic's logos anywhere. The app only says "Sign in with Epic" and "Epic Games library".

---

## 6a. Login (milestone 2), approved 2026-11-20

**Honesty note for users (goes in the README):** Epic offers no official login for third-party launchers. Fishy Client signs in the way Legendary and Heroic do, by identifying itself as the Epic Games Launcher. Epic could change this at any time and break login until an update.

### Flow
1. **Login screen:** the pixel fish logo, "Fishy Client", and a **Sign in with Epic** button.
2. The button opens a second app window, `epic-login` (about 480×720, titled "Sign in with Epic"), loading Epic's real page:
   `https://www.epicgames.com/id/login?redirectUrl=https%3A%2F%2Fwww.epicgames.com%2Fid%2Fapi%2Fredirect%3FclientId%3D34a02cf8f4414e29b15921876da36f9a%26responseType%3Dcode`
3. The user signs in there, including 2FA and captchas. Fishy Client never sees the password.
4. Epic lands the window on `https://www.epicgames.com/id/api/redirect?...`, which shows JSON containing `"authorizationCode": "<code>"`. A small script injected into that window only reads this JSON and navigates to `http://fishy-auth.localhost/done?code=<code>`. The app's navigation handler (Rust) catches that address, cancels the navigation, closes the window and keeps the code. `.localhost` never leaves the computer, even if it weren't caught.
5. Rust exchanges the code for tokens, saves them, and tells the UI "signed in". The code and tokens never pass through the main window's JavaScript.
6. If the user closes the login window, the app goes quietly back to the login screen.
   The login window is **private (InPrivate)**, so Epic's website cookies are never stored. After **Log out**, nobody on the same PC can land in the previous account by clicking Sign in. The cost is that you type your Epic password each time you sign in, which is rare because the session itself is remembered.
7. **Backup:** under the button, "Trouble signing in? Paste a code instead" opens Epic's login in the normal browser and shows a box to paste the `authorizationCode`. The box accepts the bare code or the whole JSON.

### Epic endpoints (verified against Legendary `egs.py`, 2026-11-20)
- **Host:** `https://account-public-service-prod03.ol.epicgames.com`
- **Client** (the Epic launcher's public id and secret): `34a02cf8f4414e29b15921876da36f9a` / `daafbccc737745039dffe53d94fc76cf`, sent as HTTP Basic auth.
- **Token:** `POST /account/api/oauth/token` (form-encoded):
  - code login: `grant_type=authorization_code&code=…&token_type=eg1`
  - refresh: `grant_type=refresh_token&refresh_token=…&token_type=eg1`
- **Log out:** `DELETE /account/api/oauth/sessions/kill/{access_token}` with `Authorization: bearer <access_token>`.
- **User-Agent:** `UELauncher/11.0.1-14907503+++Portal+Release-Live Windows/10.0.19041.1.256.64bit`.
- **Token response fields used:** `access_token`, `expires_at`, `refresh_token`, `refresh_expires_at`, `account_id`, `displayName`. Errors come back as `errorCode` and `errorMessage`.

### Session rules
- **Storage:** tokens are stored as one JSON entry in **Windows Credential Manager** (service `FishyClient`, account `epic-session`). Nothing goes on disk in plain text.
- **On app start:** if a session is saved and the access token expires in under 5 minutes, refresh it. If the refresh token has also expired or the refresh is rejected, delete the session and show the login screen. A network error while refreshing keeps the session and shows "Couldn't reach Epic" with **Try again**.
- **Log out:** call the kill endpoint (best effort; ignore network errors), then always delete the stored session.

### Screens added in this milestone
- **Login screen** as above, with the FishLoader (medium) shown while finishing sign-in.
- **App shell:** the slim left sidebar (Library, Downloads, Settings, Account) on the ocean background, as in the mockup.
  - **Library** shows "Signed in as *Name*. Your games show up here in the next update." (placeholder until milestone 3).
  - **Account** shows the display name and a **Log out** button.
  - Downloads and Settings are placeholders.
- The component preview from milestone 1 moves to a dev-only `/preview` route.

### Security and privacy (also closes the review's CSP item)
- **CSP** on the main window: `default-src 'self'; img-src 'self' data: https:; style-src 'self' 'unsafe-inline'; connect-src 'self' ipc: http://ipc.localhost`. Images may come from Epic's CDN later. All Epic API calls happen in Rust, not the webview.
- The unused `opener` plugin and its permission are removed. Opening the browser for the backup flow uses a single Rust command that only ever opens the Epic login URL.
- **Capabilities** apply to the `main` window only, so the `epic-login` window has no IPC access at all.
- **No telemetry.** The only network traffic is to Epic.
- Token values never appear in logs or `Debug` output: secret fields print as `[redacted]`.

### Errors shown to the user
| Situation | Message |
|---|---|
| No internet | "Couldn't reach Epic. Check your internet connection." + **Try again** |
| Code rejected or expired | "That sign-in didn't work. Please try again." |
| Pasted text without a code | "That doesn't look like a sign-in code. Paste the authorizationCode value." |

---

## 6b. Library (milestone 3), started 2026-11-20 on branch `feat/library`

### What the user sees
- **Library page:** a grid of game cards, each with tall cover art (3:4) and the title underneath. Sorted A–Z, with a **search box** and "N games" above the grid.
- **Opening the app:** the last saved library shows instantly from the cache, then refreshes quietly in the background. A small "Refreshing…" note shows while that runs. A **Refresh** button next to the search box does the same on demand (it also re-reads the installed games); it is hidden offline. (Renamed from "Updating…" on 2026-11-21 so it isn't confused with game updates.)
- **First load (no cache yet):** the medium FishLoader with "Fetching your library…".
- **Empty library:** "No fish in this pond yet. Games you own on Epic show up here."
- **Error:** "Couldn't load your library from Epic." with **Try again**. If a cache exists, the cached games stay visible and the message appears as a small banner instead.
- **Missing cover:** a dark tile with the pixel fish, so the grid never shows a broken image.
- Badges such as Installed or Update come with the Install milestone; the Library milestone doesn't show any.

### Epic endpoints (verified against Legendary `egs.py`, 2026-11-20)
All calls send `Authorization: bearer <access_token>` and the launcher User-Agent.
- **Library:** `GET https://library-service.live.use1a.on.epicgames.com/library/api/public/items?includeMetadata=true`
  - Records are in `records`. For the next page, repeat with `cursor=<responseMetadata.nextCursor>` until there's none.
  - Fields: `namespace`, `catalogItemId`, `appName`, `sandboxType`.
- **Game info:** `GET https://catalog-public-service-prod06.ol.epicgames.com/catalog/api/shared/namespace/{namespace}/bulk/items?id={catalogItemId}&includeDLCDetails=true&includeMainGameDetails=true&country=US&locale=en`
  - Response: `{ "<catalogItemId>": { title, keyImages:[{type,url}], categories:[{path}], mainGameItem? } }`.
  - Up to 8 requests run at once.
- **Cover image:** the first `keyImages` entry with type `DieselGameBoxTall`, else `OfferImageTall`, else `Thumbnail`.

### Which items count as games (same rules as Legendary)
- **Skip:**
  - `sandboxType == "PRIVATE"`
  - `namespace == "ue"` (Unreal Engine assets)
  - no `appName`, or `appName == "1"`
  - items with `mainGameItem` (DLC; it'll be shown under its game later)
  - items with a category `path == "mods"`
- Remove duplicates by `catalogItemId`.
- **Windows check** (added 2026-11-21, like Legendary's `get_game_assets`): each refresh also fetches Epic's list of apps with a Windows build (`GET {launcher}/launcher/api/public/assets/Windows?label=Live`, a JSON list of `{appName, namespace, catalogItemId, buildVersion, ...}`). A game whose `appName` isn't in it gets `installable: false`:
  - if the same namespace has an installable game with the same title, the non-Windows one is hidden (Bendy is in some libraries twice);
  - otherwise it moves to its own **Not on Windows (N)** tab (shown only when N > 0), with no button on its card and one explanation under the grid: these are mostly phone games, because Epic's mobile store shares one library with PC games. "All" and the game count only cover games with a Windows download.
  - If that list can't be fetched, every game counts as installable (the Install box still explains), and caches saved before this field count as installable too.
- If the catalog lookup for an item fails, keep it with `appName` as its title and no cover, rather than hiding a game the user owns.

### Cache and privacy
- **Location:** `%APPDATA%\FishyClient\library.json`. It holds the game list only (titles, ids, image URLs), never tokens.
- **Log out deletes it,** which closes the privacy item from §12.
- An access token that's about to expire is refreshed (one operation at a time, as in §6a) before the library calls.

### Tests
- **Rust (fake Epic server):** pagination, bearer header, filtering rules, cover choice, catalog-failure fallback, cache round-trip, and log out deleting the cache.
- **UI:** grid with titles, search, empty state, error with Try again, and a cached grid kept on error.

---

## 6c. Install (milestone 4), approved 2026-11-20 on branch `feat/install`

Launch and Updates are later milestones and are left out here.

### What the user sees
- **Install button** on every not-installed game card. It opens a small **Install box** showing:
  - the install folder: `%USERPROFILE%\Games\<folder>` by default (no admin rights needed). `<folder>` is the catalog's `customAttributes.FolderName.value`, else the game title with characters Windows forbids removed.
  - **Change folder**, which uses the system folder picker. The picked folder is the parent, and `<folder>` is added under it.
  - **Download size**, **space needed** and **free space** on that drive.
  - **Install**, disabled with the reason shown if there isn't enough space.
- **Downloads page:**
  - The current download with the real `FishProgressBar` and **Pause / Resume / Cancel**.
  - Below it, a **queue** of waiting games, each with a remove (×) button. Only one game downloads at a time.
  - Empty state: "Nothing downloading. Pick a game in your Library and press Install."
- **Cancel** asks "Stop and delete the downloaded files?". On yes, every file written so far is deleted, along with the download's saved progress.
- **App closed mid-download:** on the next start, the download appears **Paused** on the Downloads page. **Resume** continues.
- **Done:** the bar's done state. The game moves to the installed list. The Play button arrives with Launch (milestone 5); until then the done state shows "Installed".
- **Library:**
  - an **Installed** badge on installed cards
  - **All / Installed** tabs above the grid
  - a "⋯" menu on installed cards with **Uninstall**, confirmed by "Uninstall *Title*? Its game files will be deleted." Files the user added themselves stay.
  - A card that is downloading or queued shows "Downloading…" or "Queued" instead of Install.

### Epic endpoints (verified against Legendary `egs.py` and `core.py`, 2026-11-20)
- **Manifest info:** `GET https://launcher-public-service-prod06.ol.epicgames.com/launcher/api/public/assets/v2/platform/Windows/namespace/{namespace}/catalogItem/{catalogItemId}/app/{appName}/label/Live` with bearer token and launcher User-Agent.
  - The response has `elements[0]` with `hash` (SHA-1 hex of the manifest file), `buildVersion` and `manifests: [{uri, queryParams?: [{name, value}]}]`. More than one element is an error.
- **Manifest file:**
  - Each manifest URL is `uri`, plus `?name=value&...` if it has `queryParams`.
  - URLs are tried in order until one returns 200. They're sent **without** the Authorization header.
  - The bytes' SHA-1 must equal `hash`.
- **CDN base URL:** `uri` with its last `/segment` removed, deduplicated in order. The first one is used.
- **Folder name:** the catalog item (§6b) `customAttributes.FolderName.value`.
- **Chunk download:** `GET {base_url}/{chunk_dir}/{group:02}/{hash:016X}_{guid}.chunk`, no auth.
  - `guid` is the four u32 words as `%08X`, joined with no separator.
  - `chunk_dir` depends on the manifest's feature level: ≥15 `ChunksV4`, ≥6 `ChunksV3`, ≥3 `ChunksV2`, else `Chunks`.
  - User-Agent `EpicGamesLauncher/11.0.1-14907503+++Portal+Release-Live Windows/10.0.19041.1.256.64bit`. Timeout 10 s, up to 7 tries with waits of 1, 2, 4… s.

### Manifest format (ported from Legendary `models/manifest.py`)
- **Formats:** a manifest starting with `{` is the **JSON format**; anything else is **binary**. Both are read into the same `Manifest`:
  - meta: app name, build version, launch exe, launch command, feature level
  - chunk list: guid, hash, SHA-1, group, window size, file size
  - file list: name, SHA-1, executable flag, chunk parts `(guid, offset, size)`
- **Binary header** (little-endian):
  - fields: magic `0x44BEC00C`, header size, uncompressed size, compressed size, SHA-1, stored-as flags (`1` zlib, `2` encrypted), version
  - The body starts at the header size. A compressed body is zlib-inflated and SHA-1 checked.
  - Sections follow, each starting with its own byte size: Meta, Chunk list, File list, Custom fields. The reader always skips to `start + size`, so unknown newer fields are ignored.
  - The full field layout is in `crates/fishy-core/src/install/manifest/binary.rs` comments.
- **JSON format:** numbers are "blobs" (each 3-digit decimal group is one byte, little-endian), and GUIDs are 32 hex characters read as four big-endian u32.
- **Not supported:** encrypted manifests or chunks (feature level ≥ 22 with secrets). These show "This game uses a download format Fishy Client can't install yet."

### Chunk files (ported from Legendary `models/chunk.py`)
- Header: magic `0xB1FE3AA2`, header version, header size, compressed size, guid, rolling hash, stored-as; from v2, SHA-1 and hash type; from v3, uncompressed size.
- The data runs from the header size to the end, zlib-inflated if stored-as has bit 1.
- **Check:** SHA-1 of the inflated data must equal the manifest's chunk SHA-1, which every manifest has. A mismatch counts as a failed try and is downloaded again.

### How a download runs
1. **Prepare** (when the user presses Install):
   - Fetch manifest info and the manifest, check its hash, and parse it.
   - Save it to `%APPDATA%\FishyClient\downloads\<appName>\manifest`.
   - Work out the sizes. Install size is the sum of file sizes. Download size is the sum of the compressed sizes of the chunks the remaining files need.
2. **Plan:**
   - Files are sorted by lowercase name, skipping files already in the resume list.
   - Chunks are fetched in the order they're first needed.
   - A chunk used by several files stays in memory until its last use (reference counting), so it's downloaded only once.
3. **Download:**
   - Up to 16 chunks are in flight at once, received in order.
   - The writer goes file by file. It creates folders, writes each chunk part at its place in the file, and computes the file's SHA-1 while writing.
   - A file whose SHA-1 doesn't match is an error.
4. **Resume list:** `%APPDATA%\FishyClient\downloads\<appName>\resume.txt`.
   - One line `<sha1 hex>:<file name>` is appended each time a file is finished and checked.
   - On resume, listed files that still exist are skipped. Unfinished files are rewritten from the start, as in Legendary.
5. **Pause** stops the workers and keeps the resume list. **Resume** builds a new plan from it.
6. **Finish:**
   - Write the installed-game record.
   - Move the manifest to `%APPDATA%\FishyClient\manifests\<appName>.manifest` (kept for uninstall and updates).
   - Delete the download folder.
7. **Progress:**
   - Sent to the UI about 4 times a second as `downloaded` / `total` bytes (compressed, so the numbers match the download size) and a speed averaged over the last ~3 s.
   - State: `downloading`, `paused`, `error` (with message) or `done`.

### Space check
Before starting, the free space on the install folder's drive must be at least the install size, minus the bytes of files already finished when resuming. Otherwise: "Not enough space on C:. Needs 12.3 GB, 4.1 GB free."

### Saved data (`%APPDATA%\FishyClient\`)
- `installed.json`: `[{app_name, title, version, install_path, executable, launch_parameters, install_size, installed_at}]`.
- `manifests\<appName>.manifest`.
- `downloads\<appName>\` (manifest, resume list and `job.json` with title, install path and namespace/ids) for paused or unfinished downloads. On startup, each folder there shows up as a Paused download.
- **No tokens** in any of these.
- **Log out:** pauses the running download and keeps installed games and their records, because the game files are the user's own. The future "delete all my data" button (Settings) also removes these records.

### Uninstall
- Delete each file listed in the saved manifest, then remove folders that are left empty (deepest first), then the install folder itself if empty.
- Remove the record and the saved manifest.
- Files that can't be deleted (for example, the game is running) give "Couldn't delete some files. Close the game and try again." The record is kept so the user can retry.

### Errors shown to the user
| Situation | Message |
|---|---|
| No internet / CDN unreachable after retries | "Download stopped: the connection to the download server dropped." + **Retry** (resumes) |
| Not enough space | "Not enough space on C:. Needs X, Y free." |
| Encrypted or unknown format | "This game uses a download format Fishy Client can't install yet." |
| Can't write to the folder | "Couldn't write to *folder*. Pick a different folder." |
| A file fails its check | "A downloaded file was damaged. Press Retry to download it again." |
| Session expired while preparing | Back to the login screen, as in §7. A paused download stays. |

### Tests
- **Rust:**
  - binary and JSON manifests (built in tests with a small writer, plus a tiny hand-made sample file)
  - chunk header parsing and inflating
  - chunk path format
  - plan building (order, reference counts, resume skipping)
  - a full install against a fake CDN (wiremock): pause, resume, a bad chunk being retried, a damaged file being caught
  - space check, installed.json round-trip, uninstall leaving the user's own files
- **UI:** Install box (default folder, not-enough-space), Downloads page states (downloading, paused, error, queue), Installed badge and tab, uninstall confirm.

---

## 6d. Launch (milestone 5), approved 2026-11-21 on branch `feat/launch`

### What the user sees
- **Play** on every installed game card, next to the "⋯" menu.
- After a click, the button shows **Starting…** for about 5 seconds, then goes back to Play. Fishy Client doesn't track the running game: many games hand off to a second program and the first one exits, so "Running" would often be wrong.
- Errors appear in a small box on the card (table below).

### Where the launch details come from
- **Install record** (`installed.json`, §6c): install folder, `executable` and `launch_parameters` from the manifest.
- **Saved Library** (§6b). Each `Game` also keeps these catalog `customAttributes`, so games installed before this milestone work without reinstalling:
  - `AdditionalCommandLine.value`: extra arguments (default none).
  - `OwnershipToken.value == "true"`: the game needs an ownership token (default false).
  - `CanRunOffline.value != "false"`: the game may start without Epic (default true, as in Legendary).
  - `ThirdPartyManagedApp.value` or `ThirdPartyManagedProvider.value`: the game starts through another store's launcher (EA app, Ubisoft Connect). Not supported yet.
  - Caches saved before this milestone lack these fields and read as the defaults until the next refresh.
- **Session:** `account_id` and `displayName`.
- **Language:** the language part of Windows' display language (for example `en`), else `en`.

### Epic endpoints (verified against Legendary `api/egs.py` and `core.py`, 2026-11-21)
- **Exchange code:** `GET https://account-public-service-prod03.ol.epicgames.com/account/api/oauth/exchange` with bearer token and the launcher User-Agent. The response's `code` is used once and expires in about 5 minutes. A fresh one is fetched on every Play.
- **Ownership token** (only when the game needs one): `POST https://ecommerceintegration-public-service-ecomprod02.ol.epicgames.com/ecommerceintegration/api/public/platforms/EPIC/identities/{account_id}/ownershipToken`, form `nsCatalogItemId={namespace}:{catalogItemId}`, bearer token. The raw response bytes are written to `%TEMP%\FishyClient\{namespace}{catalogItemId}.ovt`.

### Command line (same order as Legendary `get_launch_parameters`)
- **Program:** `install_path` joined with `executable`, with `/` or `\` accepted and a leading slash ignored. It goes through the same safety check as install file names, so it can never point outside the game folder.
- **Working folder:** the program's own folder.
- **Arguments, in order:**
  1. `launch_parameters`, split on spaces. A part in double quotes stays one argument, with the quotes removed.
  2. `AdditionalCommandLine`, split the same way.
  3. `-AUTH_LOGIN=unused`, `-AUTH_PASSWORD={code}`, `-AUTH_TYPE=exchangecode`, `-epicapp={app_name}`, `-epicenv=Prod`
  4. `-epicovt={path}`, only if an ownership token was fetched
  5. `-EpicPortal`, `-epicusername={displayName}`, `-epicuserid={account_id}`, `-epiclocale={language}`, `-epicsandboxid={namespace}`
- **Offline:** `-AUTH_PASSWORD=` is empty and no ownership token is fetched.
- **Not yet:** `-epicdeploymentid`. Legendary reads it from the manifest info's `sidecar` field, which few games have. It gets added if a real game needs it.
- The game is started and not waited for.

### Secrets
- The exchange code and ownership token never go into logs or saved files, apart from the `.ovt` file the game must read.
- The launch command's debug form shows `-AUTH_PASSWORD=[redacted]`.
- `.ovt` files left in `%TEMP%\FishyClient\` are deleted when Fishy Client starts.
- **Known limit (same as Epic's launcher and Legendary):** other programs on the PC can read a game's start-up arguments, including the one-time code, while it is still valid.

### Errors shown to the user
| Situation | Message |
|---|---|
| No internet / Epic unreachable | "Couldn't reach Epic to sign the game in. Check your internet." + **Try again**, plus **Play offline** if the game can run offline |
| Session expired | Back to the login screen, as in §7 |
| Program file missing | "Couldn't find *Title*'s program file. Uninstall and install it again." |
| Windows needs admin rights (os error 740) | "Windows wouldn't start *Title*: it needs administrator rights." |
| Windows refuses for another reason | "Windows wouldn't start *Title*: *reason*." |
| EA app / Ubisoft game | "*Title* starts through *store*, which Fishy Client can't do yet." |
| Game not in the saved Library | "Refresh your Library, then try again." |
| Epic rejects the request | "Epic wouldn't let *Title* start (*code*). Try again later." |

### Tests
- **Rust (wiremock):** exchange code and ownership token, each for success, rejection and network failure. Every request must carry the bearer token.
- **Rust (plain):**
  - argument order and splitting with quotes
  - offline form
  - ownership-token flag
  - unsafe or missing program path
  - redacted debug form
  - catalog attribute defaults
  - old library caches still load
- **UI (vitest):** Play shown only on installed cards, Starting… then back to Play, each error message, Play offline shown only when allowed.

---

## 6e. Updates (milestone 6), approved 2026-11-21 on branch `feat/updates`

### What the user sees
- **Update check:** every Library refresh (at start-up, and on coming back online) compares each installed game's version with Epic's newest. No extra network call is needed.
- **A card with an update waiting:**
  - an orange **Update available** badge instead of "Installed"
  - an **Update** button, with **Play** and **⋯** still next to it
- **Play on an outdated game** first shows a box:
  - Text: "*Title* has an update waiting. You'll play the older version, and some online games won't start until they're updated."
  - Buttons: **Play anyway**, **Update now**, **Cancel**.
  - Tick box: **Don't show this again** (saved as a setting).
- **Update** queues the game on the Downloads page like an install, with the fish bar.
  - The card and the Downloads page say **Updating…**, and Play is hidden while the update is queued, running or paused.
  - Updates can be **paused and resumed, but not cancelled**, because halfway through some files are already new. The Downloads page shows "Updates can be paused but not cancelled." under an update, and the queue has no × for updates.
- **Done:** the badge goes back to **Installed**. The saved version and file list are the new ones.
- **Settings page** (replaces the placeholder):
  - **Update games automatically**, off by default. When on, every update a refresh finds is queued by itself.
  - **Warn before playing a game with an update waiting**, on by default. Ticking "Don't show this again" turns it off.

### Saved data
- `%APPDATA%\FishyClient\settings.json`: `{ "auto_update": false, "warn_outdated_play": true }`. A missing or unreadable file means the defaults. Settings stay on Log out. The future "delete all my data" button removes the file too.
- `job.json` (§6c) gains `update_from: Option<String>`, the installed version an update started from. None means a normal install. Old `job.json` files without it read as installs.

### Epic endpoints (verified against Legendary `core.py` `get_asset` / `is_installed_game_update_available`, 2026-11-21)
- **Newest version:** the asset list the Library already fetches (§6b, `GET /launcher/api/public/assets/Windows?label=Live`). Each element has `appName` and `buildVersion`.
  - The library refresh now returns `latest_versions: { appName: buildVersion }` next to the games.
  - An update is available when an installed game's `version` differs from its `latest_versions` entry. Games missing from the list never show an update.
- **New manifest:** the same manifest-info and manifest calls as installs (§6c). The update starts from the manifest whose `buildVersion` is the newest.

### Comparing manifests (ported from Legendary `ManifestComparison`)
- Files are matched by name, and their SHA-1 decides the group:
  - same SHA-1: **unchanged**
  - different SHA-1: **changed**
  - only in the new manifest: **added**
  - only in the old manifest: **removed**

### Update plan (ported from Legendary `run_analysis`, `re_usable`)
- **Unchanged files** count as already done. They are never written.
- **Added files** are planned like an install.
- **Changed files:** each part of the new file is checked against the old file's parts.
  - A new part `(guid, offset, size)` is **reused** when an old part with the same guid covers `offset..offset+size`. Its bytes are then read from the old file at `old_file_pos + (offset - old_part.offset)`.
  - Every other part is downloaded.
- **Chunk counts and download size** include only downloaded parts.
- **Resume:** files in `resume.txt` are skipped, as in installs.

### Writing
- **Added files** are written in place.
- **Changed files** are written to `<name>.fishy-update` in the same folder (reused parts read from the old file, the rest from chunks). After the SHA-1 check, the temporary file replaces the old one. The game is never left with a half-written file.
- **Removed files** are deleted after every planned file is done, then folders left empty, deepest first. Files that can't be deleted are left alone.
- **Safety net:** if a file that used reused parts fails its SHA-1 check, it is rebuilt once using downloaded chunks only (fetched one by one, outside the normal stream). A second failure is the usual "damaged" error.
- **Finish:**
  - the record gets the new version, executable and launch parameters, and keeps its install path and install date
  - the saved manifest is replaced
  - the download folder is deleted

### Space check
Needed space is:
- the sum of added file sizes
- plus, for each changed file, `max(0, new size - old size)`
- plus the size of the largest changed file (its temporary copy)

Files already finished in an earlier run are left out. The message is the same as for installs (§6c).

### Uninstall while an update is paused
The files listed in both the old and the new manifest are deleted. Then the paused update's download folder is removed, then the record.

### Errors
These are the same as for installs (§6c table). Two additions:

| Situation | Message |
|---|---|
| The new manifest can't be fetched | "Couldn't get the update from Epic. Try again later." |
| Game no longer in the Library list (no newest version) | No Update button is shown |

### Not in this milestone
- Epic's delta manifests (`Deltas/<new>/<old>.delta`), which shrink some patches further.
- Updating DLC separately.
- Checking for updates on a timer while the app stays open. Refreshing the Library checks again.

### Tests
- **Rust:**
  - compare (all four groups)
  - update plan: reuse ranges, a part only partly covered (so it's downloaded), the download size counting only downloaded chunks, resume skipping
  - space check for updates
  - `job.json` without `update_from`
  - settings defaults and round-trip, including a damaged file
  - asset list versions
- **Rust, full update against the fake CDN (wiremock):**
  - unchanged files are never touched
  - reused parts are never downloaded
  - changed files are swapped in
  - removed files are deleted
  - the record moves to the new version
  - pause then resume mid-update
  - the damaged-reuse fallback
- **UI:**
  - Update badge and button
  - the Play warning: Play anyway, Update now, Cancel, and the tick box saving the setting
  - Updating… state with Play hidden
  - no Cancel or × for updates
  - the Settings switches
  - auto-update queues a game after a refresh

---

## 6f. Polish (milestone 7), started 2026-11-21 on branch `feat/polish`

Polish is split into five sub-projects, built in this order. Each gets its own design here, its own plan and its own hand check:
1. **Settings, logs and your data** (this section, approved 2026-11-21)
2. **Friendlier errors** (§6f.2, approved 2026-11-22)
3. **Library filters and sort** (§6f.3, built 2026-11-22)
4. **Running games** (added 2026-11-22 at your request; designed in §6f.4):
   - the game's card shows **Running** while any program from its install folder is open, checked every few seconds, only after Play and only until the game closes
   - Play, Update and Uninstall wait while it runs
   - a setting "When a game starts": **Keep Fishy Client open** (as now) or **Minimize it, and bring it back when the game closes**; downloads keep going either way
   - fully closing Fishy Client on Play was considered and rejected: nothing would be left to reopen it, and downloads would stop
5. Small fixes: "~600 min left" says hours, and the 8 minor items from the milestone 4 review (§12), plus the deferred minors from Polish 1, 2 and 4 (built 2026-11-22; what was fixed and what was left is in §12)
6. Installer (`.msi`), last, so it packages everything above (designed in §6f.6)

### 6f.1 Settings, logs and your data

Built in two halves: **1a** logs, Account and "Delete all my data"; **1b** the four new settings.

#### What the user sees
Settings gets three sections:
- **Downloads:** Default install folder, Limit download speed, Update games automatically, Warn before playing a game with an update waiting (the last two already exist).
- **Appearance and startup:** Reduce motion, Start with Windows.
- **Account:** Log out (moved here from the sidebar, so there is one place for it), Open logs folder, Delete all my data….

#### Logs
- **Where:** `%APPDATA%\FishyClient\logs\`, one file per day (`fishy-client.YYYY-MM-DD.log`). Files older than 7 days are deleted at startup.
- **How:** the standard Rust tools: `tracing`, `tracing-subscriber` and `tracing-appender` (daily file). Info level and up.
- **What's logged:** app start and version; sign-in worked or failed (no details from Epic); Library refresh and how long it took; install, update, pause, resume, cancel and uninstall steps (game name, sizes, result); every error with the plain message the user saw.
- **Never logged:** tokens, sign-in or exchange codes, Epic's raw error text, the Epic account ID, the email or display name.
- **Two safety layers:**
  1. Secret types already print as `[redacted]` (§12).
  2. Every finished line passes through a **scrubber** before it's written. It replaces with `[redacted]`:
     - anything after `Authorization:`, `bearer ` or `basic `
     - values of `code`, `token`, `access_token`, `refresh_token`, `exchange_code`, `password` (as `key=value` or `"key": "value"`)
     - Epic's `eg1~…` tokens
     - any unbroken run of 32 or more letters, digits, `-`, `_`, `+`, `/`, `=` (long random strings), **except** in Windows paths (a run that is part of a path such as `C:\…\…` is kept, so game folders stay readable)
- **Open logs folder** opens the folder in File Explorer (it's created first if missing). The Rust side starts `explorer.exe` with that one fixed path; no plugin and no general "open any link" permission comes back (§12).

#### Delete all my data
- **Settings → Account → Delete all my data…** (a red button) opens a confirm box:
  - "This logs you out and removes everything Fishy Client saved on this PC: settings, your game list, download progress and logs."
  - Tick box, **off** by default: "Also delete my installed games (N games, X GB)". When there are unfinished downloads it also offers those ("Also delete my installed games and unfinished downloads (1 game, 2.0 GB; 1 download)", or "Also delete my unfinished downloads (1 download)" if nothing is installed yet). It's hidden when there's neither. While it's off, the box says "Your game folders stay on disk, but Fishy Client will forget them."
  - **Cancel** and **Delete**.
- **What happens, in order:**
  1. Every download stops; prepared Install boxes are forgotten.
  2. Log out (tokens leave Credential Manager, the saved library is deleted), the same as the Log out button.
  3. If ticked: each installed game is uninstalled the normal way (§6c), including both versions of a paused update.
  4. Start with Windows is turned off.
  5. Everything inside `%APPDATA%\FishyClient\` is deleted. Today's log file is still open, so it's emptied instead.
  6. The app shows the Login screen with default settings.
- **If something can't be deleted** (a file in use, usually a running game), the rest still goes, and the box says: "Some files couldn't be deleted (N). Close any running games and try again." The list of what failed goes to the log (paths only).
- Nothing in the WebView's own storage needs deleting: the app keeps nothing there.

#### Default install folder
- `settings.json` gains `install_folder` (none = the built-in default, `%USERPROFILE%\Games`).
- Settings shows the folder in use with **Change…** (the folder picker) and **Use default** (only shown when a folder is set).
- The Install box starts with this folder; it can still be changed per game.
- If the saved folder can't be used (its drive is missing), the Install box uses the built-in default and says "Your default folder isn't available, so this uses C:\Users\…\Games."

#### Max download speed
- `settings.json` gains `speed_limit_mbps` (none = no limit; 1 to 1000 MB/s).
- Settings: a switch "Limit download speed" and a number box "MB/s" (starts at 10). Out-of-range numbers are refused with "Pick a number from 1 to 1000."
- The engine shares one **limiter** (a token bucket, one second of burst) across all piece downloads. It reads the current limit every time, so a change applies to a running download right away.

#### Reduce motion
- `settings.json` gains `reduce_motion` (off by default).
- On: the fish animations use their reduced-motion versions (§3). Off: the app follows Windows' "Animation effects" setting, as before. The `reducedMotion` store becomes "Windows asks for it **or** the setting is on".

#### Start with Windows
- Uses Tauri's official **autostart** plugin (a value under the current user's `Run` key in the Registry; no admin rights).
- Off by default. It isn't saved in `settings.json`: the switch reads the real Registry state each time Settings opens, so the two can't disagree.

#### Saved data
`settings.json` becomes `{ auto_update, warn_outdated_play, install_folder, speed_limit_mbps, reduce_motion }`. Older files without the new fields still load, with the defaults filled in.

#### Errors
- Saving any setting fails: "Couldn't save your settings." and the control shows what's really saved (as now).
- The folder picker fails: "Couldn't open the folder picker."
- Start with Windows can't be changed: "Couldn't change Start with Windows."
- Open logs folder fails: "Couldn't open the logs folder. It's at %APPDATA%\FishyClient\logs."
- Delete all my data: see above. If a download won't stop within a minute, nothing is deleted: "A download wouldn't stop, so nothing was deleted. Try again in a minute." While the delete runs, no download can start or join the queue. If Start with Windows can't be turned off, it counts as one thing that couldn't be deleted.

#### Tests
- **Rust:**
  - the scrubber: each secret shape is masked; normal lines and Windows paths are unchanged
  - logging into a temp folder: a line with a fake token is written as `[redacted]`
  - old log files are pruned after 7 days; newer ones stay
  - settings: the new fields round-trip; an old file without them loads with defaults
  - default install folder: used when set and present; falls back when missing
  - the speed limiter (fake clock): 2 MB at 1 MB/s waits about 2 s; no limit never waits; a limit change applies to the next piece
  - delete all my data on a temp folder: with and without games; a locked file is counted and the rest still goes; the open log file is emptied
- **UI (vitest):**
  - Settings shows the three sections
  - each new control saves its setting and shows the saved value after a failed save
  - Use default only appears when a folder is set
  - speed box range check
  - the delete box: tick box hidden with no games, the "folders stay" note, Cancel does nothing, Delete calls the command and shows Login, the partial-failure message
  - Log out is gone from the sidebar and works from Settings
  - reduced motion follows the setting or Windows

### 6f.2 Friendlier errors (approved 2026-11-22)

Four parts, all chosen by you: clear messages, automatic retries, a crash screen, and expired sign-ins going to Login. Together they finish §7.

#### Messages: every error says what to do next
- Each message names the problem in plain words and ends with the next step. Messages that already do are unchanged.
- **Changed messages:**

  | Where | Today | New |
  |---|---|---|
  | Library refresh | Couldn't load your library from Epic. | Couldn't load your library from Epic. Check your internet, then press Try again. |
  | Download lost its connection | Download stopped: the connection to the download server dropped. | Download stopped: the connection to the download server dropped. Check your internet, then press Retry. |
  | Epic's reply not understood (sign-in) | Epic sent an unexpected reply. Please try again. | Epic sent a reply Fishy Client didn't understand. Try again in a few minutes. |
  | Credential Manager refused | Couldn't save your sign-in to Windows Credential Manager. | Couldn't save your sign-in to Windows Credential Manager. Restart Fishy Client and sign in again. |
  | Game not installed (Play, Update, Uninstall) | That game isn't installed. | That game isn't installed any more. Refresh your Library. |
  | Getting an update | Couldn't get the update from Epic. Try again later. | Couldn't get the update from Epic. Check your internet, then try again. |
  | Browser for sign-in | Couldn't open your browser. | Couldn't open your browser. Use Sign in with Epic instead. |
  | Windows wouldn't start a game | Windows wouldn't start *Title*: *reason*. | Windows wouldn't start *Title*: *reason*. Restart your PC and try again. |
  | Folder can't be written | Couldn't write to *folder*. Pick a different folder. | unchanged, but no longer used when the game is running (below) |

- **A game that's running.** When Windows refuses a write because a file is in use (Windows errors 32 "sharing violation" and 33 "lock violation"), installing, updating or rebuilding a file says:
  **"Some of *Title*'s files are in use. Close the game, then press Retry."**
  The download stops as Error, so Retry continues it. Swapping an updated file into place also counts "access denied" (Windows error 5) as in use, because that's how Windows refuses to replace a running program. Other write errors keep the folder message.
- All the unchanged messages already end with a next step (for example "Not enough space on C:. Needs 12.3 GB, 4.1 GB free." comes with the Install box's Change folder, and "A downloaded file was damaged. Press Retry to download it again.").

#### Automatic retries
- One small helper in fishy-core runs an Epic call up to **3 times**, waiting **½ s** then **1 s** between tries.
- **Only connection problems are retried:** no connection, a timeout, the connection dropping, or Epic answering 500–599 (its server briefly failing). A real answer from Epic (wrong or expired sign-in, not owned, not found) is never retried.
- **Retried calls:**
  - the library: the list of your games, the list of Windows downloads, and each game's details
  - refreshing your sign-in
  - a game's download information (manifest address) and the file list itself
  - the two calls Play makes (exchange code, ownership token)
- **Never retried:** trading the one-time sign-in code (it only works once), and Log out.
- Game pieces (chunks) keep their own retries, as in §6c.
- The worst case adds 1.5 s before an error shows. Nothing changes when everything works.
- Each retry is logged ("retrying library items (2 of 3)"), with no addresses or tokens.

#### Expired sign-in
- When Epic refuses the saved sign-in while refreshing it (it was revoked, or it expired after weeks unused), the sign-in counts as **expired**. Only Epic's "this sign-in is no longer valid" answers count: an auth-token, invalid-grant or invalid-token error code, or a bare 400/401.
- **Not expired** (you stay signed in): can't reach Epic, Epic's server errors (500–599), "too many requests" (429), a request timeout (408), a blocked or throttled answer, a reply that isn't Epic's (a Wi-Fi login page), and a Credential Manager hiccup. Offline Play never forgets the sign-in.
- **What happens, wherever it's noticed** (Library refresh, Install box, Install, Update, Play):
  1. Downloads pause; nothing is lost and Resume continues after signing in again.
  2. The expired sign-in is removed from Credential Manager, and the saved library is deleted (as on Log out). Installed games, downloads in progress and settings stay.
  3. The app shows the Login screen with: **"Your sign-in has expired. Please sign in again."**
- Fishy Client doesn't call Epic's log-out for an expired sign-in (there's nothing left to end).

#### Crash screen
- If a screen hits a bug it didn't expect, the whole window shows a calm page instead of going blank:
  - the pixel fish, and the title **"Something went wrong"**
  - "Fishy Client hit a problem it didn't expect. Your games and downloads are safe."
  - **Reload** (reloads the window; downloads keep going, because they run in the Rust part) and **Open logs folder**
- **Logging:** the error's name and message (at most 500 characters) go to the log through the scrubber, as `ui error: …`. Nothing else from the screen is sent.
- Smaller bugs that don't break a screen (a background promise that fails) are only logged the same way, without interrupting you.

#### Tests
- **Rust:**
  - each changed message's exact wording
  - a write that fails with Windows error 32 or 33 gives the "files are in use" message with the game's title; error 5 (access denied) still gives the folder message, except when swapping an updated file into place
  - the retry helper against the fake Epic server: fails twice then works (one result, 3 requests); always fails (gives up after exactly 3 requests); answers 401/404 (1 request only); answers 503 twice then works; the one-time code exchange is tried once only
  - expired: a refresh Epic refuses removes the saved sign-in and reports expired; a refresh that can't reach Epic keeps it and reports a network problem
- **UI (vitest):**
  - an expired error from Library refresh, Install, Update or Play shows Login with the expired message
  - the crash screen appears when a child component throws, Reload reloads, and the error goes to `log_ui_error`
  - an unhandled promise rejection is logged and the page stays as it is

### 6f.3 Library filters and sort (approved 2026-11-22)
A small change to the existing Library screen, so it was designed in chat (no separate plan).
- **Updates tab:** All | Installed (n) | **Updates (n)** | Not on Windows (n). It lists installed games with a newer version from Epic, and only appears when there's at least one. If it empties while open (the last update finished), the Library goes back to All.
- **Sort menu** next to the search box: **A–Z** (default), **Z–A**, **Recently installed** (installed games first, newest on top, then the rest A–Z), **Size on disk** (installed games first, biggest on top, then the rest A–Z). Letter case is ignored. It works on every tab and together with search.
- The choice is saved as `library_sort` in `settings.json` (`"az"`, `"za"`, `"installed"`, `"size"`; older files load as `"az"`), so it stays after a restart; Delete all my data resets it.
- "Recently played" needs play times, which Fishy Client doesn't record yet; it can come with Running games (sub-project 4).
- **Tests:** each sort order (case, ties, uninstalled games last, the input list unchanged), the Updates tab (shown, counted, filtering, hidden without updates, back to All when empty), the menu saving its choice, and the saved choice used together with search; Rust: the new field's default and round-trip.

### 6f.4 Running games (approved 2026-11-22)

#### What the user sees
- After a successful **Play**, the card's button says **Running** (greyed out) for as long as the game is open. **Update** and **Uninstall** are greyed out too, with a small note: "Close *Title* first." Everything comes back when the game closes.
- **Starting…** (§6d) still shows for the first few seconds, then Running takes over while the game is open.
- **Settings → When a game starts:** **Keep Fishy Client open** (default) or **Minimize it, and bring it back when the game closes**. Downloads keep going either way (they run in the Rust part).
- **Sort menu** gains **Recently played**: games with a play time first (newest on top), then other installed games A–Z, then the rest A–Z.

#### How a running game is noticed
- **Only after Play.** A successful Play adds the game (its code name and install folder) to a running list in the Rust part. Games started any other way (Epic's launcher, a shortcut) aren't watched, and the list doesn't survive a restart of Fishy Client.
- **One check every 5 seconds, only while the list has a game.** The check asks Windows once for the list of open programs and each one's file path (Windows' "limited information" query, which also works for games running as administrator or protected by anti-cheat; nothing else is read) and keeps the games that have at least one program inside their install folder. When the list is empty, the check stops.
- **Inside the folder** means the program's path starts with the install folder as whole folder names, with capital letters ignored: `C:\Games\Kelp\Bin\kelp.exe` counts for `C:\Games\Kelp`, `C:\Games\Kelp2\kelp.exe` doesn't.
- A program whose path Windows won't tell (system services) is skipped, and so is one that has already ended.
- A game Play adds while a check is running is kept until the next check.
- A second Play of the same game is refused from the click until the game is in the list (or Play failed), so it can't start twice.
- Each change sends `running-changed` with the list of running code names. `running_list` gives it on demand (the Library asks once when it opens).
- **Pressing Play on a game that's already running** can't happen from the card (it's greyed out); the command also refuses it with "*Title* is already running."
- **Update and Uninstall** commands refuse a running game with "Close *Title* first." (the UI already hides the way there).
- **Automatic updates:** an update a Library refresh finds for a running game isn't queued; it's queued when the game closes (if the setting is still on).

#### Minimize while playing
- With **Minimize** chosen, a successful Play minimizes the main window.
- When the running list becomes empty and the window was minimized by Fishy Client, the window is restored and brought to the front. Windows sometimes refuses to let a program jump to the front; then it flashes the taskbar button instead.
- With **Keep open**, nothing happens to the window.

#### Play times
- Each successful Play saves `last_played` (the time, UTC) in that game's record in `installed.json`. Older records load with no time. Updating a game keeps it; uninstalling removes it with the record.
- A failed save of the time doesn't stop the game; it's logged ("couldn't save play time").

#### Saved data
`settings.json` gains `when_game_starts` (`"keep"` or `"minimize"`; older files load as `"keep"`) and `library_sort` accepts `"played"`. Delete all my data resets both.

#### Logs and privacy
- The log says `{app_name} is running` and `{app_name} closed`. The paths of other programs on the PC are never logged, saved or sent anywhere.

#### Tests
- **Rust:**
  - the folder match: inside counts; a look-alike folder (`Kelp2`), a parent folder or another drive doesn't; capital letters and `/` vs `\` don't matter
  - the running list: a game with no program left is removed; one check serves several games; the check reports "stop" when the list is empty
  - minimize: brought back only when the list empties and only if Fishy Client minimized it
  - play times: saved on Play, kept by an update, old `installed.json` loads without them
  - settings: `when_game_starts` default and round-trip
  - Play refuses a running game; Update and Uninstall refuse a running game with the message
- **UI (vitest):**
  - Running shown from `running-changed`, Play/Update/Uninstall greyed out with the note, back to normal when it's gone
  - the "When a game starts" control saves and shows the saved value after a failed save
  - Recently played order (played newest first, then other installed, then the rest; ties A–Z)
  - auto-update queues a skipped update when the game closes

### 6f.6 Installer (`.msi`) (designed 2026-11-22 while you were away; every choice below is easy to change)

#### What you get
- One file, `Fishy Client_1.0.0_x64_en-US.msi`, built with `npm.cmd run tauri build` from `app/`. It lands in `target\release\bundle\msi\`.
- Double-clicking it installs Fishy Client to `C:\Program Files\Fishy Client\`, with a Start menu shortcut. Windows asks for administrator rights once, as for any `.msi`.
- **Apps → Installed apps** lists it as "Fishy Client" (publisher Fishy Client), so it can be uninstalled from there.

#### Choices made for you (change any of them)
- **Only `.msi`.** No `.exe` (NSIS) installer and no `.app`/`.dmg`; Windows is the only platform (§1).
- **Version 1.0.0** (you decided version 1 is done on 2026-11-22). Installing it over 0.1.0 replaces the old version; your games, settings and sign-in stay.
- **Not code-signed** (signing costs money and is listed under "Later"). Windows SmartScreen will show "Windows protected your PC": click **More info → Run anyway**. Same as most open-source apps.
- **A fixed upgrade code** (`d895a25e-2ca5-44d4-b204-1ee4cb3b89f9`), so a newer `.msi` replaces the old version instead of installing a second copy.
- **WebView2:** if it's missing (rare on Windows 10/11), the installer downloads Microsoft's small WebView2 setup.
- **Uninstalling keeps your data:** your games, `%APPDATA%\FishyClient` (settings, logs, saved Library) and the sign-in in Credential Manager stay. Use **Settings → Delete all my data** first to remove them. (An `.msi` can't safely know which game folders are yours.)
- **Licence:** the installer's details say GPL-3.0-or-later and link to the GitHub page. No licence page to click through.
- **Start with Windows:** if it was turned on while running the development build, turn it off and on again once in the installed app, so it points at the installed program.

#### Tests
- A Rust test reads `tauri.conf.json` and checks the bundle: target `msi` only, the fixed upgrade code, the product name and publisher.
- Build check: the `.msi` is built, then unpacked with `msiexec /a` into a temporary folder (this doesn't install anything) to check it holds `fishy-client.exe` (in `Program Files\Fishy Client\`).
- Hand check (yours): install, start from the Start menu, sign in still works, uninstall from Installed apps.

---

## 6g. Import games from Epic's launcher (version 1.1), approved 2026-11-22 on branch `feat/import`

For a friend who already has games installed with Epic's own launcher: Fishy Client adds them as installed, without downloading them again.

### What the user sees
- **A note at the top of the Library** when Epic's launcher has games that are (1) in the user's Library, (2) fully installed by Epic, and (3) not yet installed in Fishy Client: "3 games installed by Epic's launcher can be added." with **Add them** and a **×**. The × hides the note until the app is started again.
- **Add them** checks each game in turn. The note shows "Checking *Title*…" while it works, then one line per game:
  - **All files there:** the game becomes **Installed** at once. Play, Update and Uninstall work as for any game. No line is needed; the card changes.
  - **Some files missing or the wrong size:** "*Title*: 12 files are missing or damaged (340 MB)." with **Download them**. That queues the game on the Downloads page like an install; only those files are downloaded, into the same folder. It can be paused, resumed and cancelled like an install (Cancel removes only Fishy Client's download folder; the game's files are left as they are, so Epic's launcher can still repair it).
  - **Can't be read:** "Couldn't read *Title*'s files from Epic's launcher. Press Install to download it."
- **Left out:** add-ons (DLC, which come with their game), installs Epic marks as unfinished, entries whose game folder no longer exists, and games not in the Library.
- Third-party-store games and games needing a newer version get the existing messages (§6d, §6e) when played or updated.

### Where Epic keeps its list (checked against Legendary `models/egl.py` and `core.py` `egl_import`, and against this PC's files, 2026-11-22)
- Folder: `%PROGRAMDATA%\Epic\EpicGamesLauncher\Data\Manifests\`, one JSON `<InstallationGuid>.item` file per installed app.
- Fields used: `AppName`, `DisplayName`, `InstallLocation`, `InstallationGuid`, `AppVersionString`, `CatalogNamespace`, `CatalogItemId`, `MainGameAppName`, `bIsIncompleteInstall`.
  - An add-on has a `MainGameAppName` different from its own `AppName`.
- The game's file list: `<InstallLocation>\.egstore\<InstallationGuid>.manifest` (Epic's normal manifest format, §6c).
  - If it's missing or can't be parsed, Epic's current manifest is downloaded with the normal install calls (§6c). Files that differ from it then count as damaged and are downloaded.
- **Read-only:** Fishy Client never changes, moves or deletes anything in Epic's folder or `.egstore`. Unreadable `.item` files are skipped.

### How a game is added
1. Get the manifest (the `.egstore` copy, else download it) and parse it.
2. **Check:** every file in the manifest must exist in the install folder with the manifest's size. File names go through the same safety check as installs (§6c), so a bad manifest can't point outside the folder. No SHA-1: hashing a 30 GB game takes minutes, and Legendary's import doesn't hash either.
3. **All good:** write the download folder's `job.json`, manifest and `resume.txt` (every file), then finish it as an install (§6c): the record gets the manifest's version, executable and launch command, the manifest is saved under `manifests\`, the download folder is deleted.
4. **Some bad:** the same `job.json`, manifest and `resume.txt`, but `resume.txt` lists only the good files. The download is left **Paused** with a message, and **Download them** resumes it. The existing resume rules (§6c) skip the listed files, so only the bad ones are downloaded. `job.json` gains `imported: bool` (default false), so Cancel deletes no game files.
5. The CDN addresses (`base_urls`) come from the manifest-info call, which needs the sign-in; it's made only when files must be downloaded (step 4) or the manifest is missing.

### Sharing the folder with Epic's launcher (known limits)
- The game stays in Epic's folder, so both launchers see it.
- Uninstalling in Fishy Client deletes the game's files; Epic's launcher then shows it as broken.
- Updating in one launcher can make the other think it needs an update or a repair.
- Epic gives normal users full control of each game folder, so Fishy Client's updates and uninstall need no administrator rights there. Epic's `.egstore` folder isn't in the manifest, so it stays behind after an uninstall.

### Privacy
- `.item` files hold no account data. Only the fields above are read.
- Logs say only counts and app names ("import: 3 found", "import: added", "import: 12 files to download"). Never folder paths or file names, which can contain the Windows user name.

### Errors
| Situation | Message |
|---|---|
| Manifest missing and no internet | "Couldn't reach Epic to check *Title*. Check your internet." |
| Manifest can't be used (encrypted, unknown format) | "Couldn't read *Title*'s files from Epic's launcher. Press Install to download it." |
| Session expired while fetching | Back to the login screen (§7) |
| Download errors after **Download them** | As for installs (§6c) |

### Tests
- **Rust (tempfile):** reading `.item` files (good, broken JSON, add-on, unfinished, missing folder); finding the `.egstore` manifest; the check (all good, missing file, wrong size, unsafe name); all-good import writes the record and saved manifest and leaves no download folder; partial import leaves a paused job whose `resume.txt` lists only good files; old `job.json` without `imported`; Cancel of an imported repair deletes no game files.
- **Rust (wiremock):** missing `.egstore` manifest downloaded; a repair downloads only the missing file.
- **UI (vitest):** the note and its count, × hides it, Add them, "Checking…", each result line, Download them.

---

## 7. Error handling

- Every error shown in the UI has **a plain-English message plus a next step**, for example "Couldn't reach Epic. Check your internet. [Retry]".
- Network calls retry automatically up to 3 times with growing delays before showing an error.
- If the session has expired and can't be renewed, you're sent back to Login and no download progress is lost.
- A download that fails partway can be resumed, because finished chunks are kept.
- Settings has an **Open logs folder** button for bug reports.

---

## 8. Testing

- **Core logic:** Rust unit tests for manifest parsing, chunk hashing and update comparison, using saved sample files.
- **Network:** tests use a fake Epic server (the `wiremock` crate), so they never touch your real account.
- **UI components:** the fish loader and progress bar have a preview page showing every state (downloading, paused, error, done, reduced motion).
- **By hand on your PC:** log in, install a small free game, launch it, then test an update.

---

## 9. Build order (milestones)

| # | Milestone | You can see |
|---|---|---|
| 0 | **Setup:** install Rust and the tools, create the project | An empty app window opens |
| 1 | **Fish components:** app icon, `FishLoader`, `FishProgressBar` with a fake-data preview page | The fish animations working |
| 2 | **Login** | You sign in and it remembers you |
| 3 | **Library screen** | Your games in a grid with cover art |
| 4 | **Install + downloads screen** | A real game downloading with the fish bar |
| 5 | **Launch** | The Play button starts the game |
| 6 | **Updates** | The update badge and a working patch download |
| 7 | **Polish:** settings, search and filters, error screens, installer (`.msi`) | Ready to share on GitHub |

The fish components come first, in milestone 1, because they don't depend on Epic at all. You get to see the look of the app early.

---

## 10. Setup needed on your PC (milestone 0)

1. **Rust:** install from https://rustup.rs (choose the default options).
2. **Microsoft C++ Build Tools:** Rust's installer asks for these on Windows. Install the "Desktop development with C++" workload.
3. **WebView2:** already included in Windows 11.
4. **Node.js:** already installed (v26).
5. **Git:** already installed.

---

## 11. Open questions for later
- ~~The final icon pixel grid.~~ Done: the rounded tile with the 14×10 fish (see §2).
- ~~Whether the sidebar uses pixel icons or simple line icons.~~ Done: small square bullets, as in the mockup.
- ~~A GitHub repo name.~~ Done: `Gnammahh/fishy-client` (private).

---

## 12. Progress log (updated 2026-11-21)

### Where everything is
- **Project folder:** `Desktop\fishy-client`.
- **Backup:** private GitHub repository **https://github.com/Gnammahh/fishy-client**. All three branches are pushed; only you can see it. GitHub shows `feat/epic-login` first. To back up new work, run `git push`.
- **Branches.** All are kept separate from `master` on purpose; nothing is merged yet. Each one builds on the previous.
  - `feat/ui-foundation`: milestones 0–1, the look of the app.
  - `feat/epic-login`: milestone 2, Epic login.
  - `feat/library`: milestone 3, the Library.
  - `feat/install`: milestone 4, Install (started 2026-11-20).
  - `feat/launch`: milestone 5, Launch (started 2026-11-21, built on `feat/install`).
  - `feat/updates`: milestone 6, Updates (started 2026-11-21, built on `feat/launch`).
- **Approved visual reference:** `design/mockup.html`. It's also published as a private claude.ai artifact, "Fishy Client Mockup".
- **Plans that were built:** `docs/superpowers/plans/2026-11-20-fishy-ui-foundation.md`, `docs/superpowers/plans/2026-11-20-epic-login.md`, `docs/superpowers/plans/2026-11-20-install.md`, `docs/superpowers/plans/2026-11-21-launch.md` and `docs/superpowers/plans/2026-11-21-updates.md`.

### What's done

**Milestone 0, setup**
- Rust, the Microsoft C++ Build Tools and the Tauri + Svelte app skeleton are installed.
- Window "Fishy Client": 1200×760, min 960×600. License GPL-3.0-or-later.

**Milestone 1, the look (branch `feat/ui-foundation`)**
- **App icon:** a pixel fish outline on a black tile with rounded corners, centered, sharp from 16 px to 256 px. It's generated from `assets/sprites/fish-icon.txt` with `npm run icon`.
- **Ocean background:** black fading into blue, with three layers of pixel-stepped waves drifting slowly.
- **Loading fish:** in 3 sizes. Food drops, the fish eats it and swims back, then flips and shows "Loaded" when done.
- **Download bar:** the fish eats the dots. It has five states: downloading, paused, checking files, error with Retry, and done with Play.
- **Honest numbers:** it never shows "NaN", "Infinity" or "100%" before the last byte, and it shows the real downloaded size even when the total is unknown.
- **Screen readers:** the bar is a real progress bar that only announces state changes.
- The **component preview** lives at `/preview`. It's development only; the shipped app returns "Not found".

**Milestone 2, Epic login (branch `feat/epic-login`)**
- **Sign in with Epic** opens Epic's real login page in a private window. Email/password, 2FA and Google sign-in all work. The Google sign-in was tested live with a real account.
- **Backup:** "Trouble signing in? Paste a code instead" opens Epic in your normal browser.
- **The login is remembered** in Windows Credential Manager, split into pieces because Epic's tokens are too big for one entry. It refreshes by itself 5 minutes before it expires.
- **App shell:** sidebar with Library, Downloads, Settings and Account, plus an offline banner with **Try again**. The Account page has **Log out**.
- **Log out** ends the session at Epic and deletes every saved piece.

**Milestone 3, Library (branch `feat/library`, built 2026-11-20)**
- **Library page:** your owned games in a grid with cover art (tall box art), sorted A–Z, with a search box and a game count.
- **Filtering:** DLC, mods, Unreal Engine assets and private items are filtered out, using the same rules as Legendary. A game whose details Epic can't return is still shown, under its internal name.
- **Fast start:** the last saved library shows instantly, then refreshes from Epic in the background ("Refreshing…").
- **States:** first load shows the loading fish; there are an empty state, an error with **Try again**, and a banner that keeps your games visible if a refresh fails.
- **Offline:** shows the saved games only, with no duplicate error next to the offline banner. It refreshes by itself once Epic is reachable again.
- **Saved library:** at `%APPDATA%FishyClientlibrary.json` (game list only, never tokens). **Log out deletes it**, and the app also forgets the previous account's games in memory.
- **Covers** load straight from Epic's image servers; a missing cover shows the pixel fish.
- **Checked by hand (2026-11-20):** your real library loads after signing in.
- **Sidebar:** pinned, so Library, Downloads, Settings and Account stay visible while you scroll. A faint school of gray pixel fish drifts diagonally up behind the menu; the fish hold still when Windows animations are off.
**Milestone 4, Install (branch `feat/install`, started 2026-11-20)**
- **Design approved** and written as §6c: Install box with a default folder of `C:\Users\<you>\Games\<game>`, a Downloads page with the fish bar, pause/resume/cancel and a queue, an Installed badge and tab, and Uninstall.
- **Protocol** researched in Legendary (`egs.py`, `core.py`, `models/manifest.py`, `models/chunk.py`, `downloader/mp/*`). Our addition: every finished file is also checked against its fingerprint.
- **Decisions:**
  - Log out pauses downloads but keeps installed games.
  - Encrypted newer games are refused with a clear message.
- **Download engine built (2026-11-21), in `crates/fishy-core/src/install/`:**
  - reads both of Epic's manifest formats (binary and JSON)
  - downloads chunks from Epic's CDN, 16 at a time, retrying a bad or failed chunk up to 7 times
  - checks every chunk and every finished file against its SHA-1
  - resumes from `resume.txt`
  - keeps the installed list in `installed.json`
  - does the space check and default folder (`C:\Users\<you>\Games\<game>`)
  - uninstall deletes only the game's own files
  - **Safety:** a manifest file name like `..\..\Windows\x` is refused, so nothing is ever written outside the game folder.
  - Rust tests: 122 (67 new).
- **App and screens built (2026-11-21):**
  - **Library:** an Install button on each game, an **Installed** badge, **All / Installed (N)** tabs, a "⋯" menu with **Uninstall** (asks first), and "Downloading… / Queued / Paused" on cards.
  - **Install box:** folder, download size, space needed and free space, plus **Change folder** (Windows' own folder picker). Install is greyed out with the reason when the disk is too full.
  - **Downloads page:** the fish bar, **Pause / Resume / Cancel** (Cancel asks "Stop and delete the downloaded files?"), an "Up next" queue with ×, paused downloads with Resume, and a "Finished: …" note.
  - One game downloads at a time. Closing the app keeps downloads; they come back as Paused.
  - **Log out** pauses downloads and keeps installed games.
  - Checked in the browser preview with fake data: every screen and state looks right, with no console errors.
  - Tests: Rust 124 (core 122 + app 2), UI 85.
- **Independent code review (2026-11-21): "ready with fixes".** The parser, path safety, resume and uninstall were judged correct, and it confirmed **no token leaks**: the token goes only to Epic's manifest-info address, never to the download servers or into any file. All 8 serious findings were fixed test-first:
  1. **Slow internet:** the 10 s limit applied to each whole piece, so slower lines could never finish. It now only trips when the server goes silent for 10 s.
  2. **Bad file list from one server:** a garbage file list now falls through to Epic's backup servers instead of a wrong "can't install" message.
  3. **Epic's "not available" answers** (not owned, no Windows build) no longer say "sign-in didn't work".
  4. **Memory:** pieces shared by several files are kept in memory only up to 512 MB. Past that they're downloaded again when needed.
  5. **Space** is checked on every start and Resume (for what's left), and a full disk shows "Not enough space".
  6. **Cancel then Install again** can't clash anymore: the game stays busy until its files are deleted.
  7. **A failed Cancel** (a file in use) now shows "Couldn't delete some files…" instead of vanishing.
  8. **The queue's ×** asks "Stop and delete the downloaded files?" like Cancel.
  - Deferred (minor), listed for later:
    - Pause/Cancel in the last second of an install is ignored.
    - Inflating pieces has no size cap before the check.
    - All writing runs on one core.
    - Startup reads every paused download's file list.
    - The Install box keeps its file list in memory until restart.
    - Two games could pick the same folder name.
    - The space check doesn't count other queued games.
    - The bar's "Installed" done state is replaced by a "Finished" note.
  - Tests now: Rust 134 (core 129 + app 5), UI 86.

**Milestone 5, Launch (branch `feat/launch`, built 2026-11-21)**
- **Design approved** and written as §6d. The protocol was researched in Legendary (`core.py` `get_launch_parameters`, `api/egs.py`, `models/game.py`).
- **Decisions:**
  - **Play offline** is offered when Epic can't be reached, only for games Epic marks as able to run offline.
  - The button shows **Starting…** for 5 seconds; the running game isn't tracked.
  - The extra launch details are kept in the saved Library, so games installed earlier work without reinstalling.
- **Built (test-first), in `crates/fishy-core/src/launch/`:**
  - Asks Epic for a fresh one-time sign-in code on every Play, plus an ownership-token file for the few games that need one.
  - Builds the game's command line in Legendary's exact order.
  - The program path gets the same safety check as installs.
  - Starts the game from its own folder.
  - **Secrets:** the code shows as `[redacted]` in any debug output and is never logged or saved. Ownership-token files in `%TEMP%\FishyClient\` are deleted when the app starts.
- **App:**
  - **Play** button next to "⋯" on installed games.
  - **Starting…**
  - Plain-word errors with **Try again** and, when allowed, **Play offline**.
  - An expired sign-in goes back to the login screen.
  - EA app / Ubisoft games get a clear "can't do this yet" message.
  - Checked in the browser preview with fake data: Play, the error box, Play offline then Starting…, with no console errors.
- **Not yet:** Epic's per-game "deployment id" (few games need it). It gets added if a real game fails without it.
- **Tests now:** Rust 171 (core 164 + app 7), UI 92. New: 29 core, 2 app, 4 UI.

**Milestone 6, Updates (branch `feat/updates`, built 2026-11-21)**
- **Design approved** and written as §6e. The protocol was researched in Legendary (`downloader/mp/manager.py` `run_analysis`, `workers.py`, `models/manifest.py` `ManifestComparison`).
- **Decisions:**
  - An **Update** button by default, plus an **Update games automatically** switch (off by default).
  - **Play** stays next to Update. Playing an outdated game first warns that it's the older version and that some online games won't start; the warning has a **Don't show this again** tick box.
  - Updates can be **paused but not cancelled**.
- **Built (test-first):**
  - Every Library refresh also keeps Epic's newest version of each game (no extra call). A game whose version differs shows **Update available**.
  - The download engine now updates:
    - unchanged files are skipped
    - pieces that already exist inside a changed file are **copied from your disk** instead of downloaded
    - changed files are written beside the old one and swapped in only after their fingerprint check
    - removed files are deleted at the end
    - if the old file was edited, that one file is rebuilt from fresh downloads
  - Resume, the space check and uninstall all understand updates. Uninstall with a paused update deletes the files of both versions.
  - The first real **Settings page**: the two switches, saved in `%APPDATA%\FishyClient\settings.json`.
  - Checked in the browser preview with fake data: the badge, Update, the warning box, Updating… on the card and on the Downloads page (Pause only), and Settings.
- **Not yet:** Epic's "delta manifests" (an extra size saving), separate DLC updates, and checking for updates on a timer while the app stays open.
- **Refresh button** (added 2026-11-21 at your request): in the Library header, it fetches the library again (which re-checks for updates) and re-reads the installed games, without reloading the app. The Library's own note now says "Refreshing…" so it isn't confused with a game's "Updating…". 3 UI tests.
- **Why Refresh takes a few seconds** (measured 2026-11-21 with your account, optimized build):
  - your library list: 0.9 s (191 items)
  - Epic's Windows list with the newest versions: 0.35 s
  - **the details of each game: 4.4 s (164 separate lookups)**
  - about 5.8 s in total, and slower in `tauri dev`
  - Allowing more lookups at once didn't help, so Epic's server sets the pace.
  - A faster way (only looking up new games or games whose version changed, as Legendary does) would bring it to about 1–1.5 s. You chose to keep it as it is for now.
  - The Library now says so next to "Refreshing…", and on the Refresh button's hover tip.
- **Small things noticed:** ~~updating a game while it's running fails with the folder message instead of "close the game first".~~ Fixed in Polish 2 (§6f.2).
- **Tests now:** Rust 195 (core 183 + app 12), UI 105. New: 19 core, 5 app, 13 UI.

**Milestone 7, Polish (started 2026-11-21 on branch `feat/polish`):** split into five sub-projects (§6f). Sub-project 1, **Settings, logs and your data**, is built:
- **Log files** in `%APPDATA%\FishyClient\logs`, one per day, kept 7 days. Every line passes through a tested scrubber that replaces anything secret-shaped with `[redacted]` (tokens, sign-in codes, `Authorization:` values, long random strings), while game folders stay readable. Logged: start, sign-in worked or failed (no details), Library refresh times, install/update/pause/resume/cancel/uninstall, and each error's plain message.
- **Settings is now three sections:**
  - **Downloads:** Default install folder (Change… / Use default), Limit download speed (MB/s, changes apply to a running download), and the two update switches.
  - **Appearance and startup:** Reduce motion (calms every fish animation, not only when Windows asks), Start with Windows (reads the real Windows setting).
  - **Account:** your Epic name, **Log out** (moved here; the sidebar's Account page is gone), **Open logs folder**, **Delete all my data…**.
- **Delete all my data** asks first, with a tick box (off by default) to also delete installed games and unfinished downloads. It stops downloads, logs out, turns off Start with Windows and empties `%APPDATA%\FishyClient`. If a download won't stop within a minute, nothing is deleted.
- If the saved install folder's drive is missing, the Install box uses `%USERPROFILE%\Games` and says so.
- Checked in the browser preview with fake data: all three Settings sections, the speed box, the Delete box (ticked and not), Delete leading to Login; no console errors.
- **Independent code review: "with fixes", no critical findings.** The 4 important ones were fixed test-first (Reduce motion reaching the ocean and sidebar fish; a first unfinished download offered for deletion; no download can start or keep writing during the delete; a failed Start with Windows turn-off is reported). 6 minor ones are listed under "Small things deferred".
- **Tests now:** Rust 228 (core 205 + app 23), UI 136. New: 22 core, 11 app, 31 UI.

**Polish 2, Friendlier errors (§6f.2), built 2026-11-22:**
- **Messages say what to do next:** 8 messages gained a next step (for example "Check your internet, then press Try again"). A game that's open gets "Some of *Title*'s files are in use. Close the game, then press Retry." when installing, updating or replacing a file, instead of the folder message.
- **Automatic retries:** library, sign-in refresh, a game's file list and the two Play calls try 3 times (½ s, then 1 s) on connection problems, Epic's server errors, "too many requests" and timeouts. Never retried: the one-time sign-in code and Log out. Each retry is logged.
- **Expired sign-in:** only Epic's "no longer valid" answer counts. Then downloads pause (kept), the sign-in and saved library are removed, and Login says "Your sign-in has expired. Please sign in again." Can't reach Epic, a blocked or throttled answer, a Wi-Fi login page and a Credential Manager hiccup all keep you signed in; offline Play never signs you out.
- **Crash screen:** "Something went wrong", with Reload and Open logs folder; the error goes to the log through the scrubber. Smaller background errors are only logged.
- Checked in the browser preview with fake data: the Library error and Try again, an expired sign-in landing on Login, and the crash page; no console errors. (Dev-only switches: `?fail=library`, `?fail=expired`, `?crash=1`.)
- **Independent code review: "with fixes", no critical findings.** Fixed test-first: a throttled or blocked refresh no longer signs you out; a Credential Manager hiccup no longer counts as expired; a Wi-Fi login page at startup no longer signs you out (an older bug); updating a running game now says "close the game" (Windows reports that case as "access denied"). 6 minor ones are under "Small things deferred".
- **Tests now:** Rust 263 (core 235 + app 28), UI 144. New: 30 core, 5 app, 8 UI.

**Polish 3, Library filters and sort (§6f.3), built 2026-11-22:** an **Updates** tab (only when updates are waiting) and a **Sort** menu next to the search box (A–Z, Z–A, Recently installed, Size on disk), remembered after a restart. Checked in the browser preview with fake data: the Updates tab, Z–A on every tab, no console errors. **Tests now:** Rust 263, UI 154 (10 new).

**Polish 4, Running games (§6f.4), built 2026-11-22:** after Play the card says **Running** while any program from the game's folder is open (checked every 5 s, only while a game runs); Play, Update and Uninstall are greyed out with "Close *Title* first."; a setting **When a game starts** (Keep open / Minimize and bring back); play times saved in `installed.json`, and a **Recently played** sort. Built from a plan (`docs/superpowers/plans/2026-11-22-running-games.md`), then a fresh whole-branch review: three fixes made test-first (games running as administrator or with anti-cheat now count, a second Play can't start a game twice, a game started during a check isn't dropped). Checked in the browser preview with fake data. **Tests now:** Rust 275 (core 244 + app 31), UI 159.

**Polish 5, Small fixes, built 2026-11-22** (designed in chat, no plan): long downloads say hours (`~10 h left`); a download piece can't unpack past its stated size; faster startup with paused downloads; closing the Install box frees its file list; two games never share a folder ("Name (2)"); the space check counts other downloads on the same drive (failed ones don't); the speed limit: off or higher applies at once, the same limit saved again changes nothing, the 1–1000 range is checked in Rust too and a bad saved value loads as no limit, a wrong number goes back; logs: camelCase token keys and values before `\"` are masked, old logs are trimmed daily, a screen error is one line; sign-in: each refresh try gives up after 8 s, a refresh finishing after the sign-in changed isn't saved, an expired sign-in leaves a clean Library; running games: the check resets after a crash (dev builds), Delete all my data won't delete games that are running or starting, no update is tried after a game closes while offline, Fishy Client only comes back if still minimized; a real same-moment test for `installed.json`. Fresh review: 5 more fixes made test-first. **Tests now:** Rust 293 (core 256 + app 37), UI 162.

**Polish 6, Installer (§6f.6), built 2026-11-22:** `npm.cmd run tauri build` (from `app/`) makes `target\release\bundle\msi\Fishy Client_0.1.0_x64_en-US.msi` (4.6 MB, about 3 minutes; the first build also downloads Microsoft's WiX tools). Checked without installing: unpacked with `msiexec /a`, it holds `Program Files\Fishy Client\fishy-client.exe`; its details read Fishy Client / Fishy Client / 0.1.0, the fixed upgrade code and the GitHub link. A Rust test guards the bundle settings. **Tests now:** Rust 294 (core 256 + app 38), UI 162.

**Benchmark against the Epic Games Launcher (2026-11-22, version 1.0.0, this PC: 16 cores, Windows 11).** Each launcher was started from fully closed, left idle for 30 s, then measured with all its helper processes for 20 s:

| | Fishy Client | Epic Games Launcher |
|---|---|---|
| Program on disk | 9 MB | 869 MB |
| Its data on disk | 0.3 MB + WebView2 cache (315 MB, includes development runs) | 564 MB + 342 MB |
| Memory in use | 429 MB | 1,449 MB |
| Memory only it uses (private) | 262 MB | 1,340 MB |
| Processes | 7 (app + 6 WebView2) | 10 (launcher, 4 web helpers, 5 overlay) |
| Processor use, idle | 0.3 % | 1.7 % |
| First window | 0.1 s | 3.1 s |
| Windows services added | none | 2 (updater, Online Services) |

Most of Fishy Client's memory is Microsoft's WebView2, which draws the window. "First window" is when Windows shows a window, not when the Library has finished loading. The WebView2 cache is left as is (your choice, 2026-11-22): it is mostly cover images (271 MB), WebView2 caps it itself, and Delete all my data empties it. If it should ever shrink, ask Epic for smaller cover images rather than limiting the cache.

### Still to check by hand (milestone 7, sub-project 6)
- [x] (2026-11-22) Close the development app, then double-click the `.msi`. If SmartScreen appears: **More info → Run anyway**. Windows asks for admin rights once.
- [x] (2026-11-22) Start **Fishy Client** from the Start menu: you're still signed in, your Library and installed games show, Play works
- [x] (2026-11-22) If Start with Windows was on: turn it off and on again in Settings (so it points at the installed program)
- [ ] Optional: **Settings → Apps → Installed apps → Fishy Client → Uninstall** removes the program; your games and settings stay

### Still to check by hand (milestone 7, sub-project 5)
- [x] (2026-11-22) A download shows hours when it's long (for example with a speed limit of 1 MB/s on a big game: `~… h … min left`)
- [x] (2026-11-22) Settings → Limit download speed: type `abc` in MB/s and click away: the message shows and the box goes back to the saved number
- [x] (2026-11-22) Changing the Sort menu while a limited download runs doesn't make it jump faster

### Still to check by hand (milestone 7, sub-project 4)
- [x] (2026-11-22) Play a game: the card shows **Starting…**, then **Running**, with Update and Uninstall greyed out and "Close *Title* first."
- [x] (2026-11-22) Close the game: within about 5 seconds the card shows **Play** again
- [x] (2026-11-22) Settings → **When a game starts** → Minimize: Play minimizes Fishy Client, and it comes back when the game closes
- [x] (2026-11-22) Sort → **Recently played** puts the game you just played first
- [x] (2026-11-22) The log shows `… is running` and `… closed`, and nothing about other programs (Bendy's code name shows as `[redacted]`: the log filter hides ID-shaped names)

### Still to check by hand (milestone 7, sub-project 3)
- [x] (2026-11-22) The **Sort** menu reorders your real Library (try Size on disk: Bendy first), and the choice is still set after restarting the app
- [ ] Optional, not tried: with an update waiting (mark Bendy outdated as in the milestone 6 check), the **Updates (1)** tab shows only Bendy

### Still to check by hand (milestone 7, sub-project 2)
- [x] (2026-11-22) With Wi-Fi off, **Refresh** shows "Couldn't load your library from Epic. Check your internet, then press Try again." (after about 1.5 s of retries); with Wi-Fi back, **Try again** works
- [x] (2026-11-22) The log shows `retrying …` lines from that refresh
- [ ] Waits for a real Epic patch (a faked older version changes no files, so nothing is replaced; covered by a test that locks a file the way a running game does): updating a game while it's running says "Some of *Title*'s files are in use. Close the game, then press Retry." (start Bendy, mark it outdated as in the milestone 6 check, press Update); after closing the game, **Retry** finishes the update
- [x] (2026-11-22; the sign-in was still fresh, so the Library message showed instead of the banner) With Wi-Fi off at startup you stay signed in (the "Couldn't reach Epic" banner shows), and nothing is signed out when it comes back

### Still to check by hand (milestone 7, sub-project 1)
Checked by you on 2026-11-21 in the real app (the log and secret search were checked by script):
- [x] A log file appears in `%APPDATA%\FishyClient\logs`, and **Open logs folder** opens it
- [x] After signing in and refreshing, the log has no secrets: `Select-String -Path "$env:APPDATA\FishyClient\logs\*.log" -Pattern "eg1~|access_token=[^\[]"` finds nothing
- [x] **Limit download speed** visibly slows a download, and turning it off speeds it up again
- [x] A changed **Default install folder** is used by the Install box
- [x] **Reduce motion** stops the ocean and the sidebar fish
- [x] **Start with Windows** stays as set after restarting the app (and the app starts after signing out of Windows and back in)
- [ ] **Delete all my data**, unticked: you're logged out, `%APPDATA%\FishyClient` is empty apart from today's empty log, `cmdkey /list | findstr /i fishy` prints nothing, and game folders still exist
- [x] The two left from milestone 6: the Play warning's tick box, and settings staying as set after a restart

### Still to check by hand (milestone 6)
- [x] A game shows **Update available**, and **Update** finishes and brings it back to "Installed". Checked on 2026-11-21 with Bendy marked as an older version (2.0.0-test); it went back to 2.1.0 with nothing left in `downloads\`. A real Epic patch is still to be seen.
- [x] Play on an outdated game shows the warning; the tick box stops it showing again (2026-11-21)
- [x] The Settings switches stay as set after restarting the app (2026-11-21)

### Still to check by hand (milestone 5)
- [x] Launch a real game from Fishy Client, signed in as you: **Bendy and the Ink Machine** started on 2026-11-21

### Still to check by hand (milestone 4)
Checked on 2026-11-21 with your real account, installing **Bad Cheese**:
- [x] Install from the Library (default folder)
- [x] Pause, then Resume
- [x] App closed mid-download (it restarted while code changed), Resume finished it
- [x] Shows as Installed when done
- [x] Uninstall removes it and the badge
- [ ] Cancel a download deletes its files (not tried by hand yet; covered by tests)

**Found during the hand check:**
- **Bendy and the Ink Machine** is in the library **twice** (same game, two Epic entries). One has no Windows download, so Epic answers `download_info_not_found`. The message used to say "format Fishy Client can't install yet"; it now says Epic offers no Windows download for that item and suggests trying the other entry.
  - **Fixed on 2026-11-21** (test-first): each Library refresh asks Epic which games have a Windows download, hides the Bendy twin without one, and shows "Not on Windows" instead of Install for any other game without one (rules in §6b). Tests: 4 new Rust tests in `library/build.rs`, 2 wiremock tests in `library/client.rs`, 1 UI test in `library.test.ts`.
  - [x] Hand check on 2026-11-21: Bendy shows once and its install starts.
  - Your library had 8 more games without a Windows download, mostly phone games (Dead Cells, Rocket League Sideswipe twice, Super Meat Boy Forever Mobile, both KOTORs, MR RACER, Residual). At your request they now sit in a **Not on Windows** tab with one explanation, instead of showing a label on every card in All (2 UI tests).
- **Resume goes back a little** (about 10% for Bad Cheese). The file that was half-written when you paused starts again from its beginning; every finished file is kept. Legendary does the same.
  - Possible later improvement: continue inside big half-written files.
- **Speed in the test build** looks lower because `tauri dev` builds without optimizations, so unzipping and checking pieces is slower than in the real app.

### Privacy and security decisions
- Your password goes to Epic's page only; Fishy Client never sees it.
- Tokens are stored only in Credential Manager, never in files or the page code, and they print as `[redacted]` anywhere they could leak.
- **No tracking or analytics.** The app talks only to Epic.
- **Log files never contain secrets:** no tokens, sign-in codes, Epic's error text, account ID, email or name. A scrubber masks anything secret-shaped on every line as a second safety net. Logs stay on your PC.
- The sign-in window is **private**, so Epic's website never remembers you after Log out (this matters on shared PCs). It also has **no access** to the app's internals.
- A sign-in code is accepted **only right after Epic's own final page**, so no other page can sign you into a different account.
- **Pop-ups** (Google/Apple/Steam sign-in) are allowed; they also have no access to the app.
- Session changes happen **one at a time**, so a late refresh can't undo a logout.
- Epic's error messages, which can quote tokens, are never stored.
- **Security policy (CSP) is on.** The unused "open links" plugin was removed, and the Tauri version is pinned to 2.12.
- **Running games (§6f.4):** the paths of other programs on your PC are only compared in memory to spot a game's folder. They're never logged, saved or sent.
- **README "Privacy" section (2026-11-22):** lists every Epic address contacted, everything stored on the PC and how to delete it, for users of the installer. Delete all my data now also empties the app window's WebView2 browser data (cached cover images in `%LOCALAPPDATA%\com.fishyclient.app\`); a failure there counts as one thing not deleted.
- **Dependency check:** 0 known problems in what ships. The warnings come from a development-only tool and are left as is.

### Bugs found while testing for real, and fixed
- **The sign-in window stayed white.** It was created from a command that blocks the main thread on Windows. That command is now async.
- **"Sign in with Google" did nothing.** A review fix had blocked pop-ups, so they're allowed again (see above).
- **"Loading fish" showed as "Loading Ash".** The pixel font's "fi" ligature looks like an "A", so ligatures are turned off.

### Tests
- **Rust core:** 256 tests, plus 1 that writes to the real Credential Manager (run on purpose only). App: 37. (Counts as of 2026-11-22, milestone 7 sub-project 5.)
- **UI:** 162 tests.
- **Two independent code reviews:** every important finding was fixed test-first.

### Still to check by hand (milestone 2)
- [x] Sign in (with Google)
- [x] Still signed in after restarting the app
- [x] Closing the sign-in window midway goes back to the login screen
- [x] Log out leads to the login screen; `cmdkey /list | findstr /i fishy` prints nothing, and Sign in asks for the password again

### Known limits and risks
- **Ban risk: low, not zero.** Epic doesn't officially allow third-party launchers. Legendary and Heroic use the same method and have been used for years without known bans for normal use.
  - Be careful with online anti-cheat games, Fortnite most of all (not tested; try it on a second account first).
  - Use only your own library.
  - A secondary account is the zero-risk option.
- Some anti-cheat games may not work, the same as with Heroic. Fortnite is untested; Legendary-based tools report it starts on Windows (checked online 2026-11-22).
- **PowerShell on this PC blocks npm's script shortcut,** so use `npm.cmd` (for example `npm.cmd run tauri dev`).

### Small things deferred
- Pixels may look slightly uneven at 125%/150% Windows display scaling.
- Animations keep redrawing when nothing moves.
- Check the 24/48 px icon sizes for blur.
- Two rare race/storage edge cases, noted in `crates/fishy-core/src/auth/manager.rs`.
- Left as is in Polish 5 (your choice, 2026-11-22): writing game files on several processor cores (the internet is almost always the limit); the "Finished" note on Downloads; 32-character game names showing as `[redacted]` in logs (they look exactly like one-time codes); a lost sign-in reply signing you out a little sooner (Epic's rule); names in screen errors (none today). Pause or Cancel after the last file is written finishes the install (better than pausing at 100%).
- From the Polish 5 review (minor):
  - Cancelling the Install box while it's still loading keeps its file list until restart.
  - The space reserved for queued updates is an estimate (download size, not space needed).
  - A refresh dropped because you switched accounts mid-refresh can show an error banner on the new account's Library (needs a log-out and log-in within one refresh).
  - After Offline, a refresh finishing for a different account is still saved (needs a log-out and log-in mid-refresh).
  - The daily log trim keeps 6 dated files after a same-day restart, not 7.
  - The log filter doesn't mask short values in escaped JSON (`\"accessToken\":\"…\"`); long ones are masked.
  - A very slow connection (over 8 s per sign-in try) now shows Offline where 20 s would have worked.
  - The crash reset of the running check only works in dev builds (a crash ends the shipped app).

### Next steps
1. ~~Finish the hand checks above.~~ Done: all four passed on 2026-11-20 (Credential Manager has 0 FishyClient entries after Log out).
2. ~~Back up to a private GitHub repository.~~ Done on 2026-11-20 (`gh` installed and signed in as Gnammahh). To back up new work: `git push`.
3. ~~Milestone 3, Library.~~ Built and checked with your real library on 2026-11-20 (the saved library is deleted on Log out).
   - ~~Still open from this list: log files with secrets filtered out, and a "delete all my data" button.~~ Built in milestone 7 (Polish 1).
4. Later milestones:
   - ~~Install~~ built (milestone 4)
   - ~~Launch~~ built on 2026-11-21 (milestone 5); Bendy launched by hand on 2026-11-21. Epic's launch code is visible to other programs on the PC while a game starts (§6d).
   - ~~Updates~~ built on 2026-11-21 (milestone 6); hand checks are above.
   - Polish (milestone 7): 1 Settings, logs and your data, built 2026-11-21 and hand-checked; 2 Friendlier errors, built 2026-11-22, hand check above. 3 Library filters and sort, built 2026-11-22, hand check above. 4 Running games, built 2026-11-22 and hand-checked. 5 Small fixes and 6 Installer, built 2026-11-22 and hand-checked (the installer installs and runs). That's all of Polish, merged into `master` on 2026-11-22. **Version 1.0.0** on 2026-11-22. Later: a code-signed installer.
   - Later: signed updates and a code-signed installer
