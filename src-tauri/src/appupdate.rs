//! App updates (specs/ui.md, "App updates"): asks GitHub for the newest release of this
//! app, and when it is newer than the running copy, downloads that release's installer for
//! this system, checks it against the SHA-256 GitHub lists for the file, and opens it.
//!
//! When it runs: when the user clicks "Check for updates", and once a day after start-up
//! while "Check for updates automatically" is on (the default). That daily request is the
//! one exception to "network access only for what the user starts"; the switch turns it
//! off. Nothing is sent but the request for the release itself.
//!
//! Installing is the user's own step: on macOS the disk image opens and the user drags the
//! app into Applications; on Windows the installer starts and the app closes. No updater
//! framework and no new dependency: the download goes through our own engine.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use engine::{CancelToken, DownloadOptions};
use ks_utility::atomic::write_atomic;
use serde::{Deserialize, Serialize};

use crate::msg;
use crate::msg::Msg;

const LATEST: &str = "https://api.github.com/repos/kumarsumit17/ks-download-manager/releases/latest";
/// The daily check waits this long after the last one.
const DAY_SECS: u64 = 24 * 60 * 60;
/// Saved between runs: when the last check was, so restarts do not ask again.
const STATE_FILE: &str = "app-update.json";
/// A version built from source without a tag: never offered an update by itself.
pub const DEV_VERSION: &str = "0.0.0";

/// A newer release with an installer for this system.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Available {
    pub version: String,
    pub file: String,
    pub size: u64,
    #[serde(skip)]
    url: String,
    #[serde(skip)]
    sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "result")]
pub enum Check {
    UpToDate { version: String },
    Available { update: Available },
}

/// What the interface does once the installer is open.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Opened {
    /// Windows: the installer replaces the app, so the app closes now.
    pub quit: bool,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct State {
    last_check: u64,
}

pub struct Updater {
    current: String,
    dir: PathBuf,
    found: Mutex<Option<Available>>,
    busy: AtomicBool,
}

impl Updater {
    pub fn new(current: String, data_dir: &Path) -> Self {
        Self { current, dir: data_dir.to_path_buf(), found: Mutex::new(None), busy: AtomicBool::new(false) }
    }

    pub fn current(&self) -> &str {
        &self.current
    }

    /// The update found by the last check, if any.
    pub fn found(&self) -> Option<Available> {
        self.found.lock().ok()?.clone()
    }

    /// The version of the update found, for the Chrome extension's notice.
    pub fn found_version(&self) -> Option<String> {
        self.found().map(|a| a.version)
    }

    /// The daily check is due: a tagged build, and no check in the last day.
    pub fn due(&self) -> bool {
        if self.current == DEV_VERSION {
            return false;
        }
        let last = std::fs::read(self.dir.join(STATE_FILE))
            .ok()
            .and_then(|b| serde_json::from_slice::<State>(&b).ok())
            .map_or(0, |s| s.last_check);
        let now = now_secs();
        // A clock set back is treated as due, so a wrong date cannot stop checks for good.
        now < last || now - last >= DAY_SECS
    }

    pub async fn check(&self) -> Result<Check, Msg> {
        let json = engine::fetch_text(LATEST, 1 << 20).await.map_err(|e| msg!("appUpdate.check", detail = e))?;
        let _ = std::fs::create_dir_all(&self.dir);
        if let Ok(data) = serde_json::to_vec(&State { last_check: now_secs() }) {
            let _ = write_atomic(&self.dir.join(STATE_FILE), &data);
        }
        let release = parse(&json).ok_or_else(|| msg!("appUpdate.version"))?;
        if !newer(&release.version, &self.current) {
            if let Ok(mut f) = self.found.lock() {
                *f = None;
            }
            return Ok(Check::UpToDate { version: self.current.clone() });
        }
        let update = release.installer.ok_or_else(|| msg!("appUpdate.noFile", version = release.version.clone()))?;
        if let Ok(mut f) = self.found.lock() {
            *f = Some(update.clone());
        }
        Ok(Check::Available { update })
    }

    /// Downloads the installer found by the last check, checks it, and opens it.
    pub async fn install(&self) -> Result<Opened, Msg> {
        let update = self.found().ok_or_else(|| msg!("appUpdate.none"))?;
        if self.busy.swap(true, Ordering::SeqCst) {
            return Err(msg!("appUpdate.busy"));
        }
        let result = self.fetch_and_open(&update).await;
        self.busy.store(false, Ordering::SeqCst);
        result
    }

    async fn fetch_and_open(&self, update: &Available) -> Result<Opened, Msg> {
        let dir = self.dir.join("app-update");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| msg!("appUpdate.download", detail = e))?;
        let file = dir.join(&update.file);
        let options = DownloadOptions { cancel: CancelToken::new(), connections: 4, ..DownloadOptions::default() };
        engine::download(&update.url, &file, options).await.map_err(|e| msg!("appUpdate.download", detail = e))?;
        let got = crate::component::sha256(&file).await.ok_or_else(|| msg!("appUpdate.hash"))?;
        if got != update.sha256 {
            let _ = std::fs::remove_file(&file);
            return Err(msg!("appUpdate.damaged"));
        }
        // Marked like any other download, so the system checks it as it would a browser download.
        if let Err(e) = crate::mark::mark_from_internet(&file, &update.url) {
            eprintln!("could not mark the update as downloaded: {e}");
        }
        crate::open::open_file(&file).map_err(|e| msg!("appUpdate.open", detail = e))?;
        Ok(Opened { quit: cfg!(target_os = "windows") })
    }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

