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

/// Deletes the recorded shortcuts that are still `.lnk` files; returns how many.
pub fn remove_recorded(paths: &[PathBuf]) -> usize {
    paths
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("lnk")))
        .filter(|p| p.symlink_metadata().is_ok_and(|m| m.is_file()))
        .filter(|p| std::fs::remove_file(p).is_ok())
        .count()
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
    let args = if app_name.contains(' ') { format!("--play \"{app_name}\"") } else { format!("--play {app_name}") };
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
    remove_recorded(&rec.options.shortcuts);
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
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
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
    fn remove_deletes_only_recorded_lnk_files() {
        let dir = tempfile::tempdir().unwrap();
        let lnk = dir.path().join("Tide.lnk");
        let txt = dir.path().join("Tide.txt");
        let folder = dir.path().join("Folder.lnk");
        let other = dir.path().join("Other.lnk");
        std::fs::write(&lnk, b"x").unwrap();
        std::fs::write(&txt, b"x").unwrap();
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(&other, b"x").unwrap();
        let gone = dir.path().join("Gone.lnk");
        assert_eq!(remove_recorded(&[lnk.clone(), txt.clone(), folder.clone(), gone]), 1);
        assert!(!lnk.exists() && txt.exists() && folder.exists() && other.exists());
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
