//! Install folders and free disk space.
use super::InstallError;
use std::io;
use std::path::{Path, PathBuf};

/// `%USERPROFILE%\Games`: no admin rights needed, unlike Program Files.
pub fn default_install_root() -> PathBuf {
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Users\Public"));
    home.join("Games")
}

/// The folder new games go into: the saved default if its drive is there, else the built-in one.
/// The bool is true when a saved folder couldn't be used (the Install box says so).
pub fn install_root(saved: Option<&str>) -> (PathBuf, bool) {
    let Some(saved) = saved.map(str::trim).filter(|s| !s.is_empty()) else {
        return (default_install_root(), false);
    };
    let path = PathBuf::from(saved);
    // A missing folder is fine (the install creates it); a missing drive is not.
    let drive_there = path.is_absolute() && path.ancestors().last().is_some_and(Path::exists);
    if drive_there { (path, false) } else { (default_install_root(), true) }
}

/// `root\<folder>`: Epic's FolderName if it has one, else the title, minus characters Windows forbids.
pub fn folder_for(root: &Path, folder_name: Option<&str>, title: &str) -> PathBuf {
    let clean = |s: &str| -> String {
        // The last path piece only, so a FolderName like "..\x" can't climb out of the root.
        let last = s.rsplit(['/', '\\']).next().unwrap_or_default();
        let kept: String = last.chars().filter(|c| !c.is_control() && !r#"<>:"/\|?*"#.contains(*c)).collect();
        let squeezed = kept.split_whitespace().collect::<Vec<_>>().join(" ");
        squeezed.trim_matches(|c: char| c == '.' || c == ' ').to_string()
    };
    let name = [folder_name.map(clean), Some(clean(title))]
        .into_iter()
        .flatten()
        .find(|n| !n.is_empty())
        .unwrap_or_else(|| "Game".to_string());
    root.join(name)
}

/// Free space on the drive of `path` (or its nearest existing parent folder).
pub fn free_space(path: &Path) -> io::Result<u64> {
    let existing = path.ancestors().find(|p| p.exists()).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no existing parent"))?;
    fs4::available_space(existing)
}

/// "C:" for `C:\Games\Tide`.
pub fn drive_label(path: &Path) -> String {
    match path.components().next() {
        Some(std::path::Component::Prefix(p)) => p.as_os_str().to_string_lossy().to_uppercase(),
        _ => path.display().to_string(),
    }
}

/// `path`, or "path (2)", "path (3)"… when another game already uses that folder (capital letters
/// ignored), so two games never share one.
pub fn unique_folder(path: PathBuf, taken: &[PathBuf]) -> PathBuf {
    let key = |p: &Path| p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase();
    let used = |p: &Path| taken.iter().any(|t| key(t) == key(p));
    if !used(&path) {
        return path;
    }
    let base = path.as_os_str().to_string_lossy().trim_end_matches(['\\', '/']).to_string();
    (2..).map(|n| PathBuf::from(format!("{base} ({n})"))).find(|p| !used(p)).expect("an unused number")
}

/// Space other downloads still need on the same drive as `path`: (their folder, bytes still to
/// write).
pub fn reserved_on_drive(path: &Path, others: &[(PathBuf, u64)]) -> u64 {
    let drive = drive_label(path);
    others.iter().filter(|(p, _)| drive_label(p) == drive).map(|(_, bytes)| bytes).sum()
}

pub fn check_space(path: &Path, needed: u64, free: u64) -> Result<(), InstallError> {
    if needed > free {
        return Err(InstallError::NoSpace { drive: drive_label(path), needed, free });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_another_game_uses_gets_a_number() {
        let taken = vec![PathBuf::from(r"C:\Games\Tide"), PathBuf::from(r"c:\games\tide (2)")];
        assert_eq!(unique_folder(PathBuf::from(r"C:\Games\Tide"), &taken), PathBuf::from(r"C:\Games\Tide (3)"));
        assert_eq!(unique_folder(PathBuf::from(r"C:\Games\Reef"), &taken), PathBuf::from(r"C:\Games\Reef"));
    }

    #[test]
    fn only_downloads_on_the_same_drive_count() {
        let others = vec![(PathBuf::from(r"C:\Games\Tide"), 5), (PathBuf::from(r"c:\Other\Reef"), 7), (PathBuf::from(r"D:\Games\Kelp"), 100)];
        assert_eq!(reserved_on_drive(Path::new(r"C:\Games\New"), &others), 12);
        assert_eq!(reserved_on_drive(Path::new(r"E:\Games\New"), &others), 0);
    }

    /// A drive letter with nothing behind it on this PC.
    fn missing_drive() -> String {
        ('D'..='Z').rev().map(|c| format!("{c}:\\")).find(|d| !Path::new(d).exists()).expect("a free drive letter")
    }

    #[test]
    fn install_root_uses_the_saved_folder_or_falls_back() {
        assert_eq!(install_root(None), (default_install_root(), false));
        assert_eq!(install_root(Some("  ")), (default_install_root(), false));
        let dir = tempfile::tempdir().unwrap();
        let here = dir.path().display().to_string();
        assert_eq!(install_root(Some(&here)), (dir.path().to_path_buf(), false));
        // A folder that doesn't exist yet is fine: the install creates it.
        let later = dir.path().join("New Games").display().to_string();
        assert_eq!(install_root(Some(&later)), (dir.path().join("New Games"), false));
        let gone = format!("{}Games", missing_drive());
        assert_eq!(install_root(Some(&gone)), (default_install_root(), true));
    }

    #[test]
    fn default_root_is_games_in_the_user_folder() {
        let home = std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap();
        assert_eq!(default_install_root(), home.join("Games"));
    }

    #[test]
    fn folder_for_sanitizes() {
        let root = Path::new(r"C:\Games");
        assert_eq!(folder_for(root, None, "Fish: The Game?"), root.join("Fish The Game"));
        assert_eq!(folder_for(root, Some("TidepoolOdyssey"), "Tidepool Odyssey"), root.join("TidepoolOdyssey"));
        assert_eq!(folder_for(root, Some("..\\..\\x"), "T"), root.join("x"));
        assert_eq!(folder_for(root, None, "  Trailing dots... "), root.join("Trailing dots"));
        assert_eq!(folder_for(root, None, "<>:\"/\\|?*"), root.join("Game"));
        assert_eq!(folder_for(root, Some("  "), "Real Title"), root.join("Real Title"));
    }

    #[test]
    fn free_space_walks_up_to_existing_parent() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("not").join("yet").join("there");
        let free = free_space(&missing).unwrap();
        assert!(free > 0);
        assert_eq!(free_space(dir.path()).unwrap() / (1 << 30), free / (1 << 30));
    }

    #[test]
    fn drive_label_is_the_drive_letter() {
        assert_eq!(drive_label(Path::new(r"C:\Games\Tide")), "C:");
        assert_eq!(drive_label(Path::new(r"d:\x")), "D:");
    }

    #[test]
    fn check_space_message() {
        assert_eq!(check_space(Path::new(r"C:\Games\T"), 10, 10), Ok(()));
        assert_eq!(
            check_space(Path::new(r"C:\Games\T"), 11, 10),
            Err(InstallError::NoSpace { drive: "C:".into(), needed: 11, free: 10 })
        );
    }
}
