//! Desktop and Start menu shortcuts that start a game through Fishy Client (spec §6t).
use crate::AppInstall;
use fishy_core::install::store::{file_path, InstalledGame};
use fishy_core::install::InstallError;
use std::path::{Path, PathBuf};

const SHORTCUT_ERROR: &str = "Couldn't make the shortcut. Please try again.";

/// Fishy Client's Windows app ID: the bundle identifier, which the installer's Start menu
/// shortcut also carries.
pub const APP_ID: &str = "com.fishyclient.app";

/// Tells Windows which app this is before any window opens. Without it, Fishy started from a
/// game's shortcut shows that game's icon on the taskbar (spec §6t).
pub fn claim_app_id() {
    if !win::set_app_id(APP_ID) {
        tracing::warn!("couldn't set the Windows app ID");
    }
}

/// The shortcut's file name: the title without characters Windows forbids.
pub fn shortcut_name(title: &str, app_name: &str) -> String {
    let clean: String = title.chars().filter(|c| !matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') && !c.is_control()).collect();
    let clean = clean.trim().trim_end_matches('.').trim();
    format!("{}.lnk", if clean.is_empty() { app_name } else { clean })
}

/// A file with this name exists and Fishy Client didn't make it.
pub fn taken_by_other(path: &Path, recorded: &[PathBuf]) -> bool {
    path.symlink_metadata().is_ok() && !recorded.iter().any(|r| r == path)
}

/// The arguments a game's shortcut carries.
fn play_args(app_name: &str) -> String {
    if app_name.contains(' ') {
        format!("--play \"{app_name}\"")
    } else {
        format!("--play {app_name}")
    }
}

/// A shortcut that starts `fishy` with `--play <app_name>`, nothing else: one Fishy Client made.
pub fn link_is_ours(target: &Path, args: &str, fishy: &Path, app_name: &str) -> bool {
    let same_exe = target.as_os_str().to_string_lossy().to_lowercase() == fishy.as_os_str().to_string_lossy().to_lowercase();
    same_exe && args.trim() == play_args(app_name)
}

/// Deletes the recorded shortcuts that are still `.lnk` files starting `fishy` for this game;
/// returns how many. Anything else at a recorded path (the user replaced it) is left alone.
pub fn remove_links(paths: &[PathBuf], fishy: &Path, app_name: &str) -> usize {
    paths
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("lnk")))
        .filter(|p| p.symlink_metadata().is_ok_and(|m| m.is_file()))
        .filter(|p| win::read_lnk(p).is_some_and(|(target, args)| link_is_ours(&target, &args, fishy, app_name)))
        .filter(|p| std::fs::remove_file(p).is_ok())
        .count()
}

/// `remove_links` for this running Fishy Client.
pub fn remove_recorded(paths: &[PathBuf], app_name: &str) -> usize {
    let Ok(fishy) = std::env::current_exe() else { return 0 };
    remove_links(paths, &fishy, app_name)
}

/// Delete all my data: every installed game's shortcuts go too. Returns how many.
pub fn remove_all(games: &[InstalledGame]) -> usize {
    games.iter().map(|g| remove_recorded(&g.options.shortcuts, &g.app_name)).sum()
}

/// Where the shortcut goes.
#[derive(Clone, Copy)]
enum Place {
    Desktop,
    StartMenu,
}

fn installed_record(install: &AppInstall, app_name: &str) -> Result<InstalledGame, String> {
    install.downloads.store.installed().into_iter().find(|g| g.app_name == app_name).ok_or_else(|| InstallError::NotInstalled.user_message())
}

