//! The system's folder picker. Showing it is per platform (`crate::os`: `osascript` with a
//! fixed AppleScript on macOS, PowerShell's folder dialog on Windows; the prompt and the
//! starting folder never become part of a script). Cleaning its answer is here.

use std::path::Path;

use crate::msg::Msg;

/// Shows the picker and returns the chosen folder, or `None` when the user cancels.
#[cfg(any(target_os = "macos", windows))]
pub async fn pick_folder(prompt: &str, start: Option<&Path>) -> Result<Option<String>, Msg> {
    let answer = crate::os::pick_folder(prompt, start).await.map_err(|detail| msg!("error.picker", detail = detail))?;
    Ok(clean(&answer))
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

}
