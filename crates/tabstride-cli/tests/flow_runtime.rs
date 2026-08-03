use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use rand::Rng;
use tabstride::daemon::{self, DaemonConfig};
use tabstride::ipc_client::IpcClient;
use tabstride::timing::TIMING_FIELD;
use tabstride_protocol::{
    CancelParams, CancelResult, ErrorCode, FlowDefinition, FlowFailureData, FlowRunParams,
    FlowRunResult, FlowStep, FlowWaitMsEntry, FlowWaitMsStep, Method,
};
use tokio::net::TcpListener;

fn socket_path() -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "tabstride-flow-{}-{:08x}.sock",
        std::process::id(),
        rand::thread_rng().r#gen::<u32>()
    ));
    path
}

async fn spawn_daemon() -> (daemon::DaemonHandle, PathBuf) {
    let probe = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let socket = socket_path();
    let handle = daemon::run(DaemonConfig::new(port), Some(socket.clone()))
        .await
        .unwrap();
    (handle, socket)
}

fn wait_flow(timeout: &str, durations: &[u64]) -> FlowRunParams {
    FlowRunParams {
        session_id: "flow-test".into(),
        flow: FlowDefinition {
            name: "waits".into(),
            timeout: Some(timeout.into()),
            steps: durations
                .iter()
                .map(|duration_ms| {
                    FlowStep::WaitMs(FlowWaitMsEntry {
                        wait_ms: FlowWaitMsStep {
                            duration_ms: *duration_ms,
                        },
                    })
                })
                .collect(),
            assertions: vec![],
        },
        variables: BTreeMap::new(),
    }
}

#[tokio::test]
async fn flow_runs_all_steps_in_one_rpc() {
    let (daemon, socket) = spawn_daemon().await;
    let mut client = IpcClient::connect(&socket).await.unwrap();
    let result: FlowRunResult = client
        .call(
            "flow",
            Method::FlowRun,
            Some(wait_flow("1s", &[1, 1, 1, 1, 1])),
            Duration::from_secs(2),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.completed_steps.len(), 5);
    assert!(
        result
            .completed_steps
            .iter()
            .all(|step| step.method == "tool.wait_ms")
    );
    for step in &result.completed_steps {
        let timing = step.timing.as_ref().expect("local timing");
        assert!(timing.local_us.is_some());
        assert!(timing.websocket_us.is_none());
        assert!(timing.cdp_us.is_none());
    }
    daemon.shutdown().await;
}

#[tokio::test]
async fn flow_transport_timing_does_not_pollute_strict_params() {
    let (daemon, socket) = spawn_daemon().await;
    let mut client = IpcClient::connect(&socket).await.unwrap();
    let mut params = serde_json::to_value(wait_flow("1s", &[1])).unwrap();
    params.as_object_mut().unwrap().insert(
        TIMING_FIELD.into(),
        serde_json::json!({ "transport_probe": true }),
    );

    let result: FlowRunResult = client
        .call(
            "timed-flow",
            Method::FlowRun,
            Some(params),
            Duration::from_secs(2),
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(result.completed_steps.len(), 1);
    assert_eq!(result.completed_steps[0].method, "tool.wait_ms");
    daemon.shutdown().await;
}

#[tokio::test]
async fn total_timeout_cancels_active_child_step() {
    let (daemon, socket) = spawn_daemon().await;
    let mut client = IpcClient::connect(&socket).await.unwrap();
    let error = client
        .call::<_, FlowRunResult>(
            "flow-timeout",
            Method::FlowRun,
            Some(wait_flow("10ms", &[1_000])),
            Duration::from_secs(2),
        )
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::Timeout);
    let data: FlowFailureData = serde_json::from_value(error.data.unwrap()).unwrap();
    assert_eq!(data.failed_step, 1);
    assert_eq!(data.failed_method, "tool.wait_ms");
    let failed = data.failed_step_result.expect("started failed step");
    assert_eq!(failed.index, 1);
    assert!(failed.timing.unwrap().local_us.is_some());
    daemon.shutdown().await;
}

#[tokio::test]
async fn cancel_stops_the_whole_flow() {
    let (daemon, socket) = spawn_daemon().await;
    let socket_for_flow = socket.clone();
    let task = tokio::spawn(async move {
        let mut client = IpcClient::connect(&socket_for_flow).await.unwrap();
        client
            .call_with_id::<_, FlowRunResult>(
                "long-flow".into(),
                Method::FlowRun,
                Some(wait_flow("1m", &[30_000])),
                Duration::from_secs(5),
            )
            .await
            .unwrap()
    });
    tokio::time::sleep(Duration::from_millis(30)).await;
    let mut cancel_client = IpcClient::connect(&socket).await.unwrap();
    let cancelled: CancelResult = cancel_client
        .call(
            "cancel",
            Method::Cancel,
            Some(CancelParams {
                rpc_id: "long-flow".into(),
            }),
            Duration::from_secs(1),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(cancelled.cancelled);
    let error = task.await.unwrap().unwrap_err();
    assert_eq!(error.code, ErrorCode::Cancelled);
    let data: FlowFailureData = serde_json::from_value(error.data.unwrap()).unwrap();
    let failed = data.failed_step_result.expect("active cancelled step");
    assert_eq!(failed.method, "tool.wait_ms");
    assert!(failed.timing.unwrap().local_us.is_some());
    daemon.shutdown().await;
}
