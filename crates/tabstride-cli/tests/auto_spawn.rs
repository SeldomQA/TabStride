//! Verify `ensure_daemon` spawns the daemon when none is running.
//!
//! We can't call `ensure_daemon` directly from a test process because
//! `current_exe()` would point to the test binary, not `tabstride`. Instead we
//! drive the same effect end-to-end via `tabstride status` (which itself calls
//! into `ensure_daemon` in M3.3) — but for M3.2 we test the helper by
//! pointing `current_exe` indirection at the actual `tabstride` binary through
//! a small shim test.

#![cfg(unix)]

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use tabstride::daemon::info::{self, DaemonInfo};
use tabstride_protocol::{Frame, Method, ResponseBody, ResponseFrame};
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
fn ensure_daemon_idempotent_when_already_running() {
    // Use TABSTRIDE_HOME to isolate.
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("tabstride");
    std::fs::create_dir_all(&home).unwrap();

    // Start a daemon manually first.
    let out = Command::new(tabstride_bin())
        .args(["daemon", "start", "--port", "0", "--daemon-idle", "60s"])
        .env("TABSTRIDE_HOME", &home)
        .env("RUST_LOG", "warn")
        .output()
        .unwrap();
    assert!(out.status.success());

    let info_path = home.join("daemon.json");
    let info: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&info_path).unwrap()).unwrap();
    let pid_before = info["pid"].as_u64().unwrap() as i32;

    // Now another `tabstride daemon start` is invoked. Because the lock is
    // held, the spawned child should exit quickly and the existing
    // daemon should keep its pid.
    let _ = Command::new(tabstride_bin())
        .args(["daemon", "start", "--port", "0", "--daemon-idle", "60s"])
        .env("TABSTRIDE_HOME", &home)
        .env("RUST_LOG", "warn")
        .output()
        .unwrap();

    let info_after: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&info_path).unwrap()).unwrap();
    let pid_after = info_after["pid"].as_u64().unwrap() as i32;
    assert_eq!(pid_before, pid_after, "daemon pid should not change");

    // Clean up.
    let _ = Command::new(tabstride_bin())
        .args(["daemon", "stop"])
        .env("TABSTRIDE_HOME", &home)
        .output();
    assert!(wait_for_pid_exit(pid_before, Duration::from_secs(5)));
}

#[test]
fn business_command_sends_no_status_preflight() {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;

    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("tabstride");
    let run_dir = home.join("run");
    std::fs::create_dir_all(&run_dir).unwrap();
    let sock = run_dir.join("daemon.sock");
    let listener = UnixListener::bind(&sock).unwrap();

    let daemon_info = DaemonInfo::now(
        std::process::id(),
        sock.clone(),
        52800,
        env!("CARGO_PKG_VERSION"),
    );
    info::write_to_path(&daemon_info, &home.join("daemon.json")).unwrap();

    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        let request: Frame = serde_json::from_str(line.trim_end()).unwrap();
        let Frame::Request(request) = request else {
            panic!("expected request frame");
        };
        assert_eq!(
            request.method,
            Method::ToolWaitMs,
            "the first frame must be the business request, not system.status"
        );

        let response = Frame::Response(ResponseFrame {
            id: request.id,
            body: ResponseBody::Ok(serde_json::json!({ "waited_ms": 1 })),
        });
        serde_json::to_writer(&mut stream, &response).unwrap();
        stream.write_all(b"\n").unwrap();
        stream.flush().unwrap();
    });

    let output = Command::new(tabstride_bin())
        .args(["--json", "wait-ms", "1ms"])
        .env("TABSTRIDE_HOME", &home)
        .env("RUST_LOG", "warn")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "business command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["waited_ms"],
        1
    );
    server.join().unwrap();
}

#[test]
fn business_command_auto_spawns_when_daemon_is_missing() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("tabstride");

    let output = Command::new(tabstride_bin())
        .args(["--json", "wait-ms", "1ms"])
        .env("TABSTRIDE_HOME", &home)
        .env("RUST_LOG", "warn")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "business command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let daemon_info: DaemonInfo =
        serde_json::from_slice(&std::fs::read(home.join("daemon.json")).unwrap()).unwrap();
    let pid = daemon_info.pid as i32;
    let _ = Command::new(tabstride_bin())
        .args(["daemon", "stop"])
        .env("TABSTRIDE_HOME", &home)
        .output();
    assert!(wait_for_pid_exit(pid, Duration::from_secs(5)));
}
