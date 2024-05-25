use core::sync::atomic::{AtomicUsize, Ordering};
use core::time::Duration;

use embedded_hal_async::delay::DelayNs;

#[derive(Debug)]
pub struct ExpBackoff {
    max_ms: usize,
    initial_ms: usize,
    exp: AtomicUsize,
}

// pub struct Retry<E, F = fn(&E) -> bool> {
//     max_retries: usize,
//     should_retry: F,
//     target: &'static str,
//     _error: PhantomData<fn(E)>,
// }

// === impl ExpBackoff ===

impl ExpBackoff {
    const DEFAULT_MAX_BACKOFF: Duration = Duration::from_secs(60);

    pub const fn new(initial: Duration) -> Self {
        Self {
            max_ms: Self::DEFAULT_MAX_BACKOFF.as_millis() as usize,
            initial_ms: initial.as_millis() as usize,
            exp: AtomicUsize::new(0),
        }
    }

    pub const fn with_max(self, max: Duration) -> Self {
        Self {
            max_ms: max.as_millis() as usize,
            ..self
        }
    }

    pub async fn wait(&self, delay: &mut impl DelayNs) {
        // log::debug!(target: self.target, "backing off for {}...", self.current);
        let current = self.initial_ms * self.exp.load(Ordering::Acquire);

        if current < self.max_ms {
            self.exp.fetch_add(1, Ordering::Relaxed);
        }

        delay.delay_ms(current as u32).await;
    }

    pub fn reset(&self) {
        // log::debug!(target: self.target, "reset backoff to {}", self.initial);
        self.exp.store(1, Ordering::Release);
    }

    pub fn current(&self) -> Duration {
        Duration::from_millis(self.current_ms() as u64)
    }

    fn current_ms(&self) -> usize {
        self.initial_ms * self.exp.load(Ordering::Acquire)
    }
}

impl Default for ExpBackoff {
    fn default() -> Self {
        Self::new(Duration::from_secs(1))
    }
}

impl From<Duration> for ExpBackoff {
    fn from(initial: Duration) -> Self {
        Self::new(initial)
    }
}

impl Clone for ExpBackoff {
    fn clone(&self) -> Self {
        let Self {
            max_ms, initial_ms, ..
        } = *self;
        Self {
            max_ms,
            initial_ms,
            exp: AtomicUsize::new(0),
        }
    }
}
