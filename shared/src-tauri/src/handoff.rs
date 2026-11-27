//! Hand-off from the Chrome extension to the app (specs/extension.md).
//!
//! Chrome's native messaging starts this same program as a "host", with the calling
//! extension's origin as its argument, and talks to it over stdin/stdout. The host checks
//! that origin against our one extension ID, then forwards each message to the running app
//! over a Unix socket in the app data folder (macOS) or a local named pipe (Windows),
//! starting the app if it is not running. The socket or pipe is also the app's
//! single-instance lock.
//!
//! What stops others from adding downloads:
//! - Websites cannot use native messaging at all; Chrome only lets the extensions listed in
//!   the host manifest's `allowed_origins` (ours alone) start the host.
//! - The host refuses any origin argument but ours, exactly.
//! - The socket lives in the user's own app data folder, is created `0600`, and every
//!   connection's peer user ID must equal the socket owner's.
//! - Windows: the pipe refuses network clients, and each side checks that the process at
//!   the other end runs as the same Windows user (its token's user SID), so a pipe of the
//!   same name made first by another user on the PC is never used. The host opens the pipe
//!   at the identification level only, so the app at the other end cannot act as the user.
//! - Messages are small (8 KB), strictly shaped (unknown fields rejected), and carry only an
//!   http(s) URL and a kind. Nothing is run, opened or saved anywhere but the save folder.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Our extension's ID, fixed by the public `key` in `extension/manifest.json` (the DEV build:
/// `extension/dev.json`, so the DEV app answers only the DEV extension).
#[cfg(not(feature = "dev"))]
pub const EXTENSION_ID: &str = "ikplfdbdfikcokpegkapjdedlgifmkbe";
#[cfg(feature = "dev")]
pub const EXTENSION_ID: &str = "mhgmalnfheamcgcongjgdmepagonmmbc";
/// The app's bundle identifier (`tauri.conf.json`, or `tauri.dev.conf.json` for the DEV
/// build), for starting it through LaunchServices, its data folder and its pipe name.
#[cfg(not(feature = "dev"))]
pub const BUNDLE_ID: &str = "com.ks.downloadmanager";
#[cfg(feature = "dev")]
pub const BUNDLE_ID: &str = "com.ks.downloadmanager.dev";
/// The native messaging host name the extension connects to: the bundle identifier, so the
/// DEV build registers its own host and never replaces the production one.
pub const HOST_NAME: &str = BUNDLE_ID;
/// Longest message accepted in either direction.
pub const MAX_MESSAGE: usize = 32 * 1024;
/// How long the host waits for the app to start and open its socket.
const START_WAIT: Duration = Duration::from_secs(10);
/// `sun_path` holds 104 bytes on macOS, including the final NUL.
#[cfg(unix)]
const MAX_SOCKET_PATH: usize = 103;
/// The fallback socket folder's name, followed by the user ID.
#[cfg(unix)]
const FALLBACK_DIR: &str = "/tmp/ksdm-";
/// How long the app waits for a connection's one line before closing it.
const LINE_WAIT: Duration = Duration::from_secs(5);
/// How long the host waits for the app's answer.
const ANSWER_WAIT: Duration = Duration::from_secs(120);

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
    Add {
        v: u32,
        kind: AddKind,
        url: String,
        /// A browser download only (`kind: file`): what the browser would send with it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        headers: Option<BrowserHeaders>,
    },
    /// A second copy of the app was started: bring the window forward.
    Show { v: u32 },
}

/// For a download the browser started (specs/extension.md, "Browser downloads"): the
/// site's cookies for the file's link, the page that linked to it, and the browser's
/// identification. Used for that one download, kept in memory, never written to disk.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserHeaders {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cookie: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
}

/// Longest values accepted: a site's cookies, a referring page, a browser identification.
const MAX_COOKIE: usize = 16 * 1024;
const MAX_USER_AGENT: usize = 512;

fn http_url_ok(url: &str) -> bool {
    if url.len() > 2048 || url.chars().any(char::is_control) {
        return false;
    }
    let lower = url.to_ascii_lowercase();
    let Some(rest) = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://")) else { return false };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    !authority.is_empty() && !authority.contains('@')
}

