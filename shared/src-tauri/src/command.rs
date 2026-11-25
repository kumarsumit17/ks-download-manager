//! Starting other programs. Per platform (`crate::os`): on Windows every program the app
//! starts gets `CREATE_NO_WINDOW` (the app is a windowed program, so each console program it
//! runs would otherwise flash a console window); on macOS these are plain `Command::new`.

use std::ffi::OsStr;

/// A blocking command for `program`. Only Windows code needs it so far.
#[cfg_attr(not(windows), expect(dead_code))]
pub fn std(program: impl AsRef<OsStr>) -> std::process::Command {
    crate::os::command_std(program)
}

/// An async command for `program`.
pub fn tokio(program: impl AsRef<OsStr>) -> tokio::process::Command {
    crate::os::command_tokio(program)
}
