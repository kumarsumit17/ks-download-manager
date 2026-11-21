//! Video downloads through yt-dlp (specs/video-playlists.md). yt-dlp is a bundled,
//! unmodified program run as a child process; ffmpeg joins the streams and Deno runs the
//! JavaScript some sites need. This module builds the command lines, reads yt-dlp's
//! output, and stops it. Nothing here touches the user's folder except through the temp
//! folder the manager gives it.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use engine::CancelToken;
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command};

/// How long looking up a link may take before it is given up.
const INFO_TIMEOUT: Duration = Duration::from_secs(60);
/// After asking yt-dlp to stop, how long before it is killed outright.
const STOP_GRACE: Duration = Duration::from_secs(3);
/// The fixed name yt-dlp writes to inside our temp folder; the real name is given after.
const OUTPUT_TEMPLATE: &str = "video.%(ext)s";
/// Marks the lines of yt-dlp's output that are meant for us.
const PROGRESS: &str = "KSDM-P ";
const MERGING: &str = "KSDM-M ";
const FILE: &str = "KSDM-F ";

#[cfg(target_os = "windows")]
const YTDLP_EXE: &str = "yt-dlp.exe";
#[cfg(not(target_os = "windows"))]
const YTDLP_EXE: &str = "yt-dlp_macos";
#[cfg(target_os = "windows")]
const DENO_EXE: &str = "deno.exe";
#[cfg(not(target_os = "windows"))]
const DENO_EXE: &str = "deno";

/// Where the bundled programs are. yt-dlp can be replaced at run time by a newer copy
/// in the app data folder ("Update video component").
pub struct Tools {
    ytdlp: Mutex<PathBuf>,
    deno: PathBuf,
    ffmpeg_dir: PathBuf,
    cache: PathBuf,
    /// `<app data>/video-component`: updated copies of yt-dlp.
    pub component_dir: PathBuf,
    /// Settings that change the format choice (Part 3: "prefer MP4").
    prefer_mp4: Mutex<bool>,
    /// The speed limit in bytes per second, passed as `--limit-rate` to each new run.
    rate_limit: Mutex<Option<u64>>,
}

impl Tools {
    /// `programs`: the folder with deno, ffmpeg and ffprobe (next to the app executable);
    /// `resources`: the app's resource folder, holding `yt-dlp/`; `data`: app data.
    pub fn new(programs: &Path, resources: &Path, data: &Path) -> Self {
        let bundled = resources.join("yt-dlp").join(YTDLP_EXE);
        let component_dir = data.join("video-component");
        let ytdlp = crate::component::current(&component_dir).unwrap_or(bundled);
        Self {
            ytdlp: Mutex::new(ytdlp),
            deno: programs.join(DENO_EXE),
            ffmpeg_dir: programs.to_path_buf(),
            cache: data.join("yt-dlp-cache"),
            component_dir,
            prefer_mp4: Mutex::new(false),
            rate_limit: Mutex::new(None),
        }
    }

