//! Installing into folders that need administrator rights (spec §6k), the way Epic's launcher
//! does it: a second copy of Fishy Client, started as administrator with only
//! `--make-game-folder <path>`, creates that one brand-new game folder with full control for the
//! built-in Users group. Everything else keeps running without admin rights.
//!
//! The admin copy trusts nothing it's given (review 2026-11-22): it refuses links (junctions)
//! anywhere on the way, odd names Windows would rewrite, non-local drives and anything inside
//! Windows; it finds Windows' folder through Windows, not an environment variable; it creates the
//! folder and its permission in one step (no icacls, nothing that can be swapped in between); and
//! it checks where the folder really ended up, removing it if that isn't the expected place.
use std::path::{Component, Path, PathBuf, Prefix};

/// The only thing the admin copy of Fishy Client accepts.
pub const ARG: &str = "--make-game-folder";

/// The new folder's permission, in Windows' SDDL: full control (`FA`) for built-in Users (`BU`),
/// passed down to everything created in it (`OICI`). The parent's inherited entries stay.
pub const FOLDER_SDDL: &str = "D:(A;OICI;FA;;;BU)";

/// Why a folder isn't one the admin copy may create.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    NotAbsolute,
    /// `..`, `\\?\`, network paths, links, odd names and similar.
    Unsafe,
    DriveRoot,
    NoParent,
    /// It already exists (the admin copy only makes brand-new folders).
    NotEmpty,
    /// Inside Windows' own folder.
    SystemFolder,
}

