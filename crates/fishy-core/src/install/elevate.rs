//! Installing into folders that need administrator rights (spec §6k), the way Epic's launcher
//! does it: a second copy of Fishy Client, started as administrator with only
//! `--make-game-folder <path>`, creates that one game folder and gives the built-in Users group
//! full control of it. Everything else keeps running without admin rights.
use std::path::{Component, Path, PathBuf, Prefix};

/// The only thing the admin copy of Fishy Client accepts.
pub const ARG: &str = "--make-game-folder";

/// Why a folder isn't one the admin copy may create.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    NotAbsolute,
    /// `..`, `\\?\`, network paths and similar.
    Unsafe,
    DriveRoot,
    NoParent,
    NotEmpty,
    /// Inside Windows' own folder.
    SystemFolder,
}

fn norm(s: &str) -> String {
    s.replace('/', "\\").to_lowercase()
}

/// The folder the admin copy may create, tidied (`/` → `\`, no trailing `\`), or why not.
pub fn check_new_folder(path: &Path, windows_dir: &Path) -> Result<PathBuf, Refused> {
    let raw = path.as_os_str().to_string_lossy().replace('/', "\\");
    if raw.starts_with("\\\\") {
        return Err(Refused::Unsafe);
    }
    let trimmed = raw.trim_end_matches('\\');
    if trimmed.len() == 2 && trimmed.ends_with(':') && raw.len() > 2 {
        return Err(Refused::DriveRoot); // "C:\" itself
    }
    let tidy = PathBuf::from(trimmed);
    let mut parts = tidy.components();
    match parts.next() {
        Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_)) => {}
        Some(Component::Prefix(_)) => return Err(Refused::Unsafe),
        _ => return Err(Refused::NotAbsolute),
    }
    if parts.next() != Some(Component::RootDir) {
        return Err(Refused::NotAbsolute);
    }
    let rest: Vec<Component> = parts.collect();
    if rest.iter().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(Refused::Unsafe);
    }
    if rest.is_empty() {
        return Err(Refused::DriveRoot);
    }
    let win = norm(&windows_dir.display().to_string());
    let win = win.trim_end_matches('\\');
    let here = norm(&tidy.display().to_string());
    if here == win || here.starts_with(&format!("{win}\\")) {
        return Err(Refused::SystemFolder);
    }
    if !tidy.parent().is_some_and(Path::is_dir) {
        return Err(Refused::NoParent);
    }
    if tidy.exists() {
        let empty = tidy.is_dir() && std::fs::read_dir(&tidy).map(|mut d| d.next().is_none()).unwrap_or(false);
        if !empty {
            return Err(Refused::NotEmpty);
        }
    }
    Ok(tidy)
}

/// The admin copy was started (even with wrong arguments, which it then refuses).
pub fn is_helper_call(args: &[String]) -> bool {
    args.first().is_some_and(|a| a == ARG)
}

/// The folder from `--make-game-folder <path>`; anything else is None.
pub fn helper_path(args: &[String]) -> Option<PathBuf> {
    match args {
        [flag, path] if flag == ARG => Some(PathBuf::from(path)),
        _ => None,
    }
}