fn make(install: &AppInstall, app_name: &str, place: Place) -> Result<InstalledGame, String> {
    let rec = installed_record(install, app_name)?;
    let folder = match place {
        Place::Desktop => win::known_folder(win::Folder::Desktop),
        Place::StartMenu => win::known_folder(win::Folder::Programs).map(|p| p.join("Fishy Client")),
    }
    .ok_or(SHORTCUT_ERROR)?;
    std::fs::create_dir_all(&folder).map_err(|_| SHORTCUT_ERROR)?;
    let path = folder.join(shortcut_name(&rec.title, app_name));
    let mut recorded = rec.options.shortcuts.clone();
    if taken_by_other(&path, &recorded) {
        let place = if matches!(place, Place::Desktop) { "on your desktop" } else { "in the Start menu" };
        return Err(format!("There's already a shortcut called {} {place}.", rec.title));
    }
    let fishy = std::env::current_exe().map_err(|_| SHORTCUT_ERROR)?;
    let exe = rec.executable.trim().trim_start_matches(['/', '\\']);
    let icon = file_path(&rec.install_path, exe).ok().filter(|p| p.is_file()).unwrap_or_else(|| fishy.clone());
    let args = play_args(app_name);
    let workdir = fishy.parent().map(Path::to_path_buf).unwrap_or_default();
    win::write_lnk(&path, &fishy, &args, &icon, &workdir).map_err(|_| SHORTCUT_ERROR)?;
    if !recorded.contains(&path) {
        recorded.push(path);
    }
    install.downloads.store.set_shortcuts(app_name, recorded).map_err(|_| SHORTCUT_ERROR)?;
    tracing::info!("shortcut created");
    installed_record(install, app_name)
}

/// Removes the game's shortcuts (Remove in the Manage box, and Uninstall).
pub fn remove_for(install: &AppInstall, app_name: &str) {
    let Ok(rec) = installed_record(install, app_name) else { return };
    if rec.options.shortcuts.is_empty() {
        return;
    }
    remove_recorded(&rec.options.shortcuts, app_name);
    let _ = install.downloads.store.set_shortcuts(app_name, Vec::new());
    tracing::info!("shortcut removed");
}

#[tauri::command]
pub async fn shortcut_create(app_name: String, install: tauri::State<'_, AppInstall>) -> Result<InstalledGame, String> {
    make(&install, &app_name, Place::Desktop)
}

#[tauri::command]
pub async fn shortcut_add_start_menu(app_name: String, install: tauri::State<'_, AppInstall>) -> Result<InstalledGame, String> {
    make(&install, &app_name, Place::StartMenu)
}

#[tauri::command]
pub async fn shortcut_remove(app_name: String, install: tauri::State<'_, AppInstall>) -> Result<InstalledGame, String> {
    remove_for(&install, &app_name);
    installed_record(&install, &app_name)
}

