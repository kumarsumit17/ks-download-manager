//! Leftover download files (specs/ui.md, "Leftover download files"): our hidden video temp
//! folders, `.ksdownload` files and empty reservation files (the file that holds a
//! download's final name until it finishes) that belong to no download in the list, for
//! example after a download was removed while the app was not running, or after a crash.
//!
//! Safety rules:
//! - Only the folders the app knows are looked in (the default save folder, the folders of
//!   downloads in the list, the category folders), one level deep. Entries that are links
//!   are skipped, so nothing outside those folders is ever reached.
//! - Nothing is listed unless it carries our marker (`ks-owner.json` in a video temp folder,
//!   `<file>.ksowner` beside a `.ksdownload` file or a reservation) or a valid engine range
//!   map (`.ksdm`). An empty file without our marker is never listed.
//! - Nothing is removed automatically. Removal re-checks each item at that moment: it must
//!   still be the same kind of entry, still carry the same marker or a valid map, and still
//!   belong to no download in the list.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::files;
use crate::manager::VIDEO_TEMP_PREFIX as VIDEO_PREFIX;
use crate::marker::{self, Marker};
use crate::store::{Kind, Record};

const PART_SUFFIX: &str = ".ksdownload";

/// What proves an item is ours.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Proof {
    Marker(Marker),
    /// A valid range map for this URL.
    Map(String),
}

/// What kind of leftover an item is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum What {
    /// A hidden video temp folder.
    Folder,
    /// A `.ksdownload` file (with its range map and marker, if there).
    Part,
    /// The empty file that held a download's final name.
    Reservation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
    pub what: What,
    pub size: u64,
    proof: Proof,
}

/// What the Settings screen shows.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub path: String,
    pub what: What,
    pub size: u64,
}

impl From<&Found> for Item {
    fn from(f: &Found) -> Self {
        Self { path: f.path.to_string_lossy().into_owned(), what: f.what, size: f.size }
    }
}

/// The marker file beside a `.ksdownload` file or a reservation file: `<file>.ksowner`.
pub fn owner_file(file: &Path) -> PathBuf {
    let mut s = file.as_os_str().to_owned();
    s.push(".ksowner");
    PathBuf::from(s)
}

