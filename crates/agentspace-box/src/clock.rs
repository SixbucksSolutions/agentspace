//! The clock the waits run on, behind a trait so that a test's waits run at once.

use std::time::{Duration, Instant, SystemTime};

/// The time, as the start-and-wait step needs it.
///
/// - [`Clock::now`] is monotonic, and measures the 15-minute wait and its 5-second checks.
/// - [`Clock::utc_now`] is the wall clock, which the 20-minute rule compares the instance's
///   `LaunchTime` with (operator.md, Starting the box and waiting for it, step 2).
/// - [`Clock::sleep`] is every pause the step takes: the 5 seconds between checks and the second
///   between polls of the patch check's invocation.
pub trait Clock {
    /// A monotonic time, as the time since an origin of the clock's own choosing. It MUST NOT go
    /// backwards.
    fn now(&self) -> Duration;
    /// The wall clock's time now.
    fn utc_now(&self) -> SystemTime;
    /// Waits for `duration`.
    fn sleep(&self, duration: Duration);
}

/// The real clock: [`Instant`] since the clock was made, [`SystemTime::now`], and
/// [`std::thread::sleep`].
#[derive(Debug, Clone, Copy)]
pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    /// A clock whose monotonic origin is now.
    pub fn new() -> Self {
        SystemClock {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.origin.elapsed()
    }

    fn utc_now(&self) -> SystemTime {
        SystemTime::now()
    }

    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_clock_does_not_go_backwards() {
        let clock = SystemClock::new();
        let a = clock.now();
        clock.sleep(Duration::from_millis(1));
        let b = clock.now();
        assert!(b >= a + Duration::from_millis(1));
    }
}
