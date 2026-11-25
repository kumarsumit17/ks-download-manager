//! Starting other programs. On Windows every program the app starts gets
//! `CREATE_NO_WINDOW`: the app is a windowed program, so without it each console program
//! it runs (PowerShell, `reg`, `attrib`, `taskkill`, `certutil`, `tar`, yt-dlp) would flash
//! a console window. On macOS these are plain `Command::new`.

use std::ffi::OsStr;

/// A blocking command for `program`. Only Windows code needs it so far.
#[cfg_attr(not(windows), expect(dead_code))]
pub fn std(program: impl AsRef<OsStr>) -> std::process::Command {
    #[cfg_attr(not(windows), expect(unused_mut))]
    let mut c = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    }
    c
}

/// An async command for `program`.
pub fn tokio(program: impl AsRef<OsStr>) -> tokio::process::Command {
    #[cfg_attr(not(windows), expect(unused_mut))]
    let mut c = tokio::process::Command::new(program);
    #[cfg(windows)]
    c.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    c
}