/// Full control for the built-in Users group on this folder and what's created in it, as Epic does.
pub fn icacls_args(path: &Path) -> Vec<String> {
    vec![path.display().to_string(), "/grant".to_string(), "*S-1-5-32-545:(OI)(CI)F".to_string()]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked {
    Created,
    /// No in Windows' permission prompt.
    Declined,
    Failed,
}

/// The admin copy's exit code (or Windows' "cancelled", 1223) as an outcome.
pub fn asked_from(code: u32) -> Asked {
    match code {
        0 => Asked::Created,
        1223 => Asked::Declined,
        _ => Asked::Failed,
    }
}

fn windows_dir() -> PathBuf {
    std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
}

/// The admin copy's whole job. Exit codes: 0 done, 2 refused path, 3 couldn't create or set
/// permissions (a folder it created is removed again).
pub fn run_helper(args: &[String]) -> i32 {
    let Some(path) = helper_path(args) else { return 2 };
    let Ok(path) = check_new_folder(&path, &windows_dir()) else { return 2 };
    let created = !path.exists();
    if created && std::fs::create_dir(&path).is_err() {
        return 3;
    }
    let icacls = windows_dir().join("System32").join("icacls.exe");
    let ok = std::process::Command::new(icacls).args(icacls_args(&path)).status().is_ok_and(|s| s.success());
    if !ok {
        if created {
            let _ = std::fs::remove_dir(&path);
        }
        return 3;
    }
    0
}

/// Starts the admin copy (Windows shows its permission prompt) and waits for it.
#[cfg(windows)]
pub fn ask_for_folder(path: &Path) -> Asked {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_CANCELLED};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let wide = |s: &std::ffi::OsStr| s.encode_wide().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let Ok(exe) = std::env::current_exe() else { return Asked::Failed };
    let exe = wide(exe.as_os_str());
    let verb = wide(std::ffi::OsStr::new("runas"));
    let params = wide(std::ffi::OsStr::new(&format!("{ARG} \"{}\"", path.display())));
    // SAFETY: every pointer is to a live, NUL-terminated buffer for the duration of the call, and
    // the process handle is closed once its exit code is read.
    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpVerb = verb.as_ptr();
        info.lpFile = exe.as_ptr();
        info.lpParameters = params.as_ptr();
        info.nShow = SW_HIDE;
        if ShellExecuteExW(&mut info) == 0 {
            return if GetLastError() == ERROR_CANCELLED { Asked::Declined } else { Asked::Failed };
        }
        if info.hProcess.is_null() {
            return Asked::Failed;
        }
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 1u32;
        let got = GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
        if got == 0 {
            return Asked::Failed;
        }
        asked_from(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win() -> PathBuf {
        PathBuf::from(r"C:\Windows")
    }

    #[test]
    fn good_path_ok() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("Tide");
        assert_eq!(check_new_folder(&p, &win()), Ok(p.clone()));
    }

    #[test]
    fn relative_refused() {
        assert_eq!(check_new_folder(Path::new(r"Games\Tide"), &win()), Err(Refused::NotAbsolute));
    }

    #[test]
    fn dotdot_refused() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(check_new_folder(&dir.path().join("a").join("..").join("Tide"), &win()), Err(Refused::Unsafe));
    }

    #[test]
    fn drive_root_refused() {
        assert_eq!(check_new_folder(Path::new(r"C:\"), &win()), Err(Refused::DriveRoot));
    }

    #[test]
    fn verbatim_and_network_refused() {
        assert_eq!(check_new_folder(Path::new(r"\\?\C:\Games\Tide"), &win()), Err(Refused::Unsafe));
        assert_eq!(check_new_folder(Path::new(r"\\server\share\Tide"), &win()), Err(Refused::Unsafe));
    }

    #[test]
    fn missing_parent_refused() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(check_new_folder(&dir.path().join("Nope").join("Tide"), &win()), Err(Refused::NoParent));
    }

    #[test]
    fn existing_non_empty_refused() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("Tide");
        std::fs::create_dir(&p).unwrap();
        std::fs::write(p.join("game.exe"), b"x").unwrap();
        assert_eq!(check_new_folder(&p, &win()), Err(Refused::NotEmpty));
    }

    #[test]
    fn existing_empty_ok() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("Tide");
        std::fs::create_dir(&p).unwrap();
        assert_eq!(check_new_folder(&p, &win()), Ok(p.clone()));
    }

    #[test]
    fn inside_windows_refused() {
        let dir = tempfile::tempdir().unwrap();
        let fake_windows = dir.path().join("Windows");
        std::fs::create_dir(&fake_windows).unwrap();
        assert_eq!(check_new_folder(&fake_windows.join("Tide"), &fake_windows), Err(Refused::SystemFolder));
        let upper = PathBuf::from(fake_windows.display().to_string().to_uppercase()).join("Tide");
        assert_eq!(check_new_folder(&upper, &fake_windows), Err(Refused::SystemFolder));
    }

    #[test]
    fn slashes_and_trailing_backslash_ok() {
        let dir = tempfile::tempdir().unwrap();
        let messy = format!("{}/Tide\\", dir.path().display().to_string().replace('\\', "/"));
        assert_eq!(check_new_folder(Path::new(&messy), &win()), Ok(dir.path().join("Tide")));
    }

    #[test]
    fn helper_args_exactly_one_path() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(helper_path(&a(&[ARG, r"C:\Games\Tide"])), Some(PathBuf::from(r"C:\Games\Tide")));
        assert_eq!(helper_path(&a(&[ARG])), None);
        assert_eq!(helper_path(&a(&[ARG, "a", "b"])), None);
        assert_eq!(helper_path(&a(&["--other", "a"])), None);
        assert_eq!(helper_path(&a(&[])), None);
        assert!(is_helper_call(&a(&[ARG])), "a lone flag is still a helper call (and refused)");
        assert!(!is_helper_call(&a(&[])));
    }

    #[test]
    fn icacls_args_grant_users_full_control() {
        assert_eq!(icacls_args(Path::new(r"C:\Program Files\Epic Games\Tide")), vec![
            r"C:\Program Files\Epic Games\Tide".to_string(),
            "/grant".to_string(),
            "*S-1-5-32-545:(OI)(CI)F".to_string(),
        ]);
    }

    #[test]
    fn exit_codes_map() {
        assert_eq!(asked_from(0), Asked::Created);
        assert_eq!(asked_from(1223), Asked::Declined);
        assert_eq!(asked_from(2), Asked::Failed);
        assert_eq!(asked_from(3), Asked::Failed);
    }

    #[test]
    fn helper_refuses_bad_path_with_2_and_creates_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("Nope").join("Tide");
        assert_eq!(run_helper(&[ARG.to_string(), target.display().to_string()]), 2);
        assert!(!dir.path().join("Nope").exists());
        assert_eq!(run_helper(&[ARG.to_string()]), 2);
    }
}
