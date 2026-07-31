//! Cancellable business-RPC helper used by every long-running CLI
//! subcommand.
//!
//! M10.2 wires `SIGINT → cancel { rpc_id }` end-to-end. The CLI arms
//! a Ctrl-C handler around the IPC call, so when the user hits Ctrl-C
//! while a `tabstride tool.*` / `tabstride session.*` call is in flight:
//!
//! 1. We send a fresh `cancel { rpc_id }` IPC frame on a new
//!    connection (the original socket is still parked on `read_line`).
//! 2. Wait up to [`CANCEL_RESPONSE_GRACE`] for the original call to
//!    return with a structured `cancelled` error.
//! 3. If the daemon never replies within the grace window, synthesise
//!    a `cancelled` [`RpcError`] locally and return — CLI exits with
//!    the matching exit code rather than hanging on a wedged daemon.
//!
//! Admin commands (`tabstride status`, `tabstride doctor`, `tabstride daemon …`, `tabstride logs`)
//! intentionally bypass this helper and use `Client::call` /
//! `IpcClient::call` directly so SIGINT keeps its default
//! "kill the CLI process" behaviour for short status reads.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::Context;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tabstride_protocol::{CancelParams, CancelResult, ErrorCode, Method, RpcError, RpcId};
use tracing::debug;

use crate::cli::error::CliError;
use crate::ipc_client::IpcClient;
use crate::timing::{TIMING_FIELD, TimingTrace, epoch_us, take_trace};

static CLI_STARTED: OnceLock<Instant> = OnceLock::new();
static TIMING_ENABLED: AtomicBool = AtomicBool::new(false);
static DAEMON_CHECK_US: AtomicU64 = AtomicU64::new(0);
static RUN_ID: OnceLock<Mutex<Option<String>>> = OnceLock::new();

pub fn mark_cli_started() {
    let _ = CLI_STARTED.set(Instant::now());
}

