use core::sync::atomic::{AtomicU32, Ordering};
use core::time::Duration;
use std::marker::PhantomData;

use embedded_hal_async::delay::DelayNs;

#[derive(Debug)]
pub struct ExpBackoff {
    max: Duration,
    initial: Duration,
    exp: AtomicU32,
    target: &'static str,
}

pub struct Retry<E, F = fn(&E) -> bool> {
    max_retries: usize,
    should_retry: F,
    target: &'static str,
    _error: PhantomData<fn(E)>,
}

// === impl ExpBackoff ===

impl ExpBackoff {
    const DEFAULT_MAX_BACKOFF: Duration = Duration::from_secs(60);

    pub const fn new(initial: Duration) -> Self {
        Self {
            max: Self::DEFAULT_MAX_BACKOFF,
            initial,
            exp: AtomicU32::new(0),
            target: "retry",
        }
    }

    pub const fn with_max(self, max: Duration) -> Self {
        Self { max, ..self }
    }

    pub const fn with_target(self, target: &'static str) -> Self {
        Self { target, ..self }
    }

    pub async fn wait(&self, delay: &mut impl DelayNs) {
        // log::debug!(target: self.target, "backing off for {}...", self.current);
        let current = self.initial * self.exp.load(Ordering::Acquire);

        if current < self.max {
            self.exp.fetch_add(1, Ordering::Relaxed);
        }

        if let Ok(ns) = u32::try_from(current.as_nanos()) {
            delay.delay_ns(ns).await;
        } else if let Ok(us) = u32::try_from(current.as_micros()) {
            delay.delay_us(us).await;
        } else {
            let ms = u32::try_from(current.as_millis()).unwrap_or(u32::MAX);
            delay.delay_ms(ms).await;
        }
    }

    pub fn reset(&self) {
        // log::debug!(target: self.target, "reset backoff to {}", self.initial);
        self.exp.store(1, Ordering::Release);
    }

    pub fn current(&self) -> Duration {
        self.initial * self.exp.load(Ordering::Acquire)
    }
}
