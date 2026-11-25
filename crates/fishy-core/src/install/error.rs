use crate::auth::AuthError;
use thiserror::Error;

/// What can go wrong installing. `user_message()` is what the UI shows (spec §6c copy).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InstallError {
    #[error("network: {0}")]
    Network(String),
    #[error("rejected: {0}")]
    Rejected(String),
    #[error("unsupported format")]
    Unsupported,
    #[error("bad manifest: {0}")]
    BadManifest(String),
    #[error("bad chunk: {0}")]
    BadChunk(String),
    #[error("damaged file: {0}")]
    DamagedFile(String),
    #[error("io in {folder}: {detail}")]
    Io { folder: String, detail: String },
    #[error("no space on {drive}")]
    NoSpace { drive: String, needed: u64, free: u64 },
    #[error("files in use")]
    InUse,
    #[error("stopped")]
    Stopped,
    #[error("not installed")]
    NotInstalled,
    /// Epic has no Windows download for this library entry.
    #[error("no download")]
    NoDownload,
    /// Windows says a game file is in use, usually because the game is running (spec §6f.2).
    #[error("files in use: {0}")]
    FilesInUse(String),
    /// The saved sign-in is gone (spec §6f.2); the app sends the user back to Login.
    #[error("sign-in expired")]
    Expired,
    /// Windows won't let Fishy Client create folders there without admin rights (spec §6g.3).
    #[error("no write access in {folder}")]
    NoWriteAccess { folder: String },
    /// No in Windows' permission prompt (spec §6k).
    #[error("permission declined")]
    PermissionDeclined,
    /// The admin copy couldn't create the folder, or it still isn't writable (spec §6k).
    #[error("couldn't create {folder}")]
    FolderFailed { folder: String },
    /// Epic replied with something Fishy Client doesn't understand (docs/EPIC-CHANGES.md).
    /// The detail is never shown or logged: a parser message can quote the reply.
    #[error("unexpected reply from Epic")]
    EpicChanged(String),
}

impl InstallError {
    /// For the log: which kind of problem, in Fishy Client's own words. Never Epic's error text,
    /// tokens or URLs (spec §12). A bad manifest's reason is Fishy's own parser text.
    pub fn log_reason(&self) -> String {
        match self {
            Self::Network(_) => "network".into(),
            Self::Rejected(_) => "rejected by Epic".into(),
            Self::Unsupported => "unsupported format".into(),
            Self::BadManifest(why) => format!("manifest unreadable ({why})"),
            Self::BadChunk(_) => "damaged chunk".into(),
            Self::DamagedFile(_) => "damaged file".into(),
            Self::Io { .. } => "disk error".into(),
            Self::NoSpace { .. } => "not enough space".into(),
            Self::InUse | Self::FilesInUse(_) => "files in use".into(),
            Self::Stopped => "stopped".into(),
            Self::NotInstalled => "not installed".into(),
            Self::NoDownload => "no Windows download".into(),
            Self::Expired => "sign-in expired".into(),
            Self::NoWriteAccess { .. } => "no write access".into(),
            Self::PermissionDeclined => "permission declined".into(),
            Self::EpicChanged(_) => "unexpected reply from Epic".into(),
            Self::FolderFailed { .. } => "folder not created".into(),
        }
    }

    pub fn user_message(&self) -> String {
        match self {
            Self::Network(_) => "Download stopped: the connection to the download server dropped. Check your internet, then press Retry.".into(),
            Self::Rejected(_) => "That sign-in didn't work. Please try again.".into(),
            Self::Unsupported | Self::BadManifest(_) => "This game uses a download format Fishy Client can't install yet.".into(),
            Self::BadChunk(_) | Self::DamagedFile(_) => "A downloaded file was damaged. Press Retry to download it again.".into(),
            Self::Io { folder, .. } => format!("Couldn't write to {folder}. Pick a different folder."),
            Self::NoSpace { drive, needed, free } => {
                format!("Not enough space on {drive}. Needs {}, {} free.", format_bytes(*needed), format_bytes(*free))
            }
            Self::InUse => "Couldn't delete some files. Close the game and try again.".into(),
            Self::Stopped => "Download paused.".into(),
            Self::NotInstalled => "That game isn't installed any more. Refresh your Library.".into(),
            Self::FilesInUse(title) => format!("Some of {title}'s files are in use. Close the game, then press Retry."),
            Self::Expired => "Your sign-in has expired. Please sign in again.".into(),
            Self::PermissionDeclined => "Fishy Client needs your permission to use that folder. Pick a different folder.".into(),
            Self::EpicChanged(_) => crate::epic::CHANGED_MESSAGE.into(),
            Self::FolderFailed { folder } => format!("Couldn't create {folder}. Pick a different folder."),
            Self::NoWriteAccess { folder } => {
                format!("Fishy Client can't create folders in {folder} without administrator rights. Pick a different folder.")
            }
            Self::NoDownload => {
                "Epic doesn't offer a Windows download for this item. If the game is in your library twice, try the other one.".into()
            }
        }
    }
}

