//! Hand-off from the Chrome extension to the app (specs/extension.md).
//!
//! Chrome's native messaging starts this same program as a "host", with the calling
//! extension's origin as its argument, and talks to it over stdin/stdout. The host checks
//! that origin against our one extension ID, then forwards each message to the running app
//! over a Unix socket in the app data folder, starting the app through LaunchServices if
//! it is not running. The socket bind is also the app's single-instance lock.
//!
//! What stops others from adding downloads:
//! - Websites cannot use native messaging at all; Chrome only lets the extensions listed in
//!   the host manifest's `allowed_origins` (ours alone) start the host.
//! - The host refuses any origin argument but ours, exactly.
//! - The socket lives in the user's own app data folder, is created `0600`, and every
//!   connection's peer user ID must equal the socket owner's.
//! - Messages are small (8 KB), strictly shaped (unknown fields rejected), and carry only an
//!   http(s) URL and a kind. Nothing is run, opened or saved anywhere but the save folder.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Our extension's ID, fixed by the public `key` in `extension/manifest.json`.
pub const EXTENSION_ID: &str = "ikplfdbdfikcokpegkapjdedlgifmkbe";
/// The native messaging host name the extension connects to.
pub const HOST_NAME: &str = "com.ks.downloadmanager";
/// The app's bundle identifier, for starting it through LaunchServices.
const BUNDLE_ID: &str = "com.ks.downloadmanager";
/// Longest message accepted in either direction.
pub const MAX_MESSAGE: usize = 8 * 1024;
/// How long the host waits for the app to start and open its socket.
const START_WAIT: Duration = Duration::from_secs(10);
/// `sun_path` holds 104 bytes on macOS, including the final NUL.
const MAX_SOCKET_PATH: usize = 103;
/// The fallback socket folder's name, followed by the user ID.
const FALLBACK_DIR: &str = "/tmp/ksdm-";
/// How long the app waits for a connection's one line before closing it.
const LINE_WAIT: Duration = Duration::from_secs(5);

pub fn origin() -> String {
    format!("chrome-extension://{EXTENSION_ID}/")
}

/// A request from the extension, as it arrives on the host's stdin and as it is passed on
/// to the app.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum Request {
    /// Is the app reachable?
    Ping { v: u32 },
    /// Add a download and start it.
    Add { v: u32, kind: AddKind, url: String },
    /// A second copy of the app was started: bring the window forward.
    Show { v: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AddKind {
    /// A video page (YouTube watch, Shorts, youtu.be).
    Video,
    /// A direct image URL from the hover button.
    Image,
    /// A file the user downloaded in Chrome (optional hand-off).
    File,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reply {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    /// A message code for the extension's own language files (`app_not_found`, …). No
    /// sentence is sent: the extension shows the code's text in the browser's language.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

impl Reply {
    pub fn ok(id: Option<u64>) -> Self {
        Self { ok: true, id, code: None }
    }
    pub fn err(code: &str) -> Self {
        Self { ok: false, id: None, code: Some(code.into()) }
    }
}

/// Checks a request's contents beyond its shape: version 1, an http(s) URL without user
/// name or password, of reasonable length.
pub fn validate(r: &Request) -> Result<(), Reply> {
    let v = match r {
        Request::Ping { v } | Request::Show { v } | Request::Add { v, .. } => *v,
    };
    if v != 1 {
        return Err(Reply::err("bad_request"));
    }
    if let Request::Add { url, .. } = r {
        if url.len() > 2048 {
            return Err(Reply::err("bad_url"));
        }
        let lower = url.to_ascii_lowercase();
        let rest = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://"));
        let Some(rest) = rest else { return Err(Reply::err("bad_url")) };
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        if authority.is_empty() || authority.contains('@') || url.chars().any(char::is_control) {
            return Err(Reply::err("bad_url"));
        }
    }
    Ok(())
}

// ---- Native messaging framing (Chrome): a 4-byte length in native byte order, then JSON.

pub fn read_frame(r: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut len = [0u8; 4];
    match r.read_exact(&mut len) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let n = u32::from_ne_bytes(len) as usize;
    if n > MAX_MESSAGE {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "message too long"));
    }
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf)?;
    Ok(Some(buf))
}

pub fn write_frame(w: &mut impl Write, body: &[u8]) -> io::Result<()> {
    let n = u32::try_from(body.len()).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "too long"))?;
    w.write_all(&n.to_ne_bytes())?;
    w.write_all(body)?;
    w.flush()
}