/// Printable ASCII only: anything else could not be sent as a header value anyway.
fn header_text_ok(s: &str, max: usize) -> bool {
    s.len() <= max && s.bytes().all(|b| (0x20..0x7f).contains(&b))
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
    /// On a ping: the newer app version the app's last update check found, so the
    /// extension can say a new version is out (it never checks the network itself).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update: Option<String>,
}

impl Reply {
    pub fn ok(id: Option<u64>) -> Self {
        Self { ok: true, id, code: None, update: None }
    }
    pub fn err(code: &str) -> Self {
        Self { ok: false, id: None, code: Some(code.into()), update: None }
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
    if let Request::Add { url, kind, headers, .. } = r {
        if !http_url_ok(url) {
            return Err(Reply::err("bad_url"));
        }
        if let Some(h) = headers {
            let ok = *kind == AddKind::File
                && h.cookie.as_deref().is_none_or(|c| header_text_ok(c, MAX_COOKIE))
                && h.referer.as_deref().is_none_or(http_url_ok)
                && h.user_agent.as_deref().is_none_or(|u| header_text_ok(u, MAX_USER_AGENT));
            if !ok {
                return Err(Reply::err("bad_request"));
            }
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

/// Starts the app (this same program, without arguments) outside Chrome's job object, so it
/// is not closed with Chrome. If the job does not allow that, Explorer starts it instead.
#[cfg(windows)]
fn start_app() -> bool {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    use windows_sys::Win32::System::Threading::{CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS};
    let Ok(exe) = std::env::current_exe() else { return false };
    // Chrome's pipes are never handed on: the app gets no stdin, stdout or stderr.
    let spawn = |c: &mut Command| c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().is_ok();
    spawn(Command::new(&exe).creation_flags(CREATE_BREAKAWAY_FROM_JOB | DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP))
        || spawn(Command::new("explorer.exe").arg(&exe))
}

#[cfg(not(any(target_os = "macos", windows)))]
fn start_app() -> bool {
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
    s.set_read_timeout(Some(ANSWER_WAIT))?;
    let mut line = serde_json::to_vec(req).map_err(io::Error::other)?;
    line.push(b'\n');
    s.write_all(&line)?;
    let mut answer = String::new();
    BufReader::new(s.take(MAX_MESSAGE as u64)).read_line(&mut answer)?;
    serde_json::from_str(&answer).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// One request over the pipe, from a plain (blocking) file handle: the host has no async
/// runtime. The answer is awaited on a helper thread, so a stuck app cannot hold the host.
#[cfg(windows)]
fn ask(pipe: &Path, req: &Request) -> io::Result<Reply> {
    use std::io::{BufRead, BufReader};
    let file = open_pipe(pipe)?;
    if !win::other_end_is_same_user(&file, win::End::Server)? {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "the pipe belongs to another user"));
    }
    let mut line = serde_json::to_vec(req).map_err(io::Error::other)?;
    line.push(b'\n');
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let talk = || -> io::Result<String> {
            (&file).write_all(&line)?;
            let mut answer = String::new();
            BufReader::new((&file).take(MAX_MESSAGE as u64)).read_line(&mut answer)?;
            Ok(answer)
        };
        let _ = tx.send(talk());
    });
    let answer = rx.recv_timeout(ANSWER_WAIT).map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "no answer"))??;
    serde_json::from_str(&answer).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Opens the app's pipe as a client. A busy pipe (every instance in use for a moment) is
/// tried again for up to a second.
#[cfg(windows)]
fn open_pipe(pipe: &Path) -> io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Foundation::ERROR_PIPE_BUSY;
    use windows_sys::Win32::Storage::FileSystem::SECURITY_IDENTIFICATION;
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        // Identification level: the server may learn who we are, never act as us.
        match std::fs::OpenOptions::new().read(true).write(true).security_qos_flags(SECURITY_IDENTIFICATION).open(pipe) {
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            other => return other,
        }
    }
}

#[cfg(not(any(unix, windows)))]
fn ask(_socket: &Path, _req: &Request) -> io::Result<Reply> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "no hand-off on this system"))
}

// ---- The app side: socket path, single-instance lock, server.

