use std::fmt;

/// Everything that can stop a probe or a download. Network and server input never panics;
/// it ends up here.
#[derive(Debug)]
pub enum EngineError {
    /// The URL could not be parsed or is not http(s).
    InvalidUrl(String),
    /// Options out of range, e.g. connections outside 1..=32.
    InvalidOptions(String),
    /// The server answered with an unexpected HTTP status.
    HttpStatus(u16),
    /// Connecting, TLS, timeouts or a connection that broke while reading.
    Network(String),
    /// A 206 reply whose Content-Range does not match the range asked for.
    BadContentRange { expected: String, got: String },
    /// The server answered a ranged request with the whole file (200).
    RangeIgnored,
    /// The connection ended before its range (or the whole file) was complete.
    Incomplete { expected: u64, received: u64 },
    /// The server sent more bytes than the range or file can hold.
    TooMuchData,
    /// The bytes written at the end do not equal the expected size.
    SizeMismatch { expected: u64, written: u64 },
    /// Reading or writing the destination file failed.
    Io(std::io::Error),
    /// The file on the server changed (its ETag / Last-Modified no longer matches) while
    /// downloading, twice in a row, so no consistent copy could be made.
    FileChanged,
    /// The download was cancelled (paused) by the caller. For a resumable download the
    /// partial file and its range map stay on disk; see [`crate::discard`].
    Cancelled,
    /// No byte arrived for [`crate::DownloadOptions::stall_timeout`] (the server stopped
    /// answering). Like a pause, the partial file and its range map stay for a resume.
    Stalled,
    /// A download task stopped unexpectedly (a bug, not a server problem).
    Internal(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUrl(u) => write!(f, "invalid URL: {u}"),
            Self::InvalidOptions(m) => write!(f, "invalid options: {m}"),
            Self::HttpStatus(s) => write!(f, "server answered HTTP {s}"),
            Self::Network(m) => write!(f, "network error: {m}"),
            Self::BadContentRange { expected, got } => {
                write!(f, "server sent the wrong part of the file (asked {expected}, got {got})")
            }
            Self::RangeIgnored => write!(f, "server ignored the byte range request"),
            Self::Incomplete { expected, received } => {
                write!(f, "connection ended early: got {received} of {expected} bytes")
            }
            Self::TooMuchData => write!(f, "server sent more data than expected"),
            Self::SizeMismatch { expected, written } => {
                write!(f, "file size check failed: expected {expected} bytes, wrote {written}")
            }
            Self::Io(e) => write!(f, "file error: {e}"),
            Self::FileChanged => write!(f, "the file changed on the server during the download"),
            Self::Cancelled => write!(f, "download cancelled"),
            Self::Stalled => write!(f, "the server stopped sending data"),
            Self::Internal(m) => write!(f, "internal error: {m}"),
        }
    }
}

impl std::error::Error for EngineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for EngineError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<reqwest::Error> for EngineError {
    fn from(e: reqwest::Error) -> Self {
        // Include the source chain: reqwest's top-level message alone is vague.
        let mut msg = e.to_string();
        let mut src = std::error::Error::source(&e);
        while let Some(s) = src {
            msg.push_str(": ");
            msg.push_str(&s.to_string());
            src = s.source();
        }
        Self::Network(msg)
    }
}
