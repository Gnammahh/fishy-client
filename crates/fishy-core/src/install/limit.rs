//! Download speed limit (spec §6f.1): one token bucket shared by every chunk download.
use crate::settings::Settings;
use std::sync::Mutex;
use std::time::Duration;
use tokio::time::Instant;

/// Limits how fast chunks are fetched. The limit can change at any time, even mid-download.
///
/// A token bucket holding at most one second of bytes. A take may push it below zero (a debt);
/// the taker then sleeps until the debt is paid, so chunks bigger than a second still work and
/// several downloads at once share the rate.
pub struct SpeedLimiter {
    state: Mutex<Bucket>,
}

struct Bucket {
    rate: Option<u64>,
    tokens: f64,
    last: Instant,
}

impl SpeedLimiter {
    pub fn new() -> Self {
        Self { state: Mutex::new(Bucket { rate: None, tokens: 0.0, last: Instant::now() }) }
    }

    /// Bytes per second, or None for no limit. Starts again from an empty bucket.
    pub fn set(&self, bytes_per_sec: Option<u64>) {
        let mut b = self.state.lock().unwrap_or_else(|e| e.into_inner());
        b.rate = bytes_per_sec.filter(|&r| r > 0);
        b.tokens = 0.0;
        b.last = Instant::now();
    }

    /// Waits until `bytes` more may be downloaded.
    pub async fn take(&self, bytes: u64) {
        let wait = {
            let mut b = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let Some(rate) = b.rate else { return };
            let rate = rate as f64;
            let now = Instant::now();
            b.tokens = (b.tokens + now.duration_since(b.last).as_secs_f64() * rate).min(rate);
            b.last = now;
            b.tokens -= bytes as f64;
            if b.tokens < 0.0 { Duration::from_secs_f64(-b.tokens / rate) } else { Duration::ZERO }
        };
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }
}

impl Default for SpeedLimiter {
    fn default() -> Self {
        Self::new()
    }
}

/// The saved limit in bytes per second (1 MB = 1,000,000 bytes).
pub fn limit_bytes(s: &Settings) -> Option<u64> {
    s.speed_limit_mbps.map(|mb| u64::from(mb) * 1_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn no_limit_never_waits() {
        let l = SpeedLimiter::new();
        let start = Instant::now();
        for _ in 0..100 {
            l.take(1_000_000).await;
        }
        assert_eq!(start.elapsed(), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn two_mb_at_one_mb_per_sec_takes_about_two_seconds() {
        let l = SpeedLimiter::new();
        l.set(Some(1_000_000));
        let start = Instant::now();
        for _ in 0..4 {
            l.take(500_000).await;
        }
        let secs = start.elapsed().as_secs_f64();
        assert!((1.9..=2.1).contains(&secs), "{secs}");
    }

    #[tokio::test(start_paused = true)]
    async fn a_change_applies_to_the_next_take() {
        let l = SpeedLimiter::new();
        l.set(Some(1_000_000));
        l.take(1_000_000).await;
        let after_first = Instant::now();
        l.set(None);
        l.take(5_000_000).await;
        assert_eq!(after_first.elapsed(), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn after_a_quiet_spell_only_one_second_of_burst_is_free() {
        let l = SpeedLimiter::new();
        l.set(Some(1_000_000));
        tokio::time::sleep(Duration::from_secs(60)).await;
        let start = Instant::now();
        l.take(3_000_000).await; // 1 s banked, 2 s to wait
        let secs = start.elapsed().as_secs_f64();
        assert!((1.9..=2.1).contains(&secs), "{secs}");
    }

    #[test]
    fn limit_bytes_reads_the_setting() {
        assert_eq!(limit_bytes(&Settings { speed_limit_mbps: Some(10), ..Settings::default() }), Some(10_000_000));
        assert_eq!(limit_bytes(&Settings::default()), None);
    }
}
