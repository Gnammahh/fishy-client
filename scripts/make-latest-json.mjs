// Writes latest.json, the file installed copies of Fishy Client read to find a new version
// (spec §6j, Tauri's static update format). Run from the repository root after a signed build:
//   node scripts/make-latest-json.mjs
// It reads the version from app/package.json, the notes from app/src/lib/notes/release-notes.json
// and the signature from the .msi.sig next to the built installer, and writes latest.json beside it.
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import { homedir } from "node:os";
import { join } from "node:path";

const REPO = "https://github.com/Gnammahh/fishy-client";

/** The name GitHub gives an uploaded file: spaces become dots. */
export function releaseAssetName(fileName) {
  return fileName.replace(/ /g, ".");
}

/**
 * The app must carry the public half of the key the installer was signed with, or the release
 * could never update (spec §6j). `keyPub` is the .pub file's text, or null if it's missing.
 */
export function checkPubkey(confPubkey, keyPub) {
  if (keyPub == null) throw new Error("No key file: fishy-client.key.pub in the .fishy-client-keys folder of your user folder (see docs/RELEASING.md).");
  if (!confPubkey || confPubkey === "REPLACE_WITH_PUBLIC_KEY" || confPubkey.trim() !== keyPub.trim()) {
    throw new Error("The public key in tauri.conf.json doesn't match your key file. Fix it before releasing, or no one could update.");
  }
}

/** The update file's contents. Refuses notes for another version or an empty signature. */
export function makeLatestJson({ version, note, signature, url, pubDate }) {
  if (!note || note.version !== version) {
    throw new Error(`The newest notes are for ${note?.version}, but the app is ${version}. Add this version's notes first.`);
  }
  if (!signature || !signature.trim()) {
    throw new Error("The installer's signature is empty. Build with the signing key set (see docs/RELEASING.md).");
  }
  return {
    version,
    notes: note.items.map((i) => `- ${i}`).join("\n"),
    pub_date: pubDate,
    platforms: { "windows-x86_64": { signature: signature.trim(), url } },
  };
}

function main() {
  const root = fileURLToPath(new URL("..", import.meta.url));
  const version = JSON.parse(readFileSync(join(root, "app", "package.json"), "utf8")).version;
  const conf = JSON.parse(readFileSync(join(root, "app", "src-tauri", "tauri.conf.json"), "utf8"));
  const pubPath = join(homedir(), ".fishy-client-keys", "fishy-client.key.pub");
  checkPubkey(conf.plugins?.updater?.pubkey, existsSync(pubPath) ? readFileSync(pubPath, "utf8") : null);
  const note = JSON.parse(readFileSync(join(root, "app", "src", "lib", "notes", "release-notes.json"), "utf8"))[0];
  const msiName = `Fishy Client_${version}_x64_en-US.msi`;
  const dir = join(root, "target", "release", "bundle", "msi");
  const sigPath = join(dir, `${msiName}.sig`);
  if (!existsSync(sigPath)) throw new Error(`No ${msiName}.sig. Build with the signing key set first (see docs/RELEASING.md).`);
  // GitHub turns spaces in uploaded file names into dots, so the download is "Fishy.Client_…".
  const url = `${REPO}/releases/download/v${version}/${releaseAssetName(msiName)}`;
  const json = makeLatestJson({ version, note, signature: readFileSync(sigPath, "utf8"), url, pubDate: new Date().toISOString() });
  const out = join(dir, "latest.json");
  writeFileSync(out, JSON.stringify(json, null, 2) + "\n");
  console.log(`Wrote latest.json for ${version}.`);
}

// Run only when started as a command (Windows paths compared without caring about case).
if (process.argv[1] && pathToFileURL(process.argv[1]).href.toLowerCase() === import.meta.url.toLowerCase()) {
  main();
}
