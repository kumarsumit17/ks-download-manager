//! Per-range retry: which failures are worth another attempt, exponential backoff with
//! full jitter, and `Retry-After` (RFC 9110, section 10.2.3).

use std::time::{Duration, SystemTime};

use ks_utility::backoff::full_jitter;
use ks_utility::http_date::parse_retry_after;
use reqwest::header::{HeaderMap, RETRY_AFTER};

use crate::error::EngineError;

/// How often one range is tried again before the download fails.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Attempts in a row that bring no new bytes before the range gives up (at least 1).
    /// An attempt that brings bytes resets the count, so a long download over a flaky
    /// line keeps going as long as it makes progress.
    pub max_attempts: u32,
    /// Backoff ceiling for the first retry; doubles on each further one.
    pub base_delay: Duration,
    /// Backoff never grows past this.
    pub max_delay: Duration,
    /// A server asking (with `Retry-After`) for a longer wait than this ends the retries.
    pub max_retry_after: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 6,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
            max_retry_after: Duration::from_secs(600),
        }
    }
}

impl RetryPolicy {
    /// The wait before attempt `failures + 1`. Full jitter: a random time between zero and
    /// the backoff ceiling, so many connections that failed together do not retry together.
    /// The server's `Retry-After` is a floor. `None` when that floor is above
    /// `max_retry_after`.
    pub(crate) fn delay(&self, failures: u32, retry_after: Option<Duration>) -> Option<Duration> {
        if retry_after.is_some_and(|r| r > self.max_retry_after) {
            return None;
        }
        let jittered = full_jitter(failures, self.base_delay, self.max_delay);
        Some(jittered.max(retry_after.unwrap_or_default()))
    }
}

/// Statuses that mean "try again later" rather than "this will never work".
pub(crate) fn retryable_status(status: u16) -> bool {
    matches!(status, 408 | 429 | 500 | 502 | 503 | 504)
}

/// Network trouble and early-closed connections are retried; everything else is final.
pub(crate) fn retryable(error: &EngineError) -> bool {
    match error {
        EngineError::Network(_) | EngineError::Incomplete { .. } => true,
        EngineError::HttpStatus(s) => retryable_status(*s),
        _ => false,
    }
}

pub(crate) fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?;
    parse_retry_after(value, SystemTime::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_stays_under_the_cap_and_respects_retry_after() {
        let p = RetryPolicy {
            max_attempts: 5,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_millis(1000),
            max_retry_after: Duration::from_secs(10),
        };
        for _ in 0..200 {
            assert!(p.delay(1, None).unwrap() <= Duration::from_millis(100));
            assert!(p.delay(3, None).unwrap() <= Duration::from_millis(400));
            assert!(p.delay(30, None).unwrap() <= Duration::from_millis(1000));
            assert!(p.delay(1, Some(Duration::from_secs(2))).unwrap() >= Duration::from_secs(2));
        }
        assert_eq!(p.delay(1, Some(Duration::from_secs(11))), None, "too long a wait gives up");
        // Jitter really varies.
        let samples: std::collections::HashSet<_> = (0..50).map(|_| p.delay(4, None)).collect();
        assert!(samples.len() > 10);
    }

    #[test]
    fn what_is_retried() {
        assert!(retryable(&EngineError::Network("reset".into())));
        assert!(retryable(&EngineError::Incomplete { expected: 2, received: 1 }));
        assert!(retryable(&EngineError::HttpStatus(503)));
        assert!(retryable(&EngineError::HttpStatus(429)));
        assert!(!retryable(&EngineError::HttpStatus(404)));
        assert!(!retryable(&EngineError::RangeIgnored));
        assert!(!retryable(&EngineError::Cancelled));
    }
}
