//! The shared range table behind dynamic splitting (specs/engine.md, "Dynamic splitting").
//!
//! Each worker (one connection) owns at most one slot: a half-open byte range `[pos, end)`
//! still to fetch. A worker claims bytes from its slot before writing them, so `pos` only
//! moves forward under the table lock. A worker with nothing left takes a pending range,
//! or else steals the upper half of the slot expected to finish last, by lowering that
//! slot's `end`. Bytes below a slot's `pos` are never handed out again, so no two
//! connections ever write the same byte.
//!
//! The table also keeps the set of bytes actually written to the file, which is what the
//! range map on disk records.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::control::ConnectionRange;
use crate::download::Segment;

/// Never split off less than this; smaller requests cost more in round trips than they save.
pub(crate) const MIN_STEAL: u64 = 64 * 1024;
/// A slot's own speed is trusted only after it has run this long.
const MEASURE_AFTER: Duration = Duration::from_millis(100);

// Half-open byte ranges, the merging set of written bytes and `gaps` are generic: they
// live in ks-utility.
pub(crate) use ks_utility::ranges::{Range, gaps};
use ks_utility::ranges::RangeSet;

#[derive(Debug)]
struct Slot {
    /// Where the current assignment began (for speed and the fetched-segment log).
    start: u64,
    pos: u64,
    end: u64,
    since: Instant,
}

/// What a claim allows the worker to write.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Claim {
    pub offset: u64,
    pub len: u64,
    /// The slot is now fully claimed; the worker should stop reading this response.
    pub finished: bool,
}

#[derive(Debug)]
pub(crate) struct Table {
    slots: Vec<Option<Slot>>,
    /// Speed of each worker's previous range, used before its current one can be measured.
    worker_rate: Vec<Option<f64>>,
    pending: VecDeque<Range>,
    /// Bytes written to the file, merged and sorted.
    written: RangeSet,
    /// Every finished assignment, for `DownloadOutcome::segments`.
    fetched: Vec<Segment>,
    /// Workers that have not exited (or retired) yet.
    active: usize,
}

impl Table {
    pub fn new(workers: usize, pending: Vec<Range>, written: Vec<Range>) -> Self {
        Self {
            slots: (0..workers).map(|_| None).collect(),
            worker_rate: vec![None; workers],
            pending: pending.into_iter().filter(|r| r.end > r.start).collect(),
            written: written.into_iter().collect(),
            fetched: Vec::new(),
            active: workers,
        }
    }

    /// Ends worker `id`'s current assignment and gives it the next range: a pending one if
    /// any, else half of the slot with the most time left. `None`: nothing worth taking, and
    /// the worker exits (it no longer counts as active).
    pub fn next(&mut self, id: usize, now: Instant) -> Option<Range> {
        self.finish(id, now);
        let Some(range) = self.pending.pop_front().or_else(|| self.steal(id, now)) else {
            self.active = self.active.saturating_sub(1);
            return None;
        };
        self.slots[id] = Some(Slot { start: range.start, pos: range.start, end: range.end, since: now });
        Some(range)
    }

    /// Worker `id` gives up on its range after running out of retries. While other workers
    /// are still active, its unclaimed bytes go back to the pending queue for one of them
    /// and it retires (`true`). The last active worker cannot retire (`false`): nobody
    /// would be left to fetch the range, so the download fails. Under the table lock, so a
    /// worker exiting in `next` can never miss a range handed back here.
    pub fn retire(&mut self, id: usize, now: Instant) -> bool {
        if self.active <= 1 {
            return false;
        }
        self.finish(id, now);
        self.active -= 1;
        true
    }

    /// The part of worker `id`'s range not yet claimed.
    pub fn current(&self, id: usize) -> Option<Range> {
        self.slots.get(id)?.as_ref().map(|s| Range { start: s.pos, end: s.end })
    }

    /// Every worker's current assignment as `[start, end)` with `pos` claimed so far.
    pub fn assignments(&self) -> Vec<Option<ConnectionRange>> {
        self.slots
            .iter()
            .map(|s| s.as_ref().map(|s| ConnectionRange { start: s.start, position: s.pos, end: s.end }))
            .collect()
    }

