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
}

impl InstallError {
    pub fn user_message(&self) -> String {
        match self {
            Self::Network(_) => "Download stopped: the connection to the download server dropped.".into(),
            Self::Rejected(_) => "That sign-in didn't work. Please try again.".into(),
            Self::Unsupported | Self::BadManifest(_) => "This game uses a download format Fishy Client can't install yet.".into(),
            Self::BadChunk(_) | Self::DamagedFile(_) => "A downloaded file was damaged. Press Retry to download it again.".into(),
            Self::Io { folder, .. } => format!("Couldn't write to {folder}. Pick a different folder."),
            Self::NoSpace { drive, needed, free } => {
                format!("Not enough space on {drive}. Needs {}, {} free.", format_bytes(*needed), format_bytes(*free))
            }
            Self::InUse => "Couldn't delete some files. Close the game and try again.".into(),
            Self::Stopped => "Download paused.".into(),
            Self::NotInstalled => "That game isn't installed.".into(),
        }
    }
}

impl From<AuthError> for InstallError {
    fn from(e: AuthError) -> Self {
        match e {
            AuthError::Network(d) => Self::Network(d),
            AuthError::Rejected { code } => Self::Rejected(code),
            AuthError::Unexpected(d) | AuthError::Storage(d) => Self::Network(d),
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
        assert_eq!(InstallError::Network("x".into()).user_message(), "Download stopped: the connection to the download server dropped.");
        assert_eq!(InstallError::Unsupported.user_message(), "This game uses a download format Fishy Client can't install yet.");
        assert_eq!(
            InstallError::Io { folder: r"C:\Games\Fish".into(), detail: "denied".into() }.user_message(),
            r"Couldn't write to C:\Games\Fish. Pick a different folder."
        );
        assert_eq!(InstallError::DamagedFile("a".into()).user_message(), "A downloaded file was damaged. Press Retry to download it again.");
        assert_eq!(InstallError::BadChunk("a".into()).user_message(), "A downloaded file was damaged. Press Retry to download it again.");
        assert_eq!(InstallError::InUse.user_message(), "Couldn't delete some files. Close the game and try again.");
        assert_eq!(InstallError::Rejected("x".into()).user_message(), "That sign-in didn't work. Please try again.");
    }

    #[test]
    fn auth_errors_convert() {
        assert_eq!(InstallError::from(AuthError::Network("dns".into())), InstallError::Network("dns".into()));
        assert_eq!(InstallError::from(AuthError::Rejected { code: "c".into() }), InstallError::Rejected("c".into()));
    }
}