struct Release {
    version: String,
    installer: Option<Available>,
}

/// The installer for the system this copy was built for, by the release's file names.
fn is_installer(name: &str) -> bool {
    if cfg!(target_os = "windows") {
        name.contains("-windows-x64-setup") && name.ends_with(".exe")
    } else if cfg!(target_arch = "aarch64") {
        name.ends_with("-macos-arm64.dmg")
    } else {
        name.ends_with("-macos-x64.dmg")
    }
}

fn parse(json: &str) -> Option<Release> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let tag = v.get("tag_name")?.as_str()?.trim();
    let version = tag.strip_prefix('v').unwrap_or(tag).to_owned();
    if version.is_empty() || !version.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    let installer = v.get("assets")?.as_array()?.iter().find_map(|a| {
        let name = a.get("name")?.as_str()?;
        if !is_installer(name) || name.contains(['/', '\\']) || name.starts_with('.') {
            return None;
        }
        let url = a.get("browser_download_url")?.as_str()?;
        // Only the release's own files on GitHub.
        if !url.starts_with("https://github.com/kumarsumit17/ks-download-manager/releases/download/") {
            return None;
        }
        let sha256 = a.get("digest")?.as_str()?.strip_prefix("sha256:")?.to_ascii_lowercase();
        if sha256.len() != 64 || !sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let size = a.get("size")?.as_u64().filter(|s| *s > 0)?;
        Some(Available { version: version.clone(), file: name.to_owned(), size, url: url.to_owned(), sha256 })
    });
    Some(Release { version, installer })
}

/// Compares `MAJOR.MINOR.PATCH` versions by number.
pub fn newer(candidate: &str, current: &str) -> bool {
    let parts = |s: &str| s.trim_start_matches('v').split('.').map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    parts(candidate) > parts(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, assets: &str) -> String {
        format!(r#"{{"tag_name":"{tag}","assets":[{assets}]}}"#)
    }

    fn asset(name: &str, digest: &str) -> String {
        format!(
            r#"{{"name":"{name}","size":1234,"digest":"{digest}","browser_download_url":"https://github.com/kumarsumit17/ks-download-manager/releases/download/v0.2.0/{name}"}}"#
        )
    }

    fn ours() -> &'static str {
        if cfg!(target_os = "windows") {
            "ks-download-manager-v0.2.0-windows-x64-setup-beta.exe"
        } else if cfg!(target_arch = "aarch64") {
            "ks-download-manager-v0.2.0-macos-arm64.dmg"
        } else {
            "ks-download-manager-v0.2.0-macos-x64.dmg"
        }
    }

    #[test]
    fn versions_compare_by_number() {
        assert!(newer("0.1.10", "0.1.9"));
        assert!(newer("v1.0.0", "0.9.9"));
        assert!(!newer("0.1.3", "0.1.3"));
        assert!(!newer("0.1.2", "0.1.3"));
        assert!(newer("0.1.3", DEV_VERSION));
    }

    #[test]
    fn the_installer_for_this_system_is_picked_with_its_checksum() {
        let hash = "a".repeat(64);
        let others = ["ks-download-manager-extension-v0.2.0.zip", "ks-download-manager-v0.2.0-macos-arm64.dmg", "ks-download-manager-v0.2.0-macos-x64.dmg", "ks-download-manager-v0.2.0-windows-x64-setup-beta.exe"];
        let assets: Vec<String> = others.iter().map(|n| asset(n, &format!("sha256:{hash}"))).collect();
        let r = parse(&release("v0.2.0", &assets.join(","))).unwrap();
        assert_eq!(r.version, "0.2.0");
        let a = r.installer.unwrap();
        assert_eq!((a.file.as_str(), a.size, a.sha256.as_str()), (ours(), 1234, hash.as_str()));
    }

    #[test]
    fn a_file_without_a_checksum_or_from_elsewhere_is_not_used() {
        let r = parse(&release("v0.2.0", &asset(ours(), "md5:abc"))).unwrap();
        assert!(r.installer.is_none(), "no SHA-256");
        let elsewhere = format!(r#"{{"name":"{}","size":1,"digest":"sha256:{}","browser_download_url":"https://example.com/{}"}}"#, ours(), "b".repeat(64), ours());
        assert!(parse(&release("v0.2.0", &elsewhere)).unwrap().installer.is_none(), "not a GitHub release file");
        assert!(parse(&release("../evil", "")).is_none(), "a tag that is not a version");
    }

    #[test]
    fn the_daily_check_waits_a_day_and_skips_development_builds() {
        let dir = std::env::temp_dir().join(format!("ks-appupdate-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!Updater::new(DEV_VERSION.into(), &dir).due(), "a build without a tag never checks by itself");
        let u = Updater::new("0.1.3".into(), &dir);
        assert!(u.due(), "never checked");
        std::fs::write(dir.join(STATE_FILE), format!(r#"{{"lastCheck":{}}}"#, now_secs() - 60)).unwrap();
        assert!(!u.due(), "checked a minute ago");
        std::fs::write(dir.join(STATE_FILE), format!(r#"{{"lastCheck":{}}}"#, now_secs() - DAY_SECS - 1)).unwrap();
        assert!(u.due(), "checked over a day ago");
        std::fs::write(dir.join(STATE_FILE), format!(r#"{{"lastCheck":{}}}"#, now_secs() + 3600)).unwrap();
        assert!(u.due(), "a last check in the future (clock set back) does not block checks");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