/// The socket path: in the app data folder, or, if that path is too long for a Unix
/// socket, in a private `/tmp/ksdm-<uid>/` folder. The host and the app compute it the
/// same way. The fallback folder is used only if it is a real folder (not a link), owned by
/// us, with mode `0700` ([`check_private_dir`]); `bind` and `ask` refuse it otherwise.
#[cfg(unix)]
pub fn socket_path(data_dir: &Path) -> PathBuf {
    let preferred = data_dir.join("ext.sock");
    if preferred.as_os_str().len() <= MAX_SOCKET_PATH {
        return preferred;
    }
    // Without a user ID the folder name cannot be ours; `bind` and `ask` then refuse it.
    let uid = current_uid().map_or_else(|_| "unknown".to_owned(), |u| u.to_string());
    PathBuf::from(format!("{FALLBACK_DIR}{uid}")).join("ext.sock")
}

/// Windows: the pipe `\\.\pipe\com.ks.downloadmanager-<user name>`, the same for the host
/// and the app (the data folder is not used). The name only keeps two users' copies apart;
/// who is at the other end is checked on every connection ([`win::other_end_is_same_user`]).
#[cfg(windows)]
pub fn socket_path(_data_dir: &Path) -> PathBuf {
    pipe_name(&std::env::var("USERNAME").unwrap_or_default())
}

#[cfg(windows)]
fn pipe_name(user: &str) -> PathBuf {
    // A backslash would end the name; everything else is allowed in a pipe name.
    let user = if user.is_empty() { "unknown".to_owned() } else { user.replace('\\', "_") };
    PathBuf::from(format!(r"\\.\pipe\{BUNDLE_ID}-{user}"))
}

#[cfg(not(any(unix, windows)))]
pub fn socket_path(data_dir: &Path) -> PathBuf {
    data_dir.join("ext.sock")
}

/// The fallback folder, if `socket` is in it.
#[cfg(unix)]
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

/// What the app listens on: a Unix socket, or the first instance of the Windows pipe.
#[cfg(unix)]
pub type Listener = std::os::unix::net::UnixListener;
#[cfg(windows)]
pub type Listener = tokio::net::windows::named_pipe::NamedPipeServer;

pub enum Bound {
    #[cfg(any(unix, windows))]
    Listener(Listener),
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

/// Creates the first instance of the pipe, which is also the single-instance lock. If the
/// pipe exists, the app at the other end must be ours (same user) to count as running;
/// another user's pipe of that name is an error, and the app then runs without the
/// extension. Needs a Tokio runtime context (the pipe is registered with its reactor).
#[cfg(windows)]
pub fn bind(path: &Path) -> io::Result<Bound> {
    match tokio::net::windows::named_pipe::ServerOptions::new()
        .first_pipe_instance(true)
        .reject_remote_clients(true)
        .create(path)
    {
        Ok(server) => Ok(Bound::Listener(server)),
        Err(e) => {
            let Ok(file) = open_pipe(path) else { return Err(e) };
            if win::other_end_is_same_user(&file, win::End::Server)? {
                Ok(Bound::AlreadyRunning)
            } else {
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "the pipe belongs to another user"))
            }
        }
    }
}

#[cfg(not(any(unix, windows)))]
pub fn bind(_path: &Path) -> io::Result<Bound> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "no hand-off on this system"))
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
    use tokio::io::{AsyncReadExt, BufReader};
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
            let mut reader = BufReader::new(read.take(MAX_MESSAGE as u64));
            answer_one(&mut reader, &mut write, &*handler, line_wait).await;
        });
    }
}

/// Serves the pipe: the same protocol as the Unix socket. Each connected instance is
/// handed to a task and a new instance is made for the next client. Only clients running
/// as our own Windows user are answered.
#[cfg(windows)]
pub async fn serve(listener: Listener, path: PathBuf, handler: std::sync::Arc<dyn Handler>) {
    serve_with(listener, path, handler, LINE_WAIT).await;
}

#[cfg(windows)]
async fn serve_with(first: Listener, path: PathBuf, handler: std::sync::Arc<dyn Handler>, line_wait: Duration) {
    use tokio::io::{AsyncReadExt, BufReader};
    use tokio::net::windows::named_pipe::ServerOptions;
    let mut server = first;
    loop {
        let connected = server.connect().await.is_ok();
        let next = match ServerOptions::new().reject_remote_clients(true).create(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("the Chrome extension can no longer reach the app: {e}");
                return;
            }
        };
        let pipe = std::mem::replace(&mut server, next);
        let same_user = connected && win::other_end_is_same_user(&pipe, win::End::Client).unwrap_or(false);
        let handler = handler.clone();
        tokio::spawn(async move {
            if !same_user {
                return;
            }
            let (read, mut write) = tokio::io::split(pipe);
            let mut reader = BufReader::new(read.take(MAX_MESSAGE as u64));
            if answer_one(&mut reader, &mut write, &*handler, line_wait).await {
                // Closing our end could drop an answer the client has not read yet: wait
                // (briefly) until the client closes its end.
                let mut rest = [0u8; 64];
                let read = reader.get_mut().get_mut();
                let _ = tokio::time::timeout(LINE_WAIT, async { while matches!(read.read(&mut rest).await, Ok(n) if n > 0) {} }).await;
            }
        });
    }
}

