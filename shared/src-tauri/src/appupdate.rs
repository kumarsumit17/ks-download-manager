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

/// The DEV build (`tauri.dev.conf.json`): no update checks, no production files.
const DEV: bool = cfg!(feature = "dev");

/// The app's name, as the downloader in the update's quarantine mark (macOS).
const APP_NAME: &str = "KS Download Manager";

const REPO: &str = "kumasumit/ks-download-manager";
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

/// What the interface does once the installer is open: the app then saves and quits, so the
/// new version can replace it (ks-utility's `app_update::install`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Opened {
    /// Windows: the installer is starting, the app quits now. Otherwise (macOS) the
    /// interface quits after ks-utility's `MAC_QUIT_AFTER_MS`, since Finder cannot replace a
    /// running app.
    pub quit: bool,
}

impl From<app_update::install::Opened> for Opened {
    fn from(opened: app_update::install::Opened) -> Self {
        match opened {
            app_update::install::Opened::Windows => Opened { quit: true },
            app_update::install::Opened::Mac => Opened { quit: false },
        }
    }
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
    /// The newest version of the Chrome extension (a plain or `chrome-v` release), from the
    /// last check, for the extension's own update notice.
    extension_latest: Mutex<Option<String>>,
    busy: AtomicBool,
}

impl Updater {
    pub fn new(current: String, data_dir: &Path) -> Self {
        Self { current, dir: data_dir.to_path_buf(), found: Mutex::new(None), extension_latest: Mutex::new(None), busy: AtomicBool::new(false) }
    }

    pub fn current(&self) -> &str {
        &self.current
    }

    /// The update found by the last check, if any.
    pub fn found(&self) -> Option<Available> {
        self.found.lock().ok()?.clone()
    }

    /// The extension's newest released version (its own `chrome-v` tags or a plain tag), for
    /// the extension's notice; the extension compares it with its own version.
    pub fn extension_latest(&self) -> Option<String> {
        self.extension_latest.lock().ok()?.clone()
    }

    /// The daily check is due: a tagged production build, and no check in the last day. A
    /// DEV build never checks: it would only find production versions.
    pub fn due(&self) -> bool {
        if DEV {
            return false;
        }
        let last = std::fs::read(self.dir.join(STATE_FILE))
            .ok()
            .and_then(|b| serde_json::from_slice::<State>(&b).ok())
            .filter(|s| s.last_check > 0)
            .map(|s| UNIX_EPOCH + Duration::from_secs(s.last_check));
        app_update::check_due(true, &self.current, last, SystemTime::now())
    }

