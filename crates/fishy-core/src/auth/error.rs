use thiserror::Error;

/// What can go wrong signing in. `Display` is the user-facing message.
/// No variant ever holds a token: Epic's own `errorMessage` can quote tokens, so it's dropped.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AuthError {
    /// Epic couldn't be reached (offline, DNS, timeout). The detail never includes URLs.
    #[error("Couldn't reach Epic. Check your internet connection.")]
    Network(String),
    /// Epic answered with an error. `code` is Epic's errorCode (e.g. `…authorization_code_not_found`).
    #[error("That sign-in didn't work. Please try again.")]
    Rejected { code: String },
    /// Epic answered with something we couldn't understand.
    #[error("Epic sent an unexpected reply. Please try again.")]
    Unexpected(String),
    /// Windows Credential Manager refused to store the session.
    #[error("Couldn't save your sign-in to Windows Credential Manager.")]
    Storage(String),
    /// Epic refused the saved sign-in (or there is none): it has been forgotten (spec §6f.2).
    #[error("Your sign-in has expired. Please sign in again.")]
    Expired,
}
