use std::time::Duration;

use reqwest::header::{ACCEPT_ENCODING, HeaderValue};
use reqwest::{Client, RequestBuilder, Url};

use crate::error::EngineError;

/// One client per connection. Each `Client` has its own pool, so every range really
/// gets its own TCP connection (specs/engine.md: per-connection limits, no HTTP/2 multiplexing).
/// The crate is built without reqwest's `http2` feature, so this is always HTTP/1.1.
pub(crate) fn client() -> Result<Client, EngineError> {
    Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .read_timeout(Duration::from_secs(60))
        .user_agent(concat!("KS-Download-Manager/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(EngineError::from)
}

/// A GET with `Accept-Encoding: identity`, so the server sends the file's own bytes and
/// every offset matches the file on disk.
pub(crate) fn get(client: &Client, url: &Url) -> RequestBuilder {
    client.get(url.clone()).header(ACCEPT_ENCODING, HeaderValue::from_static("identity"))
}

pub(crate) fn parse_url(url: &str) -> Result<Url, EngineError> {
    let parsed = Url::parse(url).map_err(|e| EngineError::InvalidUrl(format!("{url}: {e}")))?;
    match parsed.scheme() {
        "http" | "https" => Ok(parsed),
        other => Err(EngineError::InvalidUrl(format!("{url}: unsupported scheme {other}"))),
    }
}

/// A parsed `Content-Range` header (RFC 9110, section 14.4).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ContentRange {
    /// `bytes first-last/complete`; complete may be unknown (`*`).
    Bytes { first: u64, last: u64, complete: Option<u64> },
    /// `bytes */complete`, sent with 416.
    Unsatisfied { complete: u64 },
}

pub(crate) fn parse_content_range(value: &str) -> Option<ContentRange> {
    let rest = value.trim().strip_prefix("bytes")?.trim_start();
    let (range, complete) = rest.split_once('/')?;
    let complete = complete.trim();
    if range.trim() == "*" {
        return Some(ContentRange::Unsatisfied { complete: complete.parse().ok()? });
    }
    let (first, last) = range.trim().split_once('-')?;
    let first: u64 = first.trim().parse().ok()?;
    let last: u64 = last.trim().parse().ok()?;
    if last < first {
        return None;
    }
    let complete = if complete == "*" { None } else { Some(complete.parse().ok()?) };
    if complete.is_some_and(|c| last >= c) {
        return None;
    }
    Some(ContentRange::Bytes { first, last, complete })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_range_forms() {
        assert_eq!(
            parse_content_range("bytes 0-0/1234"),
            Some(ContentRange::Bytes { first: 0, last: 0, complete: Some(1234) })
        );
        assert_eq!(
            parse_content_range("bytes 10-19/*"),
            Some(ContentRange::Bytes { first: 10, last: 19, complete: None })
        );
        assert_eq!(parse_content_range("bytes */0"), Some(ContentRange::Unsatisfied { complete: 0 }));
        assert_eq!(parse_content_range("bytes 5-4/10"), None);
        assert_eq!(parse_content_range("bytes 0-10/10"), None);
        assert_eq!(parse_content_range("items 0-1/2"), None);
        assert_eq!(parse_content_range("bytes abc"), None);
    }

    #[test]
    fn url_schemes() {
        assert!(parse_url("https://example.com/a").is_ok());
        assert!(matches!(parse_url("ftp://example.com/a"), Err(EngineError::InvalidUrl(_))));
        assert!(matches!(parse_url("not a url"), Err(EngineError::InvalidUrl(_))));
    }
}
