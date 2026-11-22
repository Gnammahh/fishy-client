//! Retrying Epic calls after connection problems (spec §6f.2).
use crate::auth::AuthError;
use crate::install::InstallError;
use std::future::Future;
use std::time::Duration;

/// The wait before the 2nd try; the 3rd waits twice as long.
pub const RETRY_BASE: Duration = Duration::from_millis(500);
/// Tries in all, the first included.
pub const TRIES: u32 = 3;

/// Errors worth another try: the connection, not Epic's answer.
pub trait Transient {
    fn is_transient(&self) -> bool;
}

impl Transient for AuthError {
    fn is_transient(&self) -> bool {
        matches!(self, AuthError::Network(_))
    }
}

impl Transient for InstallError {
    fn is_transient(&self) -> bool {
        matches!(self, InstallError::Network(_))
    }
}

/// Runs `f` up to [`TRIES`] times while it fails with a transient error, waiting `base`, then
/// `2 × base` between tries. `what` names the call in the log (no addresses or tokens).
pub async fn with_retries<T, E, Fut>(what: &str, base: Duration, mut f: impl FnMut() -> Fut) -> Result<T, E>
where
    E: Transient,
    Fut: Future<Output = Result<T, E>>,
{
    let mut attempt = 1;
    loop {
        match f().await {
            Err(e) if e.is_transient() && attempt < TRIES => {
                tokio::time::sleep(base * 2u32.pow(attempt - 1)).await;
                attempt += 1;
                tracing::info!("retrying {what} ({attempt} of {TRIES})");
            }
            other => return other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn succeeds_after_two_transient_failures() {
        let calls = AtomicU32::new(0);
        let start = Instant::now();
        let r: Result<u32, AuthError> = with_retries("test", RETRY_BASE, || {
            let n = calls.fetch_add(1, Ordering::SeqCst);
            async move { if n < 2 { Err(AuthError::Network("down".into())) } else { Ok(7) } }
        })
        .await;
        assert_eq!(r.unwrap(), 7);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        let secs = start.elapsed().as_secs_f64();
        assert!((1.45..=1.55).contains(&secs), "{secs}");
    }

    #[tokio::test(start_paused = true)]
    async fn gives_up_after_three_tries() {
        let calls = AtomicU32::new(0);
        let r: Result<(), AuthError> = with_retries("test", RETRY_BASE, || {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Err(AuthError::Network("down".into())) }
        })
        .await;
        assert!(matches!(r, Err(AuthError::Network(_))));
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn a_real_answer_is_not_retried() {
        let calls = AtomicU32::new(0);
        let r: Result<(), AuthError> = with_retries("test", RETRY_BASE, || {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Err(AuthError::Rejected { code: "x".into() }) }
        })
        .await;
        assert!(matches!(r, Err(AuthError::Rejected { .. })));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn install_network_errors_are_retried_too() {
        let calls = AtomicU32::new(0);
        let r: Result<(), InstallError> = with_retries("test", RETRY_BASE, || {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Err(InstallError::Network("down".into())) }
        })
        .await;
        assert!(r.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }
}
