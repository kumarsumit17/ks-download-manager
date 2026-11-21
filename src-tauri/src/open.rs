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

#[cfg(target_os = "windows")]
fn opener(path: &Path, select: bool) -> Command {
    // explorer.exe takes the path itself as its argument; no cmd.exe is involved.
    let mut cmd = Command::new("explorer.exe");
    if select {
        let mut arg = std::ffi::OsString::from("/select,");
        arg.push(path.as_os_str());
        cmd.arg(arg);
    } else {
        cmd.arg(path);
    }
    cmd
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

    #[test]
    fn the_path_is_one_argument_and_never_a_shell_command() {
        let tricky = Path::new("/tmp/-rf; echo hi $(whoami) `x` & | \"q\".zip");
        for select in [false, true] {
            let cmd = opener(tricky, select);
            let program = cmd.get_program().to_string_lossy().to_lowercase();
            assert!(!["sh", "bash", "zsh", "cmd", "cmd.exe", "powershell"].iter().any(|s| program.ends_with(s)));
            let args: Vec<_> = cmd.get_args().collect();
            let last = args.last().unwrap().to_string_lossy().into_owned();
            assert!(last.ends_with(&*tricky.to_string_lossy()), "{last}");
        }
    }

    #[test]
    fn opening_a_missing_file_is_an_error() {
        assert!(open_file(Path::new("/definitely/not/here.bin")).is_err());
        assert!(reveal(Path::new("/definitely/not/here.bin")).is_err());
    }
}
