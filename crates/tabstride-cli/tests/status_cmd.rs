//! End-to-end `tabstride status --json` and `tabstride doctor` tests.

#![cfg(unix)]

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use tempfile::TempDir;

static STATUS_CMD_TEST_LOCK: Mutex<()> = Mutex::new(());

fn status_cmd_test_guard() -> MutexGuard<'static, ()> {
    STATUS_CMD_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn tabstride_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_tabstride"))
}

fn wait_for_pid_exit(pid: i32, timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        let alive = unsafe { libc::kill(pid, 0) } == 0;
        if !alive {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn tabstride_status_json_returns_structured_payload() {
    let _guard = status_cmd_test_guard();
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("tabstride");
    std::fs::create_dir_all(&home).unwrap();

    // Start the service explicitly before querying it.
    let started = Command::new(tabstride_bin())
        .args(["daemon", "start", "--port", "0", "--daemon-idle", "60s"])
        .env("TABSTRIDE_HOME", &home)
        .env("RUST_LOG", "warn")
        .output()
        .expect("start test service");
    assert!(started.status.success());

    let out = Command::new(tabstride_bin())
        .args(["--json", "status"])
        .env("TABSTRIDE_HOME", &home)
        .env("TABSTRIDE_BROWSER_WAIT_MS", "0")
        .env("RUST_LOG", "warn")
        .output()
        .expect("run tabstride status");
    assert!(
        out.status.success(),
        "tabstride status failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8(out.stdout).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("status --json should be valid JSON");

    assert!(parsed["pid"].as_u64().unwrap() > 0);
    assert!(!parsed["daemon_version"].as_str().unwrap().is_empty());
    assert_eq!(parsed["protocol_version"], "1.0");
    assert!(parsed["sock_path"].as_str().is_some());
    assert_eq!(parsed["browsers"], serde_json::json!([]));
    assert_eq!(parsed["sessions"], serde_json::json!([]));

    let pid = parsed["pid"].as_u64().unwrap() as i32;

    // Cleanup.
    let _ = Command::new(tabstride_bin())
        .args(["daemon", "stop"])
        .env("TABSTRIDE_HOME", &home)
        .output();
    assert!(wait_for_pid_exit(pid, Duration::from_secs(5)));
}

#[test]
fn tabstride_doctor_runs_without_running_daemon() {
    let _guard = status_cmd_test_guard();
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("tabstride");
    std::fs::create_dir_all(&home).unwrap();

    // Doctor is diagnostic only and must not start the service.
    let out = Command::new(tabstride_bin())
        .args(["doctor"])
        .env("TABSTRIDE_HOME", &home)
        .env("RUST_LOG", "warn")
        .env("TABSTRIDE_DOCTOR_BROWSER_WAIT_MS", "200")
        .output()
        .expect("run tabstride doctor");
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains("tabstride home writable"),
        "doctor should mention tabstride home check: {stdout}"
    );
    assert!(
        stdout.contains("daemon running"),
        "doctor should mention daemon check: {stdout}"
    );
    assert!(
        stdout.contains("extension connected"),
        "doctor should mention extension connected check: {stdout}"
    );
    assert!(!home.join("daemon.json").exists());
}

#[test]
fn tabstride_doctor_json_returns_structured_checks() {
    let _guard = status_cmd_test_guard();
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("tabstride");
    std::fs::create_dir_all(&home).unwrap();

    // Doctor is diagnostic only and must not start the service.
    let out = Command::new(tabstride_bin())
        .args(["--json", "doctor"])
        .env("TABSTRIDE_HOME", &home)
        .env("RUST_LOG", "warn")
        .env("TABSTRIDE_DOCTOR_BROWSER_WAIT_MS", "200")
        .output()
        .expect("run tabstride doctor --json");
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("doctor --json should be valid JSON");
    let checks = parsed.as_array().expect("doctor output should be an array");
    assert!(
        checks
            .iter()
            .any(|check| check["name"] == "tabstride home writable"),
        "doctor JSON should include home check: {stdout}"
    );
    assert!(
        checks
            .iter()
            .any(|check| check["name"] == "extension connected"),
        "doctor JSON should include extension connected check: {stdout}"
    );

    assert!(!home.join("daemon.json").exists());
}

#[test]
fn tabstride_doctor_does_not_treat_live_non_daemon_pid_as_running() {
    let _guard = status_cmd_test_guard();
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("tabstride");
    std::fs::create_dir_all(home.join("run")).unwrap();

    let mut child = Command::new("sleep")
        .arg("30")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn controlled non-daemon child");

    let info = serde_json::json!({
        "pid": child.id(),
        "sock_path": home.join("run").join("not-a-daemon.sock"),
        "ws_port": 0,
        "version": env!("CARGO_PKG_VERSION"),
        "started_at_epoch_secs": 1
    });
    std::fs::write(
        home.join("daemon.json"),
        serde_json::to_vec_pretty(&info).unwrap(),
    )
    .unwrap();

    let out = Command::new(tabstride_bin())
        .args(["--json", "doctor"])
        .env("TABSTRIDE_HOME", &home)
        .env("RUST_LOG", "warn")
        .output()
        .expect("run tabstride doctor --json");
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    let checks: Vec<serde_json::Value> = serde_json::from_str(stdout.trim()).unwrap();
    let daemon = checks
        .iter()
        .find(|check| check["name"] == "daemon running")
        .expect("daemon running check");
    assert_eq!(daemon["ok"], false);

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn tabstride_doctor_flags_pid_mismatch_against_running_daemon() {
    let _guard = status_cmd_test_guard();
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("tabstride");
    std::fs::create_dir_all(&home).unwrap();

    // Bring up a real daemon so the socket is live and IPC works.
    let out = Command::new(tabstride_bin())
        .args(["daemon", "start", "--port", "0", "--daemon-idle", "60s"])
        .env("TABSTRIDE_HOME", &home)
        .env("RUST_LOG", "warn")
        .output()
        .expect("daemon start");
    assert!(
        out.status.success(),
        "daemon start failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let info_path = home.join("daemon.json");
    let original: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&info_path).unwrap()).unwrap();
    let real_pid = original["pid"].as_u64().expect("pid number") as i32;

    // Spawn a controlled "other" process so the rewritten pid is alive
    // but is definitely not our daemon. Without an alive pid the check
    // would fail for a different reason (stale-dead) than what we want.
    let mut decoy = Command::new("sleep")
        .arg("30")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn decoy pid");

    let mut tampered = original.clone();
    tampered["pid"] = serde_json::Value::from(decoy.id());
    std::fs::write(&info_path, serde_json::to_vec_pretty(&tampered).unwrap()).unwrap();

    let out = Command::new(tabstride_bin())
        .args(["--json", "doctor"])
        .env("TABSTRIDE_HOME", &home)
        .env("RUST_LOG", "warn")
        .output()
        .expect("tabstride doctor --json");
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    let checks: Vec<serde_json::Value> = serde_json::from_str(stdout.trim()).unwrap();

    let daemon = checks
        .iter()
        .find(|check| check["name"] == "daemon running")
        .expect("daemon running check");
    assert_eq!(
        daemon["ok"], false,
        "daemon running check should fail when daemon.json pid was tampered: {stdout}"
    );
    let detail = daemon["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains(&decoy.id().to_string()) && detail.contains(&real_pid.to_string()),
        "daemon running detail should mention both pids; got {detail}"
    );

    // Restore the real pid so cleanup `tabstride daemon stop` can find the daemon.
    std::fs::write(&info_path, serde_json::to_vec_pretty(&original).unwrap()).unwrap();
    let _ = decoy.kill();
    let _ = decoy.wait();

    let _ = Command::new(tabstride_bin())
        .args(["daemon", "stop"])
        .env("TABSTRIDE_HOME", &home)
        .output();
    assert!(wait_for_pid_exit(real_pid, Duration::from_secs(5)));
}
