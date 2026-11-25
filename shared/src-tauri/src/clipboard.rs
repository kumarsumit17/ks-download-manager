//! Pre-fill for Add download (Stage 5, Part 7): the clipboard's text, read only when the
//! user opens the dialog, and handed to the page only when it is one `http(s)` link.
//! Anything else on the clipboard (a password, a document) never leaves Rust, and nothing
//! is kept. Reading the clipboard is per platform (`crate::os`: `pbpaste` on macOS,
//! PowerShell's `Get-Clipboard` on Windows); deciding what counts as a link is here.

/// Longest link accepted, as for the extension's hand-off.
const MAX_LINK: usize = 2048;

/// The clipboard's link, if it holds exactly one `http(s)` link and nothing else.
pub async fn link() -> Option<String> {
    link_in(&read().await?)
}

async fn read() -> Option<String> {
    crate::os::read_clipboard().await
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