fn norm(s: &str) -> String {
    s.replace('/', "\\").trim_start_matches(r"\\?\").trim_end_matches('\\').to_lowercase()
}

fn inside(path: &str, dir: &str) -> bool {
    path == dir || path.starts_with(&format!("{dir}\\"))
}

/// A link (junction or symbolic link) instead of a plain folder.
fn is_link(p: &Path) -> bool {
    match std::fs::symlink_metadata(p) {
        Ok(m) if m.file_type().is_symlink() => true,
        #[cfg(windows)]
        Ok(m) => {
            use std::os::windows::fs::MetadataExt;
            const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
            m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        }
        #[cfg(not(windows))]
        Ok(_) => false,
        Err(_) => false,
    }
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
    if rest.is_empty() {
        return Err(Refused::DriveRoot);
    }
    // Names Windows would quietly rewrite or read as a stream: "Windows.", "Windows ", "a:b".
    let odd = |c: &Component| match c {
        Component::Normal(s) => {
            let s = s.to_string_lossy();
            s.contains(':') || s.ends_with('.') || s.ends_with(' ')
        }
        _ => true,
    };
    if rest.iter().any(odd) {
        return Err(Refused::Unsafe);
    }
    let win = norm(&windows_dir.display().to_string());
    if inside(&norm(&tidy.display().to_string()), &win) {
        return Err(Refused::SystemFolder);
    }
    if tidy.exists() || is_link(&tidy) {
        return Err(Refused::NotEmpty);
    }
    let Some(parent) = tidy.parent().filter(|p| p.is_dir()) else { return Err(Refused::NoParent) };
    // No link anywhere on the way: a junction could point the folder somewhere protected.
    if parent.ancestors().any(is_link) {
        return Err(Refused::Unsafe);
    }
    // Where the parent really is, in case something still redirects it.
    let real = std::fs::canonicalize(parent).map_err(|_| Refused::NoParent)?;
    if inside(&norm(&real.display().to_string()), &win) {
        return Err(Refused::SystemFolder);
    }
    if !local_drive(&tidy) {
        return Err(Refused::Unsafe);
    }
    Ok(tidy)
}

/// A fixed local drive: network and `subst` drives aren't visible to the admin copy.
#[cfg(windows)]
fn local_drive(path: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
    const DRIVE_FIXED: u32 = 3;
    let root: Vec<u16> = match path.components().next() {
        Some(Component::Prefix(p)) => {
            let mut v: Vec<u16> = p.as_os_str().encode_wide().collect();
            v.extend([u16::from(b'\\'), 0]);
            v
        }
        _ => return false,
    };
    // SAFETY: `root` is a NUL-terminated wide string that lives for the call.
    unsafe { GetDriveTypeW(root.as_ptr()) == DRIVE_FIXED }
}

#[cfg(not(windows))]
fn local_drive(_path: &Path) -> bool {
    false
}

/// The admin copy was started (even with wrong arguments, which it then refuses).
pub fn is_helper_call(args: &[std::ffi::OsString]) -> bool {
    args.first().is_some_and(|a| a == ARG)
}

/// The folder from `--make-game-folder <path>`; anything else is None.
pub fn helper_path(args: &[std::ffi::OsString]) -> Option<PathBuf> {
    match args {
        [flag, path] if flag == ARG => Some(PathBuf::from(path)),
        _ => None,
    }
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

/// Windows' own folder, asked from Windows (an environment variable could be changed by the user's
/// profile and point the admin copy elsewhere).
#[cfg(windows)]
pub fn windows_dir() -> PathBuf {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::SystemInformation::GetSystemWindowsDirectoryW;
    let mut buf = [0u16; 260];
    // SAFETY: the buffer and its length match.
    let n = unsafe { GetSystemWindowsDirectoryW(buf.as_mut_ptr(), buf.len() as u32) } as usize;
    if n == 0 || n >= buf.len() {
        return PathBuf::from(r"C:\Windows");
    }
    PathBuf::from(std::ffi::OsString::from_wide(&buf[..n]))
}

#[cfg(not(windows))]
pub fn windows_dir() -> PathBuf {
    PathBuf::from(r"C:\Windows")
}

/// The admin copy's whole job. Exit codes: 0 done, 2 refused path, 3 couldn't create it (or it
/// ended up somewhere unexpected, and was removed again).
pub fn run_helper(args: &[std::ffi::OsString]) -> i32 {
    let Some(path) = helper_path(args) else { return 2 };
    let Ok(path) = check_new_folder(&path, &windows_dir()) else { return 2 };
    match create_with_permission(&path) {
        Ok(()) => 0,
        Err(()) => 3,
    }
}

/// Creates the folder with [`FOLDER_SDDL`] in one step, then checks where it really is.
#[cfg(windows)]
fn create_with_permission(path: &Path) -> Result<(), ()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{LocalFree, FALSE};
    use windows_sys::Win32::Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1};
    use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
    use windows_sys::Win32::Storage::FileSystem::CreateDirectoryW;

    let wide = |s: &std::ffi::OsStr| s.encode_wide().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let sddl = wide(std::ffi::OsStr::new(FOLDER_SDDL));
    let target = wide(path.as_os_str());
    let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: the strings are NUL-terminated and live for the calls; `sd` is freed with LocalFree.
    let created = unsafe {
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut sd, std::ptr::null_mut()) == FALSE {
            return Err(());
        }
        let sa = SECURITY_ATTRIBUTES { nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: sd, bInheritHandle: FALSE };
        let ok = CreateDirectoryW(target.as_ptr(), &sa) != FALSE;
        LocalFree(sd as _);
        ok
    };
    if !created {
        return Err(());
    }
    // Where did it really end up? (A parent swapped for a link in between would show here.)
    let expected = std::fs::canonicalize(path.parent().ok_or(())?).map(|p| p.join(path.file_name().unwrap_or_default()));
    let actual = std::fs::canonicalize(path);
    let same = matches!((&expected, &actual), (Ok(e), Ok(a)) if norm(&e.display().to_string()) == norm(&a.display().to_string()));
    if !same || is_link(path) {
        let _ = std::fs::remove_dir(path);
        return Err(());
    }
    Ok(())
}

#[cfg(not(windows))]
fn create_with_permission(_path: &Path) -> Result<(), ()> {
    Err(())
}