/// True when this process was started by Chrome as the native messaging host: its first
/// argument is an extension origin.
pub fn is_host_invocation(args: &[String]) -> bool {
    args.get(1).is_some_and(|a| a.starts_with("chrome-extension://"))
}

/// Host mode. Returns the process exit code. Any origin but ours is refused before a single
/// byte is read.
pub fn run_host(args: &[String], socket: &Path) -> i32 {
    if args.get(1).map(String::as_str) != Some(origin().as_str()) {
        eprintln!("refused: not our extension");
        return 1;
    }
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    serve_host(&mut stdin, &mut stdout, &|req| forward(socket, req))
}

/// The host loop, apart from the real stdin, stdout and socket, so it can be tested.
pub fn serve_host(input: &mut impl Read, output: &mut impl Write, forward: &dyn Fn(&Request) -> Reply) -> i32 {
    loop {
        let frame = match read_frame(input) {
            Ok(Some(f)) => f,
            Ok(None) => return 0,
            Err(_) => {
                let _ = reply(output, &Reply::err("bad_request"));
                return 1;
            }
        };
        let answer = match serde_json::from_slice::<Request>(&frame) {
            Err(_) => Reply::err("bad_request"),
            Ok(req) => match validate(&req) {
                Err(r) => r,
                Ok(()) => forward(&req),
            },
        };
        if reply(output, &answer).is_err() {
            return 1;
        }
    }
}

fn reply(output: &mut impl Write, r: &Reply) -> io::Result<()> {
    write_frame(output, &serde_json::to_vec(r).map_err(io::Error::other)?)
}

/// Sends one request to the app, starting the app first when its socket is not there.
fn forward(socket: &Path, req: &Request) -> Reply {
    if let Ok(r) = ask(socket, req) {
        return r;
    }
    if matches!(req, Request::Ping { .. }) {
        return Reply::err("app_not_running");
    }
    if !start_app() {
        return Reply::err("app_not_found");
    }
    let deadline = Instant::now() + START_WAIT;
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(150));
        if let Ok(r) = ask(socket, req) {
            return r;
        }
    }
    Reply::err("app_not_found")
}

/// Starts the app the way Finder does, through LaunchServices, so it is not a child of
/// Chrome (and is not closed with it).
#[cfg(target_os = "macos")]
fn start_app() -> bool {
    std::process::Command::new("/usr/bin/open")
        .args(["-g", "-b", BUNDLE_ID])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(not(target_os = "macos"))]
fn start_app() -> bool {
    // Windows: the Release stage (named pipe, start through the shell). Not built yet.
    let _ = BUNDLE_ID;
    false
}

#[cfg(unix)]
fn ask(socket: &Path, req: &Request) -> io::Result<Reply> {
    use std::io::{BufRead, BufReader};
    use std::os::unix::net::UnixStream;
    if let Some(dir) = fallback_dir(socket) {
        // Never talk to a socket in a folder someone else could have made.
        check_private_dir(dir, current_uid()?)?;
    }
    let mut s = UnixStream::connect(socket)?;
    s.set_read_timeout(Some(Duration::from_secs(120)))?;
    let mut line = serde_json::to_vec(req).map_err(io::Error::other)?;
    line.push(b'\n');
    s.write_all(&line)?;
    let mut answer = String::new();
    BufReader::new(s.take(MAX_MESSAGE as u64)).read_line(&mut answer)?;
    serde_json::from_str(&answer).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(not(unix))]
fn ask(_socket: &Path, _req: &Request) -> io::Result<Reply> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "Windows hand-off is not built yet"))
}

// ---- The app side: socket path, single-instance lock, server.

/// The socket path: in the app data folder, or, if that path is too long for a Unix
/// socket, in a private `/tmp/ksdm-<uid>/` folder. The host and the app compute it the
/// same way. The fallback folder is used only if it is a real folder (not a link), owned by
/// us, with mode `0700` ([`check_private_dir`]); `bind` and `ask` refuse it otherwise.
pub fn socket_path(data_dir: &Path) -> PathBuf {
    let preferred = data_dir.join("ext.sock");
    if preferred.as_os_str().len() <= MAX_SOCKET_PATH {
        return preferred;
    }
    // Without a user ID the folder name cannot be ours; `bind` and `ask` then refuse it.
    let uid = current_uid().map_or_else(|_| "unknown".to_owned(), |u| u.to_string());
    PathBuf::from(format!("{FALLBACK_DIR}{uid}")).join("ext.sock")
}