    /// Claims up to `len` bytes at the front of worker `id`'s range.
    pub fn claim(&mut self, id: usize, len: u64) -> Option<Claim> {
        let slot = self.slots.get_mut(id)?.as_mut()?;
        let n = len.min(slot.end - slot.pos);
        let offset = slot.pos;
        slot.pos += n;
        Some(Claim { offset, len: n, finished: slot.pos == slot.end })
    }

    /// Records bytes that are now in the file.
    pub fn wrote(&mut self, r: Range) {
        self.written.insert(r);
    }

    pub fn written(&self) -> &[Range] {
        self.written.as_slice()
    }

    /// Finished assignments in file order.
    pub fn fetched(&self) -> Vec<Segment> {
        let mut out = self.fetched.clone();
        out.sort_by_key(|s| s.start);
        out
    }

    fn finish(&mut self, id: usize, now: Instant) {
        let Some(slot) = self.slots[id].take() else { return };
        let done = slot.pos - slot.start;
        if done > 0 {
            self.fetched.push(Segment { start: slot.start, end: slot.pos - 1, received: done });
            let secs = now.duration_since(slot.since).as_secs_f64();
            if secs > 0.0 {
                self.worker_rate[id] = Some(done as f64 / secs);
            }
        }
        if slot.pos < slot.end {
            // Only after a failure; someone else may finish it.
            self.pending.push_back(Range { start: slot.pos, end: slot.end });
        }
    }

