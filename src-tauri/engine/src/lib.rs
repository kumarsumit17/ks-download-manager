//! KS Download Manager download engine.
//!
//! Stage 2: probe a URL, then download it over 1 to 32 separate HTTP/1.1
//! connections, each writing its own byte range straight into a pre-allocated
//! file. Retry, work stealing and resume come in Stage 3 (see specs/engine.md).
//!
//! This crate has no Tauri dependency and builds and tests on its own.

mod control;
mod download;
mod error;
mod http;
mod probe;

pub use control::{CancelToken, Progress, ProgressSnapshot};
pub use download::{DEFAULT_CONNECTIONS, DownloadOptions, DownloadOutcome, MAX_CONNECTIONS, Segment, download};
pub use error::EngineError;
pub use probe::{ProbeInfo, Validator, probe};