/// Reads one line (within `line_wait`), answers it with one line. False when the
/// connection was closed without an answer (too slow, cut short or failed).
#[cfg(any(unix, windows))]
async fn answer_one(
    reader: &mut (impl tokio::io::AsyncBufRead + Unpin),
    write: &mut (impl tokio::io::AsyncWrite + Unpin),
    handler: &dyn Handler,
    line_wait: Duration,
) -> bool {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let mut line = String::new();
    match tokio::time::timeout(line_wait, reader.read_line(&mut line)).await {
        Ok(Ok(_)) if line.ends_with('\n') => {}
        _ => return false,
    }
    let answer = match serde_json::from_str::<Request>(line.trim()) {
        Err(_) => Reply::err("bad_request"),
        Ok(req) => match validate(&req) {
            Err(r) => r,
            Ok(()) => handler.handle(req).await,
        },
    };
    let Ok(mut out) = serde_json::to_vec(&answer) else { return false };
    out.push(b'\n');
    write.write_all(&out).await.is_ok()
}

/// Writes the host manifest for Chrome and for Microsoft Edge (which runs the same unpacked
/// extension, with the same ID), so our extension can find this program. Only for a browser
/// whose own folder exists (it is installed); never creates a browser's folder. Skipped while the
/// app runs from a place that goes away (see [`is_temporary_location`]); the next start from
/// a lasting place writes it.
#[cfg(not(windows))]
pub fn install_manifest(exe: &Path) -> io::Result<Option<PathBuf>> {
    if is_temporary_location(exe) {
        return Ok(None);
    }
    let Some(home) = std::env::var_os("HOME") else { return Ok(None) };
    if !cfg!(target_os = "macos") {
        return Ok(None);
    }
    let support = PathBuf::from(home).join("Library/Application Support");
    let mut written = None;
    for browser in MAC_BROWSERS {
        let dir = support.join(browser);
        if dir.is_dir() {
            written = Some(write_manifest_in(&dir.join("NativeMessagingHosts"), exe)?);
        }
    }
    Ok(written)
}

/// The browsers' folders in `~/Library/Application Support`: Chrome, then Edge.
#[cfg(not(windows))]
const MAC_BROWSERS: [&str; 2] = ["Google/Chrome", "Microsoft Edge"];

/// Writes the manifest into `dir` (created if missing), only when it differs.
#[cfg(not(windows))]
fn write_manifest_in(dir: &Path, exe: &Path) -> io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let file = dir.join(format!("{HOST_NAME}.json"));
    let body = manifest_json(exe);
    if std::fs::read_to_string(&file).ok().as_deref() != Some(body.as_str()) {
        ks_utility::atomic::write_atomic(&file, body.as_bytes())?;
    }
    Ok(file)
}

/// Windows: the manifest file goes next to the program (`<install folder>\com.ks.downloadmanager.json`)
/// and Chrome finds it through `HKCU\Software\Google\Chrome\NativeMessagingHosts\com.ks.downloadmanager`,
/// Edge through the same name under `HKCU\Software\Microsoft\Edge\NativeMessagingHosts`.
/// The installer writes them (`--register-chrome-host`) and the uninstaller deletes them;
/// each start rewrites whichever is missing or points elsewhere, so a moved copy heals
/// itself. Only for a browser whose own folder exists (`%LOCALAPPDATA%\Google\Chrome`,
/// `%LOCALAPPDATA%\Microsoft\Edge`).
#[cfg(windows)]
pub fn install_manifest(exe: &Path) -> io::Result<Option<PathBuf>> {
    if is_temporary_location(exe) {
        return Ok(None);
    }
    let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) else { return Ok(None) };
    let mut written = None;
    for (folder, hosts_key) in WINDOWS_BROWSERS {
        if local.join(folder).is_dir() {
            written = Some(register_windows(exe, hosts_key)?);
        }
    }
    Ok(written)
}