    pub fn ytdlp(&self) -> PathBuf {
        self.ytdlp.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set_ytdlp(&self, path: PathBuf) {
        *self.ytdlp.lock().unwrap_or_else(|e| e.into_inner()) = path;
    }

    pub fn set_prefer_mp4(&self, on: bool) {
        *self.prefer_mp4.lock().unwrap_or_else(|e| e.into_inner()) = on;
    }

    fn prefer_mp4(&self) -> bool {
        *self.prefer_mp4.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// An approximation of the shared limit: yt-dlp paces each run on its own, so running
    /// videos and files together can exceed it, and a change applies from the next run.
    pub fn set_rate_limit(&self, bytes_per_second: Option<u64>) {
        *self.rate_limit.lock().unwrap_or_else(|e| e.into_inner()) = bytes_per_second;
    }

    fn rate_args(&self) -> Vec<OsString> {
        match *self.rate_limit.lock().unwrap_or_else(|e| e.into_inner()) {
            Some(r) if r > 0 => vec!["--limit-rate".into(), r.to_string().into()],
            _ => Vec::new(),
        }
    }

    /// The arguments every run gets: no user config, plugins or downloaded components, our
    /// own ffmpeg, Deno and cache, named site extractors only, never a whole playlist.
    pub fn base_args(&self) -> Vec<OsString> {
        let mut deno = OsString::from("deno:");
        deno.push(&self.deno);
        let mut a: Vec<OsString> = [
            "--ignore-config",
            "--no-plugin-dirs",
            "--no-remote-components",
            "--no-update",
            "--no-cookies-from-browser",
            "--color",
            "never",
            "--ies",
            "default,-generic",
        ]
        .iter()
        .map(OsString::from)
        .collect();
        a.extend([OsString::from("--js-runtimes"), deno]);
        a.extend([OsString::from("--ffmpeg-location"), self.ffmpeg_dir.clone().into_os_string()]);
        a.extend([OsString::from("--cache-dir"), self.cache.clone().into_os_string()]);
        a
    }

    /// Best available quality (specs/video-playlists.md): highest resolution, then frame
    /// rate, then bitrate, joined with the best audio; MP4 when the codecs fit, else MKV.
    /// Never re-encoded. "Prefer MP4" only breaks ties toward MP4-friendly codecs at the
    /// same resolution and frame rate; it never lowers the resolution.
    pub fn format_args(&self) -> Vec<OsString> {
        let sort = if self.prefer_mp4() { "res,fps,vcodec:h264,acodec:aac,br" } else { "res,fps,br" };
        ["-f", "bv*+ba/b", "-S", sort, "--merge-output-format", "mp4/mkv"].iter().map(OsString::from).collect()
    }

    fn command(&self, args: Vec<OsString>, url: &str) -> Command {
        let mut c = Command::new(self.ytdlp());
        c.args(self.base_args()).args(args).arg("--").arg(url);
        c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
        own_group(&mut c);
        c
    }

    /// Looks a link up without downloading: title, final extension, size estimate.
    /// `--flat-playlist` always: a playlist link then lists its entries at once instead of
    /// looking every video up first (which can take minutes); a single video is unaffected.
    /// With `playlist` off, a video link that also names a playlist (`watch?v=…&list=…`)
    /// is that one video; a playlist link is a playlist either way.
    pub async fn info(&self, url: &str, playlist: bool) -> Result<VideoInfo, VideoError> {
        let mut args = self.format_args();
        args.extend(["-J".into(), "--flat-playlist".into()]);
        args.push(if playlist { "--yes-playlist".into() } else { "--no-playlist".into() });
        let mut c = self.command(args, url);
        let child = c.spawn().map_err(|e| VideoError::Start(e.to_string()))?;
        let output = tokio::time::timeout(INFO_TIMEOUT, collect(child)).await;
        let (code, stdout, stderr) = match output {
            Ok(r) => r.map_err(|e| VideoError::Start(e.to_string()))?,
            Err(_) => return Err(VideoError::Timeout),
        };
        if code != Some(0) {
            return Err(classify(&stderr));
        }
        if runtime_failed(&stderr) {
            return Err(VideoError::Runtime);
        }
        parse_info(&stdout)
    }

    /// Downloads `url` into `temp` (a folder of our own) under the fixed name, and returns
    /// the file yt-dlp made. yt-dlp keeps its `.part` files there, so running this again
    /// for the same folder continues where it stopped.
    pub async fn download(
        &self,
        url: &str,
        temp: &Path,
        cancel: CancelToken,
        progress: VideoProgress,
    ) -> Result<PathBuf, VideoError> {
        std::fs::create_dir_all(temp).map_err(|e| VideoError::Other(e.to_string()))?;
        let mut home = OsString::from("home:");
        home.push(temp);
        let mut tmp = OsString::from("temp:");
        tmp.push(temp);
        let mut args = self.format_args();
        args.extend(
            [
                "--no-playlist",
                "--newline",
                "--progress",
                "--progress-template",
                "download:KSDM-P %(info.format_id)s %(progress.downloaded_bytes)s %(progress.total_bytes)s %(progress.total_bytes_estimate)s",
                "--progress-template",
                "postprocess:KSDM-M %(progress.postprocessor)s %(progress.status)s",
                "--print",
                "after_move:KSDM-F %(filepath)s",
                "-o",
                OUTPUT_TEMPLATE,
            ]
            .iter()
            .map(OsString::from),
        );
        args.extend(["-P".into(), home, "-P".into(), tmp]);
        args.extend(self.rate_args());
        let mut c = self.command(args, url);
        let mut child = c.spawn().map_err(|e| VideoError::Start(e.to_string()))?;
        let stdout = child.stdout.take().expect("piped");
        let stderr = child.stderr.take().expect("piped");
        let errors = tokio::spawn(last_lines(stderr));
        let file: Arc<Mutex<Option<PathBuf>>> = Arc::default();
        let reader = {
            let (file, progress) = (file.clone(), progress.clone());
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Some(p) = line.strip_prefix(FILE) {
                        *file.lock().unwrap_or_else(|e| e.into_inner()) = Some(PathBuf::from(p.trim()));
                    } else {
                        progress.line(&line);
                    }
                }
            })
        };
        let status = tokio::select! {
            s = child.wait() => s.map_err(|e| VideoError::Other(e.to_string()))?,
            () = cancel.cancelled() => {
                stop(&mut child).await;
                let _ = reader.await;
                return Err(VideoError::Cancelled);
            }
        };
        let _ = reader.await;
        let stderr = errors.await.unwrap_or_default();
        if !status.success() {
            return Err(classify(&stderr));
        }
        // yt-dlp carries on with fewer formats when Deno fails; that is a quiet drop in
        // quality, which the spec does not allow.
        if runtime_failed(&stderr) {
            return Err(VideoError::Runtime);
        }
        let made = file.lock().unwrap_or_else(|e| e.into_inner()).take();
        match made {
            // Only a file inside our own folder is accepted.
            Some(p) if p.parent() == Some(temp) && p.is_file() => Ok(p),
            _ => Err(VideoError::Other("yt-dlp finished without a file".into())),
        }
    }

    /// The version of the yt-dlp in use, from `yt-dlp --version`.
    pub async fn version(&self, program: &Path) -> Option<String> {
        let out = Command::new(program).arg("--version").stdin(Stdio::null()).output().await.ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
    }
}

