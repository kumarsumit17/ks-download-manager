//! The user's settings: `settings.json` in the app data folder, replaced atomically
//! (specs/ui.md, "Settings"). A missing file means the defaults; a damaged or newer one is
//! set aside as `settings.json.bad` and the defaults are used.

use std::path::{Path, PathBuf};

use ks_utility::atomic::write_atomic;
use serde::{Deserialize, Serialize};

use crate::files::Category;

pub const FILE_NAME: &str = "settings.json";
const VERSION: u32 = 1;

/// Limits on the values, also enforced when settings are loaded or changed.
pub const MAX_ACTIVE: usize = 20;
pub const MAX_TOTAL_CONNECTIONS: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Downloads running at once (videos included); the rest wait their turn.
    pub max_active: usize,
    /// Connections all downloads together may have open.
    pub total_connections: usize,
    /// Most connections one download may use (1 to 32); fewer are used while fewer work.
    pub per_download: usize,
    /// Bytes per second across all downloads; `None` for no limit.
    pub speed_limit: Option<u64>,
    /// Save folder per category; `None` for the system Downloads folder.
    pub folders: Folders,
    /// Video: prefer MP4 (H.264/AAC) when it has the same resolution as the best stream.
    pub prefer_mp4: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Folders {
    pub video: Option<String>,
    pub audio: Option<String>,
    pub images: Option<String>,
    pub other: Option<String>,
}

impl Folders {
    pub fn get(&self, c: Category) -> Option<&str> {
        match c {
            Category::Video => self.video.as_deref(),
            Category::Audio => self.audio.as_deref(),
            Category::Images => self.images.as_deref(),
            Category::Other => self.other.as_deref(),
        }
    }

    pub fn all(&self) -> impl Iterator<Item = &str> {
        [&self.video, &self.audio, &self.images, &self.other].into_iter().flatten().map(String::as_str)
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_active: 3,
            total_connections: 64,
            per_download: engine::DEFAULT_CONNECTIONS,
            speed_limit: None,
            folders: Folders::default(),
            prefer_mp4: false,
        }
    }
}

impl Settings {
    /// Brings every value into its range; an empty folder means "the default".
    pub fn checked(mut self) -> Self {
        self.max_active = self.max_active.clamp(1, MAX_ACTIVE);
        self.total_connections = self.total_connections.clamp(1, MAX_TOTAL_CONNECTIONS);
        self.per_download = self.per_download.clamp(1, engine::MAX_CONNECTIONS);
        self.speed_limit = self.speed_limit.filter(|l| *l > 0);
        for f in [&mut self.folders.video, &mut self.folders.audio, &mut self.folders.images, &mut self.folders.other] {
            if f.as_deref().is_some_and(|s| s.trim().is_empty()) {
                *f = None;
            }
        }
        self
    }
}

#[derive(Serialize, Deserialize)]
struct OnDisk {
    version: u32,
    settings: Settings,
}

#[cfg(test)]
pub fn path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE_NAME)
}

pub fn load(file: &Path) -> Settings {
    let text = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Settings::default(),
        Err(_) => return set_aside(file),
    };
    match serde_json::from_str::<OnDisk>(&text) {
        Ok(d) if d.version == VERSION => d.settings.checked(),
        _ => set_aside(file),
    }
}

fn set_aside(file: &Path) -> Settings {
    let mut bad = file.as_os_str().to_owned();
    bad.push(".bad");
    let _ = std::fs::rename(file, PathBuf::from(bad));
    Settings::default()
}

pub fn save(file: &Path, settings: &Settings) -> std::io::Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let data = serde_json::to_vec_pretty(&OnDisk { version: VERSION, settings: settings.clone() }).map_err(std::io::Error::other)?;
    write_atomic(file, &data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_damaged_files_are_set_aside() {
        let dir = std::env::temp_dir().join(format!("ks-settings-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = path(&dir);
        assert_eq!(load(&file), Settings::default(), "missing file: defaults");
        let s = Settings { max_active: 5, speed_limit: Some(1000), prefer_mp4: true, ..Settings::default() };
        save(&file, &s).unwrap();
        assert_eq!(load(&file), s);
        std::fs::write(&file, b"{oops").unwrap();
        assert_eq!(load(&file), Settings::default());
        assert!(dir.join("settings.json.bad").exists(), "the damaged file is kept aside");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn values_are_kept_in_range() {
        let s = Settings {
            max_active: 0,
            total_connections: 10_000,
            per_download: 99,
            speed_limit: Some(0),
            folders: Folders { video: Some("  ".into()), ..Folders::default() },
            prefer_mp4: false,
        }
        .checked();
        assert_eq!((s.max_active, s.total_connections, s.per_download, s.speed_limit), (1, MAX_TOTAL_CONNECTIONS, 32, None));
        assert_eq!(s.folders.video, None);
        // Older files without a field get its default.
        let partial: OnDisk = serde_json::from_str(r#"{"version":1,"settings":{"maxActive":2}}"#).unwrap();
        assert_eq!(partial.settings.per_download, engine::DEFAULT_CONNECTIONS);
    }
}
