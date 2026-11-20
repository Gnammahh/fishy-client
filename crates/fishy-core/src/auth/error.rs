use thiserror::Error;

/// What can go wrong talking to Epic. `Display` is the user-facing message and never contains tokens.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AuthError {
    /// Epic couldn't be reached (offline, DNS, timeout). The detail never includes URLs.
    #[error("Couldn't reach Epic. Check your internet connection.")]
    Network(String),
    /// Epic answered with an error (bad/expired code, revoked session…).
    #[error("That sign-in didn't work. Please try again.")]
    Rejected { code: String, message: String },
    /// Epic answered with something we couldn't understand.
    #[error("Epic sent an unexpected reply. Please try again.")]
    Unexpected(String),
}
