//! Pre-fill for Add download (Stage 5, Part 7): the clipboard's text, read only when the
//! user opens the dialog, and handed to the page only when it is one `http(s)` link.
//! Anything else on the clipboard (a password, a document) never leaves Rust, and nothing
//! is kept. No crate: macOS runs the system `pbpaste`; Windows runs PowerShell's
//! `Get-Clipboard`, writing UTF-8 (Windows PowerShell 5.1 would otherwise use the console
//! code page and spoil non-English links).

/// Longest link accepted, as for the extension's hand-off.
const MAX_LINK: usize = 2048;

/// The clipboard's link, if it holds exactly one `http(s)` link and nothing else.
pub async fn link() -> Option<String> {
    link_in(&read().await?)
}

#[cfg(target_os = "macos")]
async fn read() -> Option<String> {
    let out = tokio::process::Command::new("/usr/bin/pbpaste")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .await
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(windows)]
async fn read() -> Option<String> {
    const SCRIPT: &str = "[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false; \
        [Console]::Out.Write((Get-Clipboard -Raw))";
    let out = crate::command::tokio("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-STA", "-Command", SCRIPT])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .await
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim_start_matches('\u{feff}').to_owned())
}

#[cfg(not(any(target_os = "macos", windows)))]
async fn read() -> Option<String> {
    None
}

/// One link with no spaces or control characters, `http://` or `https://`, a host, at
/// most 2048 characters, without a user name or password (as the extension requires).
fn link_in(text: &str) -> Option<String> {
    let t = text.trim();
    let rest = t.strip_prefix("https://").or_else(|| t.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let ok = t.len() <= MAX_LINK
        && !host.is_empty()
        && !host.contains('@')
        && !t.chars().any(|c| c.is_whitespace() || c.is_control());
    ok.then(|| t.to_owned())
}

#[cfg(test)]
mod tests {
    use super::link_in;

    /// Puts text on the real clipboard and reads it back the way the app does. Replaces
    /// whatever is on the clipboard of the PC running the tests.
    #[cfg(windows)]
    #[tokio::test]
    async fn the_windows_clipboard_is_read_as_unicode() {
        for url in ["https://a.example/file.zip", "https://hi.wikipedia.org/wiki/हिन्दी"] {
            let set = std::process::Command::new("powershell.exe")
                .args(["-NoProfile", "-NonInteractive", "-STA", "-Command", "Set-Clipboard -Value $env:KSDM_CLIP"])
                .env("KSDM_CLIP", url)
                .status()
                .unwrap();
            assert!(set.success());
            assert_eq!(super::read().await.as_deref().map(str::trim_end), Some(url));
            assert_eq!(super::link().await.as_deref(), Some(url));
        }
    }

    #[test]
    fn only_a_single_web_link_is_offered() {
        assert_eq!(link_in("  https://a.example/f.zip\n").as_deref(), Some("https://a.example/f.zip"));
        assert_eq!(link_in("http://a.example").as_deref(), Some("http://a.example"));
        for not in [
            "",
            "my password",
            "ftp://a.example/f",
            "https://",
            "https://a.example/a https://b.example/b",
            "see https://a.example/",
            "https://user:pw@a.example/",
            "javascript:alert(1)",
        ] {
            assert_eq!(link_in(not), None, "{not:?}");
        }
        assert_eq!(link_in(&format!("https://a.example/{}", "x".repeat(2100))), None, "too long");
    }
}
