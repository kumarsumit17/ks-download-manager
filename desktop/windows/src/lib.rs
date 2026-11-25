//! The Windows-only code of KS Download Manager ([per-platform plan](../../specs/plan-per-platform.md)).
//! The Tauri project in `shared/src-tauri` calls it through its `os` module; `desktop/mac`
//! has the same functions for macOS. No crate beyond `tokio` and `windows-sys` (both already
//! in the workspace): Explorer, PowerShell and the file's alternate data stream, with values
//! passed as arguments or environment variables, never through `cmd.exe`.
#![cfg(windows)]

use std::ffi::OsStr;
use std::io;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

// ---- Starting programs -------------------------------------------------------------------

/// A blocking command for `program`. Every program the app starts gets `CREATE_NO_WINDOW`:
/// the app is a windowed program, so each console program it runs (PowerShell, `reg`,
/// `attrib`, `taskkill`, `certutil`, `tar`, yt-dlp) would otherwise flash a console window.
pub fn command_std(program: impl AsRef<OsStr>) -> Command {
    let mut c = Command::new(program);
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

/// An async command for `program`, also without a console window.
pub fn command_tokio(program: impl AsRef<OsStr>) -> tokio::process::Command {
    let mut c = tokio::process::Command::new(program);
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

// ---- Open a file, or show it in Explorer -------------------------------------------------

/// explorer.exe takes the path itself as its argument; no cmd.exe is involved. Explorer
/// returns exit code 1 even when it worked, so its exit code is never read.
pub fn opener(path: &Path, select: bool) -> Command {
    let mut cmd = Command::new("explorer.exe");
    if select {
        cmd.raw_arg(select_arg(path));
    } else {
        cmd.arg(path);
    }
    cmd
}

/// `/select,"<path>"`, written as is: Explorer reads its command line itself and does not
/// understand the usual quoting of a whole argument (`"/select,C:\a b\c.txt"`). A Windows
/// path cannot contain `"`, so the quotes always enclose the whole path.
fn select_arg(path: &Path) -> std::ffi::OsString {
    let mut arg = std::ffi::OsString::from("/select,\"");
    arg.push(path.as_os_str());
    arg.push("\"");
    arg
}

// ---- Clipboard ---------------------------------------------------------------------------

/// The clipboard's text through PowerShell's `Get-Clipboard`, written as UTF-8 (Windows
/// PowerShell 5.1 would otherwise use the console code page and spoil non-English links).
pub async fn read_clipboard() -> Option<String> {
    const SCRIPT: &str = "[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false; \
        [Console]::Out.Write((Get-Clipboard -Raw))";
    let out = command_tokio("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-STA", "-Command", SCRIPT])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim_start_matches('\u{feff}').to_owned())
}

// ---- Mark a downloaded file as coming from the internet ----------------------------------

/// Writes the Mark of the Web: zone 3 (Internet) in the file's `Zone.Identifier` stream, so
/// SmartScreen and Office check the file before it is opened.
pub fn mark(file: &Path, url: &str) -> io::Result<()> {
    let mut stream = file.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    std::fs::write(stream, zone_identifier(url))
}

/// The `Zone.Identifier` contents. Only an `http(s)` link is recorded, and never one with
/// a line break (it could add lines to the file).
fn zone_identifier(url: &str) -> String {
    let host = (url.starts_with("https://") || url.starts_with("http://")) && !url.contains(['\r', '\n']);
    let mut s = String::from("[ZoneTransfer]\r\nZoneId=3\r\n");
    if host {
        s.push_str(&format!("HostUrl={url}\r\n"));
    }
    s
}

// ---- A finished download -----------------------------------------------------------------

/// Called once a download has its final name. Nothing yet on Windows (the jump list's
/// recent files come with the Windows step of the per-platform plan).
pub fn download_finished(_file: &Path) {}

// ---- Keep the PC awake while downloads run -----------------------------------------------

/// Set while a thread holds `ES_SYSTEM_REQUIRED`; dropping it ends that thread.
static AWAKE: std::sync::Mutex<Option<std::sync::mpsc::Sender<()>>> = std::sync::Mutex::new(None);

/// On: Windows does not go to idle sleep (the display still may). The request belongs to
/// the thread that makes it, so one thread holds it until switched off. Calling it again
/// with the same value does nothing.
pub fn set_keep_awake(on: bool) {
    use windows_sys::Win32::System::Power::{ES_CONTINUOUS, ES_SYSTEM_REQUIRED, SetThreadExecutionState};
    let mut awake = AWAKE.lock().unwrap_or_else(|e| e.into_inner());
    if on && awake.is_none() {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        std::thread::spawn(move || {
            // SAFETY: plain Win32 call with documented flag values; no pointers involved.
            unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED) };
            // Returns when the sender is dropped (switched off) or the app ends.
            let _ = rx.recv();
            // SAFETY: as above; clears this thread's request.
            unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
        });
        *awake = Some(tx);
    } else if !on {
        awake.take();
    }
}

/// Whether the request is held (for tests and diagnostics).
pub fn keeping_awake() -> bool {
    AWAKE.lock().unwrap_or_else(|e| e.into_inner()).is_some()
}

// ---- Folder picker -----------------------------------------------------------------------