/// The fallback folder, if `socket` is in it.
fn fallback_dir(socket: &Path) -> Option<&Path> {
    socket.parent().filter(|d| d.to_str().is_some_and(|d| d.starts_with(FALLBACK_DIR)))
}

/// Our own user ID. std has no `geteuid` and no new crate is added, so it comes from
/// `/usr/bin/id -u` once per process.
#[cfg(unix)]
fn current_uid() -> io::Result<u32> {
    static UID: std::sync::OnceLock<Option<u32>> = std::sync::OnceLock::new();
    let uid = UID.get_or_init(|| {
        let out = std::process::Command::new("/usr/bin/id").arg("-u").stderr(std::process::Stdio::null()).output().ok()?;
        if !out.status.success() {
            return None;
        }
        String::from_utf8(out.stdout).ok()?.trim().parse().ok()
    });
    uid.ok_or_else(|| io::Error::other("could not read the user ID"))
}

#[cfg(not(unix))]
fn current_uid() -> io::Result<u32> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "no user IDs here"))
}

/// A folder in a shared place such as `/tmp` is trusted only if it is a real folder (not a
/// link), owned by `uid`, and closed to everyone else (mode `0700`).
#[cfg(unix)]
fn check_private_dir(dir: &Path, uid: u32) -> io::Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let refuse = |why: &str| Err(io::Error::new(io::ErrorKind::PermissionDenied, format!("{}: {why}", dir.display())));
    let meta = std::fs::symlink_metadata(dir)?;
    if !meta.file_type().is_dir() {
        return refuse("not a folder");
    }
    if meta.uid() != uid {
        return refuse("owned by another user");
    }
    if meta.permissions().mode() & 0o7777 != 0o700 {
        return refuse("not private (mode must be 0700)");
    }
    Ok(())
}

#[cfg(not(unix))]
fn check_private_dir(_dir: &Path, _uid: u32) -> io::Result<()> {
    Ok(())
}

/// The data folder the app uses (Tauri's `app_data_dir`), computed without Tauri for host
/// mode: `~/Library/Application Support/com.ks.downloadmanager` on macOS.
pub fn data_dir_without_tauri() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        Some(PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support").join(BUNDLE_ID))
    }
    #[cfg(not(target_os = "macos"))]
    {
        Some(PathBuf::from(std::env::var_os("APPDATA")?).join(BUNDLE_ID))
    }
}

pub enum Bound {
    #[cfg(unix)]
    Listener(std::os::unix::net::UnixListener),
    /// Another copy of the app owns the socket and answered.
    AlreadyRunning,
}

/// Binds the socket, which is also the single-instance lock. If the file is there, it is
/// only removed when nothing answers on it (a copy that crashed left it behind).
#[cfg(unix)]
pub fn bind(path: &Path) -> io::Result<Bound> {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    if let Some(dir) = fallback_dir(path) {
        prepare_private_dir(dir)?;
    } else if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let listener = match UnixListener::bind(path) {
        Ok(l) => l,
        Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
            if UnixStream::connect(path).is_ok() {
                return Ok(Bound::AlreadyRunning);
            }
            // Only a socket is ever removed, never a regular file of the same name.
            let meta = std::fs::symlink_metadata(path)?;
            use std::os::unix::fs::FileTypeExt;
            if !meta.file_type().is_socket() {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, "not a socket"));
            }
            std::fs::remove_file(path)?;
            UnixListener::bind(path)?
        }
        Err(e) => return Err(e),
    };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(Bound::Listener(listener))
}

/// Creates the folder `0700` if it is missing, then checks it. An existing folder is never
/// changed (it may not be ours): if it is not private, it is refused.
#[cfg(unix)]
fn prepare_private_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    match std::fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => {
            // The umask may have taken bits away, never added any; make it exact.
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    check_private_dir(dir, current_uid()?)
}

#[cfg(not(unix))]
pub fn bind(_path: &Path) -> io::Result<Bound> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "Windows hand-off is not built yet"))
}

/// Tells the copy of the app that owns the socket to show its window.
pub fn ask_to_show(path: &Path) -> bool {
    ask(path, &Request::Show { v: 1 }).is_ok_and(|r| r.ok)
}

