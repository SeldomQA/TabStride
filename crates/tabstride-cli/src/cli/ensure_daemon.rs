//! Discovery for the explicitly started TabStride service.
//!
//! Business and diagnostic commands never start a background process. They
//! read the endpoint published by `tabstride serve` and fail with a stable,
//! actionable error when no live service is present.

use crate::daemon::info::{self, DaemonInfo};
use anyhow::Result;
use std::time::Instant;
use thiserror::Error;

/// Stable marker used by the central CLI renderer for the explicit-start hint.
#[derive(Debug, Error)]
#[error("TabStride service is not running")]
pub struct ServiceNotRunning;

/// Return the endpoint published by a live `tabstride serve` process.
pub fn ensure_daemon() -> Result<DaemonInfo> {
    let started = Instant::now();
    let result = info::read_valid()?.ok_or_else(|| ServiceNotRunning.into());
    crate::cli::business_rpc::record_daemon_check(started.elapsed());
    result
}
