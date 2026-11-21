//! The download list and the downloads running from it. State lives here, in Rust; the
//! interface gets a full record on every status change and one batched progress event a
//! few times a second (specs/ui.md, "Performance rules"), never one per network chunk.
//!
//! Every command names a download by ID. Paths are chosen and looked up here; the
//! interface never sends one to open, pause, resume or delete.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ks_utility::file_name::sanitize_file_name;
use engine::{Adaptive, CancelToken, ConnectionBudget, DownloadOptions, DownloadOutcome, EngineError, HostLimits, ProbeInfo, Progress, SpeedLimit};
use serde::Serialize;

use crate::files::{self, Category};
use crate::leftovers;
use crate::marker;
use crate::settings::{self, Settings};
use crate::open;
use crate::store::{self, Kind, Queue, Record, Status};
use crate::video::{Tools, VideoError, VideoInfo, VideoProgress};

/// How often progress is sent to the interface (4 times a second).
pub const TICK: Duration = Duration::from_millis(250);
/// A probe from the Add dialog is reused when the download starts within this time.
const PROBE_FRESH: Duration = Duration::from_secs(60);
/// How long quitting waits for downloads to save their range maps.
const QUIT_WAIT: Duration = Duration::from_secs(3);
/// Weight of the newest sample in the smoothed speeds.
const SMOOTHING: f64 = 0.3;

const NOT_RESUMABLE: &str =
    "This server does not support resuming, so this download cannot be paused: stopping it loses what it has.";
const INTERRUPTED: &str = "Stopped before it finished, with nothing to resume from. Resume starts it over.";
/// `Record::error_code` for a failure that the "Update video component" action may fix.
pub const VIDEO_OUTDATED: &str = "videoOutdated";
/// Prefix of the hidden folder beside the destination where yt-dlp works.
pub(crate) const VIDEO_TEMP_PREFIX: &str = ".ksdownload-video-";

/// Where the manager sends updates; the app sends them to the window as events.
pub trait Events: Send + Sync {
    fn changed(&self, record: &Record);
    fn removed(&self, id: u64);
    fn progress(&self, ticks: &[Tick]);
    /// The user's queues changed (created, renamed, deleted).
    fn queues(&self, queues: &[Queue]);
}

/// Live numbers for one running download.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tick {
    pub id: u64,
    pub size: Option<u64>,
    pub received: u64,
    /// Bytes per second, smoothed.
    pub speed: f64,
    /// One entry per connection; `None` while a connection has no range.
    pub connections: Vec<Option<ConnectionTick>>,
    /// A video whose streams are all here and are being joined by ffmpeg.
    pub merging: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTick {
    /// The connection's current range `[start, end)`, of which `[start, position)` arrived.
    pub start: u64,
    pub position: u64,
    pub end: u64,
    pub speed: f64,
}

/// What the Add dialog shows before the download starts.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeView {
    pub kind: Kind,
    pub name: String,
    pub size: Option<u64>,
    pub resumable: bool,
    pub category: Category,
}

/// Counts for the quit confirmation.
#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Active {
    pub running: usize,
    pub not_resumable: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    Pause,
    Delete,
    Quit,
}

/// Where a running download's numbers come from.
#[derive(Clone)]
enum Source {
    Engine(Progress),
    Video(VideoProgress),
}

/// What a link turned out to be when it was looked up.
#[derive(Clone)]
enum Lookup {
    File(ProbeInfo),
    Video(VideoInfo),
}

struct Running {
    cancel: CancelToken,
    progress: Source,
    stop: Option<Stop>,
    /// Set when the download is deleted while running: what to clean up once it stops.
    deleted: Option<Record>,
    last_at: Instant,
    last_received: u64,
    last_per: Vec<u64>,
    speed: f64,
    connection_speed: Vec<f64>,
}

struct Inner {
    records: Vec<Record>,
    running: HashMap<u64, Running>,
    next_id: u64,
    /// In sidebar order; the main queue is not listed.
    queues: Vec<Queue>,
    /// Quitting: nothing more is started.
    quitting: bool,
}

