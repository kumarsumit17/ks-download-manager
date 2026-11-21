//! Marks a downloaded file as coming from the internet, so the system checks it before it
//! is opened (Stage 5, Part 7): the quarantine attribute on macOS, the Mark of the Web
//! (`Zone.Identifier`) on Windows. Every download the user starts is marked, whether added
//! in the app or handed over by the Chrome extension: engine files, videos and playlist
//! entries all finish through the manager, which marks the file before it gets its final
//! name, so it is never visible there unmarked.
//!
//! **Not marked, on purpose:** the yt-dlp update (`component.rs`). A marked program that
//! is not notarised by us would be refused by macOS and the update would stop working
//! (specs/video-playlists.md, "Updates and 'downloaded from the internet' marking").
//!
//! No crate: macOS uses the system `xattr` tool, Windows writes the alternate data stream
//! with the standard library. A file system without extended attributes (FAT, exFAT,
//! some network shares) cannot carry the mark; the download still completes and the
//! failure is logged.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// The agent name the system shows ("downloaded by …").
const AGENT: &str = "KS Download Manager";

/// The quarantine value: flags, time (seconds since 1970, hex), agent, event ID.
/// `0081` = quarantined, downloaded by a non-browser app (the same flags `curl` users set
/// by hand); the empty event ID means no Launch Services history entry.
fn quarantine_value(now: SystemTime) -> String {
    let secs = now.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    format!("0081;{secs:08x};{AGENT};")
}

/// Marks `file`, downloaded from `url`. Errors are returned for logging only.
pub fn mark_from_internet(file: &Path, url: &str) -> std::io::Result<()> {
    mark(file, url)
}

#[cfg(target_os = "macos")]
fn mark(file: &Path, _url: &str) -> std::io::Result<()> {
    let status = std::process::Command::new("/usr/bin/xattr")
        .args(["-w", "com.apple.quarantine", &quarantine_value(SystemTime::now())])
        .arg(file)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    if status.success() { Ok(()) } else { Err(std::io::Error::other("xattr could not set the quarantine attribute")) }
}

/// Zone 3 is the Internet zone, written to the file's `Zone.Identifier` stream.
#[cfg(windows)]
fn mark(file: &Path, url: &str) -> std::io::Result<()> {
    let mut stream = file.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    std::fs::write(stream, zone_identifier(url))
}

#[cfg(not(any(target_os = "macos", windows)))]
fn mark(_file: &Path, _url: &str) -> std::io::Result<()> {
    Ok(())
}

/// The `Zone.Identifier` contents. Only an `http(s)` link is recorded, and never one with
/// a line break (it could add lines to the file).
#[cfg_attr(not(windows), allow(dead_code))]
fn zone_identifier(url: &str) -> String {
    let host = (url.starts_with("https://") || url.starts_with("http://")) && !url.contains(['\r', '\n']);
    let mut s = String::from("[ZoneTransfer]\r\nZoneId=3\r\n");
    if host {
        s.push_str(&format!("HostUrl={url}\r\n"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn the_quarantine_value_names_the_app_and_the_time() {
        let v = quarantine_value(UNIX_EPOCH + Duration::from_secs(0x6543_21ab));
        assert_eq!(v, "0081;654321ab;KS Download Manager;");
    }

    #[test]
    fn the_mark_of_the_web_is_the_internet_zone_and_never_takes_extra_lines() {
        assert_eq!(zone_identifier("https://a.example/x"), "[ZoneTransfer]\r\nZoneId=3\r\nHostUrl=https://a.example/x\r\n");
        assert_eq!(zone_identifier("https://a/x\r\nZoneId=0"), "[ZoneTransfer]\r\nZoneId=3\r\n");
        assert_eq!(zone_identifier("file:///etc"), "[ZoneTransfer]\r\nZoneId=3\r\n");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_marked_file_carries_the_quarantine_attribute() {
        let dir = std::env::temp_dir().join(format!("ks-mark-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.bin");
        std::fs::write(&f, b"x").unwrap();
        mark_from_internet(&f, "https://a.example/a.bin").unwrap();
        let out = std::process::Command::new("/usr/bin/xattr").args(["-p", "com.apple.quarantine"]).arg(&f).output().unwrap();
        let value = String::from_utf8_lossy(&out.stdout);
        assert!(value.starts_with("0081;") && value.contains(";KS Download Manager;"), "{value}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn a_marked_file_carries_the_mark_of_the_web() {
        let dir = std::env::temp_dir().join(format!("ks-mark-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.exe");
        std::fs::write(&f, b"x").unwrap();
        mark_from_internet(&f, "https://a.example/a.exe").unwrap();
        let mut stream = f.as_os_str().to_owned();
        stream.push(":Zone.Identifier");
        let mark = std::fs::read_to_string(&stream).unwrap();
        assert_eq!(mark, "[ZoneTransfer]\r\nZoneId=3\r\nHostUrl=https://a.example/a.exe\r\n");
        assert_eq!(std::fs::read(&f).unwrap(), b"x", "the file itself is unchanged");
        // Windows itself reads the stream: PowerShell lists it on the file.
        let out = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", "(Get-Item -LiteralPath $env:KSDM_FILE -Stream Zone.Identifier).Length"])
            .env("KSDM_FILE", &f)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), mark.len().to_string());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
