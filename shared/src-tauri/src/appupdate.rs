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
//!
//! The decisions (versions, the daily schedule, reading the release, which file, the
//! checksum rule) are the suite's shared ones from `ks_utility::app_update`
//! (ks-utility `specs/app-update.md`); this file does the request, download and opening.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use engine::{CancelToken, DownloadOptions};
use ks_utility::app_update::{self, Outcome, Platform, Repo};
use ks_utility::atomic::write_atomic;
use serde::{Deserialize, Serialize};

use crate::msg;
use crate::msg::Msg;

const REPO: &str = "kumarsumit17/ks-download-manager";
/// Saved between runs: when the last check was, so restarts do not ask again.
const STATE_FILE: &str = "app-update.json";

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
        let last = std::fs::read(self.dir.join(STATE_FILE))
            .ok()
            .and_then(|b| serde_json::from_slice::<State>(&b).ok())
            .filter(|s| s.last_check > 0)
            .map(|s| UNIX_EPOCH + Duration::from_secs(s.last_check));
        app_update::check_due(true, &self.current, last, SystemTime::now())
    }

    pub async fn check(&self) -> Result<Check, Msg> {
        let repo = repo();
        let json = engine::fetch_text(&repo.latest_release_api_url(), app_update::MAX_RESPONSE_BYTES)
            .await
            .map_err(|e| msg!("common.appUpdate.checkFailed", detail = e))?;
        let _ = std::fs::create_dir_all(&self.dir);
        if let Ok(data) = serde_json::to_vec(&State { last_check: now_secs() }) {
            let _ = write_atomic(&self.dir.join(STATE_FILE), &data);
        }
        let release = app_update::parse_release(&json, &repo).ok_or_else(|| msg!("common.appUpdate.unreadable"))?;
        let asset = Platform::current().and_then(|p| app_update::pick_asset(&release.assets, p, &repo));
        let found = match app_update::decide(&release, &self.current, asset) {
            Outcome::UpToDate { .. } => None,
            Outcome::NoInstaller { version } => return Err(msg!("common.appUpdate.noInstaller", version = version)),
            Outcome::Available { version, asset } => {
                Some(Available { version, file: asset.name, size: asset.size, url: asset.url, sha256: asset.sha256 })
            }
        };
        if let Ok(mut f) = self.found.lock() {
            f.clone_from(&found);
        }
        Ok(match found {
            Some(update) => Check::Available { update },
            None => Check::UpToDate { version: self.current.clone() },
        })
    }

    /// Downloads the installer found by the last check, checks it, and opens it.
    pub async fn install(&self) -> Result<Opened, Msg> {
        let update = self.found().ok_or_else(|| msg!("common.appUpdate.none"))?;
        if self.busy.swap(true, Ordering::SeqCst) {
            return Err(msg!("common.appUpdate.busy"));
        }
        let result = self.fetch_and_open(&update).await;
        self.busy.store(false, Ordering::SeqCst);
        result
    }

    async fn fetch_and_open(&self, update: &Available) -> Result<Opened, Msg> {
        let dir = self.dir.join("app-update");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| msg!("common.appUpdate.downloadFailed", detail = e))?;
        let file = dir.join(&update.file);
        let options = DownloadOptions { cancel: CancelToken::new(), connections: 4, ..DownloadOptions::default() };
        engine::download(&update.url, &file, options).await.map_err(|e| msg!("common.appUpdate.downloadFailed", detail = e))?;
        let got = crate::component::sha256(&file).await.ok_or_else(|| msg!("appUpdate.hash"))?;
        if !ks_utility::sha256::sha256_matches(&got, &update.sha256) {
            let _ = std::fs::remove_file(&file);
            return Err(msg!("common.appUpdate.damaged"));
        }
        // Marked like any other download, so the system checks it as it would a browser download.
        if let Err(e) = crate::mark::mark_from_internet(&file, &update.url) {
            eprintln!("could not mark the update as downloaded: {e}");
        }
        crate::open::open_file(&file).map_err(|e| msg!("common.appUpdate.openFailed", detail = e))?;
        Ok(Opened { quit: cfg!(target_os = "windows") })
    }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn repo() -> Repo {
    Repo::parse(REPO).expect("the repository name is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAMES: [&str; 4] = [
        "ks-download-manager-extension-v0.2.0.zip",
        "ks-download-manager-v0.2.0-macos-arm64.dmg",
        "ks-download-manager-v0.2.0-macos-x64.dmg",
        "ks-download-manager-v0.2.0-windows-x64-setup-beta.exe",
    ];

    fn release_json() -> String {
        let assets: Vec<String> = NAMES
            .iter()
            .map(|n| {
                format!(
                    r#"{{"name":"{n}","size":1234,"digest":"sha256:{}","browser_download_url":"https://github.com/{REPO}/releases/download/v0.2.0/{n}"}}"#,
                    "a".repeat(64)
                )
            })
            .collect();
        format!(r#"{{"tag_name":"v0.2.0","assets":[{}]}}"#, assets.join(","))
    }

    #[test]
    fn our_release_files_match_the_shared_rules() {
        let release = app_update::parse_release(&release_json(), &repo()).unwrap();
        assert_eq!(release.version, "0.2.0");
        let pick = |p| app_update::pick_asset(&release.assets, p, &repo()).map(|a| a.name.as_str());
        assert_eq!(pick(Platform::MacosArm64), Some(NAMES[1]));
        assert_eq!(pick(Platform::MacosX64), Some(NAMES[2]));
        assert_eq!(pick(Platform::WindowsX64), Some(NAMES[3]));
        assert!(Platform::current().is_some(), "built for a platform we release for");
        assert_eq!(repo().latest_release_api_url(), format!("https://api.github.com/repos/{REPO}/releases/latest"));
    }

    #[test]
    fn the_daily_check_waits_a_day_and_skips_development_builds() {
        let day = app_update::CHECK_INTERVAL.as_secs();
        let dir = std::env::temp_dir().join(format!("ks-appupdate-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!Updater::new(app_update::DEV_VERSION.into(), &dir).due(), "a build without a tag never checks by itself");
        let u = Updater::new("0.1.3".into(), &dir);
        assert!(u.due(), "never checked");
        std::fs::write(dir.join(STATE_FILE), format!(r#"{{"lastCheck":{}}}"#, now_secs() - 60)).unwrap();
        assert!(!u.due(), "checked a minute ago");
        std::fs::write(dir.join(STATE_FILE), format!(r#"{{"lastCheck":{}}}"#, now_secs() - day - 1)).unwrap();
        assert!(u.due(), "checked over a day ago");
        std::fs::write(dir.join(STATE_FILE), format!(r#"{{"lastCheck":{}}}"#, now_secs() + 3600)).unwrap();
        assert!(u.due(), "a last check in the future (clock set back) does not block checks");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
