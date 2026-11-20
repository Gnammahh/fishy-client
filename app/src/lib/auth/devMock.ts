// Development-only fake of the Rust side, so the UI can be viewed in a normal browser.
// Never imported in the Tauri app (guarded by `dev` and the absence of __TAURI_INTERNALS__).
import { mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { AuthState } from "./auth";

const SIGNED_IN: AuthState = { kind: "SignedIn", display_name: "FishFan", account_id: "0123456789abcdef0123456789abcdef" };

export function installDevMock(start: string) {
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
      }
    },
    { shouldMockEvents: true },
  );
}