impl From<AuthError> for InstallError {
    fn from(e: AuthError) -> Self {
        match e {
            AuthError::Network(d) => Self::Network(d),
            AuthError::Rejected { code } => Self::Rejected(code),
            AuthError::Unexpected(d) => Self::EpicChanged(d),
            AuthError::Storage(d) => Self::Network(d),
            AuthError::Expired => Self::Expired,
        }
    }
}

/// Binary units with one decimal, like the UI's `formatBytes`: "7.4 GB".
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut i = 0;
    while value >= 1024.0 && i < UNITS.len() - 1 {
        value /= 1024.0;
        i += 1;
    }
    format!("{value:.1} {}", UNITS[i])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_reason_says_which_kind_without_epics_text() {
        assert_eq!(InstallError::Unsupported.log_reason(), "unsupported format");
        assert_eq!(InstallError::BadManifest("zlib".into()).log_reason(), "manifest unreadable (zlib)");
        // Epic's own error text never reaches the log (spec §12).
        assert_eq!(InstallError::Rejected("errors.com.epicgames.secret".into()).log_reason(), "rejected by Epic");
        assert_eq!(InstallError::Network("https://x?token=1".into()).log_reason(), "network");
        assert_eq!(InstallError::NoDownload.log_reason(), "no Windows download");
        assert_eq!(InstallError::Expired.log_reason(), "sign-in expired");
    }

    #[test]
    fn format_bytes_uses_binary_units() {
        assert_eq!(format_bytes(0), "0.0 B");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(7 * 1024 * 1024 * 1024 + 400 * 1024 * 1024), "7.4 GB");
    }

    #[test]
    fn user_messages_match_spec() {
        let gb = 1024u64 * 1024 * 1024;
        let no_space = InstallError::NoSpace { drive: "C:".into(), needed: 12 * gb + 300 * 1024 * 1024, free: 4 * gb + 100 * 1024 * 1024 };
        assert_eq!(no_space.user_message(), "Not enough space on C:. Needs 12.3 GB, 4.1 GB free.");
        assert_eq!(
            InstallError::Network("x".into()).user_message(),
            "Download stopped: the connection to the download server dropped. Check your internet, then press Retry."
        );
        assert_eq!(InstallError::NotInstalled.user_message(), "That game isn't installed any more. Refresh your Library.");
        assert_eq!(InstallError::FilesInUse("Bendy".into()).user_message(), "Some of Bendy's files are in use. Close the game, then press Retry.");
        assert_eq!(InstallError::Unsupported.user_message(), "This game uses a download format Fishy Client can't install yet.");
        assert_eq!(
            InstallError::Io { folder: r"C:\Games\Fish".into(), detail: "denied".into() }.user_message(),
            r"Couldn't write to C:\Games\Fish. Pick a different folder."
        );
        assert_eq!(InstallError::DamagedFile("a".into()).user_message(), "A downloaded file was damaged. Press Retry to download it again.");
        assert_eq!(InstallError::BadChunk("a".into()).user_message(), "A downloaded file was damaged. Press Retry to download it again.");
        assert_eq!(InstallError::InUse.user_message(), "Couldn't delete some files. Close the game and try again.");
        assert_eq!(
            InstallError::NoWriteAccess { folder: r"C:\Program Files\Epic Games".into() }.user_message(),
            r"Fishy Client can't create folders in C:\Program Files\Epic Games without administrator rights. Pick a different folder."
        );
        assert_eq!(
            InstallError::PermissionDeclined.user_message(),
            "Fishy Client needs your permission to use that folder. Pick a different folder."
        );
        assert_eq!(
            InstallError::FolderFailed { folder: r"D:\Games\Tide".into() }.user_message(),
            r"Couldn't create D:\Games\Tide. Pick a different folder."
        );
        assert_eq!(InstallError::Rejected("x".into()).user_message(), "That sign-in didn't work. Please try again.");
        assert_eq!(
            InstallError::NoDownload.user_message(),
            "Epic doesn't offer a Windows download for this item. If the game is in your library twice, try the other one."
        );
    }

    #[test]
    fn expired_sign_in_converts_and_says_so() {
        assert_eq!(InstallError::from(AuthError::Expired), InstallError::Expired);
        assert_eq!(InstallError::Expired.user_message(), "Your sign-in has expired. Please sign in again.");
        assert_eq!(AuthError::Expired.to_string(), "Your sign-in has expired. Please sign in again.");
    }

    #[test]
    fn auth_errors_convert() {
        assert_eq!(InstallError::from(AuthError::Network("dns".into())), InstallError::Network("dns".into()));
        assert_eq!(InstallError::from(AuthError::Rejected { code: "c".into() }), InstallError::Rejected("c".into()));
    }

    #[test]
    fn a_reply_epic_changed_says_so() {
        let e = InstallError::from(AuthError::Unexpected("missing field `records`".into()));
        assert_eq!(e, InstallError::EpicChanged("missing field `records`".into()));
        assert_eq!(e.user_message(), "Epic changed something. Update Fishy Client, or check back later.");
        assert_eq!(e.log_reason(), "unexpected reply from Epic");
    }
}
