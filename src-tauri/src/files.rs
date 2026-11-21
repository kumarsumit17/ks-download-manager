//! File names, save folders and destinations. Nothing here ever overwrites a file.

use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Longest file name we create, in bytes (most file systems allow 255).
const MAX_NAME: usize = 200;
/// Gives up looking for a free "name (n).ext" after this many tries.
const MAX_SUFFIX: u32 = 9_999;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    Video,
    Audio,
    Images,
    Other,
}

/// The category of a file, from its extension.
pub fn category(name: &str) -> Category {
    let ext = Path::new(name).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "mp4" | "m4v" | "mkv" | "webm" | "mov" | "avi" | "wmv" | "flv" | "ts" | "mpg" | "mpeg" | "3gp" => Category::Video,
        "mp3" | "m4a" | "aac" | "flac" | "wav" | "ogg" | "oga" | "opus" | "wma" | "aiff" | "alac" => Category::Audio,
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "svg" | "heic" | "heif" | "avif" | "tif" | "tiff" | "ico" => {
            Category::Images
        }
        _ => Category::Other,
    }
}

/// Makes a server- or user-supplied name safe to use as one file name inside the save
/// folder: no path separators or drive letters, no control characters, no characters
/// Windows forbids, no "." / ".." or leading dots (hidden files), no trailing dots or
/// spaces, no reserved Windows device names, and at most 200 bytes (extension kept).
pub fn sanitize_name(raw: &str) -> String {
    let mut name: String = raw
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    name = name.trim().trim_start_matches('.').trim_end_matches(['.', ' ']).to_owned();
    // With separators gone a ".." can no longer climb out of the folder; drop it anyway,
    // so no name ever looks like a path step.
    while name.contains("..") {
        name = name.replace("..", ".");
    }
    if name.is_empty() {
        name = "download".to_owned();
    }
    let stem = Path::new(&name).file_stem().and_then(|s| s.to_str()).unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit());
    if reserved {
        name.insert(0, '_');
    }
    truncate(&name, MAX_NAME)
}

/// Shortens `name` to at most `max` bytes, keeping the extension and whole characters.
fn truncate(name: &str, max: usize) -> String {
    if name.len() <= max {
        return name.to_owned();
    }
    let (stem, ext) = split_ext(name);
    let ext = if ext.len() < max / 2 { ext } else { "" };
    let mut end = max - ext.len();
    while !stem.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{ext}", &stem[..end])
}

/// "movie.mp4" → ("movie", ".mp4"); "README" → ("README", "").
fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 => name.split_at(i),
        _ => (name, ""),
    }
}

/// The n-th alternative name: "movie.mp4" → "movie (2).mp4".
fn numbered(name: &str, n: u32) -> String {
    let (stem, ext) = split_ext(name);
    let suffix = format!(" ({n})");
    let stem = truncate(stem, MAX_NAME.saturating_sub(suffix.len() + ext.len()));
    format!("{stem}{suffix}{ext}")
}

/// The save folder as an absolute path to an existing directory.
pub fn check_folder(folder: &str) -> Result<PathBuf, String> {
    let path = Path::new(folder.trim());
    if folder.trim().is_empty() {
        return Err("Choose a folder to save the file in.".into());
    }
    if !path.is_absolute() {
        return Err("The save folder must be a full path.".into());
    }
    let path = path.canonicalize().map_err(|_| format!("The folder {} does not exist.", path.display()))?;
    if !path.is_dir() {
        return Err(format!("{} is not a folder.", path.display()));
    }
    Ok(path)
}

