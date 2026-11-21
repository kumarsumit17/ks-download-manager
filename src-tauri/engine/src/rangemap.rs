//! The crash-safe range map: a small text file next to the download, `<file>.ksdm`,
//! listing which bytes are already in the file.
//!
//! ```text
//! ks-download-manager range map 1
//! url https://example.com/file.bin
//! size 1048576
//! validator etag "abc"
//! done 0 65536
//! done 131072 262144
//! end
//! ```
//!
//! `done a b` is the half-open byte range `[a, b)`. `validator` is optional
//! (`etag <value>` or `last-modified <value>`). The `end` line proves the file was written
//! completely. A map is replaced by writing `<file>.ksdm.tmp`, syncing it and renaming it
//! over the old one, so a crash leaves either the old map or the new one, never a mix.
//! The engine only lists bytes that were synced to the download file before the map was
//! written, so the map never claims bytes that are not on disk.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use ks_utility::atomic::{temp_path, write_atomic};

use crate::probe::Validator;
use crate::ranges::Range;

const HEADER: &str = "ks-download-manager range map 1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RangeMap {
    /// The URL the caller asked for (before redirects).
    pub url: String,
    pub size: u64,
    pub validator: Option<Validator>,
    /// Sorted, non-overlapping, inside `[0, size)`.
    pub done: Vec<Range>,
}

pub(crate) fn path(destination: &Path) -> PathBuf {
    with_suffix(destination, ".ksdm")
}

fn with_suffix(p: &Path, suffix: &str) -> PathBuf {
    let mut s = OsString::from(p.as_os_str());
    s.push(suffix);
    PathBuf::from(s)
}

impl RangeMap {
    /// `None` when a value has a line break (it could not be read back).
    fn encode(&self) -> Option<String> {
        let one_line = |s: &str| !s.contains(['\n', '\r']);
        let mut out = format!("{HEADER}\nurl {}\nsize {}\n", self.url, self.size);
        if !one_line(&self.url) {
            return None;
        }
        match &self.validator {
            Some(Validator::ETag(v)) if one_line(v) => out.push_str(&format!("validator etag {v}\n")),
            Some(Validator::LastModified(v)) if one_line(v) => out.push_str(&format!("validator last-modified {v}\n")),
            Some(_) => return None,
            None => {}
        }
        for d in &self.done {
            out.push_str(&format!("done {} {}\n", d.start, d.end));
        }
        out.push_str("end\n");
        Some(out)
    }

    /// Strict: anything unexpected makes the whole map invalid (the download then starts fresh).
    fn decode(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        if lines.next()? != HEADER {
            return None;
        }
        let url = lines.next()?.strip_prefix("url ")?.to_owned();
        let size: u64 = lines.next()?.strip_prefix("size ")?.parse().ok()?;
        let mut validator = None;
        let mut done: Vec<Range> = Vec::new();
        let mut ended = false;
        for line in lines {
            if ended {
                return None;
            }
            if line == "end" {
                ended = true;
            } else if let Some(v) = line.strip_prefix("validator ") {
                if validator.is_some() || !done.is_empty() {
                    return None;
                }
                validator = Some(if let Some(e) = v.strip_prefix("etag ") {
                    Validator::ETag(e.to_owned())
                } else {
                    Validator::LastModified(v.strip_prefix("last-modified ")?.to_owned())
                });
            } else {
                let (a, b) = line.strip_prefix("done ")?.split_once(' ')?;
                let r = Range { start: a.parse().ok()?, end: b.parse().ok()? };
                let after_previous = done.last().is_none_or(|p| r.start > p.end);
                if r.start >= r.end || r.end > size || !after_previous {
                    return None;
                }
                done.push(r);
            }
        }
        ended.then_some(Self { url, size, validator, done })
    }

    pub fn done_bytes(&self) -> u64 {
        self.done.iter().map(|r| r.len()).sum()
    }
}

/// The saved map for `destination`, or `None` if there is none or it cannot be trusted.
pub(crate) async fn load(destination: &Path) -> Option<RangeMap> {
    let text = tokio::fs::read_to_string(path(destination)).await.ok()?;
    RangeMap::decode(&text)
}

