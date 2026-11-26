// Development-only fake of the Rust side, so the UI can be viewed in a normal browser.
// Never imported in the Tauri app (guarded by `dev` and the absence of __TAURI_INTERNALS__).
import { mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { AuthState } from "./auth";

const SIGNED_IN: AuthState = { kind: "SignedIn", display_name: "FishFan", account_id: "0123456789abcdef0123456789abcdef" };

const DEV_GAMES = ["Abyssal Lanterns", "Coral Kart", "Kelp Runner", "Tidepool Odyssey", "Reef Raiders", "Sunken Signals", "The Long Current", "Brine & Bones"].map((title) => ({
  app_name: title.replace(/\W/g, ""),
  namespace: "dev",
  catalog_item_id: title.toLowerCase().replace(/\W+/g, "-"),
  title,
  cover_url: null,
  installable: true,
}));

type DevItem = {
  app_name: string;
  title: string;
  state: string;
  downloaded: number;
  total: number;
  bytes_per_sec: number;
  error: string | null;
  update?: boolean;
};
/** Kelp Runner (installed as 1.0) has a newer version in the preview. */
const DEV_LATEST = { KelpRunner: "1.1" };
const GB = 1024 ** 3;

/** A fake download that moves forward every 250 ms, so the Downloads page can be viewed in a browser. */
function devDownloads() {
  const items: DevItem[] = [
    { app_name: "CoralKart", title: "Coral Kart", state: "paused", downloaded: 1.2 * GB, total: 3.4 * GB, bytes_per_sec: 0, error: null },
  ];
  const installed = [
    { app_name: "KelpRunner", title: "Kelp Runner", version: "1.0", install_path: "C:\\Users\\you\\Games\\Kelp Runner", executable: "Kelp.exe", launch_parameters: "", install_size: 2 * GB, installed_at: new Date().toISOString() },
    // An add-on piece, like Fortnite's LEGO content (spec §6u): no card, shown under Extra content.
    { app_name: "KelpRunnerReef", title: "Kelp Runner Reef Pack", version: "1.0", install_path: "C:\\Users\\you\\Games\\Kelp Runner", executable: "", launch_parameters: "", install_size: 0.7 * GB, installed_at: new Date().toISOString(), parent: "KelpRunner" },
  ] as { app_name: string; title: string; parent?: string; options?: Record<string, unknown>; [k: string]: unknown }[];
  const send = () => emit("downloads-changed", { items: [...items].sort((a, b) => rank(a.state) - rank(b.state)) });
  const rank = (s: string) => (s === "downloading" ? 0 : s === "queued" ? 1 : 2);
  const pump = () => {
    if (items.some((i) => i.state === "downloading")) return;
    const next = items.find((i) => i.state === "queued");
    if (next) next.state = "downloading";
  };
  setInterval(() => {
    const d = items.find((i) => i.state === "downloading");
    if (!d) return;
    d.bytes_per_sec = 18.2 * 1024 * 1024 * (0.8 + Math.random() * 0.4);
    d.downloaded = Math.min(d.total, d.downloaded + d.bytes_per_sec * 0.25 * 3);
    if (d.downloaded >= d.total) {
      items.splice(items.indexOf(d), 1);
      const existing = installed.find((g) => g.app_name === d.app_name);
      if (existing) existing.version = DEV_LATEST[d.app_name as keyof typeof DEV_LATEST] ?? existing.version;
      else installed.push({ ...installed[0], app_name: d.app_name, title: d.title, install_path: "C:\\Users\\you\\Games\\" + d.title });
      emit("installed-changed", null);
      pump();
    }
    send();
  }, 250);
  const find = (args: unknown) => items.find((i) => i.app_name === (args as { appName?: string })?.appName);
  return {
    handle(cmd: string, args: unknown): unknown {
      switch (cmd) {
        case "downloads_state":
          return { items };
        case "update_start": {
          const name = (args as { appName: string }).appName;
          const game = installed.find((g) => g.app_name === name);
          if (game && !items.some((i) => i.app_name === name)) {
            items.push({ app_name: name, title: game.title, state: "queued", downloaded: 0, total: 0.3 * GB, bytes_per_sec: 0, error: null, update: true });
            pump();
            send();
          }
          return { items };
        }
        case "installed_list":
          return installed;
        // As if Epic's launcher also lists the pack (the usual case: pieces come from Epic).
        case "pieces_in_epic":
          return installed.filter((g) => g.parent === (args as { appName: string }).appName).map((g) => g.app_name);
        case "piece_remove": {
          const name = (args as { appName: string }).appName;
          installed.splice(installed.findIndex((g) => g.app_name === name), 1);
          return installed;
        }
        case "install_options": {
          const game = (args as { game: { title: string } }).game;
          const parent = (args as { parent?: string | null }).parent ?? "C:\\Users\\you\\Games";
          const big = game.title.startsWith("The Long");
          return new Promise((r) =>
            setTimeout(() => r({ install_path: `${parent}\\${game.title}`, download_size: 3.1 * GB, install_size: big ? 900 * GB : 7.4 * GB, free_space: 695 * GB, enough_space: !big, error: big ? "Not enough space on C:. Needs 900.0 GB, 695.0 GB free." : null }), 600),
          );
        }
        case "install_start": {
          const game = (args as { game: { app_name: string; title: string } }).game;
          items.push({ app_name: game.app_name, title: game.title, state: "queued", downloaded: 0, total: 3.1 * GB, bytes_per_sec: 0, error: null });
          pump();
          send();
          return { items };
        }
        case "pick_folder":
          return "D:\\Games";
        case "download_pause": {
          const i = find(args);
          if (i) i.state = "paused";
          send();
          return null;
        }
        case "download_resume": {
          const i = find(args);
          if (i) i.state = "queued";
          pump();
          send();
          return null;
        }
        case "download_cancel": {
          const i = find(args);
          if (i) items.splice(items.indexOf(i), 1);
          pump();
          send();
          return null;
        }
        case "uninstall_game": {
          const name = (args as { appName: string }).appName;
          installed.splice(0, installed.length, ...installed.filter((g) => g.app_name !== name));
          return installed;
        }
        case "play_game": {
          // Preview only: pretend the game started. `?play=fail` shows the offline error instead.
          const offline = (args as { offline?: boolean }).offline;
          if (!offline && new URLSearchParams(location.search).get("play") === "fail") {
            throw { message: "Couldn't reach Epic to sign the game in. Check your internet.", can_offline: true, signed_out: false };
          }
          // The game "runs" for 15 seconds (spec §6f.4).
          const app = String((args as { appName?: string }).appName);
          setTimeout(() => emit("running-changed", [app]), 500);
          setTimeout(() => emit("running-changed", []), 15000);
          return null;
        }
        case "running_list":
          return [];
        // Manage box (spec §6t), preview only.
        case "game_options_set": {
          const a = args as { appName: string; autoUpdate: boolean | null; priority: boolean; launchArgs: string; launchArgsOn: boolean };
          const g = installed.find((x) => x.app_name === a.appName);
          if (!g) throw "This game isn't installed.";
          if ([...a.launchArgs].length > 1000) throw "Keep it under 1000 characters.";
          const shortcuts = (g as { options?: { shortcuts: string[] } }).options?.shortcuts ?? [];
          Object.assign(g, { options: { auto_update: a.autoUpdate, priority: a.priority, launch_args: a.launchArgs, launch_args_on: a.launchArgsOn, shortcuts } });
          return g;
        }
        case "shortcut_create":
        case "shortcut_add_start_menu":
        case "shortcut_remove": {
          const name = (args as { appName: string }).appName;
          const g = installed.find((x) => x.app_name === name) as (typeof installed)[number] & { options?: Record<string, unknown> };
          const o = { auto_update: null, priority: false, launch_args: "", launch_args_on: false, shortcuts: [] as string[], ...(g.options ?? {}) };
          if (cmd === "shortcut_create") o.shortcuts = [`C:\\Users\\you\\Desktop\\${g.title}.lnk`];
          if (cmd === "shortcut_add_start_menu") o.shortcuts = [...o.shortcuts, `C:\\Users\\you\\Start Menu\\Fishy Client\\${g.title}.lnk`];
          if (cmd === "shortcut_remove") o.shortcuts = [];
          g.options = o;
          return g;
        }
        case "verify_start": {
          const name = (args as { appName: string }).appName;
          let done = 0;
          const total = 21000;
          const t = setInterval(() => {
            done = Math.min(total, done + 1400);
            emit("verify-progress", { app_name: name, done, total });
            if (done >= total) {
              clearInterval(t);
              emit("verify-done", { app_name: name, result: { kind: "bad", files: 12, bytes: 340 * 1000 * 1000 } });
            }
          }, 250);
          return null;
        }
        case "verify_stop":
          return null;
        case "verify_repair":
          return { items };
        case "download_start_now": {
          const i = find(args);
          const mode = (args as { mode: string }).mode;
          const now = items.filter((x) => x.state === "downloading");
          if (i && mode === "pause") now.forEach((x) => (x.state = "queued"));
          if (i) i.state = "downloading";
          send();
          return { items };
        }
        case "play_request_take":
          return null;
      }
      return undefined;
    },
  };
}

let devSettings = { auto_update: false, warn_outdated_play: true, install_folder: null as string | null, speed_limit_mbps: null as number | null, reduce_motion: false, library_sort: "az", when_game_starts: "keep" };
let devAutostart = false;

export function installDevMock(start: string) {
  const dl = devDownloads();
  const fail = new URLSearchParams(location.search).get("fail");
  let failedOnce = false;
  let state: AuthState =
    start === "signedin" ? SIGNED_IN : start === "offline" ? { kind: "Offline", display_name: "FishFan" } : { kind: "SignedOut" };

  mockIPC(
    (cmd, args) => {
      switch (cmd) {
        case "auth_state":
        case "retry_auth":
          return state;
        case "start_login":
          // pretend the user signs in after a few seconds
          setTimeout(() => {
            state = SIGNED_IN;
            emit("auth-changed", state);
          }, 3000);
          return null;
        case "submit_code": {
          const input = String((args as { input?: string })?.input ?? "").trim();
          if (!/^[0-9a-f]{32}$/i.test(input)) throw "That doesn't look like a sign-in code. Paste the authorizationCode value.";
          return (state = SIGNED_IN);
        }
        case "logout":
          return (state = { kind: "SignedOut" });
        case "open_login_in_browser":
          return null;
        case "library_cached":
          return null;
        case "store_free_games":
          return {
            now: [{ title: "Mark of the Deep", url: "https://store.epicgames.com/p/mark-of-the-deep-f7dc08", image: null, date: "2026-11-26T16:00:00Z", owned: false }],
            next: [
              { title: "Bread and Fred", url: "https://store.epicgames.com/p/bread-and-fred-1db362", image: null, date: "2026-11-26T16:00:00Z", owned: false },
              { title: "Firestone Idle RPG", url: "https://store.epicgames.com/p/firestone-online-idle-rpg-bfd04b", image: null, date: "2026-11-26T16:00:00Z", owned: true },
            ],
          };
        case "store_open":
          return null;
        case "library_refresh":
          // ?fail=library: the first refresh fails; ?fail=expired: the sign-in has expired.
          if (fail === "library" && !failedOnce) {
            failedOnce = true;
            return new Promise((_, no) => setTimeout(() => no("Couldn't load your library from Epic. Check your internet, then press Try again."), 800));
          }
          if (fail === "expired") {
            state = { kind: "SignedOut" };
            const msg = "Your sign-in has expired. Please sign in again.";
            return emit("login-failed", msg)
              .then(() => emit("auth-changed", state))
              .then(() => Promise.reject(msg));
          }
          return new Promise((r) =>
            setTimeout(() => r({ updated_at: new Date().toISOString(), games: DEV_GAMES, latest_versions: DEV_LATEST }), 1500),
          );
        case "settings_get":
          return devSettings;
        case "settings_set":
          return (devSettings = { ...devSettings, ...(args as { value: object }).value });
        case "default_install_folder":
          return "C:\\Users\\you\\Games";
        case "autostart_get":
          return devAutostart;
        case "autostart_set":
          return (devAutostart = (args as { on: boolean }).on);
        case "open_logs_folder":
          return null;
        case "delete_all_data":
          return new Promise((r) => setTimeout(() => r({ failed: 0, message: null }), 800)).then((v) => {
            state = { kind: "SignedOut" };
            return v;
          });
        default:
          return dl.handle(cmd, args);
      }
    },
    { shouldMockEvents: true },
  );
}
