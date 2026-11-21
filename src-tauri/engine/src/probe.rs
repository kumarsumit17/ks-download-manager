use reqwest::StatusCode;
use reqwest::header::{CONTENT_DISPOSITION, CONTENT_RANGE, ETAG, HeaderMap, LAST_MODIFIED, RANGE};

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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Validator {
    ETag(String),
    LastModified(String),
}

impl Validator {
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
    })
}

fn validator(headers: &HeaderMap) -> Option<Validator> {
    let text = |name| headers.get(name).and_then(|v| v.to_str().ok()).map(str::trim).filter(|s| !s.is_empty());
    text(ETAG)
        .map(|e| Validator::ETag(e.to_owned()))
        .or_else(|| text(LAST_MODIFIED).map(|m| Validator::LastModified(m.to_owned())))
}

fn file_name(headers: &HeaderMap, url: &reqwest::Url) -> String {
    let from_header = headers.get(CONTENT_DISPOSITION).and_then(|v| v.to_str().ok()).and_then(disposition_name);
    let from_url = || {
        url.path_segments()
            .and_then(|mut s| s.next_back().map(str::to_owned))
            .filter(|s| !s.is_empty())
            .map(|s| percent_decode(&s))
    };
    from_header.or_else(from_url).map(|n| sanitize(&n)).filter(|n| !n.is_empty()).unwrap_or_else(|| "download".into())
}

/// `filename*=UTF-8''name` (RFC 6266/8187) wins over `filename="name"`.
fn disposition_name(value: &str) -> Option<String> {
    let params: Vec<(String, String)> = value
        .split(';')
        .skip(1)
        .filter_map(|p| p.split_once('='))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    let extended = params.iter().find(|(k, _)| k == "filename*").and_then(|(_, v)| {
        let (charset, rest) = v.split_once('\'')?;
        let (_lang, encoded) = rest.split_once('\'')?;
        charset.eq_ignore_ascii_case("utf-8").then(|| percent_decode(encoded))
    });
    extended.or_else(|| {
        params.iter().find(|(k, _)| k == "filename").map(|(_, v)| v.trim_matches('"').to_owned())
    })
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
    while i < bytes.len() {
        // Work on bytes, not str slices: a multi-byte character after '%' must not panic.
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push(h << 4 | l);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Keeps only the last path component and replaces characters no file system accepts.
fn sanitize(name: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = last
        .chars()
        .map(|c| if c.is_control() || matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c })
        .collect();
    cleaned.trim().trim_matches('.').trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disposition_forms() {
        assert_eq!(disposition_name("attachment; filename=\"a b.zip\"").as_deref(), Some("a b.zip"));
        assert_eq!(disposition_name("attachment; filename=plain.txt").as_deref(), Some("plain.txt"));
        assert_eq!(
            disposition_name("attachment; filename=\"x.bin\"; filename*=UTF-8''caf%C3%A9.bin").as_deref(),
            Some("café.bin")
        );
        assert_eq!(disposition_name("inline"), None);
    }

    #[test]
    fn names_are_made_safe() {
        assert_eq!(sanitize("../../etc/passwd"), "passwd");
        assert_eq!(sanitize("a:b?.txt"), "a_b_.txt");
        assert_eq!(sanitize(".."), "");
        assert_eq!(percent_decode("a%20b%zz%4"), "a b%zz%4");
        assert_eq!(percent_decode("%é%"), "%é%");
    }

    #[test]
    fn weak_etag_is_not_used_for_if_range() {
        assert_eq!(Validator::ETag("W/\"x\"".into()).if_range(), None);
        assert_eq!(Validator::ETag("\"x\"".into()).if_range(), Some("\"x\""));
    }
}