pub struct Manager {
    inner: Mutex<Inner>,
    store_file: PathBuf,
    events: Arc<dyn Events>,
    probes: Mutex<HashMap<String, (Instant, Lookup)>>,
    video: Mutex<Option<Arc<Tools>>>,
    settings_file: PathBuf,
    settings: Mutex<Settings>,
    /// Shared by every download: the connection total, the speed limit, and what each host
    /// worked with this session.
    budget: ConnectionBudget,
    speed: SpeedLimit,
    hosts: HostLimits,
    /// The system Downloads folder, for categories without a folder of their own.
    default_folder: Mutex<Option<PathBuf>>,
    /// The last scan for leftover download files; `None` before the first.
    leftovers: Mutex<Option<Vec<leftovers::Found>>>,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// What to tell the user when yt-dlp gives no video, and what the interface can offer.
pub fn video_message(e: &VideoError) -> (String, Option<&'static str>) {
    match e {
        VideoError::NotAVideoSite => ("This link is a web page, not a file.".into(), None),
        VideoError::Outdated(detail) => (
            format!("The video component is out of date, so this site could not be read ({detail}). Update the video component, then try again."),
            Some(VIDEO_OUTDATED),
        ),
        VideoError::SignIn => ("This video needs a sign-in. Only videos anyone can watch without signing in are supported.".into(), None),
        VideoError::Blocked => (
            "The site is temporarily blocking downloads from this network. Try again later, or on another network.".into(),
            None,
        ),
        VideoError::Forbidden(detail) => (
            format!("The site refused the download ({detail}). It may be blocked in your region or need a sign-in; if it keeps happening, update the video component."),
            Some(VIDEO_OUTDATED),
        ),
        VideoError::Unavailable => ("This video is unavailable: private, removed or blocked in your region.".into(), None),
        VideoError::Drm => ("This video is protected by DRM. Protected videos are not supported.".into(), None),
        VideoError::Live => ("Live streams are not supported.".into(), None),
        VideoError::Runtime => (
            "The video component's JavaScript runtime did not run, so the best quality could not be reached. Nothing was saved.".into(),
            None,
        ),
        VideoError::Timeout => ("The video site took too long to answer. Try again later.".into(), None),
        VideoError::Start(e) => (format!("The video component could not be started: {e}."), Some(VIDEO_OUTDATED)),
        VideoError::Cancelled => ("Stopped.".into(), None),
        VideoError::Other(m) => (format!("The video could not be downloaded: {m}"), None),
    }
}

/// The hidden folder beside the destination where yt-dlp writes a video until it is done:
/// `.ksdownload-video-<id>`, or the other name recorded in `record.temp`. A recorded name
/// is used only when it is a single name of ours (a damaged list cannot point elsewhere).
pub fn video_temp(record: &Record) -> Option<PathBuf> {
    let default = format!("{VIDEO_TEMP_PREFIX}{}", record.id);
    let name = match record.temp.as_deref() {
        Some(t) if t.starts_with(&default) && !t.contains(['/', '\\']) && t != ".." => t.to_owned(),
        _ => default,
    };
    Some(record.path.parent()?.join(name))
}

/// The temp folder for this video, if it is there and its marker names this download.
fn own_video_temp(record: &Record) -> Option<PathBuf> {
    let dir = video_temp(record)?;
    marker::of_folder(&dir).filter(|m| m.is_for(record.id, &record.url)).map(|_| dir)
}

/// Bytes yt-dlp has written so far into a video's temp folder.
fn temp_bytes(dir: &Path) -> u64 {
    std::fs::read_dir(dir).map_or(0, |d| {
        d.flatten()
            .filter(|e| e.file_name() != marker::IN_FOLDER)
            .filter_map(|e| e.metadata().ok())
            .filter(|m| m.is_file())
            .map(|m| m.len())
            .sum()
    })
}

/// Removes a video's temp folder, only when its marker names this download, and only a
/// real folder (`own_video_temp` refuses links; `remove_dir_all` never follows links
/// inside).
fn remove_video_temp(record: &Record) {
    if let Some(dir) = own_video_temp(record) {
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(windows)]
fn hide(dir: &Path) {
    // The dot hides it on macOS; Windows needs the hidden attribute. Untested until the Windows build.
    let _ = std::process::Command::new("attrib").arg("+h").arg(dir).status();
}

#[cfg(not(windows))]
fn hide(_dir: &Path) {}

/// The engine's messages start in lower case; the interface shows them as sentences.
fn sentence(e: &EngineError) -> String {
    let s = e.to_string();
    let mut c = s.chars();
    c.next().map_or(s.clone(), |f| f.to_uppercase().chain(c).collect::<String>() + ".")
}

impl Manager {
    /// Loads the saved list, and the settings from the same folder. Call
    /// [`Manager::restore`] before showing the list.
    pub fn new(store_file: PathBuf, events: Arc<dyn Events>) -> Arc<Self> {
        let store::Saved { downloads: records, next_id, queues } = store::load_all(&store_file);
        let settings_file = store_file.with_file_name(settings::FILE_NAME);
        let s = settings::load(&settings_file);
        Arc::new(Self {
            inner: Mutex::new(Inner { records, running: HashMap::new(), next_id, queues, quitting: false }),
            store_file,
            events,
            probes: Mutex::new(HashMap::new()),
            video: Mutex::new(None),
            settings_file,
            budget: ConnectionBudget::new(s.total_connections),
            speed: SpeedLimit::new(s.speed_limit),
            hosts: HostLimits::default(),
            settings: Mutex::new(s),
            default_folder: Mutex::new(None),
            leftovers: Mutex::new(None),
        })
    }

    /// The bundled programs for video downloads (set once at start-up).
    pub fn set_video_tools(&self, tools: Arc<Tools>) {
        let s = self.settings();
        tools.set_prefer_mp4(s.prefer_mp4);
        tools.set_rate_limit(s.speed_limit);
        *self.video.lock().unwrap_or_else(|e| e.into_inner()) = Some(tools);
    }

    pub fn set_default_folder(&self, folder: PathBuf) {
        *self.default_folder.lock().unwrap_or_else(|e| e.into_inner()) = Some(folder);
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Checks, saves and applies new settings. Lower limits apply as running connections
    /// and downloads end; nothing running is stopped.
    pub fn set_settings(self: &Arc<Self>, new: Settings) -> Result<Settings, String> {
        let new = new.checked();
        for folder in new.folders.all() {
            files::check_folder(folder)?;
        }
        settings::save(&self.settings_file, &new).map_err(|e| format!("Could not save the settings: {e}."))?;
        self.budget.set(new.total_connections);
        self.speed.set(new.speed_limit);
        if let Some(tools) = self.video.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            tools.set_prefer_mp4(new.prefer_mp4);
            tools.set_rate_limit(new.speed_limit);
        }
        *self.settings.lock().unwrap_or_else(|e| e.into_inner()) = new.clone();
        self.schedule();
        Ok(new)
    }

    /// The save folder for a category: its own from Settings, else the system Downloads
    /// folder.
    pub fn folder_for(&self, category: Category) -> Option<String> {
        self.settings().folders.get(category).map(str::to_owned).or_else(|| {
            self.default_folder.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|p| p.to_string_lossy().into_owned())
        })
    }

    pub fn video_tools(&self) -> Result<Arc<Tools>, String> {
        self.video.lock().unwrap_or_else(|e| e.into_inner()).clone().ok_or_else(|| "Video downloads are not available.".into())
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Saves the list. Called with the lock held, so saves happen in order; the file is
    /// small and only written on status changes, never per progress tick.
    fn save(&self, inner: &Inner) {
        if let Err(e) = store::save(&self.store_file, &inner.records, inner.next_id, &inner.queues) {
            eprintln!("could not save the download list: {e}");
        }
    }

    /// After a restart: a download that was running or paused comes back paused when the
    /// range map of its `.ksdownload` file matches (`engine::saved_state`), with the bytes
    /// the map lists; otherwise it is marked interrupted and starts over when resumed, and
    /// a leftover `.ksdownload` file of it (useless without a map) is deleted. Nothing
    /// starts by itself.
    pub async fn restore(&self) {
        let all: Vec<(u64, String, PathBuf)> = self
            .lock()
            .records
            .iter()
            .filter(|r| r.kind == Kind::File)
            .map(|r| (r.id, r.url.clone(), r.path.clone()))
            .collect();
        let mut found = HashMap::new();
        for (id, url, path) in all {
            let saved = own_partial(&url, &path).await;
            if saved.is_none() {
                discard_partial(&path).await;
            }
            found.insert(id, saved);
        }
        let mut inner = self.lock();
        for r in &mut inner.records {
            if r.kind == Kind::Video {
                // yt-dlp continues from its .part files in the temp folder, if they are there.
                if matches!(r.status, Status::Downloading | Status::Waiting | Status::Paused | Status::Interrupted) {
                    let temp = own_video_temp(r);
                    r.received = temp.as_deref().map_or(0, temp_bytes);
                    r.status = if temp.is_some() { Status::Paused } else { Status::Queued };
                    r.error = None;
                    r.error_code = None;
                }
                continue;
            }
            let Some(saved) = found.get(&r.id) else { continue };
            match (r.status, saved) {
                (Status::Queued | Status::Finished, _) => {}
                // Was waiting for a slot and never got bytes: nothing starts by itself.
                (Status::Waiting, None) if r.received == 0 => r.status = Status::Queued,
                (Status::Downloading | Status::Waiting | Status::Paused | Status::Interrupted, Some(s)) => {
                    r.status = Status::Paused;
                    r.received = s.done_bytes();
                    r.size = Some(s.size);
                    r.resumable = true;
                    r.error = None;
                }
                (Status::Downloading | Status::Waiting | Status::Paused | Status::Interrupted, None) => {
                    r.status = Status::Interrupted;
                    r.received = 0;
                    r.error = Some(INTERRUPTED.into());
                }
                (_, s) => r.received = s.as_ref().map_or(0, engine::SavedState::done_bytes),
            }
        }
        self.save(&inner);
    }

    pub fn list(&self) -> Vec<Record> {
        self.lock().records.clone()
    }

    pub fn active(&self) -> Active {
        let inner = self.lock();
        let not_resumable = inner
            .running
            .keys()
            .filter(|id| inner.records.iter().any(|r| r.id == **id && !r.resumable))
            .count();
        Active { running: inner.running.len(), not_resumable }
    }

    /// The probe cache, with entries older than [`PROBE_FRESH`] dropped on every use.
    fn probes(&self) -> MutexGuard<'_, HashMap<String, (Instant, Lookup)>> {
        let mut probes = self.probes.lock().unwrap_or_else(|e| e.into_inner());
        probes.retain(|_, (at, _)| at.elapsed() < PROBE_FRESH);
        probes
    }

    async fn lookup_cached(&self, url: &str) -> Result<Lookup, (String, Option<&'static str>)> {
        let fresh = self.probes().get(url).map(|(_, p)| p.clone());
        if let Some(p) = fresh {
            return Ok(p);
        }
        let found = self.lookup(url).await?;
        self.probes().insert(url.to_owned(), (Instant::now(), found.clone()));
        Ok(found)
    }

    async fn probe_cached(&self, url: &str) -> Result<ProbeInfo, String> {
        match self.lookup_cached(url).await {
            Ok(Lookup::File(info)) => Ok(info),
            Ok(Lookup::Video(_)) => Err("This link is a video page, not a file.".into()),
            Err((message, _)) => Err(message),
        }
    }

    /// Our engine asks first. A link that answers with a web page is passed to yt-dlp,
    /// which knows the video sites; if no site extractor knows it either, it is a plain web
    /// page and is refused, never saved. Some video sites refuse plain requests (401, 403,
    /// 405, 429); those are tried with yt-dlp too, keeping the engine's error otherwise.
    async fn lookup(&self, url: &str) -> Result<Lookup, (String, Option<&'static str>)> {
        match engine::probe(url).await {
            Ok(info) if !info.is_web_page() => Ok(Lookup::File(info)),
            Ok(_) => self.lookup_video(url).await,
            Err(e @ EngineError::HttpStatus(401 | 403 | 405 | 429)) => match self.lookup_video(url).await {
                Ok(found) => Ok(found),
                Err(_) => Err((sentence(&e), None)),
            },
            Err(e) => Err((sentence(&e), None)),
        }
    }

    async fn lookup_video(&self, url: &str) -> Result<Lookup, (String, Option<&'static str>)> {
        let tools = self.video_tools().map_err(|m| (m, None))?;
        match tools.info(url, false).await {
            Ok(info) if info.is_playlist => {
                Err(("This is a playlist link. Open a single video from it, or use the playlist download.".into(), None))
            }
            Ok(info) => Ok(Lookup::Video(info)),
            Err(e) => Err(video_message(&e)),
        }
    }

    /// For the Add dialog: what the link is, its name, size, category and whether it can
    /// be paused.
    pub async fn probe(&self, url: &str) -> Result<ProbeView, String> {
        let url = url.trim();
        let found = self.lookup(url).await.map_err(|(m, _)| m)?;
        self.probes().insert(url.to_owned(), (Instant::now(), found.clone()));
        Ok(match found {
            Lookup::File(info) => {
                let name = sanitize_file_name(&files::name_with_type(&info.file_name, info.content_type.as_deref()));
                ProbeView { kind: Kind::File, category: files::category(&name), name, size: info.size, resumable: info.resumable() }
            }
            Lookup::Video(info) => ProbeView {
                kind: Kind::Video,
                name: video_name(&info.title, &info.ext),
                size: info.size,
                resumable: true,
                category: Category::Video,
            },
        })
    }

    /// Adds a download to the list, reserving a file name that does not exist yet in
    /// `folder` ("name (1).ext" and so on). `start` is "Download now"; otherwise it waits
    /// in the list until resumed.
    pub async fn add(self: &Arc<Self>, url: &str, name: &str, folder: &str, start: bool) -> Result<Record, String> {
        let url = url.trim();
        let info = match self.lookup_cached(url).await.map_err(|(m, _)| m)? {
            Lookup::File(info) => info,
            Lookup::Video(video) => return self.add_video(url, name, folder, start, &video),
        };
        let folder = files::check_folder(folder)?;
        let suggested = files::name_with_type(&info.file_name, info.content_type.as_deref());
        let name = sanitize_file_name(if name.trim().is_empty() { &suggested } else { name });
        let record = {
            let mut inner = self.lock();
            let taken = |p: &Path| inner.records.iter().any(|r| r.path == p);
            let path = files::reserve(&folder, &name, &taken).map_err(|e| format!("Could not create the file: {e}."))?;
            let name = path.file_name().map_or(name.clone(), |n| n.to_string_lossy().into_owned());
            let record = Record {
                id: inner.next_id,
                kind: Kind::File,
                error_code: None,
                url: url.to_owned(),
                category: files::category(&name),
                name,
                path,
                status: Status::Queued,
                size: info.size,
                received: 0,
                resumable: info.resumable(),
                error: None,
                temp: None,
                added_at: now_ms(),
                queue: None,
            };
            inner.next_id += 1;
            inner.records.push(record.clone());
            self.save(&inner);
            record
        };
        self.events.changed(&record);
        if start {
            self.resume(record.id)?;
        }
        Ok(self.lock().records.iter().find(|r| r.id == record.id).cloned().unwrap_or(record))
    }

    /// Adds a video. Its final name is not reserved now: yt-dlp writes to a fixed name in
    /// its own temp folder, and only when it is done is "title.<real extension>" reserved
    /// and the file renamed into place ([`Manager::complete_video`]). `record.path` holds
    /// the planned name until then.
    fn add_video(self: &Arc<Self>, url: &str, name: &str, folder: &str, start: bool, video: &VideoInfo) -> Result<Record, String> {
        let folder = files::check_folder(folder)?;
        let name = if name.trim().is_empty() { video_name(&video.title, &video.ext) } else { sanitize_file_name(name) };
        let record = {
            let mut inner = self.lock();
            let record = Record {
                id: inner.next_id,
                kind: Kind::Video,
                url: url.to_owned(),
                category: Category::Video,
                path: folder.join(&name),
                name,
                status: Status::Queued,
                size: video.size,
                received: 0,
                resumable: true,
                error: None,
                error_code: None,
                temp: None,
                added_at: now_ms(),
                queue: None,
            };
            inner.next_id += 1;
            inner.records.push(record.clone());
            self.save(&inner);
            record
        };
        self.events.changed(&record);
        if start {
            self.resume(record.id)?;
        }
        Ok(self.lock().records.iter().find(|r| r.id == record.id).cloned().unwrap_or(record))
    }

    /// A download handed over by the Chrome extension. The link is checked against what
    /// the extension said it is: an image must be an image file, a browser download must be
    /// a file (not a page), a video must be a video page. Started at once (or waits its
    /// turn), in its category's folder.
    pub async fn add_from_extension(self: &Arc<Self>, kind: crate::handoff::AddKind, url: &str) -> crate::handoff::Reply {
        use crate::handoff::{AddKind, Reply};
        let url = url.trim();
        let found = match self.lookup_cached(url).await {
            Ok(f) => f,
            Err((message, code)) => return Reply::err(code.unwrap_or("lookup_failed"), message),
        };
        let fits = match (&found, kind) {
            (Lookup::File(info), AddKind::Image) => {
                info.content_type.as_deref().is_some_and(|t| t.starts_with("image/"))
                    || files::category(&info.file_name) == Category::Images
            }
            (Lookup::File(_), AddKind::File | AddKind::Video) | (Lookup::Video(_), AddKind::Video) => true,
            (Lookup::Video(_), AddKind::Image | AddKind::File) => false,
        };
        if !fits {
            return Reply::err("wrong_kind", match kind {
                AddKind::Image => "This link is not an image file.",
                _ => "This link is not a file.",
            });
        }
        let category = match &found {
            Lookup::Video(_) => Category::Video,
            Lookup::File(info) => files::category(&files::name_with_type(&info.file_name, info.content_type.as_deref())),
        };
        let Some(folder) = self.folder_for(category) else {
            return Reply::err("add_failed", "No downloads folder.");
        };
        match self.add(url, "", &folder, true).await {
            Ok(r) => Reply::ok(Some(r.id)),
            Err(e) => Reply::err("add_failed", e),
        }
    }

    /// A queued, paused, failed or interrupted download wants to run: it is marked
    /// Waiting and starts as soon as a slot is free ([`Manager::schedule`]).
    pub fn resume(self: &Arc<Self>, id: u64) -> Result<(), String> {
        let changed = {
            let mut inner = self.lock();
            if inner.running.contains_key(&id) {
                return Ok(());
            }
            let r = inner.records.iter_mut().find(|r| r.id == id).ok_or("No such download.")?;
            if r.status == Status::Finished {
                return Err("This download has already finished.".into());
            }
            let changed = (r.status != Status::Waiting).then(|| {
                r.status = Status::Waiting;
                r.error = None;
                r.error_code = None;
                r.clone()
            });
            if changed.is_some() {
                self.save(&inner);
            }
            changed
        };
        if let Some(r) = changed {
            self.events.changed(&r);
        }
        self.schedule();
        Ok(())
    }

    /// Starts waiting downloads while fewer than the active-download cap run: the main
    /// queue first, then the queues in sidebar order, and within a queue by date added.
    /// Videos count toward the cap like files.
    fn schedule(self: &Arc<Self>) {
        let max = self.settings().max_active;
        loop {
            let next = {
                let inner = self.lock();
                if inner.quitting || inner.running.len() >= max {
                    return;
                }
                let order = |q: Option<u64>| {
                    q.map_or(0, |q| inner.queues.iter().position(|x| x.id == q).map_or(usize::MAX, |i| i + 1))
                };
                inner
                    .records
                    .iter()
                    .filter(|r| r.status == Status::Waiting && !inner.running.contains_key(&r.id))
                    .min_by_key(|r| (order(r.queue), r.added_at, r.id))
                    .map(|r| r.id)
            };
            let Some(id) = next else { return };
            if self.start(id).is_err() {
                return;
            }
        }
    }

    /// Starts a download now.
    fn start(self: &Arc<Self>, id: u64) -> Result<(), String> {
        let (cancel, progress, record) = {
            let mut inner = self.lock();
            if inner.running.contains_key(&id) {
                return Ok(());
            }
            let r = inner.records.iter_mut().find(|r| r.id == id).ok_or("No such download.")?;
            if r.status == Status::Finished {
                return Err("This download has already finished.".into());
            }
            r.status = Status::Downloading;
            r.error = None;
            r.error_code = None;
            let record = r.clone();
            let cancel = CancelToken::new();
            let progress = match record.kind {
                Kind::File => Source::Engine(Progress::new()),
                Kind::Video => Source::Video(VideoProgress::default()),
            };
            inner.running.insert(
                id,
                Running {
                    cancel: cancel.clone(),
                    progress: progress.clone(),
                    stop: None,
                    deleted: None,
                    last_at: Instant::now(),
                    last_received: record.received,
                    last_per: Vec::new(),
                    speed: 0.0,
                    connection_speed: Vec::new(),
                },
            );
            self.save(&inner);
            (cancel, progress, record)
        };
        self.events.changed(&record);
        let me = self.clone();
        tauri::async_runtime::spawn(async move {
            let result = match progress {
                Source::Engine(progress) => me.run(&record, cancel, progress).await.map(|o| o.bytes),
                Source::Video(progress) => me.run_video(&record, cancel, progress).await,
            };
            me.finish(id, result).await;
            me.schedule();
        });
        Ok(())
    }

    /// Picks the destination (resume in place, or a free name), probes, downloads into
    /// `<name>.ksdownload`, then gives the file its final name.
    async fn run(&self, record: &Record, cancel: CancelToken, progress: Progress) -> Result<DownloadOutcome, RunError> {
        let path = self.destination(record).await?;
        let info = self.probe_cached(&record.url).await?;
        {
            let mut inner = self.lock();
            if let Some(r) = inner.records.iter_mut().find(|r| r.id == record.id) {
                r.resumable = info.resumable();
                r.size = info.size;
                let r = r.clone();
                self.save(&inner);
                drop(inner);
                self.events.changed(&r);
            }
        }
        if cancel.is_cancelled() {
            return Err(RunError::Cancelled);
        }
        let part = files::part(&path);
        // Names this download as the owner of the `.ksdownload` file, for the leftover-files
        // cleanup (a range map names the URL only, and not every download has one).
        let owner = leftovers::owner_file(&part);
        if let Err(e) = marker::write(&owner, &marker::Marker::new(record.id, &record.url)) {
            return Err(RunError::Failed(format!("Could not write in the save folder: {e}.")));
        }
        let s = self.settings();
        let options = DownloadOptions {
            cancel,
            progress,
            connections: s.per_download,
            adaptive: Some(Adaptive { hosts: self.hosts.clone(), ..Adaptive::default() }),
            budget: Some(self.budget.clone()),
            speed_limit: Some(self.speed.clone()),
            ..DownloadOptions::default()
        };
        let result = engine::download(&record.url, &part, options).await;
        if result.is_err() && !part.exists() {
            let _ = std::fs::remove_file(&owner); // the engine removed a worthless partial file
        }
        let outcome = result.map_err(|e| match e {
            EngineError::Cancelled => RunError::Cancelled,
            e => RunError::Failed(sentence(&e)),
        })?;
        self.complete(record.id, &path, &part)?;
        let _ = std::fs::remove_file(&owner);
        Ok(outcome)
    }

    /// Runs yt-dlp into the hidden temp folder beside the destination, then moves the
    /// finished file into place.
    async fn run_video(&self, record: &Record, cancel: CancelToken, progress: VideoProgress) -> Result<u64, RunError> {
        let tools = self.video_tools()?;
        files::check_folder(&record.path.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default())?;
        let temp = self.prepare_video_temp(record)?;
        let file = tools.download(&record.url, &temp, cancel, progress).await.map_err(|e| match e {
            VideoError::Cancelled => RunError::Cancelled,
            e => {
                let (message, code) = video_message(&e);
                RunError::Coded(message, code)
            }
        })?;
        let (_, bytes) = self.complete_video(record.id, &file)?;
        let current = self.lock().records.iter().find(|r| r.id == record.id).cloned();
        remove_video_temp(current.as_ref().unwrap_or(record));
        Ok(bytes)
    }

    /// The temp folder to run yt-dlp in. An existing folder is continued in only when its
    /// marker names this download (same ID and URL). Otherwise, or with no marker, it is
    /// left alone for the leftover-files cleanup, and a fresh, uniquely named folder is made
    /// and recorded in `record.temp`.
    fn prepare_video_temp(&self, record: &Record) -> Result<PathBuf, RunError> {
        if let Some(dir) = own_video_temp(record) {
            return Ok(dir);
        }
        let folder = record.path.parent().ok_or_else(|| RunError::Failed("The download has no folder.".into()))?;
        let default = format!("{VIDEO_TEMP_PREFIX}{}", record.id);
        let name = (0..1000)
            .map(|n| if n == 0 { default.clone() } else { format!("{default}-{n}") })
            .find(|n| std::fs::symlink_metadata(folder.join(n)).is_err())
            .ok_or_else(|| RunError::Failed("Could not create the temp folder.".into()))?;
        let dir = folder.join(&name);
        std::fs::create_dir(&dir).map_err(|e| RunError::Failed(format!("Could not create the temp folder: {e}.")))?;
        hide(&dir);
        marker::write(&dir.join(marker::IN_FOLDER), &marker::Marker::new(record.id, &record.url))
            .map_err(|e| RunError::Failed(format!("Could not create the temp folder: {e}.")))?;
        let mut inner = self.lock();
        if let Some(r) = inner.records.iter_mut().find(|r| r.id == record.id) {
            r.temp = (name != default).then_some(name);
            self.save(&inner);
        }
        Ok(dir)
    }

    /// Gives a finished video its final name: the title with the extension yt-dlp chose
    /// (mp4 or mkv), reserved now in the destination folder ("name (1).ext" when taken),
    /// then the file is renamed over the reservation, as [`Manager::complete`] does.
    fn complete_video(&self, id: u64, file: &Path) -> Result<(PathBuf, u64), RunError> {
        let mut inner = self.lock();
        let r = inner.records.iter().find(|r| r.id == id).ok_or_else(|| RunError::Failed("No such download.".into()))?;
        let folder = r.path.parent().ok_or_else(|| RunError::Failed("The download has no folder.".into()))?.to_path_buf();
        // From `path`, not `name`: a damaged list cannot ask for a name outside the folder.
        let planned = r.path.file_name().map_or_else(|| "video".into(), |n| n.to_string_lossy().into_owned());
        let ext = file.extension().map_or_else(|| "mp4".into(), |e| e.to_string_lossy().into_owned());
        let name = sanitize_file_name(&format!("{}.{ext}", without_media_extension(&planned)));
        let taken = |p: &Path| inner.records.iter().any(|o| o.id != id && o.path == p);
        let target = files::reserve(&folder, &name, &taken)
            .map_err(|e| RunError::Failed(format!("Downloaded, but could not create the final file: {e}.")))?;
        std::fs::rename(file, &target).map_err(|e| {
            let _ = std::fs::remove_file(&target);
            RunError::Failed(format!("Downloaded to {}, but could not move it: {e}.", file.display()))
        })?;
        let bytes = std::fs::metadata(&target).map_or(0, |m| m.len());
        if let Some(r) = inner.records.iter_mut().find(|r| r.id == id) {
            r.name = target.file_name().map_or(r.name.clone(), |n| n.to_string_lossy().into_owned());
            r.path = target.clone();
            self.save(&inner);
        }
        Ok((target, bytes))
    }

    /// Renames the finished `.ksdownload` file to its final name. That name is held by our
    /// empty reservation file, which is replaced; if something else is there now, the file
    /// gets a free "name (n).ext" instead, so nothing is overwritten.
    fn complete(&self, id: u64, path: &Path, part: &Path) -> Result<PathBuf, RunError> {
        let mut inner = self.lock();
        // Our reservation, or the name is free again (claimed with create_new, so a file
        // that appears meanwhile is never replaced).
        let free = || std::fs::OpenOptions::new().write(true).create_new(true).open(path).is_ok();
        let target = if is_reservation(path) || free() {
            path.to_path_buf()
        } else {
            let folder = path.parent().ok_or_else(|| RunError::Failed("The download has no folder.".into()))?;
            let name = path.file_name().map_or_else(|| "download".into(), |n| n.to_string_lossy().into_owned());
            let taken = |p: &Path| inner.records.iter().any(|r| r.id != id && r.path == p);
            files::reserve(folder, &name, &taken)
                .map_err(|e| RunError::Failed(format!("Downloaded, but could not create the final file: {e}.")))?
        };
        std::fs::rename(part, &target).map_err(|e| {
            RunError::Failed(format!("Downloaded to {}, but could not rename it: {e}.", part.display()))
        })?;
        if let Some(r) = inner.records.iter_mut().find(|r| r.id == id)
            && r.path != target
        {
            r.name = target.file_name().map_or(r.name.clone(), |n| n.to_string_lossy().into_owned());
            r.category = files::category(&r.name);
            r.path = target.clone();
            self.save(&inner);
        }
        Ok(target)
    }

    /// The final path to download for: the download's own name when its `.ksdownload` file
    /// has a matching range map (resume) or the empty reservation file still holds the
    /// name; otherwise a new free name in the same folder, and any leftover `.ksdownload`
    /// file of the old name is removed. An existing file is never overwritten. The name is
    /// taken from `record.path`, not `record.name`, so a damaged list cannot ask for one
    /// that is not a single file name.
    async fn destination(&self, record: &Record) -> Result<PathBuf, String> {
        if own_partial(&record.url, &record.path).await.is_some() || is_reservation(&record.path) {
            return Ok(record.path.clone());
        }
        discard_partial(&record.path).await;
        let folder = record.path.parent().ok_or("The download has no folder.")?;
        let folder = files::check_folder(&folder.to_string_lossy())?;
        let mut inner = self.lock();
        let taken = |p: &Path| inner.records.iter().any(|r| r.id != record.id && r.path == p);
        let name = record.path.file_name().map_or_else(|| "download".into(), |n| n.to_string_lossy().into_owned());
        let path = files::reserve(&folder, &name, &taken).map_err(|e| format!("Could not create the file: {e}."))?;
        if let Some(r) = inner.records.iter_mut().find(|r| r.id == record.id) {
            r.name = path.file_name().map_or(r.name.clone(), |n| n.to_string_lossy().into_owned());
            r.path = path.clone();
            r.received = 0;
            let r = r.clone();
            self.save(&inner);
            drop(inner);
            self.events.changed(&r);
        }
        Ok(path)
    }

    async fn finish(&self, id: u64, result: Result<u64, RunError>) {
        let (stop, deleted, record) = {
            let mut inner = self.lock();
            let run = inner.running.remove(&id);
            let stop = run.as_ref().and_then(|r| r.stop);
            (stop, run.and_then(|r| r.deleted), inner.records.iter().find(|r| r.id == id).cloned())
        };
        if let Some(deleted) = deleted {
            // Deleted while running: the entry is gone. A finished file stays; what is left
            // of an unfinished one goes (only if it is this download's own partial file).
            if result.is_err() {
                remove_files(&Record { status: Status::Paused, ..deleted }, false).await;
            }
            return;
        }
        let Some(record) = record else { return };
        let saved = match (&result, record.kind) {
            (Err(_), Kind::File) => own_partial(&record.url, &record.path).await,
            _ => None,
        };
        // A video keeps its place in the temp folder: yt-dlp continues from there.
        let video_temp = (record.kind == Kind::Video).then(|| own_video_temp(&record)).flatten();
        let mut inner = self.lock();
        let Some(r) = inner.records.iter_mut().find(|r| r.id == id) else { return };
        match result {
            Ok(bytes) => {
                r.status = Status::Finished;
                r.size = Some(bytes);
                r.received = bytes;
                r.error = None;
                r.error_code = None;
            }
            Err(error) if r.kind == Kind::Video => {
                r.received = video_temp.as_deref().map_or(0, temp_bytes);
                (r.status, r.error, r.error_code) = match error {
                    RunError::Cancelled if video_temp.is_some() => (Status::Paused, None, None),
                    RunError::Cancelled => (Status::Queued, None, None),
                    RunError::Failed(message) => (Status::Failed, Some(message), None),
                    RunError::Coded(message, code) => (Status::Failed, Some(message), code.map(str::to_owned)),
                };
            }
            Err(error) => {
                r.received = saved.as_ref().map_or(0, engine::SavedState::done_bytes);
                r.error_code = None;
                (r.status, r.error) = match (error, &saved) {
                    (RunError::Cancelled, Some(_)) => (Status::Paused, None),
                    // Paused before any byte arrived: nothing lost, it simply waits again.
                    (RunError::Cancelled, None) if stop == Some(Stop::Pause) => (Status::Queued, None),
                    (RunError::Cancelled, None) => (Status::Interrupted, Some(INTERRUPTED.into())),
                    (RunError::Failed(message) | RunError::Coded(message, _), _) => (Status::Failed, Some(message)),
                };
            }
        }
        let r = r.clone();
        self.save(&inner);
        drop(inner);
        self.events.changed(&r);
    }

    /// Pauses a running download. Refused when the server cannot resume it: stopping
    /// such a download deletes its partial file, which would lose data without a word.
    pub fn pause(&self, id: u64) -> Result<(), String> {
        let mut inner = self.lock();
        let r = inner.records.iter_mut().find(|r| r.id == id).ok_or("No such download.")?;
        if r.status == Status::Waiting {
            // Not started yet: it stops waiting, keeping what it has.
            r.status = if r.received > 0 { Status::Paused } else { Status::Queued };
            let r = r.clone();
            self.save(&inner);
            drop(inner);
            self.events.changed(&r);
            return Ok(());
        }
        let resumable = r.resumable;
        if !resumable {
            return Err(NOT_RESUMABLE.into());
        }
        if let Some(run) = inner.running.get_mut(&id) {
            run.stop = Some(Stop::Pause);
            run.cancel.cancel();
        }
        Ok(())
    }

    /// Removes a download from the list. A running one is stopped. Its partial file and
    /// range map are deleted; a finished file is deleted only when `delete_file` is set.
    pub async fn delete(&self, id: u64, delete_file: bool) -> Result<(), String> {
        let record = {
            let mut inner = self.lock();
            let i = inner.records.iter().position(|r| r.id == id).ok_or("No such download.")?;
            let record = inner.records.remove(i);
            if let Some(run) = inner.running.get_mut(&id) {
                run.stop = Some(Stop::Delete);
                run.deleted = Some(record.clone());
                run.cancel.cancel();
            }
            self.save(&inner);
            record
        };
        self.events.removed(id);
        remove_files(&record, delete_file).await;
        Ok(())
    }

    /// Removes every download from the list, stopping those still running. Finished
    /// files stay on disk; partial files of unfinished downloads are deleted.
    pub async fn clear_all(&self) {
        let ids: Vec<u64> = self.lock().records.iter().map(|r| r.id).collect();
        for id in ids {
            let _ = self.delete(id, false).await;
        }
    }

    fn path_of(&self, id: u64) -> Result<(PathBuf, Status), String> {
        let inner = self.lock();
        let r = inner.records.iter().find(|r| r.id == id).ok_or("No such download.")?;
        Ok((r.path.clone(), r.status))
    }

    pub fn open_file(&self, id: u64) -> Result<(), String> {
        let (path, status) = self.path_of(id)?;
        if status != Status::Finished {
            return Err("The file can be opened once the download has finished.".into());
        }
        open::open_file(&path).map_err(|e| format!("Could not open the file: {e}."))
    }

    pub fn open_folder(&self, id: u64) -> Result<(), String> {
        let (path, _) = self.path_of(id)?;
        open::reveal(&path).map_err(|e| format!("Could not open the folder: {e}."))
    }

    /// Stops every download for quitting and waits (up to 3 s) until they have saved
    /// their range maps. Resumable ones come back paused next time.
    pub async fn quit(&self) {
        {
            let mut inner = self.lock();
            inner.quitting = true;
            for run in inner.running.values_mut() {
                run.stop.get_or_insert(Stop::Quit);
                run.cancel.cancel();
            }
        }
        let deadline = Instant::now() + QUIT_WAIT;
        while !self.lock().running.is_empty() && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    // ---- Queues ------------------------------------------------------------------------

    pub fn queues(&self) -> Vec<Queue> {
        self.lock().queues.clone()
    }

    fn queues_changed(&self, inner: MutexGuard<'_, Inner>) {
        self.save(&inner);
        let queues = inner.queues.clone();
        drop(inner);
        self.events.queues(&queues);
    }

    pub fn create_queue(&self, name: &str) -> Result<Queue, String> {
        let name = queue_name(name)?;
        let mut inner = self.lock();
        if inner.queues.iter().any(|q| q.name.eq_ignore_ascii_case(&name)) {
            return Err("A queue with this name exists.".into());
        }
        let id = inner.queues.iter().map(|q| q.id).max().unwrap_or(0) + 1;
        let queue = Queue { id, name };
        inner.queues.push(queue.clone());
        self.queues_changed(inner);
        Ok(queue)
    }

    pub fn rename_queue(&self, id: u64, name: &str) -> Result<(), String> {
        let name = queue_name(name)?;
        let mut inner = self.lock();
        if inner.queues.iter().any(|q| q.id != id && q.name.eq_ignore_ascii_case(&name)) {
            return Err("A queue with this name exists.".into());
        }
        inner.queues.iter_mut().find(|q| q.id == id).ok_or("No such queue.")?.name = name;
        self.queues_changed(inner);
        Ok(())
    }

    /// Deletes a queue; its downloads move to the main queue (nothing is stopped).
    pub fn delete_queue(&self, id: u64) -> Result<(), String> {
        let mut inner = self.lock();
        let i = inner.queues.iter().position(|q| q.id == id).ok_or("No such queue.")?;
        inner.queues.remove(i);
        let moved: Vec<Record> = inner
            .records
            .iter_mut()
            .filter(|r| r.queue == Some(id))
            .map(|r| {
                r.queue = None;
                r.clone()
            })
            .collect();
        self.queues_changed(inner);
        for r in &moved {
            self.events.changed(r);
        }
        Ok(())
    }

    /// Moves a download to a queue (`None`: the main queue).
    pub fn move_to_queue(&self, id: u64, queue: Option<u64>) -> Result<(), String> {
        let mut inner = self.lock();
        if queue.is_some_and(|q| !inner.queues.iter().any(|x| x.id == q)) {
            return Err("No such queue.".into());
        }
        let r = inner.records.iter_mut().find(|r| r.id == id).ok_or("No such download.")?;
        r.queue = queue;
        let r = r.clone();
        self.save(&inner);
        drop(inner);
        self.events.changed(&r);
        Ok(())
    }

    /// Every unfinished download of the queue that is not running wants to run.
    pub fn start_queue(self: &Arc<Self>, queue: Option<u64>) -> Result<(), String> {
        let ids: Vec<u64> = {
            let inner = self.lock();
            if queue.is_some_and(|q| !inner.queues.iter().any(|x| x.id == q)) {
                return Err("No such queue.".into());
            }
            inner
                .records
                .iter()
                .filter(|r| r.queue == queue && matches!(r.status, Status::Queued | Status::Paused | Status::Failed | Status::Interrupted))
                .map(|r| r.id)
                .collect()
        };
        for id in ids {
            self.resume(id)?;
        }
        Ok(())
    }

    /// Pauses the queue's running downloads and stops its waiting ones. Downloads that
    /// cannot be paused (no resume support) keep running; their number is returned.
    pub fn stop_queue(&self, queue: Option<u64>) -> Result<usize, String> {
        let ids: Vec<(u64, Status, bool)> = {
            let inner = self.lock();
            if queue.is_some_and(|q| !inner.queues.iter().any(|x| x.id == q)) {
                return Err("No such queue.".into());
            }
            inner.records.iter().filter(|r| r.queue == queue).map(|r| (r.id, r.status, r.resumable)).collect()
        };
        let mut kept = 0;
        for (id, status, resumable) in ids {
            match status {
                Status::Waiting => self.pause(id)?,
                Status::Downloading if resumable => self.pause(id)?,
                Status::Downloading => kept += 1,
                _ => {}
            }
        }
        Ok(kept)
    }

    // ---- Leftover download files ---------------------------------------------------------

    /// The only folders looked in: the default save folder, the folders of downloads in
    /// the list, and the category folders.
    fn known_folders(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = Vec::new();
        let mut add = |p: PathBuf| {
            if !out.contains(&p) {
                out.push(p);
            }
        };
        if let Some(d) = self.default_folder.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            add(d);
        }
        for f in self.settings().folders.all() {
            add(PathBuf::from(f));
        }
        for r in self.lock().records.iter() {
            if let Some(parent) = r.path.parent() {
                add(parent.to_path_buf());
            }
        }
        out
    }

    /// Looks for leftovers now (run off the critical path at start-up, and on request).
    pub async fn scan_leftovers(&self) -> Vec<leftovers::Item> {
        let folders = self.known_folders();
        let found = leftovers::scan(&folders, &self.list()).await;
        let items = found.iter().map(leftovers::Item::from).collect();
        *self.leftovers.lock().unwrap_or_else(|e| e.into_inner()) = Some(found);
        items
    }

    /// The last scan's result, if there was one.
    pub fn leftovers(&self) -> Option<Vec<leftovers::Item>> {
        self.leftovers.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|f| f.iter().map(leftovers::Item::from).collect())
    }

