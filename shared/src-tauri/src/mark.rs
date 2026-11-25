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
//! The marking itself is per platform (`crate::os`: the system `xattr` tool on macOS, the
//! alternate data stream on Windows). A file system without extended attributes (FAT, exFAT,
//! some network shares) cannot carry the mark; the download still completes and the
//! failure is logged.

use std::path::Path;

/// Marks `file`, downloaded from `url`. Errors are returned for logging only.
pub fn mark_from_internet(file: &Path, url: &str) -> std::io::Result<()> {
    crate::os::mark(file, url)
}
