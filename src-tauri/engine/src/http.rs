use std::time::Duration;

use reqwest::header::{ACCEPT_ENCODING, COOKIE, HeaderValue, REFERER, USER_AGENT};
use reqwest::{Client, RequestBuilder, Url};

use crate::error::EngineError;

/// A read that brings nothing for this long fails and is retried (Final review: 60 s made
/// a dead server keep a row "Downloading" for 6 to 7 minutes).
pub(crate) const READ_TIMEOUT: Duration = Duration::from_secs(25);

/// One client per connection. Each `Client` has its own pool, so every range really
/// gets its own TCP connection (specs/engine.md: per-connection limits, no HTTP/2 multiplexing).
/// The crate is built without reqwest's `http2` feature, so this is always HTTP/1.1.
pub(crate) fn client() -> Result<Client, EngineError> {
    Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .read_timeout(READ_TIMEOUT)
        .user_agent(concat!("KS-Download-Manager/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(EngineError::from)
}

/// Request headers a download started in a browser carries over from it (specs/extension.md,
/// "Browser downloads"): the site's cookies, the page that linked to the file and the
/// browser's own identification, so a server that serves the file only to that browser
/// session serves it to us too. Kept in memory for that one download, never saved.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RequestHeaders {
    /// `Cookie` value, sent only to `cookie_host` (never to a host a redirect leads to).
    pub cookie: Option<String>,
    /// The host the cookies belong to (the host of the link the browser downloaded).
    pub cookie_host: Option<String>,
    pub referer: Option<String>,
    pub user_agent: Option<String>,
}

impl RequestHeaders {
    pub fn is_empty(&self) -> bool {
        self.cookie.is_none() && self.referer.is_none() && self.user_agent.is_none()
    }

    fn apply(&self, url: &Url, mut request: RequestBuilder) -> RequestBuilder {
        // A value that is not a valid header value (a line break, for one) is never sent.
        let value = |v: &Option<String>| v.as_deref().and_then(|v| HeaderValue::from_str(v).ok());
        if let Some(v) = value(&self.user_agent) {
            request = request.header(USER_AGENT, v);
        }
        if let Some(v) = value(&self.referer) {
            request = request.header(REFERER, v);
        }
        if let (Some(v), Some(host)) = (value(&self.cookie), self.cookie_host.as_deref())
            && url.host_str().is_some_and(|h| h.eq_ignore_ascii_case(host))
        {
            request = request.header(COOKIE, v);
        }
        request
    }
}

/// A GET with `Accept-Encoding: identity`, so the server sends the file's own bytes and
/// every offset matches the file on disk.
pub(crate) fn get(client: &Client, url: &Url) -> RequestBuilder {
    client.get(url.clone()).header(ACCEPT_ENCODING, HeaderValue::from_static("identity"))
}

/// [`get`] with the browser's headers for a download handed over from it.
pub(crate) fn get_with(client: &Client, url: &Url, headers: &RequestHeaders) -> RequestBuilder {
    headers.apply(url, get(client, url))
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
    fn cookies_go_only_to_their_own_host() {
        let h = RequestHeaders {
            cookie: Some("a=1".into()),
            cookie_host: Some("files.example.com".into()),
            referer: Some("https://example.com/page".into()),
            user_agent: Some("Browser/1".into()),
        };
        let client = Client::new();
        let own = get_with(&client, &Url::parse("https://files.example.com/x").unwrap(), &h).build().unwrap();
        assert_eq!(own.headers().get(COOKIE).unwrap(), "a=1");
        assert_eq!(own.headers().get(REFERER).unwrap(), "https://example.com/page");
        assert_eq!(own.headers().get(USER_AGENT).unwrap(), "Browser/1");
        let other = get_with(&client, &Url::parse("https://cdn.example.net/x").unwrap(), &h).build().unwrap();
        assert!(other.headers().get(COOKIE).is_none(), "never to another host");
        let bad = RequestHeaders { cookie: Some("a=1\r\nX: y".into()), cookie_host: Some("files.example.com".into()), ..RequestHeaders::default() };
        let r = get_with(&client, &Url::parse("https://files.example.com/x").unwrap(), &bad).build().unwrap();
        assert!(r.headers().get(COOKIE).is_none(), "a value with a line break is dropped");
    }

    #[test]
    fn url_schemes() {
        assert!(parse_url("https://example.com/a").is_ok());
        assert!(matches!(parse_url("ftp://example.com/a"), Err(EngineError::InvalidUrl(_))));
        assert!(matches!(parse_url("not a url"), Err(EngineError::InvalidUrl(_))));
    }
}
