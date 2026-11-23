//! "Update video component": replaces the bundled yt-dlp with the newest official
//! release, on the user's request only (no background checks: network access is only for
//! what the user starts). The bundled copy inside the signed app is never changed; the
//! new one goes to `<app data>/video-component/<version>/` and a `current` file names it.
//!
//! Not marked as downloaded from the internet (no quarantine attribute), deliberately:
//! macOS would refuse to run a marked program that we have not notarised. The trade-off is
//! recorded in specs/video-playlists.md ("Updates and 'downloaded from the internet'
//! marking"). Part 7's marking must keep skipping files written here.
//!
//! Integrity: the download is checked against the SHA-256 in the same release's
//! `SHA2-256SUMS`. A hash from the same place detects a damaged download, not a
//! deliberately altered release (specs/video-playlists.md, "Updating yt-dlp").
//! Hashing and unpacking use the system's own tools (`shasum`, `ditto`; `certutil` and
//! `tar` on Windows), so no hashing or zip crate is added.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use engine::{CancelToken, DownloadOptions};

use crate::msg;
use crate::msg::Msg;
use ks_utility::atomic::write_atomic;
use serde::Serialize;

use crate::video::Tools;

const LATEST: &str = "https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest";
#[cfg(not(target_os = "windows"))]
const ASSET: &str = "yt-dlp_macos.zip";
#[cfg(target_os = "windows")]
const ASSET: &str = "yt-dlp_win.zip";
#[cfg(not(target_os = "windows"))]
const EXE: &str = "yt-dlp_macos";
#[cfg(target_os = "windows")]
const EXE: &str = "yt-dlp.exe";

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "result")]
pub enum Update {
    UpToDate { version: String },
    Updated { from: String, to: String },
}

/// The updated yt-dlp named by `current`, if it is there.
pub fn current(dir: &Path) -> Option<PathBuf> {
    let name = std::fs::read_to_string(dir.join("current")).ok()?;
    let name = name.trim();
    // A version folder name only: digits and dots, nothing that leaves the folder.
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    let exe = dir.join(name).join(EXE);
    exe.is_file().then_some(exe)
}

/// Checks for a newer yt-dlp and installs it. Errors are short sentences for the user.
pub async fn update(tools: &Tools) -> Result<Update, Msg> {
    let installed = tools.version(&tools.ytdlp()).await.ok_or_else(|| msg!("update.noStart"))?;
    let release = engine::fetch_text(LATEST, 1 << 20).await.map_err(|e| msg!("update.check", detail = e))?;
    let tag = latest_tag(&release).ok_or_else(|| msg!("update.version"))?;
    if !newer(&tag, &installed) {
        return Ok(Update::UpToDate { version: installed });
    }
    let base = format!("https://github.com/yt-dlp/yt-dlp/releases/download/{tag}");
    let sums = engine::fetch_text(&format!("{base}/SHA2-256SUMS"), 1 << 16)
        .await
        .map_err(|e| msg!("update.checksums", detail = e))?;
    let expected = expected_hash(&sums, ASSET).ok_or_else(|| msg!("update.noChecksum"))?;

    let dir = &tools.component_dir;
    std::fs::create_dir_all(dir).map_err(|e| msg!("update.install", detail = e))?;
    let zip = dir.join(ASSET);
    let _ = engine::discard(&zip).await;
    let _ = std::fs::remove_file(&zip);
    let options = DownloadOptions { cancel: CancelToken::new(), connections: 4, ..DownloadOptions::default() };
    engine::download(&format!("{base}/{ASSET}"), &zip, options)
        .await
        .map_err(|e| msg!("update.download", detail = e))?;
    let got = sha256(&zip).await.ok_or_else(|| msg!("update.hash"))?;
    if got != expected {
        let _ = std::fs::remove_file(&zip);
        return Err(msg!("update.damaged"));
    }
    let unpack = dir.join(format!(".{tag}.unpacking"));
    let _ = std::fs::remove_dir_all(&unpack);
    unzip(&zip, &unpack).await?;
    let _ = std::fs::remove_file(&zip);
    let exe = unpack.join(EXE);
    if tools.version(&exe).await.as_deref() != Some(tag.as_str()) {
        let _ = std::fs::remove_dir_all(&unpack);
        return Err(msg!("update.newNoStart"));
    }
    let target = dir.join(&tag);
    let _ = std::fs::remove_dir_all(&target);
    std::fs::rename(&unpack, &target).map_err(|e| msg!("update.install", detail = e))?;
    write_atomic(&dir.join("current"), tag.as_bytes()).map_err(|e| msg!("update.install", detail = e))?;
    tools.set_ytdlp(target.join(EXE));
    // Older updates are no longer needed.
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if e.path().is_dir() && name != tag && name.chars().all(|c| c.is_ascii_digit() || c == '.') {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    Ok(Update::Updated { from: installed, to: tag })
}

fn latest_tag(json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let tag = v.get("tag_name")?.as_str()?.trim().to_owned();
    (!tag.is_empty() && tag.chars().all(|c| c.is_ascii_digit() || c == '.')).then_some(tag)
}

/// yt-dlp versions are dates (`2026.10.26`, sometimes with a fourth part), so comparing
/// the numbers in order is enough.
fn newer(candidate: &str, installed: &str) -> bool {
    let parts = |s: &str| s.split('.').map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    parts(candidate) > parts(installed)
}

fn expected_hash(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let (hash, name) = l.split_once(char::is_whitespace)?;
        (name.trim().trim_start_matches('*') == asset && hash.len() == 64).then(|| hash.to_ascii_lowercase())
    })
}

