use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use tokio::sync::Notify;

/// Cancels a download. Clone it, pass one copy in `DownloadOptions`, keep the other.
#[derive(Clone, Default, Debug)]
pub struct CancelToken(Arc<CancelInner>);

#[derive(Default, Debug)]
struct CancelInner {
    flag: AtomicBool,
    notify: Notify,
}

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.flag.store(true, Ordering::SeqCst);
        self.0.notify.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.flag.load(Ordering::SeqCst)
    }

    /// Resolves once `cancel` has been called (immediately if it already was).
    pub async fn cancelled(&self) {
        loop {
            let notified = self.0.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }
}

const UNKNOWN: u64 = u64::MAX;

/// Live byte counters for one download, safe to poll from another task or thread.
/// Updated with one atomic add per network chunk; the caller reads a snapshot whenever it likes.
#[derive(Clone, Debug)]
pub struct Progress(Arc<ProgressInner>);

#[derive(Debug)]
struct ProgressInner {
    total: AtomicU64,
    /// Bytes already in the file when the download was resumed.
    base: AtomicU64,
    per_connection: RwLock<Arc<[AtomicU64]>>,
    /// Reads each connection's current range while a download runs; `None` otherwise.
    ranges: RwLock<Option<RangeSource>>,
}

/// The range one connection is working on: the half-open byte range `[start, end)`, of
/// which `[start, position)` has arrived. `end` can move down while the connection works,
/// when an idle connection takes over the upper part of the range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConnectionRange {
    pub start: u64,
    pub position: u64,
    pub end: u64,
}

/// Called only when a snapshot is taken (a few times a second), never per chunk.
#[derive(Clone)]
pub(crate) struct RangeSource(pub(crate) Arc<dyn Fn() -> Vec<Option<ConnectionRange>> + Send + Sync>);

impl std::fmt::Debug for RangeSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RangeSource")
    }
}

/// A point-in-time copy of [`Progress`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgressSnapshot {
    /// Total size in bytes, or `None` while unknown.
    pub total: Option<u64>,
    /// Bytes in the file so far: those kept from an earlier, resumed run plus those
    /// received over all connections in this one.
    pub received: u64,
    /// Bytes received per connection in this run (connections take new ranges as they go).
    pub per_connection: Vec<u64>,
    /// The range each connection is working on, in the same order as `per_connection`.
    /// `None` for a connection between ranges, finished or retired, and for every
    /// connection once the download has stopped. A single-connection download whose size
    /// is unknown reports `None` too.
    pub connections: Vec<Option<ConnectionRange>>,
}

impl Default for Progress {
    fn default() -> Self {
        Self(Arc::new(ProgressInner {
            total: AtomicU64::new(UNKNOWN),
            base: AtomicU64::new(0),
            per_connection: RwLock::new(Arc::from(Vec::new())),
            ranges: RwLock::new(None),
        }))
    }
}

impl Progress {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> ProgressSnapshot {
        let total = self.0.total.load(Ordering::Relaxed);
        let per: Vec<u64> = self.counters().iter().map(|c| c.load(Ordering::Relaxed)).collect();
        let source = self.0.ranges.read().unwrap_or_else(|e| e.into_inner()).clone();
        let mut connections = source.map(|s| (s.0)()).unwrap_or_default();
        connections.resize(per.len(), None);
        ProgressSnapshot {
            total: (total != UNKNOWN).then_some(total),
            received: self.0.base.load(Ordering::Relaxed) + per.iter().sum::<u64>(),
            per_connection: per,
            connections,
        }
    }

    pub(crate) fn reset(&self, total: Option<u64>, connections: usize) {
        self.0.total.store(total.unwrap_or(UNKNOWN), Ordering::Relaxed);
        self.0.base.store(0, Ordering::Relaxed);
        let fresh: Vec<AtomicU64> = (0..connections).map(|_| AtomicU64::new(0)).collect();
        // A poisoned lock only means a reader panicked; the counters are still valid.
        *self.0.per_connection.write().unwrap_or_else(|e| e.into_inner()) = Arc::from(fresh);
    }

    /// Installs (or with `None` removes) the reader for the connections' current ranges.
    pub(crate) fn set_ranges(&self, source: Option<RangeSource>) {
        *self.0.ranges.write().unwrap_or_else(|e| e.into_inner()) = source;
    }

    pub(crate) fn set_base(&self, resumed: u64) {
        self.0.base.store(resumed, Ordering::Relaxed);
    }

    pub(crate) fn set_total(&self, total: u64) {
        self.0.total.store(total, Ordering::Relaxed);
    }

    /// The counters for the current download; tasks hold this so a chunk is one atomic add.
    pub(crate) fn counters(&self) -> Arc<[AtomicU64]> {
        self.0.per_connection.read().unwrap_or_else(|e| e.into_inner()).clone()
    }
}