    pub async fn check(&self) -> Result<Check, Msg> {
        // A DEV build installs next to the production app and is replaced by the next DEV
        // build by hand; it never offers a production file.
        if DEV {
            return Err(msg!("appUpdate.devBuild"));
        }
        let repo = repo();
        // The release list, not `releases/latest`: each platform has its own releases
        // (`mac-v…`, `windows-v…`, `chrome-v…`), and a plain `v…` is every platform's.
        let json = engine::fetch_text(&repo.releases_api_url(), app_update::MAX_RESPONSE_BYTES)
            .await
            .map_err(|e| msg!("common.appUpdate.checkFailed", detail = e))?;
        let _ = std::fs::create_dir_all(&self.dir);
        if let Ok(data) = serde_json::to_vec(&State { last_check: now_secs() }) {
            let _ = write_atomic(&self.dir.join(STATE_FILE), &data);
        }
        if let Ok(mut e) = self.extension_latest.lock() {
            *e = app_update::parse_releases(&json, &repo, Platform::Chrome).map(|t| t.release.version);
        }
        let platform = Platform::current().ok_or_else(|| msg!("common.appUpdate.unreadable"))?;
        let release = app_update::parse_releases(&json, &repo, platform)
            .ok_or_else(|| msg!("common.appUpdate.unreadable"))?
            .release;
        let asset = app_update::pick_asset(&release.assets, platform, &repo);
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
        // ks-utility marks it as downloaded from the internet (quarantine or Mark of the Web, so
        // the system checks it as it would a browser download) and opens it.
        let opened = tokio::task::spawn_blocking(move || app_update::install::open(&file, APP_NAME))
            .await
            .map_err(|e| msg!("common.appUpdate.openFailed", detail = e))?
            .map_err(|e| msg!("common.appUpdate.openFailed", detail = e))?;
        Ok(opened.into())
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

    #[test]
    fn the_app_quits_at_once_on_windows_and_after_a_moment_on_macos() {
        assert_eq!(Opened::from(app_update::install::Opened::Windows), Opened { quit: true });
        assert_eq!(Opened::from(app_update::install::Opened::Mac), Opened { quit: false });
        // The interface waits ks-utility's MAC_QUIT_AFTER_MS, the same as Rust's MAC_QUIT_AFTER.
        assert_eq!(app_update::install::MAC_QUIT_AFTER.as_millis(), 6000);
    }

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

    /// A release list with a plain release and one of each platform's own: each platform finds
    /// its newest release (its own or a plain one) and its own file, and the extension its version.
    #[test]
    fn each_platform_reads_its_own_releases_from_the_list() {
        let release = |tag: &str, files: &[&str]| -> String {
            let assets: Vec<String> = files
                .iter()
                .map(|n| format!(r#"{{"name":"{n}","size":9,"digest":"sha256:{}","browser_download_url":"https://github.com/{REPO}/releases/download/{tag}/{n}"}}"#, "b".repeat(64)))
                .collect();
            format!(r#"{{"tag_name":"{tag}","draft":false,"prerelease":false,"assets":[{}]}}"#, assets.join(","))
        };
        let list = format!(
            "[{},{},{},{}]",
            release("chrome-v0.2.3", &["ks-download-manager-extension-v0.2.3.zip"]),
            release("windows-v0.2.2", &["ks-download-manager-v0.2.2-windows-x64-setup-beta.exe"]),
            release("mac-v0.2.1", &["ks-download-manager-v0.2.1-macos-arm64.dmg", "ks-download-manager-v0.2.1-macos-x64.dmg"]),
            release("v0.2.0", &NAMES),
        );
        let newest = |p| app_update::parse_releases(&list, &repo(), p).map(|t| (t.release.version, t.tag));
        assert_eq!(newest(Platform::MacosArm64), Some(("0.2.1".into(), "mac-v0.2.1".into())));
        assert_eq!(newest(Platform::WindowsX64), Some(("0.2.2".into(), "windows-v0.2.2".into())));
        assert_eq!(newest(Platform::Chrome), Some(("0.2.3".into(), "chrome-v0.2.3".into())));
        let mac = app_update::parse_releases(&list, &repo(), Platform::MacosArm64).unwrap().release;
        let file = app_update::pick_asset(&mac.assets, Platform::MacosArm64, &repo()).map(|a| a.name.clone());
        assert_eq!(file.as_deref(), Some("ks-download-manager-v0.2.1-macos-arm64.dmg"));
        // Before any platform release, the plain one is every platform's.
        let plain = format!("[{}]", release("v0.2.0", &NAMES));
        assert_eq!(app_update::parse_releases(&plain, &repo(), Platform::WindowsX64).map(|t| t.release.version).as_deref(), Some("0.2.0"));
        assert_eq!(repo().releases_api_url(), format!("https://api.github.com/repos/{REPO}/releases?per_page={}", app_update::RELEASES_PER_PAGE));
    }

    /// The DEV build never checks, never offers a production file.
    #[cfg(feature = "dev")]
    #[test]
    fn a_dev_build_never_checks() {
        let dir = std::env::temp_dir().join(format!("ks-appupdate-dev-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let u = Updater::new("0.4.2".into(), &dir);
        assert!(!u.due());
        let err = tauri::async_runtime::block_on(u.check()).err().unwrap();
        assert_eq!(err, msg!("appUpdate.devBuild"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(not(feature = "dev"))]
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
