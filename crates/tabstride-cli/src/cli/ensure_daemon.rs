//! Discovery for the explicitly started TabStride service.
//!
//! Business and diagnostic commands never start a background process. They
//! read the endpoint published by `tabstride serve` and fail with a stable,
//! actionable error when no live service is present.

use crate::daemon::info::{self, DaemonInfo};
use anyhow::Result;
use thiserror::Error;

/// Stable marker used by the central CLI renderer for the explicit-start hint.
#[derive(Debug, Error)]
#[error("TabStride service is not running")]
pub struct ServiceNotRunning;

/// Return the endpoint published by a live `tabstride serve` process.
pub fn ensure_daemon() -> Result<DaemonInfo> {
    info::read_valid()?.ok_or_else(|| ServiceNotRunning.into())
}