/// What the app does with a request from the socket.
pub trait Handler: Send + Sync + 'static {
    fn handle(&self, req: Request) -> std::pin::Pin<Box<dyn std::future::Future<Output = Reply> + Send + '_>>;
}

/// Serves the socket: one line of JSON in, one line out, per connection. Only connections
/// from the socket owner's own user ID are answered. A connection that sends no complete
/// line within a few seconds is closed.
#[cfg(unix)]
pub async fn serve(listener: std::os::unix::net::UnixListener, path: PathBuf, handler: std::sync::Arc<dyn Handler>) {
    serve_with(listener, path, handler, LINE_WAIT).await;
}

#[cfg(unix)]
async fn serve_with(listener: std::os::unix::net::UnixListener, path: PathBuf, handler: std::sync::Arc<dyn Handler>, line_wait: Duration) {
    use std::os::unix::fs::MetadataExt;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    let Ok(()) = listener.set_nonblocking(true) else { return };
    let Ok(listener) = tokio::net::UnixListener::from_std(listener) else { return };
    let owner = std::fs::metadata(&path).map(|m| m.uid()).ok();
    loop {
        let Ok((stream, _)) = listener.accept().await else { continue };
        let same_user = stream.peer_cred().ok().map(|c| c.uid()) == owner && owner.is_some();
        let handler = handler.clone();
        tokio::spawn(async move {
            if !same_user {
                return;
            }
            let (read, mut write) = stream.into_split();
            let mut line = String::new();
            let mut reader = BufReader::new(read.take(MAX_MESSAGE as u64));
            match tokio::time::timeout(line_wait, reader.read_line(&mut line)).await {
                Ok(Ok(_)) if line.ends_with('\n') => {}
                _ => return, // too slow, cut short or failed: the connection is closed
            }
            let answer = match serde_json::from_str::<Request>(line.trim()) {
                Err(_) => Reply::err("bad_request"),
                Ok(req) => match validate(&req) {
                    Err(r) => r,
                    Ok(()) => handler.handle(req).await,
                },
            };
            if let Ok(mut out) = serde_json::to_vec(&answer) {
                out.push(b'\n');
                let _ = write.write_all(&out).await;
            }
        });
    }
}

/// Writes Chrome's host manifest so our extension can find this program. Only when Chrome's
/// own folder exists (Chrome is installed); never creates Chrome's folder. Skipped while the
/// app runs from a place that goes away (see [`is_temporary_location`]); the next start from
/// a lasting place writes it.
pub fn install_manifest(exe: &Path) -> io::Result<Option<PathBuf>> {
    if is_temporary_location(exe) {
        return Ok(None);
    }
    let Some(home) = std::env::var_os("HOME") else { return Ok(None) };
    let chrome = PathBuf::from(home).join("Library/Application Support/Google/Chrome");
    if !cfg!(target_os = "macos") || !chrome.is_dir() {
        return Ok(None);
    }
    let dir = chrome.join("NativeMessagingHosts");
    std::fs::create_dir_all(&dir)?;
    let file = dir.join(format!("{HOST_NAME}.json"));
    let body = manifest_json(exe);
    if std::fs::read_to_string(&file).ok().as_deref() != Some(body.as_str()) {
        ks_utility::atomic::write_atomic(&file, body.as_bytes())?;
    }
    Ok(Some(file))
}

/// True when the program runs from a mounted disk image (`/Volumes/…`) or from a copy
/// Gatekeeper moved to a random read-only folder (App Translocation). Both paths disappear,
/// so a manifest pointing there would break the extension later.
fn is_temporary_location(exe: &Path) -> bool {
    let s = exe.to_string_lossy();
    s.starts_with("/Volumes/") || s.contains("/AppTranslocation/")
}

