use crate::auth::AuthError;

/// What can go wrong pressing Play. `user_message(title)` is what the UI shows (spec §6d copy).
/// No variant ever holds the exchange code or the ownership token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchError {
    /// Epic couldn't be reached.
    Network,
    /// The saved sign-in is gone or expired: the UI goes back to Login.
    SignedOut,
    /// Epic refused the request. Holds Epic's errorCode.
    Rejected(String),
    MissingProgram,
    NeedsAdmin,
    /// Windows refused to start the program; holds its reason.
    Spawn(String),
    /// Started through another store's launcher (EA app, Ubisoft Connect).
    ThirdParty(String),
    NotInLibrary,
    NotInstalled,
}

impl LaunchError {
    pub fn user_message(&self, title: &str) -> String {
        match self {
            Self::Network => "Couldn't reach Epic to sign the game in. Check your internet.".into(),
            Self::SignedOut => "Your sign-in has expired. Please sign in again.".into(),
            Self::Rejected(code) => format!("Epic wouldn't let {title} start ({code}). Try again later."),
            Self::MissingProgram => format!("Couldn't find {title}'s program file. Uninstall and install it again."),
            Self::NeedsAdmin => format!("Windows wouldn't start {title}: it needs administrator rights."),
            Self::Spawn(reason) => format!("Windows wouldn't start {title}: {reason}. Restart your PC and try again."),
            Self::ThirdParty(store) => format!("{title} starts through {store}, which Fishy Client can't do yet."),
            Self::NotInLibrary => "Refresh your Library, then try again.".into(),
            Self::NotInstalled => "That game isn't installed any more. Refresh your Library.".into(),
        }
    }
}

impl From<AuthError> for LaunchError {
    /// For Epic calls made with a valid token (exchange code, ownership token).
    fn from(e: AuthError) -> Self {
        match e {
            AuthError::Network(_) => Self::Network,
            AuthError::Rejected { code } => Self::Rejected(code),
            AuthError::Unexpected(_) | AuthError::Storage(_) => Self::Rejected("unexpected_reply".into()),
            AuthError::Expired => Self::SignedOut,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_say_what_to_do_next() {
        assert_eq!(LaunchError::NotInstalled.user_message("Reef"), "That game isn't installed any more. Refresh your Library.");
        assert_eq!(LaunchError::Spawn("x".into()).user_message("Reef"), "Windows wouldn't start Reef: x. Restart your PC and try again.");
    }

    #[test]
    fn messages_match_the_spec() {
        let t = "Bad Cheese";
        assert_eq!(LaunchError::Network.user_message(t), "Couldn't reach Epic to sign the game in. Check your internet.");
        assert_eq!(LaunchError::MissingProgram.user_message(t), "Couldn't find Bad Cheese's program file. Uninstall and install it again.");
        assert_eq!(LaunchError::NeedsAdmin.user_message(t), "Windows wouldn't start Bad Cheese: it needs administrator rights.");
        assert_eq!(LaunchError::Spawn("Access is denied".into()).user_message(t), "Windows wouldn't start Bad Cheese: Access is denied. Restart your PC and try again.");
        assert_eq!(LaunchError::ThirdParty("the EA app".into()).user_message(t), "Bad Cheese starts through the EA app, which Fishy Client can't do yet.");
        assert_eq!(LaunchError::NotInLibrary.user_message(t), "Refresh your Library, then try again.");
        assert_eq!(LaunchError::Rejected("errors.x".into()).user_message(t), "Epic wouldn't let Bad Cheese start (errors.x). Try again later.");
    }

    #[test]
    fn auth_errors_map_without_details() {
        assert_eq!(LaunchError::from(AuthError::Network("dns".into())), LaunchError::Network);
        assert_eq!(LaunchError::from(AuthError::Rejected { code: "c".into() }), LaunchError::Rejected("c".into()));
    }
}