/// The PowerShell script. The values reach it only as environment variables. `answer` is
/// the line that shows the dialog; the test puts a line there that answers without one.
fn windows_script(answer: &str) -> String {
    [
        "[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false",
        "Add-Type -AssemblyName System.Windows.Forms",
        "$d = New-Object System.Windows.Forms.FolderBrowserDialog",
        "$d.Description = $env:KSDM_PROMPT",
        "if ($env:KSDM_START) { $d.SelectedPath = $env:KSDM_START }",
        answer,
    ]
    .join("; ")
}

const SHOW_DIALOG: &str = "if ($d.ShowDialog() -eq 'OK') { [Console]::Out.Write($d.SelectedPath) }";

async fn run_windows_script(script: &str, prompt: &str, start: Option<&Path>) -> Result<String, String> {
    let out = command_tokio("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-STA", "-Command", script])
        .env("KSDM_PROMPT", prompt)
        .env("KSDM_START", start.map(|p| p.as_os_str().to_owned()).unwrap_or_default())
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("-".into());
    }
    // A byte-order mark is never part of a path.
    Ok(String::from_utf8_lossy(&out.stdout).trim_start_matches('\u{feff}').to_owned())
}

/// Shows the system's folder dialog. `Ok` holds its raw answer (empty when the user
/// cancels); `Err` a short detail for the error message.
pub async fn pick_folder(prompt: &str, start: Option<&Path>) -> Result<String, String> {
    run_windows_script(&windows_script(SHOW_DIALOG), prompt, start).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explorer_selects_the_file_with_the_path_in_quotes() {
        let file = Path::new(r"C:\Users\a b\Downloads\वीडियो, 1.mp4");
        assert_eq!(select_arg(file), std::ffi::OsString::from(r#"/select,"C:\Users\a b\Downloads\वीडियो, 1.mp4""#));
        let cmd = opener(file, true);
        assert_eq!(cmd.get_program(), "explorer.exe");
        assert_eq!(cmd.get_args().collect::<Vec<_>>(), [select_arg(file).as_os_str()]);
        let cmd = opener(file, false);
        assert_eq!(cmd.get_args().collect::<Vec<_>>(), [file.as_os_str()]);
    }

    #[test]
    fn the_mark_of_the_web_is_the_internet_zone_and_never_takes_extra_lines() {
        assert_eq!(zone_identifier("https://a.example/x"), "[ZoneTransfer]\r\nZoneId=3\r\nHostUrl=https://a.example/x\r\n");
        assert_eq!(zone_identifier("https://a/x\r\nZoneId=0"), "[ZoneTransfer]\r\nZoneId=3\r\n");
        assert_eq!(zone_identifier("file:///etc"), "[ZoneTransfer]\r\nZoneId=3\r\n");
    }

    #[test]
    fn a_marked_file_carries_the_mark_of_the_web() {
        let dir = std::env::temp_dir().join(format!("ks-mark-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.exe");
        std::fs::write(&f, b"x").unwrap();
        mark(&f, "https://a.example/a.exe").unwrap();
        let mut stream = f.as_os_str().to_owned();
        stream.push(":Zone.Identifier");
        let mark = std::fs::read_to_string(&stream).unwrap();
        assert_eq!(mark, "[ZoneTransfer]\r\nZoneId=3\r\nHostUrl=https://a.example/a.exe\r\n");
        assert_eq!(std::fs::read(&f).unwrap(), b"x", "the file itself is unchanged");
        // Windows itself reads the stream: PowerShell lists it on the file.
        let out = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", "(Get-Item -LiteralPath $env:KSDM_FILE -Stream Zone.Identifier).Length"])
            .env("KSDM_FILE", &f)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), mark.len().to_string());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn keeping_awake_is_switched_on_and_off() {
        set_keep_awake(true);
        set_keep_awake(true);
        assert!(keeping_awake());
        set_keep_awake(false);
        assert!(!keeping_awake());
    }

    /// Puts text on the real clipboard and reads it back the way the app does. Replaces
    /// whatever is on the clipboard of the PC running the tests.
    #[tokio::test]
    async fn the_clipboard_is_read_as_unicode() {
        for url in ["https://a.example/file.zip", "https://hi.wikipedia.org/wiki/हिन्दी"] {
            let set = Command::new("powershell.exe")
                .args(["-NoProfile", "-NonInteractive", "-STA", "-Command", "Set-Clipboard -Value $env:KSDM_CLIP"])
                .env("KSDM_CLIP", url)
                .status()
                .unwrap();
            assert!(set.success());
            assert_eq!(read_clipboard().await.as_deref().map(str::trim_end), Some(url));
        }
    }

    /// The real script, set up exactly as for the dialog, answering with the starting folder
    /// instead of showing the dialog: a Hindi folder name must come back unchanged.
    #[tokio::test]
    async fn the_picker_script_returns_unicode_paths_unchanged() {
        let dir = std::env::temp_dir().join(format!("ks-picker-{}", std::process::id())).join("डाउनलोड वीडियो");
        std::fs::create_dir_all(&dir).unwrap();
        let echo = windows_script("[Console]::Out.Write($d.SelectedPath)");
        let got = run_windows_script(&echo, "फ़ोल्डर चुनें", Some(&dir)).await.unwrap();
        assert_eq!(Some(got.as_str()), dir.to_str());
        assert!(windows_script(SHOW_DIALOG).ends_with(SHOW_DIALOG));
        std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }
}
