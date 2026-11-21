//! The system's folder picker, without a new crate. macOS: `osascript` runs a fixed
//! AppleScript; the prompt and the starting folder are passed as arguments (`argv`) and
//! never become part of the script. Windows: PowerShell's folder dialog, with both values
//! in environment variables; written but **untested** (Release stage).

use std::path::Path;

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
pub async fn pick_folder(prompt: &str, start: Option<&Path>) -> Result<Option<String>, String> {
    let mut c = tokio::process::Command::new("/usr/bin/osascript");
    for line in SCRIPT {
        c.arg("-e").arg(line);
    }
    // A prompt starting with "-" would read as an option; ours never does, but make sure.
    c.arg(prompt.trim_start_matches('-'));
    if let Some(dir) = start.filter(|d| d.is_dir()) {
        c.arg(dir);
    }
    let out = c.output().await.map_err(|e| format!("Could not open the folder picker: {e}."))?;
    if !out.status.success() {
        return Err("Could not open the folder picker.".into());
    }
    let chosen = String::from_utf8_lossy(&out.stdout).trim_end_matches(['\n', '\r']).to_owned();
    Ok(clean(&chosen))
}

#[cfg(windows)]
pub async fn pick_folder(prompt: &str, start: Option<&Path>) -> Result<Option<String>, String> {
    // Untested. The values reach the script only as environment variables.
    const SCRIPT: &str = "Add-Type -AssemblyName System.Windows.Forms; \
        $d = New-Object System.Windows.Forms.FolderBrowserDialog; \
        $d.Description = $env:KSDM_PROMPT; \
        if ($env:KSDM_START) { $d.SelectedPath = $env:KSDM_START }; \
        if ($d.ShowDialog() -eq 'OK') { [Console]::Out.Write($d.SelectedPath) }";
    let out = tokio::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-STA", "-Command", SCRIPT])
        .env("KSDM_PROMPT", prompt)
        .env("KSDM_START", start.map(|p| p.as_os_str().to_owned()).unwrap_or_default())
        .output()
        .await
        .map_err(|e| format!("Could not open the folder picker: {e}."))?;
    if !out.status.success() {
        return Err("Could not open the folder picker.".into());
    }
    Ok(clean(&String::from_utf8_lossy(&out.stdout)))
}

#[cfg(not(any(target_os = "macos", windows)))]
pub async fn pick_folder(_prompt: &str, _start: Option<&Path>) -> Result<Option<String>, String> {
    Err("The folder picker is not available on this system.".into())
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
}
