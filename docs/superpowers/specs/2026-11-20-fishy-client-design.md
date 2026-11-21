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
- Importing games already installed by Epic's launcher

### Known limits, the same ones Heroic and Legendary have
- **Fortnite** isn't supported.
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

**Where data lives:** settings and the cache go in `%APPDATA%\FishyClient\`, and logs in `%LOCALAPPDATA%\FishyClient\logs\`. The installed-games list is a JSON file in the settings folder.

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
- **Opening the app:** the last saved library shows instantly from the cache, then refreshes quietly in the background. A small "Updating…" note shows while that runs.
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
- **Approved visual reference:** `design/mockup.html`. It's also published as a private claude.ai artifact, "Fishy Client Mockup".
- **Plans that were built:** `docs/superpowers/plans/2026-11-20-fishy-ui-foundation.md`, `docs/superpowers/plans/2026-11-20-epic-login.md`, `docs/superpowers/plans/2026-11-20-install.md` and `docs/superpowers/plans/2026-11-21-launch.md`.

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
- **Fast start:** the last saved library shows instantly, then refreshes from Epic in the background ("Updating…").
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
- The sign-in window is **private**, so Epic's website never remembers you after Log out (this matters on shared PCs). It also has **no access** to the app's internals.
- A sign-in code is accepted **only right after Epic's own final page**, so no other page can sign you into a different account.
- **Pop-ups** (Google/Apple/Steam sign-in) are allowed; they also have no access to the app.
- Session changes happen **one at a time**, so a late refresh can't undo a logout.
- Epic's error messages, which can quote tokens, are never stored.
- **Security policy (CSP) is on.** The unused "open links" plugin was removed, and the Tauri version is pinned to 2.12.
- **Dependency check:** 0 known problems in what ships. The warnings come from a development-only tool and are left as is.

### Bugs found while testing for real, and fixed
- **The sign-in window stayed white.** It was created from a command that blocks the main thread on Windows. That command is now async.
- **"Sign in with Google" did nothing.** A review fix had blocked pop-ups, so they're allowed again (see above).
- **"Loading fish" showed as "Loading Ash".** The pixel font's "fi" ligature looks like an "A", so ligatures are turned off.

### Tests
- **Rust core:** 164 tests, plus 1 that writes to the real Credential Manager (run on purpose only). App: 7. (Counts as of 2026-11-21, milestone 5.)
- **UI:** 92 tests.
- **Two independent code reviews:** every important finding was fixed test-first.

### Still to check by hand (milestone 2)
- [x] Sign in (with Google)
- [x] Still signed in after restarting the app
- [x] Closing the sign-in window midway goes back to the login screen
- [x] Log out leads to the login screen; `cmdkey /list | findstr /i fishy` prints nothing, and Sign in asks for the password again

### Known limits and risks
- **Ban risk: low, not zero.** Epic doesn't officially allow third-party launchers. Legendary and Heroic use the same method and have been used for years without known bans for normal use.
  - Avoid online anti-cheat games (Fortnite etc.).
  - Use only your own library.
  - A secondary account is the zero-risk option.
- Fortnite and some anti-cheat games won't work, the same as with Heroic.
- **PowerShell on this PC blocks npm's script shortcut,** so use `npm.cmd` (for example `npm.cmd run tauri dev`).

### Small things deferred
- "~600 min left" should say hours.
- Pixels may look slightly uneven at 125%/150% Windows display scaling.
- Animations keep redrawing when nothing moves.
- Check the 24/48 px icon sizes for blur.
- Two rare race/storage edge cases, noted in `crates/fishy-core/src/auth/manager.rs`.

### Next steps
1. ~~Finish the hand checks above.~~ Done: all four passed on 2026-11-20 (Credential Manager has 0 FishyClient entries after Log out).
2. ~~Back up to a private GitHub repository.~~ Done on 2026-11-20 (`gh` installed and signed in as Gnammahh). To back up new work: `git push`.
3. ~~Milestone 3, Library.~~ Built and checked with your real library on 2026-11-20 (the saved library is deleted on Log out).
   - Still open from this list: log files with secrets filtered out, and a "delete all my data" button (they fit best with Settings).
4. Later milestones:
   - ~~Install~~ built (milestone 4)
   - ~~Launch~~ built on 2026-11-21 (milestone 5); Bendy launched by hand on 2026-11-21. Epic's launch code is visible to other programs on the PC while a game starts (§6d).
   - Updates
   - Polish, signed updates and a code-signed installer
