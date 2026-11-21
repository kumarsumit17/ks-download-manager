//! Categories, save folders and destinations. Nothing here ever overwrites a file.
//! Making names safe and numbering them ("name (1).ext") is in `ks_utility::file_name`.

use std::path::{Path, PathBuf};

use ks_utility::file_name::reserve_free_name;
use serde::{Deserialize, Serialize};

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
/// disk nor is `taken` (destinations of other downloads in the list), nor has another
/// download's in-progress file or range map next to it, and creates it empty, so no other
/// program or download can take it before the engine opens it.
pub fn reserve(folder: &Path, name: &str, taken: &dyn Fn(&Path) -> bool) -> std::io::Result<PathBuf> {
    let skip = |candidate: &Path| {
        taken(candidate) || [ksdm(candidate), part(candidate), ksdm(&part(candidate))].iter().any(|p| p.exists())
    };
    reserve_free_name(folder, name, &skip)
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