    /// Removes what the last scan found (after the user confirmed), each item re-checked
    /// at this moment, then scans again. Returns how many were removed and kept.
    pub async fn remove_leftovers(&self) -> (usize, usize) {
        let found = self.leftovers.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_default();
        let (removed, kept) = leftovers::remove(&found, &self.list()).await;
        self.scan_leftovers().await;
        (removed, kept.len())
    }

    /// Reads every running download's progress and sends one batched event.
    pub fn tick(&self) {
        let ticks: Vec<Tick> = {
            let mut inner = self.lock();
            let now = Instant::now();
            inner.running.iter_mut().map(|(id, run)| run.sample(*id, now)).collect()
        };
        if !ticks.is_empty() {
            self.events.progress(&ticks);
        }
    }
}

/// Why a run ended without a finished file.
#[derive(Debug, PartialEq, Eq)]
enum RunError {
    /// Stopped on purpose (pause, delete, quit), told apart by type, not by message text.
    Cancelled,
    Failed(String),
    /// A failure the interface can offer something for (`Record::error_code`).
    Coded(String, Option<&'static str>),
}

impl From<String> for RunError {
    fn from(message: String) -> Self {
        Self::Failed(message)
    }
}

impl Running {
    fn sample(&mut self, id: u64, now: Instant) -> Tick {
        let snap = match &self.progress {
            Source::Engine(p) => p.snapshot(),
            Source::Video(v) => {
                let (received, total, merging) = v.snapshot();
                let dt = now.duration_since(self.last_at).as_secs_f64().max(1e-3);
                let delta = received.saturating_sub(self.last_received);
                self.speed += SMOOTHING * (delta as f64 / dt - self.speed);
                self.last_at = now;
                self.last_received = received;
                return Tick { id, size: total, received, speed: self.speed, connections: Vec::new(), merging };
            }
        };
        let dt = now.duration_since(self.last_at).as_secs_f64().max(1e-3);
        let smooth = |old: f64, delta: u64| old + SMOOTHING * (delta as f64 / dt - old);
        // Counters restart when the engine restarts a download; a drop is not negative speed.
        self.speed = smooth(self.speed, snap.received.saturating_sub(self.last_received));
        self.connection_speed.resize(snap.per_connection.len(), 0.0);
        for (i, &n) in snap.per_connection.iter().enumerate() {
            let before = self.last_per.get(i).copied().unwrap_or(0);
            self.connection_speed[i] = smooth(self.connection_speed[i], n.saturating_sub(before));
        }
        self.last_at = now;
        self.last_received = snap.received;
        self.last_per = snap.per_connection;
        Tick {
            id,
            size: snap.total,
            received: snap.received,
            speed: self.speed,
            connections: snap
                .connections
                .iter()
                .zip(&self.connection_speed)
                .map(|(c, &speed)| c.map(|c| ConnectionTick { start: c.start, position: c.position, end: c.end, speed }))
                .collect(),
            merging: false,
        }
    }
}

/// The range map of the `.ksdownload` file for final name `path`, if it belongs to a
/// download of `url`.
async fn own_partial(url: &str, path: &Path) -> Option<engine::SavedState> {
    engine::saved_state(&files::part(path)).await.filter(|s| s.url == url)
}

/// Deletes the `.ksdownload` file for final name `path` and its range map, if there.
/// Only downloads in the list use that name, so it is never a file of the user's.
async fn discard_partial(path: &Path) {
    let part = files::part(path);
    let _ = engine::discard(&part).await;
    let owner = leftovers::owner_file(&part);
    if std::fs::symlink_metadata(&owner).is_ok_and(|m| m.is_file()) {
        let _ = std::fs::remove_file(owner);
    }
}

/// Cleans up after a download leaves the list (it is not running any more).
async fn remove_files(record: &Record, delete_file: bool) {
    let path = &record.path;
    if record.kind == Kind::Video && record.status != Status::Finished {
        // Running: the task cleans up when yt-dlp has stopped. Otherwise only our own temp
        // folder goes; the planned final name was never created.
        if record.status != Status::Downloading {
            remove_video_temp(record);
        }
        return;
    }
    match record.status {
        Status::Finished => {
            if delete_file && path.is_file() {
                let _ = std::fs::remove_file(path);
            }
        }
        // Running: the task cleans up when the engine has stopped writing.
        Status::Downloading => {}
        _ => {
            discard_partial(path).await;
            if is_reservation(path) {
                // The empty file that held the name; nothing of the user's is in it.
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

/// A queue name: trimmed, 1 to 60 characters.
fn queue_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 60 {
        return Err("A queue name needs 1 to 60 characters.".into());
    }
    Ok(name.to_owned())
}

/// "Title.mp4" from a video's title and extension, made safe as a file name.
fn video_name(title: &str, ext: &str) -> String {
    let title = sanitize_file_name(title);
    let ext = if ext.is_empty() { "mp4" } else { ext };
    format!("{title}.{ext}")
}

/// A video's name without a media extension. Only a known media extension is removed, so
/// a typed name such as "Mr. Smith" keeps its full text.
fn without_media_extension(name: &str) -> &str {
    const MEDIA: [&str; 14] = ["mp4", "mkv", "webm", "mov", "m4v", "avi", "flv", "ts", "mp3", "m4a", "opus", "ogg", "aac", "wav"];
    match name.rfind('.') {
        Some(i) if i > 0 && MEDIA.contains(&name[i + 1..].to_ascii_lowercase().as_str()) => &name[..i],
        _ => name,
    }
}

/// The empty file that holds a download's name until the engine writes it (see
/// [`files::reserve`]): reusing it overwrites nothing.
fn is_reservation(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Log(Mutex<Vec<String>>);

    impl Events for Log {
        fn changed(&self, r: &Record) {
            self.0.lock().unwrap().push(format!("changed {} {:?}", r.id, r.status));
        }
        fn removed(&self, id: u64) {
            self.0.lock().unwrap().push(format!("removed {id}"));
        }
        fn progress(&self, ticks: &[Tick]) {
            self.0.lock().unwrap().push(format!("progress {}", ticks.len()));
        }
        fn queues(&self, queues: &[Queue]) {
            self.0.lock().unwrap().push(format!("queues {}", queues.len()));
        }
    }

    fn temp_dir() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir()
            .join(format!("ks-manager-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    fn record(id: u64, dir: &Path, name: &str, status: Status) -> Record {
        Record {
            id,
            kind: Kind::File,
            error_code: None,
            temp: None,
            url: format!("https://example.com/{name}"),
            name: name.into(),
            path: dir.join(name),
            category: Category::Other,
            status,
            size: Some(1000),
            received: 123,
            resumable: true,
            error: None,
            added_at: 0,
            queue: None,
        }
    }

    /// The `.ksdownload` file of 1000 bytes with 300 done, and its range map (format in
    /// engine/src/rangemap.rs).
    fn partial(r: &Record) {
        std::fs::write(files::part(&r.path), vec![0u8; 1000]).unwrap();
        let map = format!(
            "ks-download-manager range map 1\nurl {}\nsize 1000\nvalidator etag \"v1\"\ndone 0 300\nend\n",
            r.url
        );
        std::fs::write(files::ksdm(&files::part(&r.path)), map).unwrap();
    }

    fn gone(path: &Path) -> bool {
        !files::part(path).exists() && !files::ksdm(&files::part(path)).exists()
    }

    fn manager(dir: &Path, records: &[Record]) -> (Arc<Manager>, Arc<Log>) {
        let file = store::path(&dir.join("data"));
        store::save(&file, records, records.iter().map(|r| r.id).max().unwrap_or(0) + 1, &[]).unwrap();
        let log = Arc::new(Log::default());
        (Manager::new(file, log.clone()), log)
    }

    fn get(m: &Manager, id: u64) -> Record {
        m.list().into_iter().find(|r| r.id == id).unwrap()
    }

    /// A link that fails at once without any network: nothing listens on port 9 here.
    fn refused(r: Record) -> Record {
        Record { url: format!("http://127.0.0.1:9/{}", r.name), ..r }
    }

    fn with_settings(dir: &Path, s: Settings) {
        settings::save(&dir.join("data").join(settings::FILE_NAME), &s).unwrap();
    }

    async fn wait_until(m: &Manager, done: impl Fn(&[Record]) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done(&m.list()) {
            assert!(Instant::now() < deadline, "timed out: {:?}", m.list().iter().map(|r| (r.id, r.status)).collect::<Vec<_>>());
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn waiting_downloads_start_in_queue_order_within_the_cap() {
        let dir = temp_dir();
        with_settings(&dir, Settings { max_active: 1, ..Settings::default() });
        let records = [
            Record { queue: Some(1), added_at: 1, ..refused(record(1, &dir, "a.zip", Status::Queued)) },
            Record { added_at: 3, ..refused(record(2, &dir, "b.zip", Status::Queued)) },
            Record { added_at: 2, ..refused(record(3, &dir, "c.zip", Status::Queued)) },
        ];
        let file = store::path(&dir.join("data"));
        store::save(&file, &records, 4, &[Queue { id: 1, name: "Later".into() }]).unwrap();
        let log = Arc::new(Log::default());
        let m = Manager::new(file, log.clone());
        for id in [1, 2, 3] {
            m.resume(id).unwrap();
            let running = m.list().iter().filter(|r| r.status == Status::Downloading).count();
            assert!(running <= 1, "never more than the cap");
        }
        wait_until(&m, |l| l.iter().all(|r| r.status == Status::Failed)).await;
        let mut started: Vec<String> = log.0.lock().unwrap().iter().filter(|e| e.ends_with("Downloading")).cloned().collect();
        started.dedup(); // a start and the probe result each send the record
        // 1 started at once (it was the only one waiting); then the main queue by date
        // added (3 before 2).
        assert_eq!(started, ["changed 1 Downloading", "changed 3 Downloading", "changed 2 Downloading"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn a_waiting_download_stops_waiting_when_paused_and_after_a_restart() {
        let dir = temp_dir();
        let fresh = Record { received: 0, ..record(1, &dir, "a.zip", Status::Waiting) };
        let begun = record(2, &dir, "b.zip", Status::Waiting);
        partial(&begun);
        let (m, _) = manager(&dir, &[fresh, begun, record(3, &dir, "c.zip", Status::Waiting)]);
        m.restore().await;
        assert_eq!(get(&m, 1).status, Status::Queued, "never started: nothing starts by itself");
        assert_eq!(get(&m, 2).status, Status::Paused, "has a matching range map");
        assert_eq!(get(&m, 3).status, Status::Interrupted, "had bytes, but no map");

        let mut r = get(&m, 2);
        r.status = Status::Waiting;
        m.lock().records[1] = r;
        m.pause(2).unwrap();
        assert_eq!(get(&m, 2).status, Status::Paused);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn queues_are_created_renamed_and_deleted_into_the_main_queue() {
        let dir = temp_dir();
        let (m, log) = manager(&dir, &[record(1, &dir, "a.zip", Status::Paused)]);
        let q = m.create_queue("  Night  ").unwrap();
        assert_eq!(q.name, "Night");
        assert!(m.create_queue("night").is_err(), "names are unique");
        assert!(m.create_queue(" ").is_err());
        m.rename_queue(q.id, "Weekend").unwrap();
        m.move_to_queue(1, Some(q.id)).unwrap();
        assert!(m.move_to_queue(1, Some(99)).is_err());
        assert_eq!(get(&m, 1).queue, Some(q.id));
        m.delete_queue(q.id).unwrap();
        assert_eq!(get(&m, 1).queue, None, "moved to the main queue");
        assert!(m.queues().is_empty());
        // Saved with the list.
        let saved = store::load_all(&store::path(&dir.join("data")));
        assert!(saved.queues.is_empty() && saved.downloads[0].queue.is_none());
        assert!(log.0.lock().unwrap().iter().any(|e| e == "queues 1"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn stopping_a_queue_ends_its_waiting() {
        let dir = temp_dir();
        let (m, _) = manager(&dir, &[
            Record { queue: None, ..record(1, &dir, "a.zip", Status::Waiting) },
            Record { received: 0, ..record(2, &dir, "b.zip", Status::Waiting) },
        ]);
        assert_eq!(m.stop_queue(None).unwrap(), 0);
        assert_eq!((get(&m, 1).status, get(&m, 2).status), (Status::Paused, Status::Queued));
        assert!(m.stop_queue(Some(5)).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn settings_are_checked_saved_and_applied() {
        let dir = temp_dir();
        let (m, _) = manager(&dir, &[]);
        let bad = Settings { folders: settings::Folders { video: Some("/no/such/folder".into()), ..Default::default() }, ..Settings::default() };
        assert!(m.set_settings(bad).is_err(), "a category folder must exist");
        let s = Settings {
            max_active: 0,
            speed_limit: Some(2048),
            folders: settings::Folders { images: Some(dir.to_string_lossy().into_owned()), ..Default::default() },
            ..Settings::default()
        };
        let saved = m.set_settings(s).unwrap();
        assert_eq!(saved.max_active, 1);
        assert_eq!(m.folder_for(Category::Images).as_deref(), Some(dir.to_str().unwrap()));
        m.set_default_folder(PathBuf::from("/tmp"));
        assert_eq!(m.folder_for(Category::Audio).as_deref(), Some("/tmp"));
        assert_eq!(m.speed.get(), Some(2048));
        assert_eq!(settings::load(&dir.join("data").join(settings::FILE_NAME)), saved);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn leftovers_are_looked_for_only_in_known_folders_and_never_include_the_list() {
        let dir = temp_dir();
        let other = temp_dir(); // not known to the app
        let mine = record(1, &dir, "a.zip", Status::Paused);
        partial(&mine);
        let orphan = record(2, &dir, "b.zip", Status::Paused);
        partial(&orphan);
        let unknown = record(3, &other, "c.zip", Status::Paused);
        partial(&unknown);
        let (m, _) = manager(&dir, std::slice::from_ref(&mine));
        let items = m.scan_leftovers().await;
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(items[0].path, files::part(&orphan.path).to_string_lossy());
        assert_eq!(m.remove_leftovers().await, (1, 0));
        assert!(gone(&orphan.path), "removed with its map");
        assert!(!gone(&mine.path) && !gone(&unknown.path), "the list's and unknown folders' files stay");
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_dir_all(&other).unwrap();
    }

    #[tokio::test]
    async fn restore_uses_saved_state_to_decide_what_can_resume() {
        let dir = temp_dir();
        let running = record(1, &dir, "a.bin", Status::Downloading);
        let paused = record(2, &dir, "b.bin", Status::Paused);
        let lost = record(3, &dir, "c.bin", Status::Downloading);
        let other_url = record(4, &dir, "d.bin", Status::Paused);
        let queued = record(5, &dir, "e.bin", Status::Queued);
        let finished = record(6, &dir, "f.bin", Status::Finished);
        partial(&running);
        partial(&paused);
        partial(&Record { url: "https://elsewhere.example/d.bin".into(), ..other_url.clone() });
        // A leftover in-progress file with no map at all.
        std::fs::write(files::part(&lost.path), b"half").unwrap();
        let (m, _) = manager(&dir, &[running.clone(), paused, lost.clone(), other_url.clone(), queued.clone(), finished.clone()]);
        m.restore().await;

        assert!(files::part(&running.path).exists(), "a resumable .ksdownload file is kept");
        assert!(gone(&lost.path) && gone(&other_url.path), "leftovers without a matching map are deleted");

        for id in [1, 2] {
            let r = get(&m, id);
            assert_eq!((r.status, r.received, r.size, r.resumable), (Status::Paused, 300, Some(1000), true));
        }
        for id in [3, 4] {
            let r = get(&m, id);
            assert_eq!((r.status, r.received), (Status::Interrupted, 0), "id {id}");
            assert!(r.error.is_some());
        }
        assert_eq!(get(&m, 5), queued);
        assert_eq!(get(&m, 6), finished);
        // The restored list is what is on disk now.
        assert_eq!(store::load(&store::path(&dir.join("data"))), m.list());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn a_download_that_cannot_resume_cannot_be_paused() {
        let dir = temp_dir();
        let r = Record { resumable: false, ..record(1, &dir, "a.bin", Status::Downloading) };
        let (m, _) = manager(&dir, &[r, record(2, &dir, "b.bin", Status::Paused)]);
        let err = m.pause(1).unwrap_err();
        assert!(err.contains("cannot be paused"), "{err}");
        assert!(m.pause(2).is_ok());
        assert!(m.pause(99).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn the_destination_is_never_someone_elses_file() {
        let dir = temp_dir();
        // Its own partial file: resumed in place.
        let own = record(1, &dir, "own.bin", Status::Paused);
        partial(&own);
        // The empty file reserved when it was added: reused.
        let reserved = record(2, &dir, "reserved.bin", Status::Queued);
        std::fs::write(&reserved.path, b"").unwrap();
        // A file of the user's (or a leftover without a map): kept, a new name is used.
        let foreign = record(3, &dir, "photo.jpg", Status::Interrupted);
        std::fs::write(&foreign.path, b"precious").unwrap();
        // A range map for another URL is not this download's partial file either.
        let other = record(4, &dir, "other.bin", Status::Paused);
        partial(&Record { url: "https://elsewhere.example/other.bin".into(), ..other.clone() });
        let (m, log) = manager(&dir, &[own.clone(), reserved.clone(), foreign.clone(), other.clone()]);

        assert_eq!(m.destination(&own).await.unwrap(), own.path);
        assert_eq!(m.destination(&reserved).await.unwrap(), reserved.path);
        let new = m.destination(&foreign).await.unwrap();
        assert_eq!(new, dir.join("photo (1).jpg"));
        assert_eq!(std::fs::read(&foreign.path).unwrap(), b"precious");
        assert_eq!(get(&m, 3).path, new);
        assert_eq!(get(&m, 3).name, "photo (1).jpg");
        assert!(log.0.lock().unwrap().contains(&"changed 3 Interrupted".to_string()));
        // Its leftover .ksdownload file (map for another URL) is ours to remove; the final
        // name is free, so it is reserved again.
        assert_eq!(m.destination(&other).await.unwrap(), other.path);
        assert!(gone(&other.path));
        assert!(is_reservation(&other.path));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn a_damaged_name_does_not_stop_the_destination() {
        // downloads.json edited or damaged: `name` is not a single file name, `path` is.
        let outer = temp_dir();
        let dir = outer.join("downloads");
        std::fs::create_dir(&dir).unwrap();
        let mut records = Vec::new();
        for (id, bad) in [(1, "../evil.bin"), (2, ""), (3, "/abs/x"), (4, "a/b")] {
            let r = Record { name: bad.into(), ..record(id, &dir, &format!("file{id}.bin"), Status::Interrupted) };
            std::fs::write(&r.path, b"precious").unwrap();
            records.push(r);
        }
        let (m, _) = manager(&dir, &records);
        for r in &records {
            let new = m.destination(r).await.unwrap();
            assert_eq!(new, dir.join(format!("file{} (1).bin", r.id)), "name {:?}", r.name);
            assert_eq!(std::fs::read(&r.path).unwrap(), b"precious");
            assert_eq!(get(&m, r.id).name, format!("file{} (1).bin", r.id));
        }
        assert!(!outer.join("evil.bin").exists(), "nothing created outside the folder");
        std::fs::remove_dir_all(&outer).unwrap();
    }

    #[tokio::test]
    async fn delete_removes_partials_and_keeps_finished_files_unless_asked() {
        let dir = temp_dir();
        let paused = record(1, &dir, "p.bin", Status::Paused);
        partial(&paused);
        let keep = record(2, &dir, "keep.bin", Status::Finished);
        std::fs::write(&keep.path, b"done").unwrap();
        let remove = record(3, &dir, "remove.bin", Status::Finished);
        std::fs::write(&remove.path, b"done").unwrap();
        // Failed: its leftover .ksdownload goes, the file at its final name is not ours.
        let failed = record(4, &dir, "f.bin", Status::Failed);
        std::fs::write(&failed.path, b"not ours").unwrap();
        std::fs::write(files::part(&failed.path), b"half").unwrap();
        // Paused, with the empty reservation holding its name: both go.
        let held = record(5, &dir, "held.bin", Status::Paused);
        partial(&held);
        std::fs::write(&held.path, b"").unwrap();
        let (m, log) = manager(&dir, &[paused.clone(), keep.clone(), remove.clone(), failed.clone(), held.clone()]);

        m.delete(1, false).await.unwrap();
        assert!(gone(&paused.path));
        m.delete(5, false).await.unwrap();
        assert!(gone(&held.path) && !held.path.exists());
        m.delete(2, false).await.unwrap();
        assert!(keep.path.exists());
        m.delete(3, true).await.unwrap();
        assert!(!remove.path.exists());
        m.delete(4, false).await.unwrap();
        assert!(failed.path.exists() && gone(&failed.path));
        assert!(m.list().is_empty());
        assert!(m.delete(1, false).await.is_err(), "already gone");
        assert_eq!(log.0.lock().unwrap().iter().filter(|l| l.starts_with("removed")).count(), 5);
        assert!(store::load(&store::path(&dir.join("data"))).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_finished_download_replaces_only_its_own_reservation() {
        let dir = temp_dir();
        let held = record(1, &dir, "held.zip", Status::Downloading);
        let taken = record(2, &dir, "taken.zip", Status::Downloading);
        let missing = record(3, &dir, "missing.zip", Status::Downloading);
        std::fs::write(&held.path, b"").unwrap();
        std::fs::write(&taken.path, b"the user's own file").unwrap();
        for r in [&held, &taken, &missing] {
            std::fs::write(files::part(&r.path), b"complete download").unwrap();
        }
        let (m, _) = manager(&dir, &[held.clone(), taken.clone(), missing.clone()]);

        // Our empty reservation is replaced.
        assert_eq!(m.complete(1, &held.path, &files::part(&held.path)).unwrap(), held.path);
        assert_eq!(std::fs::read(&held.path).unwrap(), b"complete download");
        assert!(!files::part(&held.path).exists());

        // Something else took the name: kept, and the download gets "name (1)".
        let new = m.complete(2, &taken.path, &files::part(&taken.path)).unwrap();
        assert_eq!(new, dir.join("taken (1).zip"));
        assert_eq!(std::fs::read(&taken.path).unwrap(), b"the user's own file");
        assert_eq!(std::fs::read(&new).unwrap(), b"complete download");
        assert_eq!((get(&m, 2).name.as_str(), get(&m, 2).path), ("taken (1).zip", new));

        // The reservation was removed: the name is free again and is used.
        assert_eq!(m.complete(3, &missing.path, &files::part(&missing.path)).unwrap(), missing.path);
        assert_eq!(std::fs::read(&missing.path).unwrap(), b"complete download");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn old_probes_are_dropped_when_the_cache_is_used() {
        let dir = temp_dir();
        let (m, _) = manager(&dir, &[]);
        let info = Lookup::File(ProbeInfo {
            final_url: "u".into(),
            size: None,
            ranges: false,
            validator: None,
            file_name: "f".into(),
            content_type: None,
        });
        let old = Instant::now().checked_sub(PROBE_FRESH + Duration::from_secs(1));
        if let Some(old) = old {
            m.probes.lock().unwrap().insert("old".into(), (old, info.clone()));
            m.probes.lock().unwrap().insert("new".into(), (Instant::now(), info));
            let probes = m.probes();
            assert!(!probes.contains_key("old") && probes.contains_key("new"));
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn open_needs_a_finished_download_and_no_path_from_outside() {
        let dir = temp_dir();
        let (m, _) = manager(&dir, &[record(1, &dir, "a.bin", Status::Paused)]);
        assert!(m.open_file(1).unwrap_err().contains("finished"));
        assert!(m.open_file(42).is_err());
        assert!(m.open_folder(42).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn video(id: u64, dir: &Path, name: &str, status: Status) -> Record {
        Record { kind: Kind::Video, url: "https://www.youtube.com/watch?v=x".into(), size: None, received: 0, ..record(id, dir, name, status) }
    }

    /// A video temp folder of `r`'s own, with its marker.
    fn own_temp(r: &Record) -> PathBuf {
        let t = video_temp(r).unwrap();
        std::fs::create_dir(&t).unwrap();
        marker::write(&t.join(marker::IN_FOLDER), &marker::Marker::new(r.id, &r.url)).unwrap();
        t
    }

    #[test]
    fn a_finished_video_takes_its_real_extension_and_never_overwrites() {
        let dir = temp_dir();
        let v = video(1, &dir, "Big Buck Bunny.mp4", Status::Downloading);
        // The user already has a file with the final name.
        std::fs::write(dir.join("Big Buck Bunny.mkv"), b"the user's own").unwrap();
        let temp = video_temp(&v).unwrap();
        std::fs::create_dir(&temp).unwrap();
        std::fs::write(temp.join("video.mkv"), b"merged video").unwrap();
        let (m, _) = manager(&dir, std::slice::from_ref(&v));

        let (target, bytes) = m.complete_video(1, &temp.join("video.mkv")).unwrap();
        assert_eq!(target, dir.join("Big Buck Bunny (1).mkv"));
        assert_eq!(bytes, 12);
        assert_eq!(std::fs::read(dir.join("Big Buck Bunny.mkv")).unwrap(), b"the user's own");
        assert_eq!(std::fs::read(&target).unwrap(), b"merged video");
        assert_eq!(get(&m, 1).name, "Big Buck Bunny (1).mkv");
        assert!(!dir.join("Big Buck Bunny.mp4").exists(), "the planned name was never reserved");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn videos_come_back_paused_only_with_their_temp_folder() {
        let dir = temp_dir();
        let with = video(1, &dir, "a.mp4", Status::Downloading);
        let without = video(2, &dir, "b.mp4", Status::Paused);
        let temp = own_temp(&with);
        std::fs::write(temp.join("video.f1.mp4.part"), vec![0u8; 700]).unwrap();
        let (m, _) = manager(&dir, &[with, without]);
        m.restore().await;
        assert_eq!((get(&m, 1).status, get(&m, 1).received), (Status::Paused, 700));
        assert_eq!((get(&m, 2).status, get(&m, 2).received), (Status::Queued, 0));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn deleting_a_video_removes_only_its_own_temp_folder() {
        let dir = temp_dir();
        let v = video(1, &dir, "clip.mp4", Status::Paused);
        // A file of the user's at the planned name, and another download's temp folder.
        std::fs::write(&v.path, b"precious").unwrap();
        let temp = own_temp(&v);
        std::fs::write(temp.join("video.mp4.part"), b"x").unwrap();
        let other = dir.join(format!("{VIDEO_TEMP_PREFIX}2"));
        std::fs::create_dir(&other).unwrap();
        let (m, _) = manager(&dir, std::slice::from_ref(&v));
        m.delete(1, true).await.unwrap();
        assert!(!temp.exists());
        assert!(other.exists());
        assert_eq!(std::fs::read(&v.path).unwrap(), b"precious");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Real network and the real bundled programs (from `target/debug`, put there by
    /// `cargo build` after `npm run sidecars`). Run with
    /// `cargo test real_video -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "needs the network and the bundled programs"]
    async fn real_video_download_pause_resume_and_finish() {
        let dir = temp_dir();
        let bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug");
        let (m, _) = manager(&dir, &[]);
        m.set_video_tools(Arc::new(Tools::new(&bin, &bin, &dir.join("data"))));
        let url = std::env::var("KSDM_VIDEO_URL").unwrap_or_else(|_| "https://www.youtube.com/watch?v=jNQXAC9IVRw".into());
        let view = m.probe(&url).await.unwrap();
        assert_eq!(view.kind, Kind::Video);
        println!("probe: {view:?}");
        let r = m.add(&url, "", dir.to_str().unwrap(), true).await.unwrap();
        // Pause shortly after it starts, then resume.
        tokio::time::sleep(Duration::from_millis(2500)).await;
        m.pause(r.id).unwrap();
        while m.active().running > 0 {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let paused = get(&m, r.id);
        println!("after pause: {:?} {} bytes", paused.status, paused.received);
        assert!(matches!(paused.status, Status::Paused | Status::Queued | Status::Finished), "{paused:?}");
        if paused.status != Status::Finished {
            m.resume(r.id).unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(180);
        while get(&m, r.id).status == Status::Downloading && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let done = get(&m, r.id);
        println!("finished: {done:?}");
        assert_eq!(done.status, Status::Finished, "{:?}", done.error);
        assert!(done.path.is_file() && done.received > 100_000);
        assert!(!video_temp(&done).unwrap().exists(), "temp folder removed");
        // A plain web page is refused, never saved.
        assert_eq!(m.probe("https://example.com/").await.unwrap_err(), "This link is a web page, not a file.");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn a_leftover_temp_folder_of_another_url_is_never_continued_in() {
        let dir = temp_dir();
        let v = video(1, &dir, "clip.mp4", Status::Paused);
        // A folder with this download's default name, left by something else.
        let old = video_temp(&v).unwrap();
        std::fs::create_dir(&old).unwrap();
        std::fs::write(old.join("video.mp4.part"), b"someone else's bytes").unwrap();
        marker::write(&old.join(marker::IN_FOLDER), &marker::Marker::new(1, "https://other.example/v")).unwrap();
        // And one with no marker at all, under the next name.
        let unmarked = dir.join(format!("{VIDEO_TEMP_PREFIX}1-1"));
        std::fs::create_dir(&unmarked).unwrap();
        let (m, _) = manager(&dir, std::slice::from_ref(&v));
        m.restore().await;
        assert_eq!(get(&m, 1).status, Status::Queued, "not resumable from a folder that is not ours");

        let fresh = m.prepare_video_temp(&get(&m, 1)).unwrap();
        assert_eq!(fresh, dir.join(format!("{VIDEO_TEMP_PREFIX}1-2")));
        assert_eq!(std::fs::read(old.join("video.mp4.part")).unwrap(), b"someone else's bytes");
        assert!(unmarked.exists() && std::fs::read_dir(&unmarked).unwrap().next().is_none());
        let r = get(&m, 1);
        assert_eq!(video_temp(&r).unwrap(), fresh);
        // Deleting the download removes only its own folder.
        m.delete(1, false).await.unwrap();
        assert!(!fresh.exists() && old.exists() && unmarked.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn a_matching_temp_folder_resumes_as_before() {
        let dir = temp_dir();
        let v = video(1, &dir, "clip.mp4", Status::Paused);
        let own = video_temp(&v).unwrap();
        std::fs::create_dir(&own).unwrap();
        marker::write(&own.join(marker::IN_FOLDER), &marker::Marker::new(1, &v.url)).unwrap();
        std::fs::write(own.join("video.f1.mp4.part"), vec![0u8; 500]).unwrap();
        let (m, _) = manager(&dir, std::slice::from_ref(&v));
        m.restore().await;
        assert_eq!(get(&m, 1).status, Status::Paused);
        assert_eq!(m.prepare_video_temp(&get(&m, 1)).unwrap(), own);
        assert_eq!(std::fs::read(own.join("video.f1.mp4.part")).unwrap().len(), 500);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn ids_are_never_reused_after_deleting_the_newest() {
        let dir = temp_dir();
        let (m, _) = manager(&dir, &[]);
        let tools = Arc::new(Tools::new(&dir, &dir, &dir));
        m.set_video_tools(tools);
        let info = VideoInfo { title: "A".into(), ext: "mp4".into(), size: None, entries: vec![], is_playlist: false };
        let first = m.add_video("https://v.example/1", "", dir.to_str().unwrap(), false, &info).unwrap();
        m.delete(first.id, false).await.unwrap();
        let second = m.add_video("https://v.example/2", "", dir.to_str().unwrap(), false, &info).unwrap();
        assert!(second.id > first.id);
        // Also after a restart.
        let (again, _) = (Manager::new(store::path(&dir.join("data")), Arc::new(Log::default())), ());
        let third = again.add_video("https://v.example/3", "", dir.to_str().unwrap(), false, &info).unwrap();
        assert!(third.id > second.id);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn video_names_are_safe() {
        assert_eq!(video_name("a/b: c?", "mkv"), "a_b_ c_.mkv");
        assert_eq!(video_name("x", ""), "x.mp4");
        assert_eq!(without_media_extension("Title.mp4"), "Title");
        assert_eq!(without_media_extension("Mr. Smith"), "Mr. Smith", "a dot is not always an extension");
        assert_eq!(without_media_extension("v1.2 release.MKV"), "v1.2 release");
        assert_eq!(without_media_extension(".hidden"), ".hidden");
        assert_eq!(video_message(&VideoError::NotAVideoSite).0, "This link is a web page, not a file.");
        assert_eq!(video_message(&VideoError::Outdated("x".into())).1, Some(VIDEO_OUTDATED));
    }

    #[test]
    fn nothing_running_sends_no_progress() {
        let dir = temp_dir();
        let (m, log) = manager(&dir, &[record(1, &dir, "a.bin", Status::Paused)]);
        m.tick();
        assert!(log.0.lock().unwrap().is_empty());
        assert_eq!(m.active(), Active::default());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn speeds_are_smoothed_and_never_negative() {
        let progress = Progress::new();
        let start = Instant::now();
        let mut run = Running {
            cancel: CancelToken::new(),
            progress: Source::Engine(progress),
            stop: None,
            deleted: None,
            last_at: start,
            last_received: 1000,
            last_per: vec![500, 500],
            speed: 0.0,
            connection_speed: vec![0.0, 0.0],
        };
        // The counters went back to zero (the engine restarted the download).
        let tick = run.sample(7, start + Duration::from_millis(250));
        assert_eq!(tick.id, 7);
        assert_eq!(tick.speed, 0.0);
        assert!(tick.connections.is_empty());
    }
}