/// Starts the admin copy (Windows shows its permission prompt) and waits up to a minute for it.
#[cfg(windows)]
pub fn ask_for_folder(path: &Path) -> Asked {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_CANCELLED, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, TerminateProcess, WaitForSingleObject};
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let wide = |s: &std::ffi::OsStr| s.encode_wide().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let Ok(exe) = std::env::current_exe() else { return Asked::Failed };
    let exe = wide(exe.as_os_str());
    let verb = wide(std::ffi::OsStr::new("runas"));
    // The path never ends in "\" (tidied) and folder names can't contain '"', so plain quotes hold.
    let mut params: Vec<u16> = std::ffi::OsStr::new(ARG).encode_wide().collect();
    params.extend(" \"".encode_utf16());
    params.extend(path.as_os_str().encode_wide());
    params.extend("\"".encode_utf16());
    params.push(0);
    // SAFETY: every pointer is to a live, NUL-terminated buffer for the duration of the call, and
    // the process handle is closed on every path once it's obtained.
    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI;
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
        let outcome = if WaitForSingleObject(info.hProcess, 60_000) == WAIT_OBJECT_0 {
            let mut code = 1u32;
            if GetExitCodeProcess(info.hProcess, &mut code) == 0 { Asked::Failed } else { asked_from(code) }
        } else {
            TerminateProcess(info.hProcess, 3);
            Asked::Failed
        };
        CloseHandle(info.hProcess);
        outcome
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
    fn existing_empty_refused_too() {
        // The admin copy only ever makes a brand-new folder (review: an "empty" one could be a link).
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("Tide");
        std::fs::create_dir(&p).unwrap();
        assert_eq!(check_new_folder(&p, &win()), Err(Refused::NotEmpty));
    }

    #[test]
    fn trailing_dot_space_and_streams_refused() {
        let dir = tempfile::tempdir().unwrap();
        for bad in ["Tide.", "Tide ", "Tide::$INDEX_ALLOCATION", "Tide:x"] {
            assert_eq!(check_new_folder(&dir.path().join(bad), &win()), Err(Refused::Unsafe), "{bad}");
        }
        let parent_dot = PathBuf::from(format!("{}.", dir.path().display())).join("Tide");
        assert_eq!(check_new_folder(&parent_dot, &win()), Err(Refused::Unsafe));
    }

    fn junction(link: &Path, target: &Path) {
        let ok = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap()
            .success();
        assert!(ok, "mklink /J needs no admin rights");
    }

    #[test]
    fn junction_leaf_refused() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("protected");
        std::fs::create_dir(&target).unwrap();
        let link = dir.path().join("Tide");
        junction(&link, &target);
        assert!(check_new_folder(&link, &win()).is_err());
    }

    #[test]
    fn junction_in_the_parent_chain_refused() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("elsewhere");
        std::fs::create_dir(&target).unwrap();
        let link = dir.path().join("Games");
        junction(&link, &target);
        assert_eq!(check_new_folder(&link.join("Tide"), &win()), Err(Refused::Unsafe));
    }

    #[test]
    fn windows_dir_comes_from_windows_not_the_environment() {
        let w = windows_dir();
        assert!(w.join("System32").is_dir(), "{}", w.display());
    }

    #[test]
    fn helper_creates_the_folder_with_users_full_control() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("Tide");
        assert_eq!(run_helper(&[ARG.into(), p.clone().into_os_string()]), 0);
        assert!(p.is_dir());
        let acl = std::process::Command::new("icacls").arg(&p).output().unwrap();
        let text = String::from_utf8_lossy(&acl.stdout);
        assert!(text.contains("BUILTIN\\Users:(OI)(CI)(F)"), "{text}");
        // a second run refuses: the folder exists now
        assert_eq!(run_helper(&[ARG.into(), p.clone().into_os_string()]), 2);
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
        let a = |v: &[&str]| v.iter().map(std::ffi::OsString::from).collect::<Vec<_>>();
        assert_eq!(helper_path(&a(&[ARG, r"C:\Games\Tide"])), Some(PathBuf::from(r"C:\Games\Tide")));
        assert_eq!(helper_path(&a(&[ARG])), None);
        assert_eq!(helper_path(&a(&[ARG, "a", "b"])), None);
        assert_eq!(helper_path(&a(&["--other", "a"])), None);
        assert_eq!(helper_path(&a(&[])), None);
        assert!(is_helper_call(&a(&[ARG])), "a lone flag is still a helper call (and refused)");
        assert!(!is_helper_call(&a(&[])));
    }

    #[test]
    fn folder_permission_is_users_full_control_inherited() {
        assert_eq!(FOLDER_SDDL, "D:(A;OICI;FA;;;BU)");
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
        assert_eq!(run_helper(&[ARG.into(), target.clone().into_os_string()]), 2);
        assert!(!dir.path().join("Nope").exists());
        assert_eq!(run_helper(&[ARG.into()]), 2);
    }
}
