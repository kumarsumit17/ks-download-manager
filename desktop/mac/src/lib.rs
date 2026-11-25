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

/// Sets the quarantine attribute, so Gatekeeper checks the file before it is opened, and
/// records the link it came from. A file system without extended attributes (FAT, exFAT,
/// some shares) cannot carry either.
pub fn mark(file: &Path, url: &str) -> io::Result<()> {
    let status = Command::new("/usr/bin/xattr")
        .args(["-w", "com.apple.quarantine", &quarantine_value(SystemTime::now())])
        .arg(file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if !status.success() {
        return Err(io::Error::other("xattr could not set the quarantine attribute"));
    }
    if let Err(e) = set_where_from(file, url) {
        eprintln!("could not record where the file came from: {e}");
    }
    Ok(())
}

/// Records where the file came from (`kMDItemWhereFroms`), as Safari does: Finder's Get Info
/// and Spotlight show it. Best effort: a file system without extended attributes cannot
/// carry it. The value is a binary property list, made by the system's `plutil`.
fn set_where_from(file: &Path, url: &str) -> io::Result<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) || url.contains(['\r', '\n']) {
        return Ok(());
    }
    let escaped = url.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><array><string>{escaped}</string></array></plist>\n"
    );
    let mut child = Command::new("/usr/bin/plutil")
        .args(["-convert", "binary1", "-o", "-", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    {
        use std::io::Write;
        child.stdin.take().ok_or_else(|| io::Error::other("plutil stdin"))?.write_all(xml.as_bytes())?;
    }
    let out = child.wait_with_output()?;
    if !out.status.success() || out.stdout.is_empty() {
        return Err(io::Error::other("plutil could not make the property list"));
    }
    let hex: String = out.stdout.iter().map(|b| format!("{b:02x}")).collect();
    let status = Command::new("/usr/bin/xattr")
        .args(["-wx", "com.apple.metadata:kMDItemWhereFroms", &hex])
        .arg(file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() { Ok(()) } else { Err(io::Error::other("xattr could not set where the file came from")) }
}

// ---- A finished download -----------------------------------------------------------------

/// Called once a download has its final name: a file saved in `~/Downloads` makes the Dock's
/// Downloads stack bounce, as a browser download does (the `com.apple.DownloadFileFinished`
/// distributed notification, posted through `osascript`'s JavaScript bridge; the path is an
/// argument, never part of the script).
pub fn download_finished(file: &Path) {
    let Some(home) = std::env::var_os("HOME") else { return };
    if !file.starts_with(Path::new(&home).join("Downloads")) {
        return;
    }
    const SCRIPT: &str = "ObjC.import('Foundation'); function run(argv) { \
        $.NSDistributedNotificationCenter.defaultCenter.postNotificationNameObject('com.apple.DownloadFileFinished', argv[0]); }";
    let _ = Command::new("/usr/bin/osascript")
        .args(["-l", "JavaScript", "-e", SCRIPT])
        .arg(file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|mut c| std::thread::spawn(move || c.wait()));
}

// ---- Keep the Mac awake while downloads run ----------------------------------------------

/// The `caffeinate` process holding the "no idle sleep" assertion, while one runs.
static AWAKE: std::sync::Mutex<Option<std::process::Child>> = std::sync::Mutex::new(None);

/// On: the Mac does not go to idle sleep (the display still may). `caffeinate -i -w <pid>`
/// holds a power assertion and ends by itself if the app ends, even after a crash. Off:
/// the assertion is released. Calling it again with the same value does nothing.
pub fn set_keep_awake(on: bool) {
    let mut awake = AWAKE.lock().unwrap_or_else(|e| e.into_inner());
    let alive = awake.as_mut().is_some_and(|c| matches!(c.try_wait(), Ok(None)));
    if on && !alive {
        *awake = Command::new("/usr/bin/caffeinate")
            .args(["-i", "-w", &std::process::id().to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok();
    } else if !on && let Some(mut c) = awake.take() {
        let _ = c.kill();
        let _ = c.wait();
    }
}

/// Whether the assertion is held (for tests and diagnostics).
pub fn keeping_awake() -> bool {
    AWAKE.lock().unwrap_or_else(|e| e.into_inner()).as_mut().is_some_and(|c| matches!(c.try_wait(), Ok(None)))
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

    #[test]
    fn a_marked_file_says_where_it_came_from() {
        let dir = std::env::temp_dir().join(format!("ks-where-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.bin");
        std::fs::write(&f, b"x").unwrap();
        mark(&f, "https://a.example/a.bin?x=1&y=<2>").unwrap();
        let out = Command::new("/usr/bin/mdls").args(["-raw", "-name", "kMDItemWhereFroms"]).arg(&f).output().unwrap();
        let shown = String::from_utf8_lossy(&out.stdout);
        // Spotlight may not have indexed the temp folder; the attribute itself must be there.
        let raw = Command::new("/usr/bin/xattr").args(["-px", "com.apple.metadata:kMDItemWhereFroms"]).arg(&f).output().unwrap();
        assert!(raw.status.success() && !raw.stdout.is_empty(), "no where-from attribute ({shown})");
        let hex: String = String::from_utf8_lossy(&raw.stdout).split_whitespace().collect();
        let bytes: Vec<u8> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
        assert!(bytes.starts_with(b"bplist00"));
        let back = Command::new("/usr/bin/plutil").args(["-convert", "xml1", "-o", "-", "--", "-"]).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
        let mut back = back;
        {
            use std::io::Write;
            back.stdin.take().unwrap().write_all(&bytes).unwrap();
        }
        let xml = String::from_utf8(back.wait_with_output().unwrap().stdout).unwrap();
        assert!(xml.contains("<string>https://a.example/a.bin?x=1&amp;y=&lt;2&gt;</string>"), "{xml}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn keeping_awake_holds_and_releases_a_power_assertion() {
        set_keep_awake(true);
        set_keep_awake(true); // a second call starts no second process
        assert!(keeping_awake());
        let pid = AWAKE.lock().unwrap().as_ref().unwrap().id();
        let ps = Command::new("/bin/ps").args(["-o", "command=", "-p", &pid.to_string()]).output().unwrap();
        assert!(String::from_utf8_lossy(&ps.stdout).contains("caffeinate -i -w"), "{:?}", ps);
        set_keep_awake(false);
        assert!(!keeping_awake());
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
