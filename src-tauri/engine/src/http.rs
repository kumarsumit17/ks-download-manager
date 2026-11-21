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

// `Content-Range` parsing is generic: it lives in ks-utility.
pub(crate) use ks_utility::content_range::{ContentRange, parse_content_range};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_schemes() {
        assert!(parse_url("https://example.com/a").is_ok());
        assert!(matches!(parse_url("ftp://example.com/a"), Err(EngineError::InvalidUrl(_))));
        assert!(matches!(parse_url("not a url"), Err(EngineError::InvalidUrl(_))));
    }
}
