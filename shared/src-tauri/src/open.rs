//! Opens a file with its default app, or shows it in Finder / Explorer. The path is passed
//! to the system opener as a single argument, never through a shell, so no character in a
//! file name can be read as a command. Callers look the path up by download ID; the
//! interface never sends a path here.

use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::os::opener;

/// Opens `file` with the app the system uses for it.
pub fn open_file(file: &Path) -> io::Result<()> {
    if !file.is_file() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "the file is no longer there"));
    }
    run(opener(file, false))
}

/// Shows `file` selected in its folder; if the file is gone, opens the folder.
pub fn reveal(file: &Path) -> io::Result<()> {
    if file.exists() {
        return run(opener(file, true));
    }
    match file.parent().filter(|d| d.is_dir()) {
        Some(dir) => run(opener(dir, false)),
        None => Err(io::Error::new(io::ErrorKind::NotFound, "the folder is no longer there")),
    }
}

/// Opens a web address in the default browser (only fixed `https` addresses of our own).
pub fn open_url(url: &str) -> io::Result<()> {
    if !url.starts_with("https://") {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "not an https address"));
    }
    run(crate::os::url_opener(url))
}

fn run(mut cmd: Command) -> io::Result<()> {
    // Not waited for: the opener returns at once, and the child is reaped by the system.
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_path_is_one_argument_and_never_a_shell_command() {
        // `"` cannot be in a Windows file name; Windows' own quoting test is in desktop/windows.
        #[cfg(not(target_os = "windows"))]
        let tricky = Path::new("/tmp/-rf; echo hi $(whoami) `x` & | \"q\".zip");
        #[cfg(target_os = "windows")]
        let tricky = Path::new(r"C:\tmp\-rf; echo hi $(whoami) `x` & | %PATH% ^q.zip");
        for select in [false, true] {
            let cmd = opener(tricky, select);
            let program = cmd.get_program().to_string_lossy().to_lowercase();
            assert!(!["sh", "bash", "zsh", "cmd", "cmd.exe", "powershell"].iter().any(|s| program.ends_with(s)));
            let args: Vec<_> = cmd.get_args().collect();
            let last = args.last().unwrap().to_string_lossy().into_owned();
            // Windows' "/select," argument closes the path with a quote.
            #[cfg(target_os = "windows")]
            let last = last.trim_end_matches('"');
            assert!(last.ends_with(&*tricky.to_string_lossy()), "{last}");
        }
    }

    #[test]
    fn only_https_addresses_are_opened() {
        assert!(open_url("file:///etc/passwd").is_err());
        assert!(open_url("javascript:alert(1)").is_err());
        let cmd = crate::os::url_opener("https://github.com/kumasumit/ks-download-manager#readme");
        assert_eq!(cmd.get_args().last().unwrap(), "https://github.com/kumasumit/ks-download-manager#readme");
    }

    #[test]
    fn opening_a_missing_file_is_an_error() {
        assert!(open_file(Path::new("/definitely/not/here.bin")).is_err());
        assert!(reveal(Path::new("/definitely/not/here.bin")).is_err());
    }
}
