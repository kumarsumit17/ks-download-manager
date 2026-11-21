//! The download list on disk: `downloads.json` in the app data folder
//! (macOS: ~/Library/Application Support/com.ks.downloadmanager/, Windows:
//! %APPDATA%\com.ks.downloadmanager\). Written atomically: a temp file next to it is
//! written and synced, then renamed over the old one, so a crash leaves either the old
//! list or the new one, never half of one.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use ks_utility::atomic::write_atomic;
use serde::{Deserialize, Serialize};

use crate::files::Category;

pub const FILE_NAME: &str = "downloads.json";
/// Bumped when the format changes incompatibly.
const VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    /// Added with "Download later"; never started.
    Queued,
    Downloading,
    /// Stopped by the user; the partial file and its range map are on disk.
    Paused,
    Finished,
    Failed,
    /// Stopped by quitting or a crash without anything to resume from; starts over.
    Interrupted,
}

/// What does the fetching: our own engine for files, yt-dlp for video-site links.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    #[default]
    File,
    Video,
}

/// One download in the list.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub id: u64,
    /// Lists saved before video support have no kind: those are files.
    #[serde(default)]
    pub kind: Kind,
    pub url: String,
    pub name: String,
    /// Full path of the file. Chosen and checked in Rust; the interface only displays it.
    pub path: PathBuf,
    pub category: Category,
    pub status: Status,
    pub size: Option<u64>,
    pub received: u64,
    /// From the last probe: the server allows pausing and resuming this download.
    pub resumable: bool,
    pub error: Option<String>,
    /// What the interface can offer for the error, e.g. `videoOutdated` (an update button).
    #[serde(default)]
    pub error_code: Option<String>,
    /// A video's temp folder name beside the destination, when it is not the default
    /// `.ksdownload-video-<id>` (that one was there already and belonged to something else).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temp: Option<String>,
    /// Milliseconds since the Unix epoch.
    pub added_at: u64,
}

#[derive(Serialize, Deserialize)]
struct OnDisk {
    version: u32,
    /// The ID the next download gets. Kept so a deleted download's ID is never reused
    /// (its leftover files, if any, keep naming only it).
    #[serde(default)]
    next_id: Option<u64>,
    downloads: Vec<Record>,
}

/// The saved list and the next free ID (never below every ID in the list).
pub struct Saved {
    pub downloads: Vec<Record>,
    pub next_id: u64,
}

pub fn path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE_NAME)
}

/// The saved list. A missing file is an empty list; a damaged or newer one is set aside
/// as `downloads.json.bad` (so it is not overwritten) and the list starts empty.
#[cfg(test)]
pub fn load(file: &Path) -> Vec<Record> {
    load_all(file).downloads
}

pub fn load_all(file: &Path) -> Saved {
    let text = match fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) if e.kind() == ErrorKind::NotFound => return Saved { downloads: Vec::new(), next_id: 1 },
        Err(_) => return Saved { downloads: set_aside(file), next_id: 1 },
    };
    match serde_json::from_str::<OnDisk>(&text) {
        Ok(d) if d.version == VERSION => {
            let after_list = d.downloads.iter().map(|r| r.id).max().unwrap_or(0) + 1;
            Saved { next_id: d.next_id.unwrap_or(0).max(after_list), downloads: d.downloads }
        }
        _ => Saved { downloads: set_aside(file), next_id: 1 },
    }
}

fn set_aside(file: &Path) -> Vec<Record> {
    let _ = fs::rename(file, with_suffix(file, ".bad"));
    Vec::new()
}

/// Replaces the saved list atomically: write `downloads.json.tmp`, sync it, rename it
/// over `downloads.json`, then sync the folder so the rename itself is durable.
pub fn save(file: &Path, downloads: &[Record], next_id: u64) -> std::io::Result<()> {
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_vec_pretty(&OnDisk { version: VERSION, next_id: Some(next_id), downloads: downloads.to_vec() })
        .map_err(std::io::Error::other)?;
    // One writer: every save runs under the manager's lock.
    write_atomic(file, &json)
}

fn with_suffix(p: &Path, suffix: &str) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        std::env::temp_dir().join(format!("ks-store-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)))
    }

    pub fn record(id: u64) -> Record {
        Record {
            id,
            kind: Kind::File,
            error_code: None,
            temp: None,
            url: format!("https://example.com/{id}.zip"),
            name: format!("{id}.zip"),
            path: PathBuf::from(format!("/tmp/{id}.zip")),
            category: Category::Other,
            status: Status::Paused,
            size: Some(1000),
            received: 400,
            resumable: true,
            error: None,
            added_at: 1_790_000_000_000,
        }
    }

    #[test]
    fn round_trip() {
        let dir = temp_dir();
        let file = path(&dir);
        assert!(load(&file).is_empty(), "no file yet");
        let list = vec![record(1), Record { status: Status::Failed, error: Some("x".into()), size: None, ..record(2) }];
        save(&file, &list, 3).unwrap();
        assert_eq!(load(&file), list);
        assert!(!with_suffix(&file, ".tmp").exists(), "temp file renamed away");
        save(&file, &list[..1], 3).unwrap();
        assert_eq!(load(&file), list[..1]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_next_id_survives_deleting_the_newest_download() {
        let dir = temp_dir();
        let file = path(&dir);
        save(&file, &[record(1)], 6).unwrap();
        assert_eq!(load_all(&file).next_id, 6, "IDs 2 to 5 were used and deleted");
        save(&file, &[record(9)], 2).unwrap();
        assert_eq!(load_all(&file).next_id, 10, "never below an ID in the list");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn lists_from_before_video_support_still_load() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let file = path(&dir);
        let old = r#"{"version":1,"downloads":[{"id":1,"url":"https://e.x/a","name":"a","path":"/tmp/a",
            "category":"other","status":"paused","size":10,"received":1,"resumable":true,"error":null,"addedAt":0}]}"#;
        fs::write(&file, old).unwrap();
        let list = load(&file);
        assert_eq!((list.len(), list[0].kind, list[0].error_code.clone()), (1, Kind::File, None));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_damaged_file_is_set_aside_not_overwritten() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let file = path(&dir);
        fs::write(&file, b"{ not json").unwrap();
        assert!(load(&file).is_empty());
        assert_eq!(fs::read(with_suffix(&file, ".bad")).unwrap(), b"{ not json");
        assert!(!file.exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
