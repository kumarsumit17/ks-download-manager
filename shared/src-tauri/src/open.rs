//! Opens a file with its default app, or shows it in Finder / Explorer. The path is passed
//! to the system opener as a single argument, never through a shell, so no character in a
//! file name can be read as a command. Callers look the path up by download ID; the
//! interface never sends a path here.

use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

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

fn run(mut cmd: Command) -> io::Result<()> {
    // Not waited for: the opener returns at once, and the child is reaped by the system.
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map(drop)
}

#[cfg(target_os = "macos")]
fn opener(path: &Path, select: bool) -> Command {
    let mut cmd = Command::new("/usr/bin/open");
    if select {
        cmd.arg("-R");
    }
    // "--" ends the options, so a name starting with "-" is still a path.
    cmd.arg("--").arg(path);
    cmd
}

/// explorer.exe takes the path itself as its argument; no cmd.exe is involved. Explorer
/// returns exit code 1 even when it worked, so its exit code is never read (`run` does not
/// wait for it).
#[cfg(target_os = "windows")]
fn opener(path: &Path, select: bool) -> Command {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new("explorer.exe");
    if select {
        cmd.raw_arg(select_arg(path));
    } else {
        cmd.arg(path);
    }
    cmd
}

/// `/select,"<path>"`, written as is: Explorer reads its command line itself and does not
/// understand the usual quoting of a whole argument (`"/select,C:\a b\c.txt"`). A Windows
/// path cannot contain `"`, so the quotes always enclose the whole path.
#[cfg(target_os = "windows")]
fn select_arg(path: &Path) -> std::ffi::OsString {
    let mut arg = std::ffi::OsString::from("/select,\"");
    arg.push(path.as_os_str());
    arg.push("\"");
    arg
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn opener(path: &Path, select: bool) -> Command {
    // Not a release target; kept so the crate builds and tests on other systems.
    let target = if select { path.parent().unwrap_or(path) } else { path };
    let mut cmd = Command::new("xdg-open");
    cmd.arg(target);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "windows")]
    #[test]
    fn explorer_selects_the_file_with_the_path_in_quotes() {
        let file = Path::new(r"C:\Users\a b\Downloads\वीडियो, 1.mp4");
        assert_eq!(select_arg(file), std::ffi::OsString::from(r#"/select,"C:\Users\a b\Downloads\वीडियो, 1.mp4""#));
        let cmd = opener(file, true);
        assert_eq!(cmd.get_program(), "explorer.exe");
        assert_eq!(cmd.get_args().collect::<Vec<_>>(), [select_arg(file).as_os_str()]);
        let cmd = opener(file, false);
        assert_eq!(cmd.get_args().collect::<Vec<_>>(), [file.as_os_str()]);
    }

    #[test]
    fn the_path_is_one_argument_and_never_a_shell_command() {
        // `"` cannot be in a Windows file name; Windows' own test is the one above.
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
    fn opening_a_missing_file_is_an_error() {
        assert!(open_file(Path::new("/definitely/not/here.bin")).is_err());
        assert!(reveal(Path::new("/definitely/not/here.bin")).is_err());
    }
}
