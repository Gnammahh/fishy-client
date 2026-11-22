# Making a release

How to publish a new version of Fishy Client so installed copies update themselves (spec §6j). Run the commands in PowerShell from the repository folder (`C:\Users\<you>\Desktop\fishy-client`).

## Once: the signing key

Every installer is signed, and Fishy Client only installs updates signed with this key. Create the folder `.fishy-client-keys` in your user folder first.

```powershell
npm.cmd --prefix app run tauri signer generate -- --ci -w "$env:USERPROFILE\.fishy-client-keys\fishy-client.key"
```

- This makes `fishy-client.key` (private, never share it, never put it in the repository) and `fishy-client.key.pub` (public; its text goes into `app/src-tauri/tauri.conf.json` → `plugins.updater.pubkey`).
- **Back up the whole `.fishy-client-keys` folder** (a USB stick or a password manager). If the key is lost, installed copies can't be updated automatically any more: everyone would have to install the next version by hand once.

## Every release

1. **Version and notes.**
   - Set the new version (for example `1.2.0`) in `app/package.json` (and the two `"version"` lines of `app/package-lock.json` for `fishy-client`), `app/src-tauri/tauri.conf.json`, `app/src-tauri/Cargo.toml`, and the version check in the bundle test in `app/src-tauri/src/lib.rs`.
   - Add the new version's notes at the **top** of `app/src/lib/notes/release-notes.json`. A test fails if they're missing.
   - Run the tests: `cargo test --workspace`, then `npm.cmd --prefix app test`.
2. **Build, signed.** The key file's location is handed to the build; its contents are never shown. Signing is switched on only here, by `tauri.release.conf.json`, so a plain `npm.cmd run tauri build` still works for anyone without the key (unsigned, no updates). The key has no password, and the build needs that password variable to exist but be empty, which PowerShell can't do (setting it to `""` deletes it), so this step runs in **Git Bash** (or ask Claude to run it):

   ```bash
   TAURI_SIGNING_PRIVATE_KEY="$USERPROFILE/.fishy-client-keys/fishy-client.key" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" npm.cmd --prefix app run tauri build -- --config src-tauri/tauri.release.conf.json
   ```

   This makes `target\release\bundle\msi\Fishy Client_<version>_x64_en-US.msi` and its `.msi.sig`.
3. **The update file.**

   ```powershell
   node scripts/make-latest-json.mjs
   ```

   This writes `target\release\bundle\msi\latest.json`. It refuses if the newest notes aren't for this version, if the signature is missing, or if the public key in `tauri.conf.json` doesn't match your key file (a release with the wrong key could never update).
4. **Publish** (the repository must be public, so installed copies can read it):

   ```powershell
   gh release create v<version> "target\release\bundle\msi\Fishy Client_<version>_x64_en-US.msi" "target\release\bundle\msi\latest.json" --title "Fishy Client <version>" --notes-file release-notes.md --draft
   gh release edit v<version> --draft=false
   ```

   `release-notes.md` is a short text for the Release page (what's new, known limits, how to install). The release is made as a **draft** first and published once both files are uploaded, so no copy ever reads a `latest.json` whose installer isn't there yet.

   GitHub renames the installer to `Fishy.Client_<version>_x64_en-US.msi` (spaces become dots); `latest.json` already points at that name.
5. **Check:** on a PC with the previous version, start Fishy Client: the bar "Fishy Client <version> is available." appears; Update now installs it and restarts. Windows asks for administrator rights once (the app is installed for all users); if you say no, Fishy Client stays closed: start it again from the Start menu.
