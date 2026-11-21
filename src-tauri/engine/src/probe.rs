use reqwest::StatusCode;
use reqwest::header::{CONTENT_DISPOSITION, CONTENT_RANGE, CONTENT_TYPE, ETAG, HeaderMap, LAST_MODIFIED, RANGE};

use ks_utility::content_disposition::{content_disposition_filename, percent_decode};
use ks_utility::file_name::sanitize_file_name;

use crate::error::EngineError;
use crate::http::{ContentRange, client, get, parse_content_range, parse_url};

/// What the server says about a file before downloading it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeInfo {
    /// The URL after redirects; ranges are fetched from here.
    pub final_url: String,
    /// Total size in bytes, or `None` when the server does not say.
    pub size: Option<u64>,
    /// True when the server answered a byte range request with 206 (or 416 for an empty file).
    pub ranges: bool,
    /// ETag, else Last-Modified, used to make sure every range comes from the same file.
    pub validator: Option<Validator>,
    /// From Content-Disposition, else the last part of the URL path, made safe as a file name.
    pub file_name: String,
    /// The media type from `Content-Type`, lower case, without parameters (`text/html`).
    pub content_type: Option<String>,
}

impl ProbeInfo {
    /// True when the link answers with a web page rather than a file.
    pub fn is_web_page(&self) -> bool {
        matches!(self.content_type.as_deref(), Some("text/html" | "application/xhtml+xml"))
    }

    /// True when a download of this file can be paused and resumed: the server supports
    /// ranges, the size is known and not zero, and it gave a strong validator (an ETag that
    /// is not weak, or Last-Modified). Otherwise stopping the download loses what it has.
    pub fn resumable(&self) -> bool {
        self.ranges && self.size.is_some_and(|s| s > 0) && self.validator.as_ref().is_some_and(Validator::is_strong)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Validator {
    ETag(String),
    LastModified(String),
}

impl Validator {
    /// Strong enough to prove a partial file belongs to this version (usable in `If-Range`).
    pub fn is_strong(&self) -> bool {
        self.if_range().is_some()
    }

    /// The value to send in `If-Range`. Weak ETags are not allowed there (RFC 9110, 13.1.5).
    pub(crate) fn if_range(&self) -> Option<&str> {
        match self {
            Self::ETag(e) if e.starts_with("W/") => None,
            Self::ETag(e) | Self::LastModified(e) => Some(e),
        }
    }
}

/// Learns size, range support, validator and file name with one ranged GET (`bytes=0-0`).
/// HEAD is not used: many servers reject it or answer it differently from GET.
pub async fn probe(url: &str) -> Result<ProbeInfo, EngineError> {
    let url = parse_url(url)?;
    let client = client()?;
    let resp = get(&client, &url).header(RANGE, "bytes=0-0").send().await?;
    let status = resp.status();
    let headers = resp.headers().clone();
    let final_url = resp.url().clone();
    let content_length = resp.content_length();
    // Dropping the response closes the connection without reading a large 200 body.
    drop(resp);

    let range_header = headers.get(CONTENT_RANGE).and_then(|v| v.to_str().ok()).and_then(parse_content_range);
    let (size, ranges) = match status {
        StatusCode::PARTIAL_CONTENT => match range_header {
            Some(ContentRange::Bytes { first: 0, complete, .. }) => (complete, true),
            // A 206 without a usable Content-Range cannot be trusted for ranges.
            _ => (None, false),
        },
        StatusCode::RANGE_NOT_SATISFIABLE => match range_header {
            // `bytes */0`: an empty file on a range-capable server.
            Some(ContentRange::Unsatisfied { complete }) => (Some(complete), true),
            _ => return Err(EngineError::HttpStatus(status.as_u16())),
        },
        s if s.is_success() => (content_length, false),
        s => return Err(EngineError::HttpStatus(s.as_u16())),
    };

    Ok(ProbeInfo {
        file_name: file_name(&headers, &final_url),
        final_url: final_url.to_string(),
        size,
        ranges,
        validator: validator(&headers),
        content_type: media_type(&headers),
    })
}

fn media_type(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(CONTENT_TYPE)?.to_str().ok()?;
    let t = value.split(';').next()?.trim().to_ascii_lowercase();
    (!t.is_empty()).then_some(t)
}

/// Fetches a small text resource (at most `max` bytes), for the few non-download requests
/// the user starts, such as checking for a newer video component.
pub async fn fetch_text(url: &str, max: usize) -> Result<String, EngineError> {
    let body = fetch_bytes(url, max).await?;
    String::from_utf8(body).map_err(|_| EngineError::Network("the reply is not text".into()))
}

/// Fetches a small resource (at most `max` bytes) into memory, for the few non-download
/// requests the user starts, such as a playlist's thumbnails. Fails with `TooMuchData` as
/// soon as the reply passes `max`, so a large reply is never read whole.
pub async fn fetch_bytes(url: &str, max: usize) -> Result<Vec<u8>, EngineError> {
    let url = parse_url(url)?;
    let client = client()?;
    let mut resp = get(&client, &url).send().await?;
    if !resp.status().is_success() {
        return Err(EngineError::HttpStatus(resp.status().as_u16()));
    }
    let mut body = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        if body.len() + chunk.len() > max {
            return Err(EngineError::TooMuchData);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

pub(crate) fn validator(headers: &HeaderMap) -> Option<Validator> {
    let text = |name| headers.get(name).and_then(|v| v.to_str().ok()).map(str::trim).filter(|s| !s.is_empty());
    text(ETAG)
        .map(|e| Validator::ETag(e.to_owned()))
        .or_else(|| text(LAST_MODIFIED).map(|m| Validator::LastModified(m.to_owned())))
}

fn file_name(headers: &HeaderMap, url: &reqwest::Url) -> String {
    let from_header =
        headers.get(CONTENT_DISPOSITION).and_then(|v| v.to_str().ok()).and_then(content_disposition_filename);
    let from_url = || {
        url.path_segments()
            .and_then(|mut s| s.next_back().map(str::to_owned))
            .filter(|s| !s.is_empty())
            .map(|s| percent_decode(&s))
    };
    from_header.or_else(from_url).map(|n| sanitize(&n)).filter(|n| !n.is_empty()).unwrap_or_else(|| "download".into())
}

/// Keeps only the last path component, then makes it a safe file name.
fn sanitize(name: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or(name);
    sanitize_file_name(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_made_safe() {
        assert_eq!(sanitize("../../etc/passwd"), "passwd");
        assert_eq!(sanitize("a:b?.txt"), "a_b_.txt");
        assert_eq!(sanitize(".."), "download");
    }

    #[test]
    fn media_type_drops_parameters() {
        let mut h = HeaderMap::new();
        h.insert(CONTENT_TYPE, "Text/HTML; charset=utf-8".parse().unwrap());
        assert_eq!(media_type(&h).as_deref(), Some("text/html"));
        assert_eq!(media_type(&HeaderMap::new()), None);
    }

    #[test]
    fn weak_etag_is_not_used_for_if_range() {
        assert_eq!(Validator::ETag("W/\"x\"".into()).if_range(), None);
        assert_eq!(Validator::ETag("\"x\"".into()).if_range(), Some("\"x\""));
    }
}