/// Each browser's folder in `%LOCALAPPDATA%`, and where it looks up native messaging hosts
/// for the current user: Chrome, then Edge.
#[cfg(windows)]
const WINDOWS_BROWSERS: [(&str, &str); 2] = [
    (r"Google\Chrome", r"HKCU\Software\Google\Chrome\NativeMessagingHosts"),
    (r"Microsoft\Edge", r"HKCU\Software\Microsoft\Edge\NativeMessagingHosts"),
];

/// Writes the manifest next to `exe` and names it in `<hosts_key>\<HOST_NAME>`, each only
/// when it differs. `hosts_key` is a parameter so tests use a key of their own.
#[cfg(windows)]
fn register_windows(exe: &Path, hosts_key: &str) -> io::Result<PathBuf> {
    let dir = exe.parent().ok_or_else(|| io::Error::other("the program has no folder"))?;
    let file = dir.join(format!("{HOST_NAME}.json"));
    let body = manifest_json(exe);
    if std::fs::read_to_string(&file).ok().as_deref() != Some(body.as_str()) {
        ks_utility::atomic::write_atomic(&file, body.as_bytes())?;
    }
    let value = file.to_str().ok_or_else(|| io::Error::other("the install folder's name is not valid Unicode"))?;
    let key = format!(r"{hosts_key}\{HOST_NAME}");
    // A value reg.exe printed in another code page reads as different and is simply rewritten.
    if reg::default_value(&key)?.as_deref() != Some(value) {
        reg::set_default(&key, value)?;
    }
    Ok(file)
}

/// The registry through the system's `reg.exe` (no crate).
#[cfg(windows)]
mod reg {
    use std::io;
    use std::process::Stdio;

    fn reg(args: &[&str]) -> io::Result<std::process::Output> {
        crate::command::std("reg.exe").args(args).stdin(Stdio::null()).stderr(Stdio::null()).output()
    }

    /// A key's default value, or `None` when the key or the value is not there.
    pub fn default_value(key: &str) -> io::Result<Option<String>> {
        let out = reg(&["query", key, "/ve"])?;
        if !out.status.success() {
            return Ok(None);
        }
        // "    (Default)    REG_SZ    C:\…" (the label is in the system's language).
        let text = String::from_utf8_lossy(&out.stdout);
        Ok(text.lines().find_map(|l| l.split_once("REG_SZ").map(|(_, v)| v.trim().to_owned())))
    }

    pub fn set_default(key: &str, value: &str) -> io::Result<()> {
        check(reg(&["add", key, "/ve", "/t", "REG_SZ", "/d", value, "/f"])?)
    }

    #[cfg(test)]
    pub fn delete(key: &str) -> io::Result<()> {
        check(reg(&["delete", key, "/f"])?)
    }

    fn check(out: std::process::Output) -> io::Result<()> {
        if out.status.success() { Ok(()) } else { Err(io::Error::other(format!("reg.exe failed ({})", out.status))) }
    }
}

/// Who is at the other end of a pipe: the only `unsafe` code of the hand-off, a few calls
/// into Windows through `windows-sys` (docs/THIRD_PARTY.md).
#[cfg(windows)]
mod win {
    use std::io;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Security::{GetLengthSid, GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
    use windows_sys::Win32::System::Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId};
    use windows_sys::Win32::System::Threading::{OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION};

    /// Which end of the pipe to ask about.
    pub enum End {
        Client,
        Server,
    }

