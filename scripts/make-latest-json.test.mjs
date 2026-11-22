// @vitest-environment node
import { describe, it, expect } from "vitest";
import { makeLatestJson, releaseAssetName, checkPubkey } from "./make-latest-json.mjs";

const base = {
  version: "1.2.0",
  note: { version: "1.2.0", date: "2026-12-01", items: ["Faster fish.", "Fewer bugs."] },
  signature: "dW50cnVzdGVkIGNvbW1lbnQ=",
  url: "https://github.com/Gnammahh/fishy-client/releases/download/v1.2.0/Fishy%20Client_1.2.0_x64_en-US.msi",
  pubDate: "2026-12-01T12:00:00Z",
};

describe("make-latest-json", () => {
  it("writes_tauri_static_json", () => {
    expect(makeLatestJson(base)).toEqual({
      version: "1.2.0",
      notes: "- Faster fish.\n- Fewer bugs.",
      pub_date: "2026-12-01T12:00:00Z",
      platforms: { "windows-x86_64": { signature: "dW50cnVzdGVkIGNvbW1lbnQ=", url: base.url } },
    });
  });

  it("refuses_mismatched_version", () => {
    expect(() => makeLatestJson({ ...base, note: { ...base.note, version: "1.1.0" } })).toThrow(/notes/);
  });

  it("uses_the_name_github_gives_the_upload", () => {
    expect(releaseAssetName("Fishy Client_1.2.0_x64_en-US.msi")).toBe("Fishy.Client_1.2.0_x64_en-US.msi");
  });

  it("refuses_a_placeholder_or_wrong_public_key", () => {
    expect(() => checkPubkey("REPLACE_WITH_PUBLIC_KEY", "abc")).toThrow(/public key/);
    expect(() => checkPubkey("abc", "xyz\n")).toThrow(/public key/);
    expect(() => checkPubkey("abc", null)).toThrow(/key file/);
    expect(() => checkPubkey("abc", "abc\n")).not.toThrow();
  });

  it("refuses_an_empty_signature", () => {
    expect(() => makeLatestJson({ ...base, signature: "  " })).toThrow(/signature/);
  });
});