pub(crate) async fn sha256(file: &Path) -> Option<String> {
    #[cfg(not(target_os = "windows"))]
    let out = tokio::process::Command::new("/usr/bin/shasum").args(["-a", "256"]).arg(file).stdin(Stdio::null()).output().await.ok()?;
    #[cfg(target_os = "windows")]
    let out = crate::command::tokio("certutil").arg("-hashfile").arg(file).arg("SHA256").stdin(Stdio::null()).output().await.ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.split_whitespace().find(|w| w.len() == 64 && w.chars().all(|c| c.is_ascii_hexdigit())).map(str::to_ascii_lowercase)
}

async fn unzip(zip: &Path, into: &Path) -> Result<(), Msg> {
    #[cfg(not(target_os = "windows"))]
    let status = tokio::process::Command::new("/usr/bin/ditto").args(["-x", "-k"]).arg(zip).arg(into).status().await;
    #[cfg(target_os = "windows")]
    let status = {
        let _ = std::fs::create_dir_all(into);
        crate::command::tokio("tar").arg("-xf").arg(zip).arg("-C").arg(into).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().await
    };
    match status {
        Ok(s) if s.success() => Ok(()),
        _ => Err(msg!("update.unpack")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_number() {
        assert!(newer("2026.11.02", "2026.10.26"));
        assert!(newer("2026.10.26.1", "2026.10.26"));
        assert!(!newer("2026.10.26", "2026.10.26"));
        assert!(!newer("2026.9.30", "2026.10.1"));
    }

    #[test]
    fn checksums_are_found_by_exact_name() {
        let sums = "aaaa  yt-dlp\n".to_owned()
            + &"b".repeat(64) + "  yt-dlp_macos.zip\n" + &"c".repeat(64) + "  yt-dlp_macos\n";
        assert_eq!(expected_hash(&sums, "yt-dlp_macos.zip"), Some("b".repeat(64)));
        assert_eq!(expected_hash(&sums, "yt-dlp_macos"), Some("c".repeat(64)));
        assert_eq!(expected_hash(&sums, "yt-dlp"), None, "too short to be a SHA-256");
    }

    /// The system tools the update uses: hashing a known file, then unpacking a zip.
    #[tokio::test]
    async fn the_system_tools_hash_and_unpack() {
        let dir = std::env::temp_dir().join(format!("ks-component-tools-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("in")).unwrap();
        let file = dir.join("in").join(EXE);
        std::fs::write(&file, b"abc").unwrap();
        // SHA-256 of "abc" (FIPS 180-2 test vector).
        assert_eq!(sha256(&file).await.as_deref(), Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"));
        let zip = dir.join("t.zip");
        #[cfg(windows)]
        let made = std::process::Command::new("tar").arg("-a").arg("-cf").arg(&zip).arg("-C").arg(dir.join("in")).arg(EXE).status().unwrap();
        #[cfg(not(windows))]
        let made = std::process::Command::new("/usr/bin/ditto").args(["-c", "-k"]).arg(dir.join("in")).arg(&zip).status().unwrap();
        assert!(made.success());
        unzip(&zip, &dir.join("out")).await.unwrap();
        assert_eq!(std::fs::read(dir.join("out").join(EXE)).unwrap(), b"abc");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_a_plain_version_names_a_folder() {
        assert_eq!(latest_tag(r#"{"tag_name":"2026.11.02"}"#).as_deref(), Some("2026.11.02"));
        assert_eq!(latest_tag(r#"{"tag_name":"../../evil"}"#), None);
        let dir = std::env::temp_dir().join(format!("ks-component-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("2026.11.02")).unwrap();
        std::fs::write(dir.join("2026.11.02").join(EXE), b"x").unwrap();
        std::fs::write(dir.join("current"), "2026.11.02\n").unwrap();
        assert_eq!(current(&dir), Some(dir.join("2026.11.02").join(EXE)));
        std::fs::write(dir.join("current"), "../2026.11.02").unwrap();
        assert_eq!(current(&dir), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