/// Puts the child in its own process group, so stopping it also stops what it started
/// (ffmpeg, Deno).
#[cfg(unix)]
fn own_group(c: &mut Command) {
    c.process_group(0);
}

#[cfg(windows)]
fn own_group(c: &mut Command) {
    // CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW. Untested until the Windows build.
    c.creation_flags(0x0000_0200 | 0x0800_0000);
}

/// Stops yt-dlp and everything it started: SIGTERM to the whole group, then SIGKILL
/// after a short grace period. Uses the system `kill` (no unsafe code, no extra crate).
async fn stop(child: &mut Child) {
    let Some(pid) = child.id() else { return };
    signal_group(pid, "TERM");
    if tokio::time::timeout(STOP_GRACE, child.wait()).await.is_err() {
        signal_group(pid, "KILL");
        let _ = child.wait().await;
    }
}

#[cfg(unix)]
fn signal_group(pid: u32, signal: &str) {
    // The group ID equals the child's PID (process_group(0)); "-PID" names the group.
    let _ = std::process::Command::new("/bin/kill")
        .args(["-s", signal, "--", &format!("-{pid}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(windows)]
fn signal_group(pid: u32, _signal: &str) {
    // The whole tree, forcibly. Untested until the Windows build.
    let _ = std::process::Command::new("taskkill")
        .args(["/T", "/F", "/PID", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

async fn collect(mut child: Child) -> std::io::Result<(Option<i32>, String, String)> {
    let stdout = child.stdout.take().expect("piped");
    let stderr = child.stderr.take().expect("piped");
    let out = tokio::spawn(read_all(stdout));
    let err = tokio::spawn(last_lines(stderr));
    let status = child.wait().await?;
    let out = out.await.unwrap_or_default();
    let err = err.await.unwrap_or_default();
    Ok((status.code(), out, err))
}

async fn read_all(r: impl AsyncRead + Unpin) -> String {
    use tokio::io::AsyncReadExt;
    let mut s = String::new();
    let _ = BufReader::new(r).read_to_string(&mut s).await;
    s
}

/// Marks a JavaScript-runtime failure kept from anywhere in stderr.
const RUNTIME_FAILED: &str = "KSDM-RUNTIME-FAILED";

/// True when yt-dlp reported that Deno could not solve a site's challenge.
fn runtime_failed(stderr: &str) -> bool {
    stderr.lines().any(|l| l == RUNTIME_FAILED)
}

/// The last 40 lines of stderr, enough to find yt-dlp's `ERROR:` line, plus a marker line
/// when Deno failed anywhere in the run.
async fn last_lines(r: impl AsyncRead + Unpin) -> String {
    let mut lines = BufReader::new(r).lines();
    let mut keep = std::collections::VecDeque::new();
    let mut runtime = false;
    while let Ok(Some(line)) = lines.next_line().await {
        if line.starts_with("WARNING:") && line.contains("[jsc]") && line.contains("Error") {
            runtime = true;
        }
        if keep.len() == 40 {
            keep.pop_front();
        }
        keep.push_back(line);
    }
    if runtime {
        keep.push_back(RUNTIME_FAILED.into());
    }
    Vec::from(keep).join("\n")
}

/// What the Add dialog needs about a video link.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    pub title: String,
    /// The extension of the finished file (`mp4` or `mkv`).
    pub ext: String,
    /// Estimated total of the chosen streams, when the site says.
    pub size: Option<u64>,
    /// Entries when the link is a playlist (Part 5); empty for a single video.
    pub entries: Vec<PlaylistEntry>,
    pub is_playlist: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistEntry {
    /// Position in the playlist, from 1, counting every entry yt-dlp listed.
    pub index: u32,
    pub url: String,
    pub title: String,
    pub duration: Option<f64>,
    pub thumbnail: Option<String>,
    /// Private, removed or otherwise not downloadable, as far as the list shows.
    pub unavailable: bool,
}

fn parse_info(json: &str) -> Result<VideoInfo, VideoError> {
    let v: serde_json::Value =
        serde_json::from_str(json.trim()).map_err(|_| VideoError::Other("yt-dlp gave no usable information".into()))?;
    let title = v.get("title").and_then(|t| t.as_str()).unwrap_or("video").to_owned();
    if v.get("_type").and_then(|t| t.as_str()) == Some("playlist") {
        let entries = v
            .get("entries")
            .and_then(|e| e.as_array())
            .map(|a| a.iter().enumerate().filter_map(|(i, e)| parse_entry(u32::try_from(i + 1).ok()?, e)).collect())
            .unwrap_or_default();
        return Ok(VideoInfo { title, ext: String::new(), size: None, entries, is_playlist: true });
    }
    let live = v.get("is_live").and_then(|l| l.as_bool()) == Some(true)
        || matches!(v.get("live_status").and_then(|l| l.as_str()), Some("is_live" | "is_upcoming"));
    if live {
        return Err(VideoError::Live);
    }
    let ext = v.get("ext").and_then(|e| e.as_str()).unwrap_or("mp4").to_owned();
    let duration = v.get("duration").and_then(|d| d.as_f64());
    let size_of = |f: &serde_json::Value| {
        let n = |k: &str| f.get(k).and_then(|x| x.as_f64());
        n("filesize").or_else(|| n("filesize_approx")).or_else(|| Some(n("tbr")? * 125.0 * duration?))
    };
    let size = match v.get("requested_formats").and_then(|r| r.as_array()) {
        Some(formats) => formats.iter().map(size_of).sum::<Option<f64>>(),
        None => size_of(&v),
    };
    Ok(VideoInfo { title, ext, size: size.map(|s| s as u64), entries: Vec::new(), is_playlist: false })
}

fn parse_entry(index: u32, e: &serde_json::Value) -> Option<PlaylistEntry> {
    let s = |k: &str| e.get(k).and_then(|x| x.as_str()).map(str::to_owned);
    let url = s("url").or_else(|| s("webpage_url"))?;
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return None;
    }
    let title = s("title").unwrap_or_default();
    let unavailable = title.is_empty()
        || is_nested_list(e)
        || matches!(title.as_str(), "[Private video]" | "[Deleted video]" | "[Unavailable video]")
        || matches!(s("availability").as_deref(), Some("private" | "needs_auth" | "subscriber_only" | "premium_only"));
    let thumbnail = e
        .get("thumbnails")
        .and_then(|t| t.as_array())
        .and_then(|t| t.last())
        .and_then(|t| t.get("url"))
        .and_then(|u| u.as_str())
        .map(str::to_owned)
        .or_else(|| s("thumbnail"));
    Some(PlaylistEntry { index, url, title, duration: e.get("duration").and_then(|d| d.as_f64()), thumbnail, unavailable })
}

/// An entry that is itself a list (a channel's "Videos" tab, a playlist inside a playlist).
/// It is shown but not offered: each entry is downloaded as one video with
/// `--no-playlist`, and a list cannot be one.
fn is_nested_list(e: &serde_json::Value) -> bool {
    let s = |k: &str| e.get(k).and_then(|x| x.as_str()).unwrap_or("");
    let key = s("ie_key").to_ascii_lowercase();
    s("_type") == "playlist" || key.ends_with("tab") || key.contains("playlist") || key.contains("channel")
}

/// Why yt-dlp did not give us a video.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VideoError {
    /// No site extractor knows this link: it is an ordinary web page.
    NotAVideoSite,
    /// The site changed and this yt-dlp cannot read it any more.
    Outdated(String),
    SignIn,
    /// The site temporarily blocks this network ("confirm you're not a bot").
    Blocked,
    /// The site refused the stream (HTTP 403) without saying why.
    Forbidden(String),
    Unavailable,
    Drm,
    Live,
    /// Deno, which yt-dlp needs for some sites, did not run.
    Runtime,
    Timeout,
    /// yt-dlp could not be started at all (missing or blocked program).
    Start(String),
    Cancelled,
    Other(String),
}

/// Sorts yt-dlp's last `ERROR:` line into what the user can do about it.
pub fn classify(stderr: &str) -> VideoError {
    let line = stderr.lines().rev().find(|l| l.starts_with("ERROR:")).unwrap_or("").to_owned();
    let low = line.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| low.contains(n));
    if has(&["no suitable extractor", "unsupported url"]) {
        VideoError::NotAVideoSite
    } else if has(&["drm"]) {
        VideoError::Drm
    } else if has(&["not a bot", "unusual traffic", "too many requests", "http error 429"]) {
        VideoError::Blocked
    } else if has(&["sign in", "log in", "login required", "private video", "members-only", "confirm your age"]) {
        VideoError::SignIn
    } else if has(&["is live", "live event", "premieres in", "live stream"]) {
        VideoError::Live
    } else if has(&["unable to extract", "nsig", "signature extraction", "failed to extract", "player response", "requested format is not available", "unable to download api page", "please report this issue"]) {
        VideoError::Outdated(short(&line))
    } else if has(&["http error 403"]) {
        VideoError::Forbidden(short(&line))
    } else if has(&["video unavailable", "this video is unavailable", "has been removed", "not available in your country", "no longer available", "http error 404"]) {
        VideoError::Unavailable
    } else if line.is_empty() {
        VideoError::Other(short(stderr.lines().last().unwrap_or("yt-dlp stopped with an error")))
    } else {
        VideoError::Other(short(&line))
    }
}

fn short(line: &str) -> String {
    let s = line.trim_start_matches("ERROR:").trim();
    if s.chars().count() > 200 { s.chars().take(200).collect::<String>() + "…" } else { s.to_owned() }
}

/// Live numbers for one video download, filled from yt-dlp's progress lines. Video and
/// audio come down one after the other; each stream's bytes are kept apart and added up.
#[derive(Clone, Default)]
pub struct VideoProgress(Arc<Mutex<ProgressState>>);

#[derive(Default)]
struct ProgressState {
    /// (format ID, bytes so far, total or estimate)
    streams: Vec<(String, u64, Option<u64>)>,
    merging: bool,
}

impl VideoProgress {
    fn line(&self, line: &str) {
        let mut s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(rest) = line.strip_prefix(PROGRESS) {
            if let Some((id, done, total)) = parse_progress(rest) {
                match s.streams.iter_mut().find(|(f, ..)| *f == id) {
                    Some(stream) => (stream.1, stream.2) = (done, total.or(stream.2)),
                    None => s.streams.push((id, done, total)),
                }
                s.merging = false;
            }
        } else if let Some(rest) = line.strip_prefix(MERGING) {
            // Merger or fixups running: the bytes are all here, ffmpeg is joining them.
            s.merging = rest.ends_with("started");
        }
    }

    /// (bytes so far, total if known, joining streams now)
    pub fn snapshot(&self) -> (u64, Option<u64>, bool) {
        let s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let done = s.streams.iter().map(|x| x.1).sum();
        let total = s.streams.iter().map(|x| x.2).sum::<Option<u64>>();
        (done, total, s.merging)
    }
}

/// `<format id> <downloaded> <total> <estimate>`; yt-dlp writes `NA` for unknown values
/// and may write a number as a float.
fn parse_progress(rest: &str) -> Option<(String, u64, Option<u64>)> {
    let mut parts = rest.split_whitespace();
    let id = parts.next()?.to_owned();
    let num = |s: Option<&str>| s.and_then(|s| s.parse::<f64>().ok()).filter(|n| n.is_finite() && *n >= 0.0).map(|n| n as u64);
    let done = num(parts.next())?;
    let total = num(parts.next());
    let estimate = num(parts.next());
    Some((id, done, total.or(estimate)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools() -> Tools {
        Tools::new(Path::new("/app/MacOS"), Path::new("/app/Resources"), Path::new("/data"))
    }

    #[test]
    fn every_run_ignores_user_config_plugins_and_remote_code() {
        let a: Vec<String> = tools().base_args().iter().map(|s| s.to_string_lossy().into_owned()).collect();
        for flag in ["--ignore-config", "--no-plugin-dirs", "--no-remote-components", "--no-update"] {
            assert!(a.contains(&flag.to_string()), "{flag}");
        }
        let after = |f: &str| a.iter().position(|x| x == f).map(|i| a[i + 1].clone());
        assert_eq!(after("--js-runtimes").unwrap(), format!("deno:/app/MacOS/{DENO_EXE}"));
        assert_eq!(after("--ffmpeg-location").unwrap(), "/app/MacOS");
        assert_eq!(after("--cache-dir").unwrap(), "/data/yt-dlp-cache");
        assert_eq!(after("--ies").unwrap(), "default,-generic", "the generic extractor is off");
        assert!(!a.iter().any(|x| x.contains("cookies") && x != "--no-cookies-from-browser"));
    }

    #[test]
    fn the_url_comes_after_a_double_dash() {
        let c = tools().command(vec![], "--exec=evil");
        let args: Vec<_> = c.as_std().get_args().map(|s| s.to_string_lossy().into_owned()).collect();
        let n = args.len();
        assert_eq!(&args[n - 2..], ["--", "--exec=evil"]);
    }

    #[test]
    fn best_quality_never_re_encodes_and_prefers_mp4_only_at_equal_resolution() {
        let t = tools();
        let a: Vec<String> = t.format_args().iter().map(|s| s.to_string_lossy().into_owned()).collect();
        assert_eq!(a, ["-f", "bv*+ba/b", "-S", "res,fps,br", "--merge-output-format", "mp4/mkv"]);
        t.set_prefer_mp4(true);
        let a: Vec<String> = t.format_args().iter().map(|s| s.to_string_lossy().into_owned()).collect();
        assert!(a[3].starts_with("res,fps,"), "resolution and frame rate still come first");
        assert!(!a.iter().any(|x| x.contains("recode") || x.contains("remux")));
    }

    #[test]
    fn progress_lines_add_up_both_streams() {
        let p = VideoProgress::default();
        p.line("KSDM-P 628 1000 NA 5000.0");
        p.line("KSDM-P 628 5000 5000 NA");
        p.line("KSDM-P 258 100 400 NA");
        p.line("[download] something else");
        assert_eq!(p.snapshot(), (5100, Some(5400), false));
        p.line("KSDM-M Merger started");
        assert!(p.snapshot().2);
        p.line("KSDM-M Merger finished");
        assert!(!p.snapshot().2);
        assert_eq!(parse_progress("x NA NA NA"), None);
        assert_eq!(parse_progress("x 10 NA NA"), Some(("x".into(), 10, None)));
    }

    #[test]
    fn errors_are_sorted_by_what_the_user_can_do() {
        let e = |s: &str| classify(&format!("WARNING: x\nERROR: {s}"));
        assert_eq!(e("No suitable extractor found for URL https://example.com/"), VideoError::NotAVideoSite);
        assert_eq!(e("Unsupported URL: https://example.com/"), VideoError::NotAVideoSite);
        assert_eq!(e("[youtube] x: This video is DRM protected"), VideoError::Drm);
        assert_eq!(e("[youtube] x: Sign in to confirm you're not a bot"), VideoError::Blocked);
        assert_eq!(e("[youtube] x: Sign in to confirm your age"), VideoError::SignIn);
        assert_eq!(e("[youtube] x: This video is unavailable"), VideoError::Unavailable);
        assert!(matches!(e("[youtube] x: Unable to extract nsig function"), VideoError::Outdated(_)));
        assert!(matches!(e("unable to download video data: HTTP Error 403: Forbidden"), VideoError::Forbidden(_)));
        assert!(matches!(e("something new"), VideoError::Other(m) if m == "something new"));
    }

    #[tokio::test]
    async fn a_failing_javascript_runtime_is_noticed() {
        let err = b"[youtube] x: Downloading\nWARNING: [youtube] [jsc] Error solving n challenge request using \"deno\" provider\n[info] x: Downloading 1 format(s)\n";
        let text = last_lines(&err[..]).await;
        assert!(runtime_failed(&text));
        assert!(!runtime_failed(&last_lines(&b"WARNING: other\n"[..]).await));
    }

    #[test]
    fn info_reads_title_extension_and_size() {
        let json = r#"{"title":"Big Buck Bunny","ext":"mkv","duration":10,"requested_formats":[
            {"format_id":"628","filesize":null,"tbr":8000},{"format_id":"258","filesize":30000}]}"#;
        let i = parse_info(json).unwrap();
        assert_eq!((i.title.as_str(), i.ext.as_str(), i.size, i.is_playlist), ("Big Buck Bunny", "mkv", Some(10_030_000), false));
        assert_eq!(parse_info(r#"{"title":"x","is_live":true}"#), Err(VideoError::Live), "live streams are refused");
        let pl = parse_info(
            r#"{"_type":"playlist","title":"List","entries":[
            {"url":"https://www.youtube.com/watch?v=a","title":"A","duration":61},
            {"url":"https://www.youtube.com/watch?v=b","title":"[Private video]"},
            {"url":"javascript:alert(1)","title":"bad"},
            {"_type":"url","ie_key":"YoutubeTab","url":"https://www.youtube.com/@x/videos","title":"Videos"},
            {"url":"https://www.youtube.com/watch?v=c","title":"C","thumbnails":[{"url":"https://i.ytimg.com/s.jpg"},{"url":"https://i.ytimg.com/l.jpg"}]}]}"#,
        )
        .unwrap();
        assert!(pl.is_playlist);
        assert_eq!(pl.entries.len(), 4, "only http(s) entries");
        assert!(!pl.entries[0].unavailable && pl.entries[1].unavailable);
        assert!(pl.entries[2].unavailable, "a list inside the list is not one video");
        let idx: Vec<u32> = pl.entries.iter().map(|e| e.index).collect();
        assert_eq!(idx, [1, 2, 4, 5], "positions count every listed entry");
        assert_eq!(pl.entries[3].thumbnail.as_deref(), Some("https://i.ytimg.com/l.jpg"), "the largest thumbnail");
    }
}
