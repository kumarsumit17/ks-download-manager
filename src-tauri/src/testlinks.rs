//! Links for tests, on every system: the link guards (leftover cleanup, temp folders,
//! markers, playlist folders) must refuse each kind the system has. Unix: symbolic links.
//! Windows: file and folder symbolic links, and folder junctions, which any user can make
//! (no privilege or Developer Mode needed), so they are the realistic case there.

use std::path::{Path, PathBuf};

/// A link at `link` to the file `target`.
pub fn file(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(target, link)
        .unwrap_or_else(|e| panic!("cannot make a file symbolic link on this Windows (privilege or Developer Mode): {e}"));
}

/// Every kind of folder link this system has, each made at `<base>-<kind>` and pointing to
/// `target`. Returns the links made.
pub fn folder_links(target: &Path, base: &Path) -> Vec<PathBuf> {
    let at = |kind: &str| {
        let mut s = base.as_os_str().to_owned();
        s.push(format!("-{kind}"));
        PathBuf::from(s)
    };
    let mut made = Vec::new();
    #[cfg(unix)]
    {
        let link = at("symlink");
        std::os::unix::fs::symlink(target, &link).unwrap();
        made.push(link);
    }
    #[cfg(windows)]
    {
        let link = at("symlink");
        std::os::windows::fs::symlink_dir(target, &link)
            .unwrap_or_else(|e| panic!("cannot make a folder symbolic link on this Windows (privilege or Developer Mode): {e}"));
        made.push(link);
        let link = at("junction");
        // mklink does not take the `\\?\` form that canonicalize() gives.
        let plain = |p: &Path| PathBuf::from(p.to_string_lossy().trim_start_matches(r"\\?\"));
        let status = std::process::Command::new("cmd.exe").args(["/C", "mklink", "/J"]).arg(plain(&link)).arg(plain(target)).output().unwrap();
        assert!(status.status.success(), "mklink /J failed: {}", String::from_utf8_lossy(&status.stdout));
        made.push(link);
    }
    for link in &made {
        // Made as a link, and it reaches the target.
        assert!(std::fs::symlink_metadata(link).unwrap().file_type().is_symlink(), "{} is not a link", link.display());
        assert!(link.join(".").is_dir(), "{} does not reach its target", link.display());
    }
    made
}
