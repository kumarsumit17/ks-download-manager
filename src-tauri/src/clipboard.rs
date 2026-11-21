//! Pre-fill for Add download (Stage 5, Part 7): the clipboard's text, read only when the
//! user opens the dialog, and handed to the page only when it is one `http(s)` link.
//! Anything else on the clipboard (a password, a document) never leaves Rust, and nothing
//! is kept. No crate: macOS runs the system `pbpaste`; Windows runs PowerShell's
//! `Get-Clipboard` (written but untested until the Windows build).

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
    let out = tokio::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", "Get-Clipboard -Raw"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .await
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
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