pub fn set_timing_enabled(enabled: bool) {
    TIMING_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn set_run_id(run_id: Option<String>) {
    *RUN_ID
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = run_id;
}

pub fn record_daemon_check(duration: Duration) {
    DAEMON_CHECK_US.store(duration.as_micros() as u64, Ordering::Relaxed);
}

/// Hard cap on how long we wait for the cancelled RPC to settle after
/// SIGINT triggers. Picked per design §4.6 ("≤ 2s, then force exit").
pub const CANCEL_RESPONSE_GRACE: Duration = Duration::from_secs(2);

/// Hard cap on the cancel-frame's own IPC round-trip. Independent of
/// the original call's timeout because cancel must answer promptly.
const CANCEL_FRAME_TIMEOUT: Duration = Duration::from_secs(2);

/// Issue a business RPC against the daemon with SIGINT-driven
/// cancellation.
pub fn call<P, R>(
    sock: PathBuf,
    rpc_id_prefix: &str,
    method: Method,
    params: Option<P>,
    call_timeout: Duration,
) -> Result<R, CliError>
where
    P: Serialize + Send + 'static,
    R: DeserializeOwned + Send + 'static,
{
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build tokio runtime for business RPC")
        .map_err(CliError::Local)?;
    rt.block_on(async move {
        call_async::<P, R>(sock, rpc_id_prefix, method, params, call_timeout).await
    })
}

/// Same as [`call`] but assumes a pre-existing tokio runtime context.
pub async fn call_async<P, R>(
    sock: PathBuf,
    rpc_id_prefix: &str,
    method: Method,
    params: Option<P>,
    call_timeout: Duration,
) -> Result<R, CliError>
where
    P: Serialize + Send + 'static,
    R: DeserializeOwned + Send + 'static,
{
    let call_started = Instant::now();
    let cli_startup_us = CLI_STARTED
        .get()
        .map(|started| started.elapsed().as_micros() as u64)
        .unwrap_or_default();
    let daemon_check_us = DAEMON_CHECK_US.swap(0, Ordering::Relaxed);
    let rpc_id: RpcId = format!("{}-{}", rpc_id_prefix, random_short_id());
    let method_name = method.as_str();
    let mut params_value = match params {
        Some(params) => serde_json::to_value(params)
            .context("serialise business RPC params")
            .map_err(CliError::Local)?,
        None => serde_json::json!({}),
    };
    if let Some(map) = params_value.as_object_mut() {
        map.insert(
            TIMING_FIELD.to_string(),
            serde_json::to_value(TimingTrace {
                run_id: RUN_ID
                    .get_or_init(|| Mutex::new(None))
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .clone(),
                agent_received_at: Some(epoch_us()),
                ..TimingTrace::default()
            })
            .expect("TimingTrace is serialisable"),
        );
    }
    let connect_started = Instant::now();
    let mut client = IpcClient::connect(&sock)
        .await
        .map_err(|error| CliError::Local(error.context("connect to TabStride service")))?;
    let ipc_connect_us = connect_started.elapsed().as_micros() as u64;
    let rpc_id_for_call = rpc_id.clone();
    let rpc_id_for_cancel = rpc_id.clone();

    let main_fut = async move {
        client
            .call_with_id::<serde_json::Value, serde_json::Value>(
                rpc_id_for_call,
                method,
                Some(params_value),
                call_timeout,
            )
            .await
    };
    tokio::pin!(main_fut);

    let outcome = tokio::select! {
        biased;
        res = &mut main_fut => res?,
        sig = wait_for_sigint() => {
            sig.context("install SIGINT handler").map_err(CliError::Local)?;
            debug!(rpc_id = %rpc_id_for_cancel, "SIGINT: forwarding cancel to daemon");
            let _ = send_cancel(&sock, &rpc_id_for_cancel).await;
            match tokio::time::timeout(CANCEL_RESPONSE_GRACE, &mut main_fut).await {
                Ok(res) => res?,
                Err(_) => {
                    debug!(
                        rpc_id = %rpc_id_for_cancel,
                        "SIGINT: cancel grace elapsed; synthesising cancelled error"
                    );
                    return Err(CliError::from_rpc(RpcError {
                        code: ErrorCode::Cancelled,
                        message: "rpc did not respond within cancel grace window".into(),
                        data: None,
                    }));
                }
            }
        }
    };

    match outcome {
        Ok(mut raw) => {
            let trace = take_trace(&mut raw);
            if TIMING_ENABLED.load(Ordering::Relaxed) {
                print_timing(
                    method_name,
                    trace.as_ref(),
                    cli_startup_us,
                    daemon_check_us,
                    ipc_connect_us,
                    call_started.elapsed().as_micros() as u64,
                );
            }
            serde_json::from_value(raw)
                .context("decode business RPC result")
                .map_err(CliError::Local)
        }
        Err(mut error) => {
            let trace = error.data.as_mut().and_then(take_trace);
            if TIMING_ENABLED.load(Ordering::Relaxed) {
                print_timing(
                    method_name,
                    trace.as_ref(),
                    cli_startup_us,
                    daemon_check_us,
                    ipc_connect_us,
                    call_started.elapsed().as_micros() as u64,
                );
            }
            Err(CliError::from_rpc(error))
        }
    }
}

fn print_timing(
    method: &str,
    trace: Option<&TimingTrace>,
    startup_us: u64,
    daemon_check_us: u64,
    ipc_connect_us: u64,
    local_runtime_us: u64,
) {
    let value = |value: Option<u64>| {
        value
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".into())
    };
    eprintln!("Timing {method}");
    eprintln!("  cli_startup_us          {startup_us}");
    eprintln!("  daemon_check_us         {daemon_check_us}");
    eprintln!("  ipc_connect_us          {ipc_connect_us}");
    if let Some(trace) = trace {
        eprintln!("  queue_wait_us           {}", value(trace.queue_wait_us()));
        eprintln!("  websocket_us            {}", value(trace.websocket_us()));
        eprintln!(
            "  extension_dispatch_us   {}",
            value(trace.extension_dispatch_us())
        );
        eprintln!("  cdp_us                  {}", value(trace.cdp_us()));
        eprintln!(
            "  total_runtime_us        {}",
            value(trace.total_runtime_us())
        );
    } else {
        eprintln!("  full_chain              unavailable");
    }
    eprintln!("  cli_runtime_us          {local_runtime_us}");
}

/// Send a `cancel { rpc_id }` frame over a fresh connection so it
/// lands on the daemon while the original call is still parked.
///
/// The on-the-wire method name is `cancel` (not `system.cancel`)
/// because that is the identifier registered in
/// [`tabstride_protocol::Method::Cancel`] today — design §4.3 lists the
/// bare `cancel` namespace, and the implementation has used that
/// name since M9. The CLI sticks to whatever the protocol crate
/// exports as `Method::Cancel` so drift between docs and code
/// stays loud.
pub async fn send_cancel(sock: &Path, rpc_id: &str) -> anyhow::Result<()> {
    let mut client = IpcClient::connect(sock).await?;
    let cancel_id = format!("cancel-{}", random_short_id());
    let _ignored: anyhow::Result<std::result::Result<CancelResult, RpcError>> = client
        .call_with_id::<_, CancelResult>(
            cancel_id,
            Method::Cancel,
            Some(CancelParams {
                rpc_id: rpc_id.to_string(),
            }),
            CANCEL_FRAME_TIMEOUT,
        )
        .await;
    Ok(())
}

fn random_short_id() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; 4];
    rng.fill(&mut bytes[..]);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Wait for either Ctrl-C or process SIGTERM. Returning here means
/// the user has asked us to stop.
async fn wait_for_sigint() -> anyhow::Result<()> {
    tokio::signal::ctrl_c().await.context("listen for SIGINT")?;
    Ok(())
}
