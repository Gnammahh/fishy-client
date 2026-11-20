//! Install folders and free disk space.
use super::InstallError;
use std::io;
use std::path::{Path, PathBuf};

/// `%USERPROFILE%\Games`: no admin rights needed, unlike Program Files.
pub fn default_install_root() -> PathBuf {
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Users\Public"));
    home.join("Games")
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