/// Looks in each folder (once, one level deep) for our leftovers that no record owns.
pub async fn scan(folders: &[PathBuf], records: &[Record]) -> Vec<Found> {
    let mut seen = Vec::<PathBuf>::new();
    let mut out = Vec::new();
    for folder in folders {
        let Ok(canonical) = std::fs::canonicalize(folder) else { continue };
        if seen.contains(&canonical) {
            continue;
        }
        seen.push(canonical);
        let Ok(entries) = std::fs::read_dir(folder) else { continue };
        for entry in entries.flatten() {
            if let Some(f) = examine(&entry.path()).await
                && !belongs(&f, records)
            {
                out.push(f);
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// One entry of a known folder: ours, or `None`. Links are never followed.
async fn examine(path: &Path) -> Option<Found> {
    let name = path.file_name()?.to_str()?;
    let meta = std::fs::symlink_metadata(path).ok()?;
    if name.starts_with(VIDEO_PREFIX) && meta.is_dir() {
        let m = marker::of_folder(path)?;
        return Some(Found { path: path.to_path_buf(), what: What::Folder, size: folder_size(path), proof: Proof::Marker(m) });
    }
    if !meta.is_file() {
        return None;
    }
    if name.ends_with(PART_SUFFIX) {
        let proof = proof_of_part(path).await?;
        let size = meta.len() + extras_size(&[files::ksdm(path), owner_file(path)]);
        return Some(Found { path: path.to_path_buf(), what: What::Part, size, proof });
    }
    // An empty file is a reservation only with our marker beside it; our own side files
    // are never taken for one.
    if meta.len() == 0 && !OUR_SUFFIXES.iter().any(|s| name.ends_with(s)) {
        let m = marker::read(&owner_file(path))?;
        let size = extras_size(&[owner_file(path)]);
        return Some(Found { path: path.to_path_buf(), what: What::Reservation, size, proof: Proof::Marker(m) });
    }
    None
}

/// Names of the files we keep beside a download; never reservations themselves.
const OUR_SUFFIXES: [&str; 3] = [PART_SUFFIX, ".ksdm", ".ksowner"];

/// Bytes in those of `paths` that are regular files.
fn extras_size(paths: &[PathBuf]) -> u64 {
    paths.iter().filter_map(|p| std::fs::symlink_metadata(p).ok()).filter(|m| m.is_file()).map(|m| m.len()).sum()
}

async fn proof_of_part(path: &Path) -> Option<Proof> {
    if let Some(m) = marker::read(&owner_file(path)) {
        return Some(Proof::Marker(m));
    }
    // The map must be a regular file too, not a link to someone else's.
    if !std::fs::symlink_metadata(files::ksdm(path)).is_ok_and(|m| m.is_file()) {
        return None;
    }
    engine::saved_state(path).await.map(|s| Proof::Map(s.url))
}

/// Bytes in a folder's own files (yt-dlp keeps its files flat); links count as nothing.
fn folder_size(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| std::fs::symlink_metadata(e.path()).ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

/// True when a download in the list owns the item.
fn belongs(f: &Found, records: &[Record]) -> bool {
    records.iter().any(|r| match f.what {
        What::Folder => r.kind == Kind::Video && matches!(&f.proof, Proof::Marker(m) if m.is_for(r.id, &r.url)),
        What::Part => r.kind == Kind::File && files::part(&r.path) == f.path,
        // Any download whose name it holds, of either kind.
        What::Reservation => r.path == f.path,
    })
}

/// Removes the items that are still ours and still unowned; returns how many were removed
/// and the paths that were kept (changed since the scan, or could not be removed).
pub async fn remove(found: &[Found], records: &[Record]) -> (usize, Vec<PathBuf>) {
    let mut removed = 0;
    let mut kept = Vec::new();
    for f in found {
        let still = examine(&f.path).await.is_some_and(|now| now.what == f.what && now.proof == f.proof);
        if !still || belongs(f, records) {
            kept.push(f.path.clone());
            continue;
        }
        let extras = match f.what {
            What::Folder => vec![],
            What::Part => vec![files::ksdm(&f.path), owner_file(&f.path)],
            What::Reservation => vec![owner_file(&f.path)],
        };
        let result = if f.what == What::Folder {
            // Removes links inside as links; never what they point to.
            std::fs::remove_dir_all(&f.path)
        } else {
            std::fs::remove_file(&f.path).and_then(|()| {
                for extra in extras {
                    if std::fs::symlink_metadata(&extra).is_ok_and(|m| m.is_file()) {
                        std::fs::remove_file(&extra)?;
                    }
                }
                Ok(())
            })
        };
        match result {
            Ok(()) => removed += 1,
            Err(_) => kept.push(f.path.clone()),
        }
    }
    (removed, kept)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::Category;
    use crate::store::Status;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ks-leftovers-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn record(id: u64, kind: Kind, path: PathBuf, url: &str) -> Record {
        Record {
            id,
            kind,
            url: url.into(),
            name: "x".into(),
            path,
            category: Category::Other,
            status: Status::Paused,
            size: None,
            received: 0,
            resumable: true,
            error: None,
            error_code: None,
            temp: None,
            added_at: 0,
            queue: None,
            playlist: None,
        }
    }

    fn video_folder(base: &Path, id: u64, url: &str) -> PathBuf {
        let d = base.join(format!("{VIDEO_PREFIX}{id}"));
        std::fs::create_dir_all(&d).unwrap();
        marker::write(&d.join(marker::IN_FOLDER), &Marker::new(id, url)).unwrap();
        std::fs::write(d.join("video.f1.mp4.part"), vec![0u8; 100]).unwrap();
        d
    }

    #[tokio::test]
    async fn a_marked_orphan_is_found_and_removed() {
        let base = dir("orphan");
        let folder = video_folder(&base, 5, "https://v.example/a");
        let part = base.join("file.zip.ksdownload");
        std::fs::write(&part, vec![0u8; 50]).unwrap();
        marker::write(&owner_file(&part), &Marker::new(6, "https://f.example/file.zip")).unwrap();

        let found = scan(std::slice::from_ref(&base), &[]).await;
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found.iter().any(|f| f.what == What::Folder && f.path == folder && f.size >= 100));
        let (removed, kept) = remove(&found, &[]).await;
        assert_eq!((removed, kept.len()), (2, 0));
        assert!(!folder.exists() && !part.exists() && !owner_file(&part).exists());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[tokio::test]
    async fn reservations_are_listed_only_when_empty_and_marked() {
        let base = dir("reserved");
        let orphan = base.join("song.mp3");
        std::fs::write(&orphan, b"").unwrap();
        marker::write(&owner_file(&orphan), &Marker::new(3, "https://f.example/song.mp3")).unwrap();
        // Empty without a marker, or marked but not empty (a finished file): never listed.
        std::fs::write(base.join("plain.txt"), b"").unwrap();
        let full = base.join("full.bin");
        std::fs::write(&full, b"data").unwrap();
        marker::write(&owner_file(&full), &Marker::new(4, "https://f.example/full.bin")).unwrap();
        // An empty file whose marker is a link to a marker elsewhere is not ours.
        let linked = base.join("linked.bin");
        std::fs::write(&linked, b"").unwrap();
        std::os::unix::fs::symlink(owner_file(&full), owner_file(&linked)).unwrap();

        let found = scan(std::slice::from_ref(&base), &[]).await;
        assert_eq!(found.iter().map(|f| (&f.path, f.what)).collect::<Vec<_>>(), vec![(&orphan, What::Reservation)]);
        // Owned by a download in the list: not listed.
        let owned = [record(3, Kind::File, orphan.clone(), "https://f.example/song.mp3")];
        assert!(scan(std::slice::from_ref(&base), &owned).await.is_empty());
        // Written to since the scan: kept.
        std::fs::write(&orphan, b"now in use").unwrap();
        assert_eq!(remove(&found, &[]).await, (0, vec![orphan.clone()]));
        std::fs::write(&orphan, b"").unwrap();
        assert_eq!(remove(&found, &[]).await.0, 1);
        assert!(!orphan.exists() && !owner_file(&orphan).exists());
        assert!(base.join("plain.txt").exists() && full.exists() && owner_file(&full).exists() && linked.exists());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[tokio::test]
    async fn unmarked_entries_with_our_names_are_never_listed_or_touched() {
        let base = dir("unmarked");
        let folder = base.join(format!("{VIDEO_PREFIX}9"));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("keep.txt"), b"mine").unwrap();
        let part = base.join("notes.ksdownload");
        std::fs::write(&part, b"not ours").unwrap();
        assert!(scan(std::slice::from_ref(&base), &[]).await.is_empty());
        assert!(folder.join("keep.txt").exists() && part.exists());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[tokio::test]
    async fn items_of_downloads_in_the_list_are_never_listed() {
        let base = dir("owned");
        video_folder(&base, 3, "https://v.example/a");
        let path = base.join("big.iso");
        let part = files::part(&path);
        std::fs::write(&part, b"x").unwrap();
        marker::write(&owner_file(&part), &Marker::new(4, "https://f.example/big.iso")).unwrap();
        let records = [
            record(3, Kind::Video, base.join("a.mp4"), "https://v.example/a"),
            record(4, Kind::File, path, "https://f.example/big.iso"),
        ];
        assert!(scan(std::slice::from_ref(&base), &records).await.is_empty());
        // A folder whose marker names the same ID but another URL is not that download's.
        let other = [record(3, Kind::Video, base.join("a.mp4"), "https://v.example/other")];
        assert_eq!(scan(std::slice::from_ref(&base), &other).await.iter().filter(|f| f.what == What::Folder).count(), 1);
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[tokio::test]
    async fn links_out_of_the_folder_are_not_followed() {
        let base = dir("links");
        let outside = dir("links-outside");
        let target = video_folder(&outside, 8, "https://v.example/b");
        std::os::unix::fs::symlink(&target, base.join(format!("{VIDEO_PREFIX}8"))).unwrap();
        let far = outside.join("far.ksdownload");
        std::fs::write(&far, b"x").unwrap();
        marker::write(&owner_file(&far), &Marker::new(9, "https://f.example/far")).unwrap();
        std::os::unix::fs::symlink(&far, base.join("near.ksdownload")).unwrap();
        // A real part file whose marker is a link to a marker elsewhere is not ours either.
        let real = base.join("real.ksdownload");
        std::fs::write(&real, b"x").unwrap();
        std::os::unix::fs::symlink(owner_file(&far), owner_file(&real)).unwrap();

        assert!(scan(std::slice::from_ref(&base), &[]).await.is_empty());
        assert!(target.join(marker::IN_FOLDER).exists() && far.exists());
        std::fs::remove_dir_all(&base).unwrap();
        std::fs::remove_dir_all(&outside).unwrap();
    }

    #[tokio::test]
    async fn an_item_that_changed_since_the_scan_is_kept() {
        let base = dir("changed");
        let folder = video_folder(&base, 5, "https://v.example/a");
        let found = scan(std::slice::from_ref(&base), &[]).await;
        // Its marker now names another download.
        marker::write(&folder.join(marker::IN_FOLDER), &Marker::new(6, "https://v.example/z")).unwrap();
        let (removed, kept) = remove(&found, &[]).await;
        assert_eq!((removed, kept), (0, vec![folder.clone()]));
        // Or a download in the list now owns it.
        let found = scan(std::slice::from_ref(&base), &[]).await;
        let (removed, _) = remove(&found, &[record(6, Kind::Video, base.join("z.mp4"), "https://v.example/z")]).await;
        assert_eq!(removed, 0);
        assert!(folder.exists());
        std::fs::remove_dir_all(&base).unwrap();
    }
}
