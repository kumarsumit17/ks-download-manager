//! The per-platform code ([per-platform plan](../../../specs/plan-per-platform.md)): on macOS
//! the `ksdm-mac` crate in `desktop/mac`, on Windows `ksdm-windows` in `desktop/windows`.
//! Both have the same functions; the rest of the app calls them only through this module.
//! Other systems are not release targets: a small fallback below keeps the app building and
//! its tests running there.

#[cfg(target_os = "macos")]
pub use ksdm_mac::*;

#[cfg(windows)]
pub use ksdm_windows::*;

#[cfg(not(any(target_os = "macos", windows)))]
pub use other::*;

#[cfg(not(any(target_os = "macos", windows)))]
mod other {
    use std::ffi::OsStr;
    use std::path::Path;
    use std::process::Command;

    pub fn command_std(program: impl AsRef<OsStr>) -> Command {
        Command::new(program)
    }

    pub fn command_tokio(program: impl AsRef<OsStr>) -> tokio::process::Command {
        tokio::process::Command::new(program)
    }

    pub fn opener(path: &Path, select: bool) -> Command {
        let target = if select { path.parent().unwrap_or(path) } else { path };
        let mut cmd = Command::new("xdg-open");
        cmd.arg(target);
        cmd
    }

    pub async fn read_clipboard() -> Option<String> {
        None
    }

    pub fn mark(_file: &Path, _url: &str) -> std::io::Result<()> {
        Ok(())
    }

    pub fn download_finished(_file: &Path) {}

    pub fn set_keep_awake(_on: bool) {}
}