    /// True when the process at `end` of `pipe` runs as the same Windows user as this one.
    pub fn other_end_is_same_user(pipe: &impl AsRawHandle, end: End) -> io::Result<bool> {
        let handle = pipe.as_raw_handle();
        let mut pid = 0u32;
        // SAFETY: `handle` is a live pipe handle borrowed from `pipe` for this call; `pid`
        // is a valid place for the answer.
        let ok = unsafe {
            match end {
                End::Client => GetNamedPipeClientProcessId(handle, &mut pid),
                End::Server => GetNamedPipeServerProcessId(handle, &mut pid),
            }
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        is_same_user(pid)
    }

    /// True when process `pid` runs as our user. A process we may not look at is an error.
    pub fn is_same_user(pid: u32) -> io::Result<bool> {
        Ok(user_sid(pid)? == user_sid(std::process::id())?)
    }

    /// The user SID of process `pid`, as bytes.
    fn user_sid(pid: u32) -> io::Result<Vec<u8>> {
        // SAFETY: every handle is checked before use and owned, so it is closed on every
        // path; the token buffer is 8-byte aligned and as long as Windows asked for; the SID
        // points into that buffer, which is alive while the SID is copied.
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return Err(io::Error::last_os_error());
            }
            let process = OwnedHandle::from_raw_handle(process);
            let mut token = std::ptr::null_mut();
            if OpenProcessToken(process.as_raw_handle(), TOKEN_QUERY, &mut token) == 0 {
                return Err(io::Error::last_os_error());
            }
            let token = OwnedHandle::from_raw_handle(token);
            let mut len = 0u32;
            GetTokenInformation(token.as_raw_handle(), TokenUser, std::ptr::null_mut(), 0, &mut len);
            if len == 0 {
                return Err(io::Error::last_os_error());
            }
            let mut buf = vec![0u64; (len as usize).div_ceil(8)];
            if GetTokenInformation(token.as_raw_handle(), TokenUser, buf.as_mut_ptr().cast(), len, &mut len) == 0 {
                return Err(io::Error::last_os_error());
            }
            let sid = (*buf.as_ptr().cast::<TOKEN_USER>()).User.Sid;
            let n = GetLengthSid(sid) as usize;
            Ok(std::slice::from_raw_parts(sid.cast::<u8>(), n).to_vec())
        }
    }
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
        let add = |url: &str| Request::Add { v: 1, kind: AddKind::Image, url: url.into(), headers: None };
        for url in ["file:///etc/passwd", "javascript:alert(1)", "https://user:pw@example.com/a.jpg", "https://", "https://a/\nb"] {
            assert!(validate(&add(url)).is_err(), "{url}");
        }
        assert!(validate(&add(&format!("https://a/{}", "x".repeat(3000)))).is_err());
        assert!(validate(&Request::Ping { v: 2 }).is_err());
        assert!(validate(&add("http://pbs.twimg.com/media/A?format=jpg&name=orig")).is_ok());
    }

    #[test]
    fn browser_headers_come_only_with_a_browser_download_and_are_checked() {
        let ok: Request = serde_json::from_str(
            r#"{"type":"add","v":1,"kind":"file","url":"https://files.example.com/a.zip","headers":{"cookie":"s=1; t=2","referer":"https://example.com/page","userAgent":"Mozilla/5.0"}}"#,
        )
        .unwrap();
        assert!(validate(&ok).is_ok());
        let with = |kind: AddKind, h: BrowserHeaders| Request::Add { v: 1, kind, url: "https://a/b.zip".into(), headers: Some(h) };
        let h = BrowserHeaders { cookie: Some("s=1".into()), ..BrowserHeaders::default() };
        assert!(validate(&with(AddKind::Image, h.clone())).is_err(), "only for browser downloads");
        assert!(validate(&with(AddKind::File, BrowserHeaders { cookie: Some("a=1\r\nX: y".into()), ..BrowserHeaders::default() })).is_err(), "no line breaks");
        assert!(validate(&with(AddKind::File, BrowserHeaders { referer: Some("javascript:x".into()), ..BrowserHeaders::default() })).is_err());
        assert!(validate(&with(AddKind::File, BrowserHeaders { user_agent: Some("x".repeat(600)), ..BrowserHeaders::default() })).is_err());
        assert!(validate(&with(AddKind::File, BrowserHeaders { cookie: Some("x".repeat(MAX_COOKIE + 1)), ..BrowserHeaders::default() })).is_err());
        let unknown = r#"{"type":"add","v":1,"kind":"file","url":"https://a/b","headers":{"authorization":"x"}}"#;
        assert!(serde_json::from_str::<Request>(unknown).is_err(), "only the three known headers");
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

    #[cfg(unix)]
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
        // macOS cannot create a socket with close-on-exec set atomically, so a program another
        // test starts at that moment may hold a copy of the listener until it exits, and the
        // socket still answers meanwhile. Wait for that, briefly; the app itself binds before
        // it starts any program.
        let mut again = bind(&path).unwrap();
        for _ in 0..100 {
            if matches!(again, Bound::Listener(_)) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            again = bind(&path).unwrap();
        }
        assert!(matches!(again, Bound::Listener(_)));
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

    #[cfg(any(unix, windows))]
    struct Echo;

    #[cfg(any(unix, windows))]
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

    /// The manifest each browser's folder gets: created with its folder, rewritten only when
    /// it differs. Chrome and Edge are both looked for.
    #[cfg(not(windows))]
    #[test]
    fn the_manifest_is_written_into_a_browsers_hosts_folder() {
        assert_eq!(MAC_BROWSERS, ["Google/Chrome", "Microsoft Edge"]);
        let dir = std::env::temp_dir().join(format!("ks-hosts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let exe = Path::new("/Applications/KS Download Manager.app/Contents/MacOS/ks-download-manager");
        let file = write_manifest_in(&dir.join("NativeMessagingHosts"), exe).unwrap();
        assert_eq!(file, dir.join(format!("NativeMessagingHosts/{HOST_NAME}.json")));
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(v["path"], exe.to_str().unwrap());
        assert_eq!(v["allowed_origins"], serde_json::json!([origin()]));
        std::fs::write(&file, "old").unwrap();
        write_manifest_in(&dir.join("NativeMessagingHosts"), exe).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), manifest_json(exe));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A socket path (in a fresh folder) or pipe name of this test's own.
    #[cfg(unix)]
    fn endpoint(what: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ks{what}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("s.sock")
    }

    #[cfg(windows)]
    fn endpoint(what: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().subsec_nanos();
        pipe_name(&format!("test-{what}-{}-{nanos}", std::process::id()))
    }

    #[cfg(any(unix, windows))]
    #[tokio::test]
    async fn the_host_reaches_the_running_app() {
        let path = endpoint("trip");
        let Bound::Listener(listener) = bind(&path).unwrap() else { panic!("bound") };
        tokio::spawn(serve(listener, path.clone(), std::sync::Arc::new(Echo)));
        let host_path = path.clone();
        let (code, out) = tokio::task::spawn_blocking(move || {
            let mut input = framed(r#"{"type":"ping","v":1}"#);
            input.extend(framed(r#"{"type":"add","v":1,"kind":"image","url":"https://a.example/x.jpg"}"#));
            let mut out = Vec::new();
            let code = serve_host(&mut input.as_slice(), &mut out, &|req| forward(&host_path, req));
            (code, out)
        })
        .await
        .unwrap();
        assert_eq!(code, 0);
        assert_eq!(replies(&out), vec![Reply::ok(None), Reply::ok(None)], "both went to the app and back");
        #[cfg(unix)]
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn the_pipe_is_local_and_named_per_user() {
        assert_eq!(pipe_name("Asha"), PathBuf::from(r"\\.\pipe\com.ks.downloadmanager-Asha"));
        assert_eq!(pipe_name(r"PC\Asha"), PathBuf::from(r"\\.\pipe\com.ks.downloadmanager-PC_Asha"));
        assert_eq!(pipe_name(""), PathBuf::from(r"\\.\pipe\com.ks.downloadmanager-unknown"));
        let user = std::env::var("USERNAME").unwrap();
        assert_eq!(socket_path(Path::new(r"C:\ignored")), pipe_name(&user));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn the_pipe_is_a_single_instance_lock() {
        let name = endpoint("lock");
        let first = bind(&name).unwrap();
        assert!(matches!(first, Bound::Listener(_)));
        assert!(matches!(bind(&name).unwrap(), Bound::AlreadyRunning), "a live pipe is not taken over");
        drop(first);
        assert!(matches!(bind(&name).unwrap(), Bound::Listener(_)), "free again once the first copy is gone");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn a_pipe_connection_without_a_complete_line_is_closed() {
        use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
        use tokio::net::windows::named_pipe::ClientOptions;
        let name = endpoint("slow");
        let Bound::Listener(listener) = bind(&name).unwrap() else { panic!("bound") };
        tokio::spawn(serve_with(listener, name.clone(), std::sync::Arc::new(Echo), Duration::from_millis(200)));

        // Half a line, then silence: closed without an answer. Windows reports the closed
        // end as a broken pipe, Tokio may also report it as the end of the stream.
        let mut slow = ClientOptions::new().open(&name).unwrap();
        slow.write_all(br#"{"type":"ping""#).await.unwrap();
        let mut got = Vec::new();
        let read = tokio::time::timeout(Duration::from_secs(3), slow.read_to_end(&mut got)).await;
        let closed = matches!(read, Ok(Ok(0))) || matches!(&read, Ok(Err(e)) if e.kind() == io::ErrorKind::BrokenPipe);
        assert!(closed && got.is_empty(), "closed with nothing sent: {read:?} {got:?}");

        // A complete line is still answered, and the app at the other end is our own user.
        let ok = ClientOptions::new().open(&name).unwrap();
        assert!(win::other_end_is_same_user(&ok, win::End::Server).unwrap());
        let mut ok = BufReader::new(ok);
        ok.get_mut().write_all(b"{\"type\":\"ping\",\"v\":1}\n").await.unwrap();
        let mut answer = String::new();
        tokio::time::timeout(Duration::from_secs(3), ok.read_line(&mut answer)).await.unwrap().unwrap();
        assert!(answer.contains("\"ok\":true"), "{answer}");
    }

    #[cfg(windows)]
    #[test]
    fn another_users_process_is_not_ours() {
        assert!(win::is_same_user(std::process::id()).unwrap());
        // winlogon.exe runs as SYSTEM in every session. Refused either way: a different
        // SID, or a process we may not look at.
        let out = std::process::Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq winlogon.exe", "/FO", "CSV", "/NH"])
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        let pid: u32 = text.split(',').nth(1).and_then(|p| p.trim_matches('"').parse().ok()).unwrap_or_else(|| panic!("no winlogon: {text}"));
        assert!(!matches!(win::is_same_user(pid), Ok(true)), "winlogon ({pid}) is not our user");
    }

    #[cfg(windows)]
    #[test]
    fn the_host_is_registered_in_the_registry_and_healed() {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().subsec_nanos();
        let key = format!(r"HKCU\Software\KSDM-test-{}-{nanos}", std::process::id());
        let dir = std::env::temp_dir().join(format!("ks-reg-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("KS Download Manager.exe");
        std::fs::write(&exe, b"").unwrap();

        let file = register_windows(&exe, &key).unwrap();
        assert_eq!(file, dir.join("com.ks.downloadmanager.json"));
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(v["path"], exe.to_str().unwrap());
        assert_eq!(v["allowed_origins"], serde_json::json!([origin()]));
        let host = format!(r"{key}\{HOST_NAME}");
        assert_eq!(reg::default_value(&host).unwrap().as_deref(), file.to_str());

        // A key pointing elsewhere, and a missing file, are put right by the next start.
        reg::set_default(&host, r"C:\elsewhere\x.json").unwrap();
        std::fs::remove_file(&file).unwrap();
        register_windows(&exe, &key).unwrap();
        assert_eq!(reg::default_value(&host).unwrap().as_deref(), file.to_str());
        assert!(file.is_file());

        reg::delete(&key).unwrap();
        assert_eq!(reg::default_value(&host).unwrap(), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn the_data_folder_is_the_one_tauri_uses() {
        // Tauri's app_data_dir: the Roaming AppData known folder, then the identifier.
        let out = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false; [Console]::Out.Write([Environment]::GetFolderPath('ApplicationData'))",
            ])
            .output()
            .unwrap();
        let roaming = PathBuf::from(String::from_utf8(out.stdout).unwrap());
        assert!(roaming.is_absolute(), "{}", roaming.display());
        assert_eq!(data_dir_without_tauri(), Some(roaming.join(BUNDLE_ID)));
    }

    #[test]
    fn the_bundle_id_is_the_apps_identifier() {
        let prod: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let dev: serde_json::Value = serde_json::from_str(include_str!("../tauri.dev.conf.json")).unwrap();
        let conf = if cfg!(feature = "dev") { dev } else { prod };
        assert_eq!(conf["identifier"], BUNDLE_ID);
        assert_eq!(HOST_NAME, BUNDLE_ID);
        assert_eq!(crate::marker::APP, BUNDLE_ID);
    }

    #[test]
    fn the_manifest_names_only_our_extension() {
        let v: serde_json::Value = serde_json::from_str(&manifest_json(Path::new("/Applications/K.app/Contents/MacOS/k"))).unwrap();
        assert_eq!(v["allowed_origins"], serde_json::json!([format!("chrome-extension://{EXTENSION_ID}/")]));
        assert_eq!(v["type"], "stdio");
        assert_eq!(v["name"], HOST_NAME);
    }
}