fn manifest_json(exe: &Path) -> String {
    let v = serde_json::json!({
        "name": HOST_NAME,
        "description": "KS Download Manager",
        "path": exe,
        "type": "stdio",
        "allowed_origins": [origin()],
    });
    serde_json::to_string_pretty(&v).unwrap_or_default() + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn framed(json: &str) -> Vec<u8> {
        let mut v = Vec::new();
        write_frame(&mut v, json.as_bytes()).unwrap();
        v
    }

    fn replies(mut out: &[u8]) -> Vec<Reply> {
        let mut all = Vec::new();
        while let Some(f) = read_frame(&mut out).unwrap() {
            all.push(serde_json::from_slice(&f).unwrap());
        }
        all
    }

    #[test]
    fn requests_are_strictly_shaped() {
        let ok: Request = serde_json::from_str(r#"{"type":"add","v":1,"kind":"video","url":"https://www.youtube.com/watch?v=x"}"#).unwrap();
        assert!(validate(&ok).is_ok());
        for bad in [
            r#"{"type":"add","v":1,"kind":"video","url":"https://a/","cookies":"x"}"#,
            r#"{"type":"add","v":1,"kind":"program","url":"https://a/"}"#,
            r#"{"type":"run","v":1}"#,
        ] {
            assert!(serde_json::from_str::<Request>(bad).is_err(), "{bad}");
        }
        let add = |url: &str| Request::Add { v: 1, kind: AddKind::Image, url: url.into() };
        for url in ["file:///etc/passwd", "javascript:alert(1)", "https://user:pw@example.com/a.jpg", "https://", "https://a/\nb"] {
            assert!(validate(&add(url)).is_err(), "{url}");
        }
        assert!(validate(&add(&format!("https://a/{}", "x".repeat(3000)))).is_err());
        assert!(validate(&Request::Ping { v: 2 }).is_err());
        assert!(validate(&add("http://pbs.twimg.com/media/A?format=jpg&name=orig")).is_ok());
    }

    #[test]
    fn the_host_answers_each_message_and_stops_at_the_end() {
        let mut input = framed(r#"{"type":"ping","v":1}"#);
        input.extend(framed(r#"{"type":"add","v":1,"kind":"image","url":"ftp://x/"}"#));
        input.extend(framed("not json"));
        let mut out = Vec::new();
        let code = serve_host(&mut input.as_slice(), &mut out, &|_| Reply::ok(Some(7)));
        assert_eq!(code, 0);
        let r = replies(&out);
        assert_eq!(r.len(), 3);
        assert_eq!(r[0], Reply::ok(Some(7)));
        assert_eq!(r[1].code.as_deref(), Some("bad_url"), "never forwarded");
        assert_eq!(r[2].code.as_deref(), Some("bad_request"));
    }

    #[test]
    fn an_oversized_frame_is_refused() {
        let mut input = (MAX_MESSAGE as u32 + 1).to_ne_bytes().to_vec();
        input.extend(vec![b' '; MAX_MESSAGE + 1]);
        let mut out = Vec::new();
        assert_eq!(serve_host(&mut input.as_slice(), &mut out, &|_| unreachable!()), 1);
    }

    #[test]
    fn only_our_extension_is_a_host_caller() {
        let args = |o: &str| vec!["app".to_string(), o.to_string()];
        assert!(is_host_invocation(&args("chrome-extension://abc/")));
        assert!(!is_host_invocation(&["app".to_string()]));
        assert_eq!(run_host(&args("chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/"), Path::new("/nonexistent")), 1);
        assert_eq!(run_host(&args(&format!("{}x", origin())), Path::new("/nonexistent")), 1);
    }

    #[test]
    fn socket_paths_fit_the_unix_limit() {
        let short = Path::new("/Users/me/Library/Application Support/com.ks.downloadmanager");
        assert_eq!(socket_path(short), short.join("ext.sock"));
        let long = PathBuf::from(format!("/Users/{}/Library/Application Support/com.ks.downloadmanager", "u".repeat(80)));
        let p = socket_path(&long);
        assert!(p.to_string_lossy().starts_with("/tmp/ksdm-") && p.as_os_str().len() <= MAX_SOCKET_PATH, "{}", p.display());
    }

    #[cfg(unix)]
    #[test]
    fn the_socket_is_a_single_instance_lock_and_stale_files_are_replaced() {
        let dir = std::env::temp_dir().join(format!("ksh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.sock");
        let first = bind(&path).unwrap();
        assert!(matches!(first, Bound::Listener(_)));
        assert!(matches!(bind(&path).unwrap(), Bound::AlreadyRunning), "a live socket is not taken over");
        drop(first);
        // The file is left behind, nobody answers: it is replaced.
        assert!(path.exists());
        assert!(matches!(bind(&path).unwrap(), Bound::Listener(_)));
        // A regular file with that name is never removed.
        let other = dir.join("file.sock");
        std::fs::write(&other, b"keep").unwrap();
        assert!(bind(&other).is_err());
        assert_eq!(std::fs::read(&other).unwrap(), b"keep");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn the_fallback_folder_must_be_a_private_folder_of_ours() {
        use std::os::unix::fs::PermissionsExt;
        let uid = current_uid().unwrap();
        let base = std::env::temp_dir().join(format!("ksp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        let good = base.join("good");
        prepare_private_dir(&good).unwrap();
        assert_eq!(std::fs::metadata(&good).unwrap().permissions().mode() & 0o7777, 0o700, "created 0700");
        check_private_dir(&good, uid).unwrap();
        assert!(check_private_dir(&good, uid + 1).is_err(), "another user's folder is refused");

        let open = base.join("open");
        std::fs::create_dir(&open).unwrap();
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(prepare_private_dir(&open).is_err(), "a folder others can read is refused");
        assert_eq!(std::fs::metadata(&open).unwrap().permissions().mode() & 0o7777, 0o755, "and left unchanged");

        let link = base.join("link");
        std::os::unix::fs::symlink(&good, &link).unwrap();
        assert!(prepare_private_dir(&link).is_err(), "a link, even to a good folder, is refused");

        let file = base.join("file");
        std::fs::write(&file, b"").unwrap();
        assert!(prepare_private_dir(&file).is_err(), "a file is refused");

        // `bind` and `ask` apply it to the /tmp fallback only.
        assert_eq!(fallback_dir(Path::new("/tmp/ksdm-501/ext.sock")), Some(Path::new("/tmp/ksdm-501")));
        assert_eq!(fallback_dir(&base.join("ext.sock")), None);
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[cfg(unix)]
    struct Echo;

    #[cfg(unix)]
    impl Handler for Echo {
        fn handle(&self, _req: Request) -> std::pin::Pin<Box<dyn std::future::Future<Output = Reply> + Send + '_>> {
            Box::pin(async { Reply::ok(None) })
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_connection_without_a_complete_line_is_closed() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let dir = std::env::temp_dir().join(format!("kss-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.sock");
        let Bound::Listener(listener) = bind(&path).unwrap() else { panic!("bound") };
        tokio::spawn(serve_with(listener, path.clone(), std::sync::Arc::new(Echo), Duration::from_millis(200)));

        // Half a line, then silence: closed without an answer.
        let mut slow = tokio::net::UnixStream::connect(&path).await.unwrap();
        slow.write_all(br#"{"type":"ping""#).await.unwrap();
        let mut got = Vec::new();
        let read = tokio::time::timeout(Duration::from_secs(3), slow.read_to_end(&mut got)).await;
        assert!(matches!(read, Ok(Ok(0))), "closed with nothing sent: {read:?} {got:?}");

        // A complete line is still answered.
        let mut ok = tokio::net::UnixStream::connect(&path).await.unwrap();
        ok.write_all(b"{\"type\":\"ping\",\"v\":1}\n").await.unwrap();
        let mut answer = String::new();
        tokio::time::timeout(Duration::from_secs(3), ok.read_to_string(&mut answer)).await.unwrap().unwrap();
        assert!(answer.contains("\"ok\":true"), "{answer}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn no_manifest_from_a_disk_image_or_a_translocated_copy() {
        let dmg = Path::new("/Volumes/KS Download Manager/KS Download Manager.app/Contents/MacOS/ks-download-manager");
        let moved = Path::new("/private/var/folders/x1/abc/T/AppTranslocation/0A1B-2C3D/d/KS Download Manager.app/Contents/MacOS/ks-download-manager");
        assert!(is_temporary_location(dmg));
        assert!(is_temporary_location(moved));
        assert_eq!(install_manifest(dmg).unwrap(), None);
        assert_eq!(install_manifest(moved).unwrap(), None);
        assert!(!is_temporary_location(Path::new("/Applications/KS Download Manager.app/Contents/MacOS/ks-download-manager")));
        assert!(!is_temporary_location(Path::new("/Users/a/Applications/KS Download Manager.app/Contents/MacOS/ks-download-manager")));
    }

    #[test]
    fn the_manifest_names_only_our_extension() {
        let v: serde_json::Value = serde_json::from_str(&manifest_json(Path::new("/Applications/K.app/Contents/MacOS/k"))).unwrap();
        assert_eq!(v["allowed_origins"], serde_json::json!([format!("chrome-extension://{EXTENSION_ID}/")]));
        assert_eq!(v["type"], "stdio");
        assert_eq!(v["name"], HOST_NAME);
    }
}
