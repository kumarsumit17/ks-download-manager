//! The macOS-only code of KS Download Manager ([per-platform plan](../../specs/plan-per-platform.md)).
//! The Tauri project in `shared/src-tauri` calls it through its `os` module; `desktop/windows`
//! has the same functions for Windows. No crate beyond `tokio`: every call here uses a
//! program that ships with macOS (`open`, `pbpaste`, `xattr`, `osascript`), with values passed
//! as arguments, never through a shell.
#![cfg(target_os = "macos")]

use std::ffi::OsStr;
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

// ---- Starting programs -------------------------------------------------------------------

/// A blocking command for `program` (plain on macOS; Windows hides the console window).
pub fn command_std(program: impl AsRef<OsStr>) -> Command {
    Command::new(program)
}

/// An async command for `program`.
pub fn command_tokio(program: impl AsRef<OsStr>) -> tokio::process::Command {
    tokio::process::Command::new(program)
}

// ---- Open a file, or show it in Finder ---------------------------------------------------

/// `open <path>` opens the file with its app; `open -R <path>` shows it selected in Finder.
pub fn opener(path: &Path, select: bool) -> Command {
    let mut cmd = Command::new("/usr/bin/open");
    if select {
        cmd.arg("-R");
    }
    // "--" ends the options, so a name starting with "-" is still a path.
    cmd.arg("--").arg(path);
    cmd
}

// ---- Clipboard ---------------------------------------------------------------------------

/// The clipboard's text, through the system `pbpaste`.
pub async fn read_clipboard() -> Option<String> {
    let out = tokio::process::Command::new("/usr/bin/pbpaste")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

// ---- Mark a downloaded file as coming from the internet ----------------------------------

/// The agent name macOS shows ("downloaded by …").
const AGENT: &str = "KS Download Manager";

/// The quarantine value: flags, time (seconds since 1970, hex), agent, event ID.
/// `0081` = quarantined, downloaded by a non-browser app (the same flags `curl` users set
/// by hand); the empty event ID means no Launch Services history entry.
fn quarantine_value(now: SystemTime) -> String {
    let secs = now.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    format!("0081;{secs:08x};{AGENT};")
}

/// Sets the quarantine attribute, so Gatekeeper checks the file before it is opened. A file
/// system without extended attributes (FAT, exFAT, some shares) cannot carry it.
pub fn mark(file: &Path, _url: &str) -> io::Result<()> {
    let status = Command::new("/usr/bin/xattr")
        .args(["-w", "com.apple.quarantine", &quarantine_value(SystemTime::now())])
        .arg(file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() { Ok(()) } else { Err(io::Error::other("xattr could not set the quarantine attribute")) }
}

// ---- Folder picker -----------------------------------------------------------------------

/// The AppleScript never changes; the prompt and the starting folder are passed as `argv`
/// and never become part of the script.
const SCRIPT: &[&str] = &[
    "on run argv",
    "set p to item 1 of argv",
    "tell current application to activate",
    "try",
    "if (count of argv) > 1 then",
    "set f to choose folder with prompt p default location (POSIX file (item 2 of argv))",
    "else",
    "set f to choose folder with prompt p",
    "end if",
    "return POSIX path of f",
    "on error number n",
    "if n is -128 then return \"\"",
    "error number n",
    "end try",
    "end run",
];

/// Shows the system's folder picker. `Ok` holds the picker's raw answer (empty when the
/// user cancels); `Err` a short detail for the error message.
pub async fn pick_folder(prompt: &str, start: Option<&Path>) -> Result<String, String> {
    let mut c = tokio::process::Command::new("/usr/bin/osascript");
    for line in SCRIPT {
        c.arg("-e").arg(line);
    }
    // A prompt starting with "-" would read as an option; ours never does, but make sure.
    c.arg(prompt.trim_start_matches('-'));
    if let Some(dir) = start.filter(|d| d.is_dir()) {
        c.arg(dir);
    }
    let out = c.output().await.map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("-".into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
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
    fn a_marked_file_carries_the_quarantine_attribute() {
        let dir = std::env::temp_dir().join(format!("ks-mark-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.bin");
        std::fs::write(&f, b"x").unwrap();
        mark(&f, "https://a.example/a.bin").unwrap();
        let out = Command::new("/usr/bin/xattr").args(["-p", "com.apple.quarantine"]).arg(&f).output().unwrap();
        let value = String::from_utf8_lossy(&out.stdout);
        assert!(value.starts_with("0081;") && value.contains(";KS Download Manager;"), "{value}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The picker's script must compile; this does not show a dialog.
    #[test]
    fn the_picker_script_compiles() {
        let dir = std::env::temp_dir().join(format!("ks-picker-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("p.scpt");
        let mut c = Command::new("/usr/bin/osacompile");
        for line in SCRIPT {
            c.arg("-e").arg(line);
        }
        let status = c.arg("-o").arg(&out).status().unwrap();
        assert!(status.success());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn finder_selects_the_file_and_a_dash_name_stays_a_path() {
        let cmd = opener(Path::new("/tmp/-x.zip"), true);
        assert_eq!(cmd.get_program(), "/usr/bin/open");
        let args: Vec<_> = cmd.get_args().collect();
        assert_eq!(args, ["-R", "--", "/tmp/-x.zip"]);
    }
}
