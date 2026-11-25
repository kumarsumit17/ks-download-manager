//! The system's folder picker, without a new crate. macOS: `osascript` runs a fixed
//! AppleScript; the prompt and the starting folder are passed as arguments (`argv`) and
//! never become part of the script. Windows: PowerShell's folder dialog, with both values
//! in environment variables. Its answer is written as UTF-8 (Windows PowerShell 5.1 would
//! otherwise use the console code page and spoil, for example, Hindi folder names). The
//! script is tested on Windows without the dialog; the dialog itself is a hand check.

use std::path::Path;

use crate::msg::Msg;

/// The script never changes; only its arguments do.
#[cfg(target_os = "macos")]
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

/// Shows the picker and returns the chosen folder, or `None` when the user cancels.
#[cfg(target_os = "macos")]
pub async fn pick_folder(prompt: &str, start: Option<&Path>) -> Result<Option<String>, Msg> {
    let mut c = tokio::process::Command::new("/usr/bin/osascript");
    for line in SCRIPT {
        c.arg("-e").arg(line);
    }
    // A prompt starting with "-" would read as an option; ours never does, but make sure.
    c.arg(prompt.trim_start_matches('-'));
    if let Some(dir) = start.filter(|d| d.is_dir()) {
        c.arg(dir);
    }
    let out = c.output().await.map_err(|e| msg!("error.picker", detail = e))?;
    if !out.status.success() {
        return Err(msg!("error.picker", detail = "-"));
    }
    let chosen = String::from_utf8_lossy(&out.stdout).trim_end_matches(['\n', '\r']).to_owned();
    Ok(clean(&chosen))
}

/// The PowerShell script. The values reach it only as environment variables. `answer` is
/// the line that shows the dialog; the test puts a line there that answers without one.
#[cfg(windows)]
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

#[cfg(windows)]
const SHOW_DIALOG: &str = "if ($d.ShowDialog() -eq 'OK') { [Console]::Out.Write($d.SelectedPath) }";

#[cfg(windows)]
async fn run_windows_script(script: &str, prompt: &str, start: Option<&Path>) -> Result<Option<String>, Msg> {
    let out = crate::command::tokio("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-STA", "-Command", script])
        .env("KSDM_PROMPT", prompt)
        .env("KSDM_START", start.map(|p| p.as_os_str().to_owned()).unwrap_or_default())
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|e| msg!("error.picker", detail = e))?;
    if !out.status.success() {
        return Err(msg!("error.picker", detail = "-"));
    }
    // A byte-order mark is never part of a path.
    Ok(clean(String::from_utf8_lossy(&out.stdout).trim_start_matches('\u{feff}')))
}

#[cfg(windows)]
pub async fn pick_folder(prompt: &str, start: Option<&Path>) -> Result<Option<String>, Msg> {
    run_windows_script(&windows_script(SHOW_DIALOG), prompt, start).await
}

#[cfg(not(any(target_os = "macos", windows)))]
pub async fn pick_folder(_prompt: &str, _start: Option<&Path>) -> Result<Option<String>, Msg> {
    Err(msg!("error.pickerMissing"))
}

/// The picker's answer: empty means cancelled; a trailing slash is dropped ("/a/b/").
fn clean(s: &str) -> Option<String> {
    let s = s.trim_matches(['\n', '\r']);
    if s.is_empty() {
        return None;
    }
    let s = if s.len() > 1 { s.trim_end_matches(['/', '\\']) } else { s };
    Some(s.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_are_cleaned() {
        assert_eq!(clean(""), None);
        assert_eq!(clean("\n"), None);
        assert_eq!(clean("/Users/me/Movies/\n").as_deref(), Some("/Users/me/Movies"));
        assert_eq!(clean("/").as_deref(), Some("/"));
    }

    /// The script must compile; this does not show a dialog.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_script_compiles() {
        let dir = std::env::temp_dir().join(format!("ks-picker-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("p.scpt");
        let mut c = std::process::Command::new("/usr/bin/osacompile");
        for line in SCRIPT {
            c.arg("-e").arg(line);
        }
        let status = c.arg("-o").arg(&out).status().unwrap();
        assert!(status.success());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The real script, set up exactly as for the dialog, answering with the starting folder
    /// instead of showing the dialog: a Hindi folder name must come back unchanged.
    #[cfg(windows)]
    #[tokio::test]
    async fn the_windows_script_returns_unicode_paths_unchanged() {
        let dir = std::env::temp_dir().join(format!("ks-picker-{}", std::process::id())).join("डाउनलोड वीडियो");
        std::fs::create_dir_all(&dir).unwrap();
        let echo = windows_script("[Console]::Out.Write($d.SelectedPath)");
        let got = run_windows_script(&echo, "फ़ोल्डर चुनें", Some(&dir)).await.unwrap();
        assert_eq!(got.as_deref(), dir.to_str());
        assert!(windows_script(SHOW_DIALOG).ends_with(SHOW_DIALOG));
        std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }
}