    fn steal(&mut self, thief: usize, now: Instant) -> Option<Range> {
        let rates: Vec<Option<f64>> = (0..self.slots.len()).map(|i| self.measured_rate(i, now)).collect();
        let known: Vec<f64> = rates.iter().flatten().copied().collect();
        let average = (!known.is_empty()).then(|| known.iter().sum::<f64>() / known.len() as f64);

        let victim = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                let s = s.as_ref()?;
                let remaining = s.end - s.pos;
                if i == thief || remaining < 2 * MIN_STEAL {
                    return None;
                }
                // Time left at this connection's speed: its own once measurable, else its
                // previous range's, else the average. With no speed at all, by size.
                let rate = rates[i].or(self.worker_rate[i]).or(average).unwrap_or(1.0).max(1.0);
                Some((i, remaining as f64 / rate, remaining))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1).then(a.2.cmp(&b.2)))?
            .0;
        let slot = self.slots[victim].as_mut()?;
        let mid = slot.pos + (slot.end - slot.pos) / 2;
        let stolen = Range { start: mid, end: slot.end };
        slot.end = mid;
        Some(stolen)
    }

    fn measured_rate(&self, i: usize, now: Instant) -> Option<f64> {
        let s = self.slots[i].as_ref()?;
        let elapsed = now.duration_since(s.since);
        (elapsed >= MEASURE_AFTER && s.pos > s.start).then(|| (s.pos - s.start) as f64 / elapsed.as_secs_f64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: u64 = 1 << 20;

    #[test]
    fn claims_never_pass_the_end() {
        let mut t = Table::new(1, vec![Range { start: 0, end: 10 }], Vec::new());
        let now = Instant::now();
        assert_eq!(t.next(0, now), Some(Range { start: 0, end: 10 }));
        assert_eq!(t.claim(0, 4), Some(Claim { offset: 0, len: 4, finished: false }));
        assert_eq!(t.claim(0, 100), Some(Claim { offset: 4, len: 6, finished: true }));
        assert_eq!(t.claim(0, 1), Some(Claim { offset: 10, len: 0, finished: true }));
        assert_eq!(t.next(0, now), None, "nothing left, nothing to steal");
    }

    #[test]
    fn assignments_follow_claims_and_steals() {
        let now = Instant::now();
        let mut t = Table::new(3, vec![Range { start: 0, end: 4 * MB }], Vec::new());
        assert_eq!(t.assignments(), vec![None, None, None], "nobody has started");
        t.next(0, now);
        t.claim(0, MB);
        assert_eq!(t.assignments()[0], Some(ConnectionRange { start: 0, position: MB, end: 4 * MB }));
        // Worker 1 takes the upper half of what worker 0 has left; worker 0's end moves down.
        let stolen = t.next(1, now).expect("steals");
        let mid = MB + 3 * MB / 2;
        assert_eq!(stolen, Range { start: mid, end: 4 * MB });
        assert_eq!(
            t.assignments(),
            vec![
                Some(ConnectionRange { start: 0, position: MB, end: mid }),
                Some(ConnectionRange { start: mid, position: mid, end: 4 * MB }),
                None,
            ]
        );
        // A worker that finds nothing left has no assignment.
        t.claim(0, 4 * MB);
        t.claim(1, 4 * MB);
        assert_eq!(t.next(0, now), None);
        assert_eq!(t.assignments()[0], None);
    }

    #[test]
    fn idle_worker_steals_half_of_the_slowest_and_ranges_never_overlap() {
        let start = Instant::now();
        let mut t = Table::new(3, vec![Range { start: 0, end: 4 * MB }, Range { start: 4 * MB, end: 8 * MB }], Vec::new());
        t.next(0, start);
        t.next(1, start);
        // After a second: worker 0 fetched 3 MB (fast), worker 1 only 100 KB (slow).
        let later = start + Duration::from_secs(1);
        t.claim(0, 3 * MB);
        t.claim(1, 100 * 1024);
        let stolen = t.next(2, later).expect("steals");
        // Worker 1 has 3.9 MB at 100 KB/s left, worker 0 1 MB at 3 MB/s: worker 1 is the victim.
        let left = 4 * MB + 100 * 1024;
        let mid = left + (8 * MB - left) / 2;
        assert_eq!(stolen, Range { start: mid, end: 8 * MB });
        assert_eq!(t.current(1), Some(Range { start: left, end: mid }));
        // The victim's claims now stop at the new end.
        let c = t.claim(1, 8 * MB).unwrap();
        assert_eq!((c.offset, c.offset + c.len, c.finished), (left, mid, true));
    }

    #[test]
    fn small_ranges_are_not_split() {
        let now = Instant::now();
        let mut t = Table::new(2, vec![Range { start: 0, end: 2 * MIN_STEAL - 1 }], Vec::new());
        t.next(0, now);
        assert_eq!(t.next(1, now), None);
    }

    #[test]
    fn new_slot_is_judged_by_its_previous_speed() {
        // Worker 0 just took a fresh range after a fast one; worker 1 has been slow for a while.
        // The fresh slot must not look "infinitely slow" and be split instead.
        let t0 = Instant::now();
        let mut t = Table::new(3, vec![Range { start: 0, end: MB }, Range { start: MB, end: 3 * MB }], Vec::new());
        t.next(0, t0);
        t.next(1, t0);
        let t1 = t0 + Duration::from_secs(1);
        t.claim(0, MB);
        t.claim(1, 10 * 1024); // 10 KB/s
        let r = t.next(0, t1).expect("worker 0 steals from worker 1");
        assert!(r.start > MB);
        // Now worker 2 comes: worker 0's slot (fresh, 1 MB/s history) vs worker 1 (10 KB/s).
        let before_1 = t.current(1).unwrap();
        let before_0 = t.current(0).unwrap();
        t.next(2, t1).expect("steals");
        assert_eq!(t.current(0), Some(before_0), "the fast fresh slot is left alone");
        assert!(t.current(1).unwrap().end < before_1.end, "the slow one is split");
    }

    #[test]
    fn a_retired_worker_hands_its_range_to_the_others() {
        let now = Instant::now();
        let mut t = Table::new(2, vec![Range { start: 0, end: 100 }, Range { start: 100, end: 200 }], Vec::new());
        t.next(0, now);
        t.next(1, now);
        t.claim(0, 40);
        assert!(t.retire(0, now), "worker 1 is still active");
        assert_eq!(t.current(0), None);
        // Worker 1 finishes its own range, then picks up the rest of worker 0's.
        t.claim(1, 100);
        assert_eq!(t.next(1, now), Some(Range { start: 40, end: 100 }));
        assert!(!t.retire(1, now), "the last active worker cannot retire");
    }

    #[test]
    fn a_worker_that_exited_no_longer_counts_as_active() {
        let now = Instant::now();
        let mut t = Table::new(2, vec![Range { start: 0, end: 100 }], Vec::new());
        t.next(0, now);
        assert_eq!(t.next(1, now), None, "nothing for worker 1: it exits");
        assert!(!t.retire(0, now), "worker 0 is alone now");
    }
}