#[cfg(windows)]
mod win {
    use std::path::{Path, PathBuf};
    use windows::core::{Interface, HSTRING};
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, STGM_READ};
    use windows::Win32::UI::Shell::{
        FOLDERID_Desktop, FOLDERID_Programs, IShellLinkW, SHGetKnownFolderPath, SetCurrentProcessExplicitAppUserModelID, ShellLink,
        KNOWN_FOLDER_FLAG,
    };

    pub fn set_app_id(id: &str) -> bool {
        unsafe { SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(id)).is_ok() }
    }

    #[cfg(test)]
    pub fn app_id() -> Option<String> {
        unsafe {
            let p = windows::Win32::UI::Shell::GetCurrentProcessExplicitAppUserModelID().ok()?;
            let id = p.to_string().ok();
            CoTaskMemFree(Some(p.0 as *const _));
            id
        }
    }

    pub enum Folder {
        Desktop,
        Programs,
    }

    /// Asks Windows, so a Desktop moved to OneDrive is found.
    pub fn known_folder(which: Folder) -> Option<PathBuf> {
        let id = match which {
            Folder::Desktop => &FOLDERID_Desktop,
            Folder::Programs => &FOLDERID_Programs,
        };
        unsafe {
            let p = SHGetKnownFolderPath(id, KNOWN_FOLDER_FLAG(0), None).ok()?;
            let path = p.to_string().ok().map(PathBuf::from);
            CoTaskMemFree(Some(p.0 as *const _));
            path
        }
    }

    /// Writes a `.lnk` with Windows' own shortcut maker, on a thread of its own so COM is set up
    /// the way it needs.
    pub fn write_lnk(path: &Path, target: &Path, args: &str, icon: &Path, workdir: &Path) -> Result<(), String> {
        let (path, target, args, icon, workdir) = (path.to_path_buf(), target.to_path_buf(), args.to_string(), icon.to_path_buf(), workdir.to_path_buf());
        std::thread::spawn(move || unsafe {
            let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let result = (|| -> windows::core::Result<()> {
                let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
                link.SetPath(&HSTRING::from(target.as_os_str()))?;
                link.SetArguments(&HSTRING::from(args.as_str()))?;
                link.SetIconLocation(&HSTRING::from(icon.as_os_str()), 0)?;
                link.SetWorkingDirectory(&HSTRING::from(workdir.as_os_str()))?;
                link.cast::<IPersistFile>()?.Save(&HSTRING::from(path.as_os_str()), true)
            })();
            if init.is_ok() {
                CoUninitialize();
            }
            result.map_err(|e| e.message())
        })
        .join()
        .map_err(|_| "shortcut thread failed".to_string())?
    }

    /// Reads a `.lnk`'s target and arguments with Windows' own shortcut reader. None if it
    /// isn't a shortcut Windows can read.
    pub fn read_lnk(path: &Path) -> Option<(PathBuf, String)> {
        let path = path.to_path_buf();
        std::thread::spawn(move || unsafe {
            let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let result = (|| -> windows::core::Result<(PathBuf, String)> {
                let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
                link.cast::<IPersistFile>()?.Load(&HSTRING::from(path.as_os_str()), STGM_READ)?;
                let mut target = vec![0u16; 32768];
                link.GetPath(&mut target, std::ptr::null_mut(), 0)?;
                let mut args = vec![0u16; 32768];
                link.GetArguments(&mut args)?;
                let text = |b: &[u16]| String::from_utf16_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())]);
                Ok((PathBuf::from(text(&target)), text(&args)))
            })();
            if init.is_ok() {
                CoUninitialize();
            }
            result.ok()
        })
        .join()
        .ok()
        .flatten()
    }
}

#[cfg(not(windows))]
mod win {
    use std::path::{Path, PathBuf};
    pub enum Folder {
        Desktop,
        Programs,
    }
    pub fn known_folder(_: Folder) -> Option<PathBuf> {
        None
    }
    pub fn set_app_id(_: &str) -> bool {
        true
    }
    pub fn write_lnk(_: &Path, _: &Path, _: &str, _: &Path, _: &Path) -> Result<(), String> {
        Err("Windows only".into())
    }
    pub fn read_lnk(_: &Path) -> Option<(PathBuf, String)> {
        None
    }
}

