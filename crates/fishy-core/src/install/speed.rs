//! Download speed averaged over the last few seconds.
use std::collections::VecDeque;
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(3);

pub struct SpeedMeter {
    samples: VecDeque<(Instant, u64)>,
}

impl Default for SpeedMeter {
    fn default() -> Self {
        Self::new()
    }
}

impl SpeedMeter {
    pub fn new() -> Self {
        Self { samples: VecDeque::new() }
    }

    /// Records the running total of bytes downloaded at `now`.
    pub fn add(&mut self, now: Instant, total_bytes: u64) {
        self.samples.push_back((now, total_bytes));
        // Drop samples older than the window, keeping at least two to measure between.
        while self.samples.len() > 2 && now.duration_since(self.samples[0].0) > WINDOW {
            self.samples.pop_front();
        }
    }

    pub fn bytes_per_sec(&self) -> f64 {
        let (Some(&(t0, b0)), Some(&(t1, b1))) = (self.samples.front(), self.samples.back()) else { return 0.0 };
        let secs = t1.duration_since(t0).as_secs_f64();
        if secs <= 0.0 {
            return 0.0;
        }
        b1.saturating_sub(b0) as f64 / secs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_meter_averages_last_three_seconds() {
        let t0 = Instant::now();
        let mut m = SpeedMeter::new();
        assert_eq!(m.bytes_per_sec(), 0.0);
        m.add(t0, 0);
        assert_eq!(m.bytes_per_sec(), 0.0);
        m.add(t0 + Duration::from_secs(1), 1000);
        assert_eq!(m.bytes_per_sec(), 1000.0);
        // A burst long ago stops counting once it's older than 3 s.
        m.add(t0 + Duration::from_secs(2), 10_000);
        m.add(t0 + Duration::from_secs(6), 10_000);
        m.add(t0 + Duration::from_secs(7), 12_000);
        let speed = m.bytes_per_sec();
        assert!((500.0..=2000.0).contains(&speed), "{speed}");
    }
}
