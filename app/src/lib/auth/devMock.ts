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

type DevItem = { app_name: string; title: string; state: string; downloaded: number; total: number; bytes_per_sec: number; error: string | null };
const GB = 1024 ** 3;

/** A fake download that moves forward every 250 ms, so the Downloads page can be viewed in a browser. */
function devDownloads() {
  const items: DevItem[] = [
    { app_name: "CoralKart", title: "Coral Kart", state: "paused", downloaded: 1.2 * GB, total: 3.4 * GB, bytes_per_sec: 0, error: null },
  ];
  const installed = [
    { app_name: "KelpRunner", title: "Kelp Runner", version: "1.0", install_path: "C:\\Users\\you\\Games\\Kelp Runner", executable: "Kelp.exe", launch_parameters: "", install_size: 2 * GB, installed_at: new Date().toISOString() },
  ];
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
      installed.push({ ...installed[0], app_name: d.app_name, title: d.title, install_path: "C:\\Users\\you\\Games\\" + d.title });
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
        case "installed_list":
          return installed;
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
      }
      return undefined;
    },
  };
}

export function installDevMock(start: string) {
  const dl = devDownloads();
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
        case "library_refresh":
          return new Promise((r) => setTimeout(() => r({ updated_at: new Date().toISOString(), games: DEV_GAMES }), 1500));
        default:
          return dl.handle(cmd, args);
      }
    },
    { shouldMockEvents: true },
  );
}