/// A real shortcut to this test program for `app_name`, as Create makes it.
#[cfg(all(test, windows))]
pub(crate) fn make_test_link(path: &Path, app_name: &str) {
    let exe = std::env::current_exe().unwrap();
    win::write_lnk(path, &exe, &play_args(app_name), &exe, path.parent().unwrap()).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_name_cleans_titles() {
        assert_eq!(shortcut_name("Sol: Cesto?", "Sol"), "Sol Cesto.lnk");
        assert_eq!(shortcut_name(r#"A/B\C*D"E<F>G|H"#, "x"), "ABCDEFGH.lnk");
        assert_eq!(shortcut_name("  ", "Tide"), "Tide.lnk");
        assert_eq!(shortcut_name("Reef\u{7}Run ", "x"), "ReefRun.lnk");
        assert_eq!(shortcut_name("Rocket League®", "x"), "Rocket League®.lnk");
    }

    #[test]
    fn refuses_a_name_already_taken() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("Tide.lnk");
        assert!(!taken_by_other(&p, &[]), "missing");
        std::fs::write(&p, b"x").unwrap();
        assert!(taken_by_other(&p, &[]), "someone else's");
        assert!(!taken_by_other(&p, std::slice::from_ref(&p)), "ours");
    }

    #[test]
    fn only_a_link_to_fishy_playing_this_game_is_ours() {
        let fishy = Path::new(r"C:\Program Files\Fishy Client\Fishy Client.exe");
        assert!(link_is_ours(fishy, "--play Tide", fishy, "Tide"));
        assert!(link_is_ours(Path::new(r"c:\program files\fishy client\fishy client.exe"), "--play Tide", fishy, "Tide"), "Windows ignores case");
        assert!(link_is_ours(fishy, r#"--play "Sol Cesto""#, fishy, "Sol Cesto"));
        assert!(!link_is_ours(fishy, "--play Kelp", fishy, "Tide"), "another game's");
        assert!(!link_is_ours(fishy, "--play Tide --other", fishy, "Tide"));
        assert!(!link_is_ours(fishy, "", fishy, "Tide"));
        assert!(!link_is_ours(Path::new(r"C:\Windows\System32\cmd.exe"), "--play Tide", fishy, "Tide"), "not Fishy");
    }

    /// Writes real shortcuts with Windows' API.
    #[cfg(windows)]
    #[test]
    fn remove_deletes_only_recorded_links_to_fishy_for_this_game() {
        let dir = tempfile::tempdir().unwrap();
        let fishy = std::env::current_exe().unwrap();
        let elsewhere = dir.path().join("other.exe");
        std::fs::write(&elsewhere, b"x").unwrap();
        let ours = dir.path().join("Tide.lnk");
        let other_game = dir.path().join("Kelp.lnk");
        let other_target = dir.path().join("Elsewhere.lnk");
        win::write_lnk(&ours, &fishy, "--play Tide", &fishy, dir.path()).unwrap();
        win::write_lnk(&other_game, &fishy, "--play Kelp", &fishy, dir.path()).unwrap();
        win::write_lnk(&other_target, &elsewhere, "--play Tide", &fishy, dir.path()).unwrap();
        let not_a_link = dir.path().join("Fake.lnk");
        let txt = dir.path().join("Tide.txt");
        let folder = dir.path().join("Folder.lnk");
        std::fs::write(&not_a_link, b"x").unwrap();
        std::fs::write(&txt, b"x").unwrap();
        std::fs::create_dir(&folder).unwrap();
        let gone = dir.path().join("Gone.lnk");
        let recorded = [ours.clone(), other_game.clone(), other_target.clone(), not_a_link.clone(), txt.clone(), folder.clone(), gone];
        assert_eq!(remove_links(&recorded, &fishy, "Tide"), 1);
        assert!(!ours.exists());
        assert!(other_game.exists() && other_target.exists() && not_a_link.exists() && txt.exists() && folder.exists());
    }

    #[test]
    fn the_app_id_is_the_installers() {
        // The installer's Start menu shortcut carries the bundle identifier as its app ID.
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(conf["identifier"], APP_ID);
    }

    #[cfg(windows)]
    #[test]
    fn windows_keeps_the_app_id() {
        assert!(win::set_app_id(APP_ID));
        assert_eq!(win::app_id().as_deref(), Some(APP_ID));
    }

    /// Writes a real shortcut with Windows' API (run with `--ignored`).
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn write_lnk_makes_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("Tide.lnk");
        let exe = std::env::current_exe().unwrap();
        win::write_lnk(&p, &exe, "--play Tide", &exe, dir.path()).unwrap();
        assert!(p.is_file() && std::fs::metadata(&p).unwrap().len() > 0);
        assert!(win::known_folder(win::Folder::Desktop).is_some_and(|d| d.is_dir()));
    }
}