/// Serialises every write and removal of a range map. `write_atomic` allows one writer
/// per path, and a write runs on a blocking thread that keeps going even when the
/// download that started it is dropped or aborted mid-checkpoint (a crash, a cancelled
/// task). Without this lock that orphaned write could collide with the next download's
/// write of the same map.
///
/// The lock serialises map writes but does not order them: an orphaned write that takes
/// the lock after a fresh start's `remove` or `save` still lands, and can bring back a
/// stale map. So callers must stop a download with its cancel token (`DownloadOptions::cancel`),
/// which lets every checkpoint, including the last one, finish before the download returns,
/// and must never drop or abort the download future mid-checkpoint.
/// Writes are a few hundred bytes every 2 s per download, so one lock for all is enough.
static MAP_IO: Mutex<()> = Mutex::new(());

fn map_io() -> MutexGuard<'static, ()> {
    // A poisoned lock only means a writer panicked; the files are still consistent.
    MAP_IO.lock().unwrap_or_else(|e| e.into_inner())
}

/// Replaces the map atomically (write a temp file, sync, rename, sync the folder).
pub(crate) async fn save(destination: &Path, map: &RangeMap) -> std::io::Result<()> {
    let text = map
        .encode()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "range map value has a line break"))?;
    // Covers the map only; the caller syncs the download file before saving it.
    let final_path = path(destination);
    tokio::task::spawn_blocking(move || {
        let _one_at_a_time = map_io();
        write_atomic(&final_path, text.as_bytes())
    })
    .await
    .map_err(std::io::Error::other)?
}

/// Removes the map (and a stray temp file); a missing file is not an error. Waits for any
/// write still in progress, so a removed map stays removed.
pub(crate) async fn remove(destination: &Path) -> std::io::Result<()> {
    let p = path(destination);
    tokio::task::spawn_blocking(move || {
        let _one_at_a_time = map_io();
        for f in [temp_path(&p), p] {
            match std::fs::remove_file(&f) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        Ok(())
    })
    .await
    .map_err(std::io::Error::other)?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> RangeMap {
        RangeMap {
            url: "https://example.com/a b.bin?x=1".into(),
            size: 1000,
            validator: Some(Validator::ETag("\"abc\"".into())),
            done: vec![Range { start: 0, end: 100 }, Range { start: 500, end: 1000 }],
        }
    }

    #[test]
    fn round_trip() {
        let m = sample();
        assert_eq!(RangeMap::decode(&m.encode().unwrap()), Some(m.clone()));
        let lm = RangeMap { validator: Some(Validator::LastModified("Sun, 06 Nov 1994 08:49:37 GMT".into())), ..m.clone() };
        assert_eq!(RangeMap::decode(&lm.encode().unwrap()), Some(lm));
        let none = RangeMap { validator: None, done: vec![], ..m };
        assert_eq!(RangeMap::decode(&none.encode().unwrap()), Some(none));
    }

    #[test]
    fn damaged_maps_are_rejected() {
        let good = sample().encode().unwrap();
        let truncated = &good[..good.len() - 4]; // no "end"
        assert_eq!(RangeMap::decode(truncated), None);
        assert_eq!(RangeMap::decode(""), None);
        assert_eq!(RangeMap::decode("garbage\n"), None);
        for bad in [
            good.replace("done 500 1000", "done 500 1001"), // past the end
            good.replace("done 500 1000", "done 50 1000"),  // overlaps
            good.replace("done 500 1000", "done 500 500"),  // empty
            good.replace("size 1000", "size x"),
            good.replace("end\n", "end\nmore\n"),
            good.replace("validator etag", "validator crc"),
        ] {
            assert_eq!(RangeMap::decode(&bad), None, "{bad}");
        }
    }

    #[test]
    fn line_breaks_cannot_be_encoded() {
        let m = RangeMap { validator: Some(Validator::ETag("a\nb".into())), ..sample() };
        assert_eq!(m.encode(), None);
    }

    #[test]
    fn map_path_appends_a_suffix() {
        assert_eq!(path(Path::new("/x/file.tar.gz")), PathBuf::from("/x/file.tar.gz.ksdm"));
    }
}
