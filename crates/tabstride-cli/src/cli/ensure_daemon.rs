//! Daemon discovery and auto-spawn helpers.
//!
//! Normal business commands trust a live `daemon.json` entry and connect
//! directly; they do not issue a `system.status` preflight before every real
//! request. If that direct connection fails, [`recover_daemon`] starts the
//! daemon and lets the caller retry once.
//!
//! Auto-spawn flow (per design §3.1):
//! 1. Read `daemon.json`. If the recorded pid is alive, return its info.
//! 2. Otherwise spawn `tabstride daemon start` (the same binary), inheriting
//!    `TABSTRIDE_HOME` if set, and poll `daemon.json` for a live pid until
//!    [`SPAWN_DEADLINE`] elapses.
//! 3. If polling times out, return an error with hints.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::daemon::info::{self, DaemonInfo};
use anyhow::{Context, Result};

/// Maximum time to wait for an auto-spawned daemon to become ready.
pub const SPAWN_DEADLINE: Duration = Duration::from_millis(3_000);

/// Read `daemon.json` if it's valid; spawn the daemon otherwise. Returns
/// the connection handle the caller should use.
pub fn ensure_daemon() -> Result<DaemonInfo> {
    if let Some(running) = info::read_valid()? {
        return Ok(running);
    }
    recover_daemon()
}

/// Start the daemon after a direct IPC connection failed, then return the
/// endpoint published by the ready daemon. `tabstride daemon start` is
/// idempotent, so a concurrent CLI racing us to start the service is safe.
pub fn recover_daemon() -> Result<DaemonInfo> {
    spawn_daemon()?;
    wait_for_ready(SPAWN_DEADLINE)
        .with_context(|| "auto-spawned daemon failed to become ready in time")
}

fn spawn_daemon() -> Result<()> {
    let exe = tabstride_executable()?;
    let mut cmd = Command::new(exe);
    cmd.arg("daemon")
        .arg("start")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // The child re-uses inherited env (TABSTRIDE_HOME etc), so tests that set
    // a temp home work transparently.
    let status = cmd
        .status()
        .context("spawn `tabstride daemon start` for auto-spawn")?;
    if !status.success() {
        return Err(anyhow::anyhow!(
            "`tabstride daemon start` exited with status {status:?}"
        ));
    }
    Ok(())
}

fn wait_for_ready(timeout: Duration) -> Result<DaemonInfo> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(info) = info::read_valid()? {
            return Ok(info);
        }
        if Instant::now() >= deadline {
            return Err(anyhow::anyhow!(
                "no valid daemon.json after {timeout:?}; check `tabstride logs`"
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn tabstride_executable() -> Result<PathBuf> {
    std::env::current_exe().context("locate current executable for auto-spawn")
}
