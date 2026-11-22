// Writes latest.json, the file installed copies of Fishy Client read to find a new version
// (spec §6j, Tauri's static update format). Run from the repository root after a signed build:
//   node scripts/make-latest-json.mjs
// It reads the version from app/package.json, the notes from app/src/lib/notes/release-notes.json
// and the signature from the .msi.sig next to the built installer, and writes latest.json beside it.
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

const REPO = "https://github.com/Gnammahh/fishy-client";

/** The name GitHub gives an uploaded file: spaces become dots. */
export function releaseAssetName(fileName) {
  return fileName.replace(/ /g, ".");
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

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  main();
}
