//! Verify the daemon emits a daily-rotated log file and `tabstride logs`
//! surfaces its contents.

#![cfg(unix)]

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use tempfile::TempDir;

fn tabstride_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_tabstride"))
}

fn wait_for_pid_exit(pid: i32, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let alive = unsafe { libc::kill(pid, 0) } == 0;
        if !alive {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn daemon_writes_log_and_tabstride_logs_prints_them() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("tabstride");
    std::fs::create_dir_all(&home).unwrap();

    let out = Command::new(tabstride_bin())
        .args(["daemon", "start", "--port", "0", "--daemon-idle", "60s"])
        .env("TABSTRIDE_HOME", &home)
        .env("RUST_LOG", "info")
        .output()
        .unwrap();
    assert!(out.status.success());

    // Give the daemon a moment to flush at least one log line.
    std::thread::sleep(Duration::from_millis(400));

    let logs = Command::new(tabstride_bin())
        .args(["logs", "-n", "50"])
        .env("TABSTRIDE_HOME", &home)
        .output()
        .expect("tabstride logs");
    assert!(logs.status.success());
    let body = String::from_utf8(logs.stdout).unwrap();
    assert!(
        body.contains("daemon ready") || body.contains("ipc server listening"),
        "logs should include at least one daemon line: {body}"
    );
    let first_json_line = body
        .lines()
        .find(|line| !line.trim().is_empty())
        .expect("at least one log line");
    serde_json::from_str::<serde_json::Value>(first_json_line)
        .expect("daemon logs should be JSON Lines");

    // Cleanup.
    let info: serde_json::Value =
        serde_json::from_slice(&std::fs::read(home.join("daemon.json")).unwrap()).unwrap();
    let pid = info["pid"].as_u64().unwrap() as i32;
    let _ = Command::new(tabstride_bin())
        .args(["daemon", "stop"])
        .env("TABSTRIDE_HOME", &home)
        .output();
    assert!(wait_for_pid_exit(pid, Duration::from_secs(5)));
}

#[test]
fn tabstride_logs_handles_empty_home_gracefully() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("tabstride");
    std::fs::create_dir_all(&home).unwrap();
    let logs = Command::new(tabstride_bin())
        .args(["logs"])
        .env("TABSTRIDE_HOME", &home)
        .output()
        .expect("tabstride logs");
    assert!(logs.status.success());
}
