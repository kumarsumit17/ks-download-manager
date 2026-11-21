//! Per-range retry: which failures are worth another attempt, exponential backoff with
//! full jitter, and `Retry-After` (RFC 9110, section 10.2.3).

use std::hash::{BuildHasher, Hasher, RandomState};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
        let doubling = 2u32.saturating_pow(failures.saturating_sub(1));
        let ceiling = self.base_delay.saturating_mul(doubling).min(self.max_delay);
        let ceiling_ms = u64::try_from(ceiling.as_millis()).unwrap_or(u64::MAX);
        let jittered = Duration::from_millis(random_below(ceiling_ms.saturating_add(1)));
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

/// `Retry-After` is either a number of seconds or an HTTP-date.
fn parse_retry_after(value: &str, now: SystemTime) -> Option<Duration> {
    let value = value.trim();
    if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) {
        return Some(Duration::from_secs(value.parse().unwrap_or(u64::MAX)));
    }
    let at = UNIX_EPOCH + Duration::from_secs(parse_http_date(value)?);
    // A date in the past means "now".
    Some(at.duration_since(now).unwrap_or_default())
}

/// Seconds since 1970 for the three HTTP-date forms recipients must accept
/// (RFC 9110, section 5.6.7): IMF-fixdate, RFC 850 and asctime.
fn parse_http_date(value: &str) -> Option<u64> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    let (day, month, year, time) = match parts.as_slice() {
        // Sun, 06 Nov 1994 08:49:37 GMT
        [_, d, m, y, t, "GMT"] => (d.parse().ok()?, *m, y.parse().ok()?, *t),
        // Sunday, 06-Nov-94 08:49:37 GMT
        [_, dmy, t, "GMT"] => {
            let mut it = dmy.split('-');
            let (d, m, y) = (it.next()?, it.next()?, it.next()?);
            let y: i64 = y.parse().ok()?;
            let year = match y {
                0..=69 => 2000 + y,
                70..=99 => 1900 + y,
                y => y,
            };
            (d.parse().ok()?, m, year, *t)
        }
        // Sun Nov  6 08:49:37 1994
        [_, m, d, t, y] => (d.parse().ok()?, *m, y.parse().ok()?, *t),
        _ => return None,
    };
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let month = MONTHS.iter().position(|m| *m == month)? as i64 + 1;
    let mut hms = time.split(':').map(|p| p.parse::<i64>().ok());
    let (h, mi, s) = (hms.next()??, hms.next()??, hms.next()??);
    if hms.next().is_some() || !(1..=31).contains(&day) || h > 23 || mi > 59 || s > 60 || h < 0 || mi < 0 || s < 0 {
        return None;
    }
    let secs = days_from_civil(year, month, day) * 86_400 + h * 3600 + mi * 60 + s;
    u64::try_from(secs).ok()
}

/// Days since 1970-01-01 in the proleptic Gregorian calendar (H. Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12; // March = 0
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// A random number below `bound` (0 when `bound` is 0). Jitter needs no cryptographic
/// quality: std's randomly keyed hasher plus a counter is enough, and needs no crate.
fn random_below(bound: u64) -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    if bound == 0 {
        return 0;
    }
    let mut h = RandomState::new().build_hasher();
    h.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    h.write_u128(SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
    h.finish() % bound
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_dates_in_all_three_forms() {
        // The example date from RFC 9110.
        assert_eq!(parse_http_date("Sun, 06 Nov 1994 08:49:37 GMT"), Some(784_111_777));
        assert_eq!(parse_http_date("Sunday, 06-Nov-94 08:49:37 GMT"), Some(784_111_777));
        assert_eq!(parse_http_date("Sun Nov  6 08:49:37 1994"), Some(784_111_777));
        assert_eq!(parse_http_date("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
        assert_eq!(parse_http_date("Tue, 29 Feb 2028 12:00:00 GMT"), Some(1_835_438_400));
        assert_eq!(parse_http_date("Sun, 06 Nov 1994 08:49:37 PST"), None);
        assert_eq!(parse_http_date("Sun, 06 Foo 1994 08:49:37 GMT"), None);
        assert_eq!(parse_http_date("Sun, 06 Nov 1994 25:49:37 GMT"), None);
        assert_eq!(parse_http_date("soon"), None);
    }

    #[test]
    fn retry_after_seconds_and_dates() {
        let now = UNIX_EPOCH + Duration::from_secs(784_111_777);
        assert_eq!(parse_retry_after("120", now), Some(Duration::from_secs(120)));
        assert_eq!(parse_retry_after(" 0 ", now), Some(Duration::ZERO));
        assert_eq!(parse_retry_after("Sun, 06 Nov 1994 08:50:07 GMT", now), Some(Duration::from_secs(30)));
        assert_eq!(parse_retry_after("Sun, 06 Nov 1994 08:00:00 GMT", now), Some(Duration::ZERO), "past = now");
        assert_eq!(parse_retry_after("-5", now), None);
        assert_eq!(parse_retry_after("", now), None);
    }

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