/// Picks the first of `name`, `name (1)`, `name (2)`, … in `folder` that neither exists on
/// disk nor is `taken` (destinations of other downloads in the list), and creates it empty,
/// so no other program or download can take it before the engine opens it.
pub fn reserve(folder: &Path, name: &str, taken: &dyn Fn(&Path) -> bool) -> std::io::Result<PathBuf> {
    for n in 0..=MAX_SUFFIX {
        let candidate = folder.join(if n == 0 { name.to_owned() } else { numbered(name, n) });
        // A name whose in-progress file (or its map) is there belongs to another download.
        let busy = [ksdm(&candidate), part(&candidate), ksdm(&part(&candidate))].iter().any(|p| p.exists());
        if taken(&candidate) || busy {
            continue;
        }
        match OpenOptions::new().write(true).create_new(true).open(&candidate) {
            Ok(_) => return Ok(candidate),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(ErrorKind::AlreadyExists, "no free file name left"))
}

/// The engine's range map next to `destination` (specs/engine.md, "Range map").
pub fn ksdm(destination: &Path) -> PathBuf {
    with_suffix(destination, ".ksdm")
}

/// Where a download is written until it finishes: `<final name>.ksdownload`. Its range
/// map is `<final name>.ksdownload.ksdm`. On success it is renamed to the final name.
pub fn part(final_path: &Path) -> PathBuf {
    with_suffix(final_path, PART_SUFFIX)
}

const PART_SUFFIX: &str = ".ksdownload";

fn with_suffix(p: &Path, suffix: &str) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir()
            .join(format!("ks-files-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    #[test]
    fn names_lose_separators_and_dot_steps() {
        assert_eq!(sanitize_name("../../etc/passwd"), "_._etc_passwd");
        assert_eq!(sanitize_name("..\\..\\boot.ini"), "_._boot.ini");
        assert_eq!(sanitize_name("C:\\Windows\\x.exe"), "C__Windows_x.exe");
        assert_eq!(sanitize_name(".."), "download");
        assert_eq!(sanitize_name("."), "download");
        assert_eq!(sanitize_name("   "), "download");
        assert_eq!(sanitize_name(".hidden"), "hidden");
        assert_eq!(sanitize_name("a..b.txt"), "a.b.txt");
        assert_eq!(sanitize_name("report.pdf. . "), "report.pdf");
        assert_eq!(sanitize_name("bad\u{0}name\n.txt"), "bad_name_.txt");
        assert_eq!(sanitize_name("what?<>|*\".zip"), "what______.zip");
        assert_eq!(sanitize_name("CON.txt"), "_CON.txt");
        assert_eq!(sanitize_name("com1"), "_com1");
        assert_eq!(sanitize_name("console.log"), "console.log");
        assert_eq!(sanitize_name("holiday 2026.mp4"), "holiday 2026.mp4");
        for raw in ["../x", "a/../b", "..", "/abs/path", "x\\..\\y"] {
            let n = sanitize_name(raw);
            assert!(!n.contains('/') && !n.contains('\\') && !n.contains(".."), "{raw} -> {n}");
        }
    }

    #[test]
    fn long_names_keep_their_extension() {
        let long = format!("{}.mp4", "é".repeat(300));
        let n = sanitize_name(&long);
        assert!(n.len() <= MAX_NAME, "{}", n.len());
        assert!(n.ends_with(".mp4"));
        let numbered = numbered(&n, 12);
        assert!(numbered.len() <= MAX_NAME && numbered.ends_with(" (12).mp4"));
    }

    #[test]
    fn in_progress_names() {
        assert_eq!(part(Path::new("/d/movie.mp4")), PathBuf::from("/d/movie.mp4.ksdownload"));
        assert_eq!(ksdm(&part(Path::new("/d/movie.mp4"))), PathBuf::from("/d/movie.mp4.ksdownload.ksdm"));
    }

    #[test]
    fn categories_come_from_the_extension() {
        assert_eq!(category("a.MP4"), Category::Video);
        assert_eq!(category("a.flac"), Category::Audio);
        assert_eq!(category("a.jpeg"), Category::Images);
        assert_eq!(category("a.zip"), Category::Other);
        assert_eq!(category("noext"), Category::Other);
    }

    #[test]
    fn folder_must_exist_and_be_a_directory() {
        let dir = temp_dir();
        assert_eq!(check_folder(dir.to_str().unwrap()).unwrap(), dir);
        assert!(check_folder("").is_err());
        assert!(check_folder("relative/folder").is_err());
        assert!(check_folder(dir.join("missing").to_str().unwrap()).is_err());
        let file = dir.join("file.txt");
        std::fs::write(&file, b"x").unwrap();
        assert!(check_folder(file.to_str().unwrap()).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reserve_never_overwrites() {
        let dir = temp_dir();
        let none = |_: &Path| false;
        std::fs::write(dir.join("movie.mp4"), b"keep me").unwrap();
        let a = reserve(&dir, "movie.mp4", &none).unwrap();
        assert_eq!(a, dir.join("movie (1).mp4"));
        assert_eq!(std::fs::read(dir.join("movie.mp4")).unwrap(), b"keep me");
        assert!(a.exists(), "the name is held by an empty file");
        let b = reserve(&dir, "movie.mp4", &none).unwrap();
        assert_eq!(b, dir.join("movie (2).mp4"));
        // A name another download in the list will use is skipped, and so is one with a
        // range map left next to it.
        let taken = |p: &Path| p == dir.join("README");
        std::fs::write(ksdm(&dir.join("README (1)")), b"map").unwrap();
        assert_eq!(reserve(&dir, "README", &taken).unwrap(), dir.join("README (2)"));
        // So is a name whose in-progress file or its map is there.
        std::fs::write(part(&dir.join("a.zip")), b"part").unwrap();
        std::fs::write(ksdm(&part(&dir.join("a (1).zip"))), b"map").unwrap();
        assert_eq!(reserve(&dir, "a.zip", &none).unwrap(), dir.join("a (2).zip"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
