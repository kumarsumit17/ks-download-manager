//! Ownership markers: a small file that says which download a temp folder (or an engine
//! `.ksdownload` file without a range map) belongs to. The app only ever continues in, or
//! removes, something whose marker names that download (specs/video-playlists.md, "Files";
//! specs/ui.md, "Leftover download files").

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use ks_utility::atomic::write_atomic;
use serde::{Deserialize, Serialize};

/// The app identifier written into every marker.
pub const APP: &str = "com.ks.downloadmanager";
/// The marker's name inside a video temp folder.
pub const IN_FOLDER: &str = "ks-owner.json";
/// Markers are tiny; anything larger is not ours.
const MAX_LEN: u64 = 4096;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Marker {
    pub app: String,
    pub id: u64,
    pub url: String,
    /// Milliseconds since the Unix epoch.
    pub created: u64,
}

impl Marker {
    pub fn new(id: u64, url: &str) -> Self {
        let created = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
        Self { app: APP.into(), id, url: url.into(), created }
    }

    /// True when this marker names download `id` of `url`.
    pub fn is_for(&self, id: u64, url: &str) -> bool {
        self.app == APP && self.id == id && self.url == url
    }
}

pub fn write(file: &Path, marker: &Marker) -> std::io::Result<()> {
    write_atomic(file, &serde_json::to_vec_pretty(marker).map_err(std::io::Error::other)?)
}

/// The marker in `file`, if it is a regular file (not a link) that holds one of ours.
pub fn read(file: &Path) -> Option<Marker> {
    let meta = std::fs::symlink_metadata(file).ok()?;
    if !meta.is_file() || meta.len() > MAX_LEN {
        return None;
    }
    let m: Marker = serde_json::from_slice(&std::fs::read(file).ok()?).ok()?;
    (m.app == APP).then_some(m)
}

/// The marker of a temp folder; `None` unless the folder is a real folder (not a link).
pub fn of_folder(dir: &Path) -> Option<Marker> {
    if !std::fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir()) {
        return None;
    }
    read(&dir.join(IN_FOLDER))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_round_trip_and_name_one_download() {
        let dir = std::env::temp_dir().join(format!("ks-marker-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let m = Marker::new(7, "https://a.example/v");
        write(&dir.join(IN_FOLDER), &m).unwrap();
        let back = of_folder(&dir).unwrap();
        assert!(back.is_for(7, "https://a.example/v"));
        assert!(!back.is_for(7, "https://b.example/v"));
        assert!(!back.is_for(8, "https://a.example/v"));
        std::fs::write(dir.join(IN_FOLDER), br#"{"app":"other.app","id":7,"url":"x","created":0}"#).unwrap();
        assert_eq!(of_folder(&dir), None, "another app's file is not ours");
        std::fs::write(dir.join(IN_FOLDER), b"not json").unwrap();
        assert_eq!(of_folder(&dir), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
