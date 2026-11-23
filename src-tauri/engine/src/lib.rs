//! KS Download Manager download engine.
//!
//! Probe a URL, then download it over 1 to 32 separate HTTP/1.1 connections that write
//! straight into a pre-allocated file. Connections share the work (an idle one takes half
//! of the slowest range), every range is retried on its own with backoff and jitter, and a
//! crash-safe range map next to the file lets a later call resume with `If-Range`
//! (see specs/engine.md).
//!
//! This crate has no Tauri dependency and builds and tests on its own.

mod control;
mod download;
mod error;
mod http;
mod limits;
mod probe;
mod rangemap;
mod ranges;
mod retry;

pub use control::{CancelToken, ConnectionRange, Progress, ProgressSnapshot};
pub use download::{
    DEFAULT_CONNECTIONS, DownloadOptions, DownloadOutcome, MAX_CONNECTIONS, SavedState, Segment, discard, download,
    saved_state,
};
pub use error::EngineError;
pub use limits::{Adaptive, ConnectionBudget, HostLimits, MIN_PER_CONNECTION, SpeedLimit};
pub use http::RequestHeaders;
pub use probe::{ProbeInfo, SmallClient, Validator, fetch_text, probe, probe_with};
pub use retry::RetryPolicy;
