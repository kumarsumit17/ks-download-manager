//! Limits shared between downloads, set by the app (specs/engine.md, "Scheduler and
//! limits"): a budget of connections across all downloads, a speed limit across all
//! downloads, and the number of connections each host has worked well with this session.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::Notify;

/// How many connections all downloads together may have open. A connection holds one
/// [`Permit`] while it runs; a lower total takes effect as connections close.
#[derive(Clone, Debug)]
pub struct ConnectionBudget(Arc<BudgetInner>);

#[derive(Debug)]
struct BudgetInner {
    state: Mutex<(usize, usize)>, // (total, in use)
    freed: Notify,
}

/// One connection's share of the budget, returned when dropped.
#[derive(Debug)]
pub(crate) struct Permit(Arc<BudgetInner>);

impl Drop for Permit {
    fn drop(&mut self) {
        let mut s = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        s.1 = s.1.saturating_sub(1);
        drop(s);
        self.0.freed.notify_waiters();
    }
}

impl ConnectionBudget {
    pub fn new(total: usize) -> Self {
        Self(Arc::new(BudgetInner { state: Mutex::new((total.max(1), 0)), freed: Notify::new() }))
    }

    pub fn set(&self, total: usize) {
        self.0.state.lock().unwrap_or_else(|e| e.into_inner()).0 = total.max(1);
        self.0.freed.notify_waiters();
    }

    pub(crate) fn try_acquire(&self) -> Option<Permit> {
        let mut s = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        (s.1 < s.0).then(|| {
            s.1 += 1;
            Permit(self.0.clone())
        })
    }

    pub(crate) async fn acquire(&self) -> Permit {
        loop {
            let freed = self.0.freed.notified();
            if let Some(p) = self.try_acquire() {
                return p;
            }
            freed.await;
        }
    }
}

/// A speed limit in bytes per second across every download that shares it. `None` while
/// unlimited. A token bucket holding at most a quarter second of data, so bursts stay
/// short.
#[derive(Clone, Debug, Default)]
pub struct SpeedLimit(Arc<Mutex<Bucket>>);

#[derive(Debug, Default)]
struct Bucket {
    rate: Option<u64>,
    /// May go negative: the debt is paid by waiting.
    tokens: f64,
    last: Option<Instant>,
}

impl Bucket {
    /// How long to wait after taking `n` bytes at `now`.
    fn take(&mut self, n: u64, now: Instant) -> Duration {
        let Some(rate) = self.rate.filter(|r| *r > 0) else { return Duration::ZERO };
        let rate = rate as f64;
        let capacity = rate / 4.0;
        let elapsed = self.last.map_or(0.0, |l| now.duration_since(l).as_secs_f64());
        self.last = Some(now);
        self.tokens = (self.tokens + elapsed * rate).min(capacity) - n as f64;
        if self.tokens >= 0.0 { Duration::ZERO } else { Duration::from_secs_f64(-self.tokens / rate) }
    }
}

impl SpeedLimit {
    pub fn new(bytes_per_second: Option<u64>) -> Self {
        let s = Self::default();
        s.set(bytes_per_second);
        s
    }

    pub fn set(&self, bytes_per_second: Option<u64>) {
        let mut b = self.0.lock().unwrap_or_else(|e| e.into_inner());
        b.rate = bytes_per_second.filter(|r| *r > 0);
        b.tokens = 0.0;
        b.last = None;
    }

    pub fn get(&self) -> Option<u64> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).rate
    }

    /// Accounts for `n` bytes just received and waits as long as the limit needs.
    pub(crate) async fn take(&self, n: u64) {
        let wait = self.0.lock().unwrap_or_else(|e| e.into_inner()).take(n, Instant::now());
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }
}

/// The number of connections each host worked well with, remembered for this session so
/// the next download from it starts there instead of finding it again.
#[derive(Clone, Debug, Default)]
pub struct HostLimits(Arc<Mutex<HashMap<String, usize>>>);

impl HostLimits {
    pub fn get(&self, host: &str) -> Option<usize> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).get(host).copied()
    }

    pub fn set(&self, host: &str, connections: usize) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).insert(host.to_owned(), connections.max(1));
    }
}

/// Adaptive connection count (specs/engine.md, "Adaptive connections"): start with
/// `initial` (or what the host worked with before), add one while the speed keeps rising,
/// back off on 429, 503, resets and timeouts, and add connections back once the trouble
/// has stopped.
#[derive(Clone, Debug)]
pub struct Adaptive {
    /// Connections to start with when the host has no remembered count.
    pub initial: usize,
    pub hosts: HostLimits,
    /// How often the count is looked at.
    pub tick: Duration,
    /// How long without trouble before connections are added.
    pub calm: Duration,
}

/// Never give a connection less than this much of the file: a small file gets few
/// connections (a 9 KB image gets one).
pub const MIN_PER_CONNECTION: u64 = 512 * 1024;

impl Default for Adaptive {
    fn default() -> Self {
        Self { initial: 4, hosts: HostLimits::default(), tick: Duration::from_secs(1), calm: Duration::from_secs(5) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bucket_allows_a_short_burst_then_paces() {
        let mut b = Bucket { rate: Some(1000), ..Bucket::default() };
        let t0 = Instant::now();
        // Starts empty: the first 1000 bytes cost a second.
        assert_eq!(b.take(1000, t0), Duration::from_secs(1));
        // A second later the debt is paid; the bucket holds at most a quarter second.
        let t1 = t0 + Duration::from_secs(10);
        assert_eq!(b.take(250, t1), Duration::ZERO);
        assert!(b.take(500, t1) >= Duration::from_millis(499));
        let mut off = Bucket::default();
        assert_eq!(off.take(1 << 30, t0), Duration::ZERO, "no limit");
    }

    #[tokio::test]
    async fn the_budget_can_shrink_and_grow() {
        let b = ConnectionBudget::new(2);
        let p1 = b.try_acquire().unwrap();
        let _p2 = b.try_acquire().unwrap();
        assert!(b.try_acquire().is_none());
        b.set(3);
        let _p3 = b.try_acquire().unwrap();
        b.set(1);
        drop(p1);
        assert!(b.try_acquire().is_none(), "the lower total holds once permits come back");
        let waiter = tokio::spawn({
            let b = b.clone();
            async move { b.acquire().await }
        });
        drop((_p2, _p3));
        assert!(tokio::time::timeout(Duration::from_secs(1), waiter).await.is_ok(), "a freed permit wakes the waiter");
    }

    #[test]
    fn hosts_are_remembered() {
        let h = HostLimits::default();
        assert_eq!(h.get("a.example"), None);
        h.set("a.example", 0);
        assert_eq!(h.get("a.example"), Some(1));
    }
}
