//! M7 integration: drive the 7 new tool RPCs (navigate /
//! navigate_back / navigate_forward / reload / click / fill / press)
//! through the IPC + per-session queue + fake extension and assert
//! the wire shapes line up. The extension stub mirrors each method's
//! `*Result` so we exercise the daemon's serialise → forward →
//! deserialise → return path end-to-end.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use serde_json::{Value, json};
use tabstride::daemon::{self, DaemonConfig};
use tabstride::ipc_client::IpcClient;
use tabstride_protocol::system::{HandshakeParams, HandshakeResult};
use tabstride_protocol::tools::{
    ClickParams, ClickResult, FillParams, FillResult, KeyModifier, Locator, MouseButton,
    NavigateBackParams, NavigateBackResult, NavigateForwardParams, NavigateForwardResult,
    NavigateParams, NavigateResult, PressParams, PressResult, ReloadParams, ReloadResult,
    SelectParams, SelectResult, SessionStartParams, SessionStartResult, WaitUntil,
};
use tabstride_protocol::{
    BrowserPeerInfo, ErrorCode, FlowDefinition, FlowRunParams, FlowRunResult, Frame, Method,
    RequestFrame, ResponseBody, ResponseFrame, RpcError,
};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::handshake::client::generate_key;
use tokio_tungstenite::tungstenite::http::Request;
use tokio_tungstenite::tungstenite::protocol::Message;

const TEST_EXT_ID: &str = "abcdefghijklmnopabcdefghijklmnop";

fn ref_locator(value: &str) -> Locator {
    Locator {
        ref_: Some(value.into()),
        ..Locator::default()
    }
}

fn css_locator(value: &str) -> Locator {
    Locator {
        css: Some(value.into()),
        ..Locator::default()
    }
}

fn tempfile_path(prefix: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let mut rng = rand::thread_rng();
    let suffix: String = (0..8)
        .map(|_| char::from_digit(rng.gen_range(0..16), 16).unwrap())
        .collect();
    p.push(format!("{prefix}-{}-{suffix}.sock", std::process::id()));
    p
}

async fn spawn_daemon() -> (daemon::DaemonHandle, PathBuf) {
    let probe = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let sock = tempfile_path("tabstride-test-tools-m7");
    let handle = daemon::run(DaemonConfig::new(port), Some(sock.clone()))
        .await
        .unwrap();
    (handle, sock)
}

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn connect_ext(addr: std::net::SocketAddr) -> Ws {
    let origin = format!("chrome-extension://{TEST_EXT_ID}");
    let url = format!("ws://{addr}/");
    let req = Request::builder()
        .method("GET")
        .uri(&url)
        .header("Host", addr.to_string())
        .header("Upgrade", "websocket")
        .header("Connection", "Upgrade")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", generate_key())
        .header("Origin", origin)
        .body(())
        .unwrap();
    let (ws, _) = tokio_tungstenite::connect_async(req).await.unwrap();
    ws
}

async fn do_handshake(ws: &mut Ws) -> HandshakeResult {
    let params = HandshakeParams {
        client: "tabstride-extension".into(),
        version: "0.1.0-dev.0".parse().unwrap(),
        protocol_version: "1.0".into(),
        instance_id: TEST_EXT_ID.into(),
        browser: BrowserPeerInfo {
            name: "chrome".into(),
            version: "131.0".into(),
        },
        min_compatible_peer: Some("0.1.0-dev.0".parse().unwrap()),
        min_compatible_protocol: Some("1.0".into()),
        label: "Test".into(),
    };
    let req = RequestFrame {
        id: "hs".into(),
        method: Method::SystemHandshake,
        params: Some(serde_json::to_value(params).unwrap()),
    };
    ws.send(Message::Text(serde_json::to_string(&req).unwrap()))
        .await
        .unwrap();
    let resp = ws.next().await.unwrap().unwrap();
    let text = match resp {
        Message::Text(t) => t,
        _ => panic!("expected text"),
    };
    let frame: ResponseFrame = serde_json::from_str(&text).unwrap();
    match frame.body {
        ResponseBody::Ok(v) => serde_json::from_value(v).unwrap(),
        ResponseBody::Err(e) => panic!("handshake rejected: {e:?}"),
    }
}

fn run_extension<F>(ws: Ws, reply: F)
where
    F: Fn(&RequestFrame) -> ResponseBody + Send + Sync + 'static,
{
    let (writer, reader) = ws.split();
    let writer: Arc<Mutex<SplitSink<Ws, Message>>> = Arc::new(Mutex::new(writer));
    let mut reader: SplitStream<Ws> = reader;
    let reply = Arc::new(reply);
    let mut window_id: i64 = 100;
    tokio::spawn(async move {
        while let Some(Ok(msg)) = reader.next().await {
            let text = match msg {
                Message::Text(t) => t,
                Message::Close(_) => break,
                _ => continue,
            };
            let frame: Frame = match serde_json::from_str(&text) {
                Ok(f) => f,
                Err(_) => continue,
            };
            let Frame::Request(req) = frame else { continue };
            let body = match req.method {
                Method::ToolSessionStart => {
                    let _: SessionStartParams =
                        serde_json::from_value(req.params.clone().unwrap()).unwrap();
                    let id = window_id;
                    window_id += 1;
                    ResponseBody::Ok(
                        serde_json::to_value(SessionStartResult {
                            agent_window_id: Some(id),
                            attached_tab_id: None,
                        })
                        .unwrap(),
                    )
                }
                Method::ToolSessionStop => ResponseBody::Ok(json!({})),
                _ => reply(&req),
            };
            let resp = ResponseFrame {
                id: req.id.clone(),
                body,
            };
            let mut w = writer.lock().await;
            w.send(Message::Text(serde_json::to_string(&resp).unwrap()))
                .await
                .unwrap();
        }
    });
}

async fn ipc_session_start(sock: &PathBuf) -> String {
    let mut ipc = IpcClient::connect(sock).await.unwrap();
    #[derive(serde::Deserialize)]
    struct R {
        session_id: String,
    }
    let r: R = ipc
        .call::<(), _>("s", Method::SessionStart, None, Duration::from_secs(5))
        .await
        .unwrap()
        .expect("session.start ok");
    r.session_id
}

async fn ipc_tool_call<P: serde::Serialize, R: serde::de::DeserializeOwned>(
    sock: &PathBuf,
    method: Method,
    params: P,
) -> Result<R, RpcError> {
    let mut ipc = IpcClient::connect(sock).await.unwrap();
    ipc.call::<P, R>("t", method, Some(params), Duration::from_secs(15))
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn navigate_round_trips_wait_until_and_reached() {
    let (handle, sock) = spawn_daemon().await;
    let mut ws = connect_ext(handle.ws_addr()).await;
    let _ = do_handshake(&mut ws).await;

    run_extension(ws, |req| {
        assert_eq!(req.method, Method::ToolNavigate);
        let p: NavigateParams = serde_json::from_value(req.params.clone().unwrap()).unwrap();
        assert_eq!(p.url, "https://example.com/");
        assert_eq!(p.wait_until, Some(WaitUntil::DomContentLoaded));
        ResponseBody::Ok(
            serde_json::to_value(NavigateResult {
                tab_id: 17,
                url: p.url.clone(),
                final_url: Some("https://example.com/landing".into()),
                reached: "domcontentloaded".into(),
                error_text: None,
                dialogs: vec![],
            })
            .unwrap(),
        )
    });

    let session_id = ipc_session_start(&sock).await;
    let result: NavigateResult = ipc_tool_call(
        &sock,
        Method::ToolNavigate,
        NavigateParams {
            session_id,
            url: "https://example.com/".into(),
            tab_id: None,
            wait_until: Some(WaitUntil::DomContentLoaded),
            timeout_ms: Some(5_000),
        },
    )
    .await
    .expect("navigate ok");
    assert_eq!(result.tab_id, 17);
    assert_eq!(result.reached, "domcontentloaded");
    assert_eq!(
        result.final_url.as_deref(),
        Some("https://example.com/landing")
    );
    handle.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn five_step_flow_reuses_the_existing_tool_queue() {
    let (handle, sock) = spawn_daemon().await;
    let mut ws = connect_ext(handle.ws_addr()).await;
    let _ = do_handshake(&mut ws).await;
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen_by_extension = Arc::clone(&seen);
    run_extension(ws, move |req| {
        seen_by_extension
            .lock()
            .unwrap()
            .push(req.method.as_str().to_string());
        match req.method {
            Method::ToolNavigate => ResponseBody::Ok(json!({
                "tab_id": 17, "url": "https://demo.playwright.dev/todomvc/#/",
                "final_url": "https://demo.playwright.dev/todomvc/#/",
                "reached": "domcontentloaded"
            })),
            Method::ToolFill => ResponseBody::Ok(json!({
                "tab_id": 17, "used_selector": ".new-todo", "value_length": 3
            })),
            Method::ToolPress => ResponseBody::Ok(json!({
                "tab_id": 17, "key": "Enter", "code": "Enter", "modifiers": []
            })),
            Method::ToolClick => ResponseBody::Ok(json!({
                "tab_id": 17, "used_selector": ".toggle", "x": 1.0, "y": 1.0
            })),
            Method::ToolSnapshot => ResponseBody::Ok(json!({
                "text": "@e1 checkbox checked", "ref_count": 1,
                "tab_id": 17, "truncated": false
            })),
            _ => panic!("unexpected flow method: {:?}", req.method),
        }
    });

    let session_id = ipc_session_start(&sock).await;
    let flow: FlowDefinition =
        serde_yaml::from_str(include_str!("../../../examples/flows/todomvc.yaml")).unwrap();
    let result: FlowRunResult = ipc_tool_call(
        &sock,
        Method::FlowRun,
        FlowRunParams {
            session_id,
            flow,
            variables: std::collections::BTreeMap::from([("task".into(), "吃饭".into())]),
        },
    )
    .await
    .expect("flow ok");
    assert_eq!(result.completed_steps.len(), 5);
    assert_eq!(
        *seen.lock().unwrap(),
        [
            "tool.navigate",
            "tool.fill",
            "tool.press",
            "tool.click",
            "tool.snapshot"
        ]
    );
    handle.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn individual_cli_rpc_and_flow_share_the_same_locator_wire_shape() {
    let (handle, sock) = spawn_daemon().await;
    let mut ws = connect_ext(handle.ws_addr()).await;
    let _ = do_handshake(&mut ws).await;
    let seen = Arc::new(std::sync::Mutex::new(Vec::<Locator>::new()));
    let seen_by_extension = Arc::clone(&seen);
    run_extension(ws, move |req| {
        assert_eq!(req.method, Method::ToolClick);
        let params: ClickParams =
            serde_json::from_value(req.params.clone().expect("click params")).unwrap();
        seen_by_extension
            .lock()
            .unwrap()
            .push(params.target.clone());
        ResponseBody::Ok(
            serde_json::to_value(ClickResult {
                tab_id: 17,
                used_target: params.target.clone(),
                used_ref: params.target.ref_.clone(),
                used_selector: params.target.css.clone(),
                x: 10.0,
                y: 20.0,
                dialogs: vec![],
            })
            .unwrap(),
        )
    });

    let session_id = ipc_session_start(&sock).await;
    let target = Locator {
        role: Some("button".into()),
        name: Some("Save".into()),
        exact: Some(true),
        ..Locator::default()
    };
    let direct: ClickResult = ipc_tool_call(
        &sock,
        Method::ToolClick,
        ClickParams {
            session_id: session_id.clone(),
            target: target.clone(),
            tab_id: None,
            button: None,
            click_count: None,
            modifiers: None,
            timeout_ms: Some(5_000),
        },
    )
    .await
    .expect("direct click ok");

    let flow: FlowDefinition = serde_yaml::from_str(
        r#"name: locator-parity
steps:
  - click:
      target:
        role: button
        name: Save
        exact: true
"#,
    )
    .unwrap();
    let flow_result: FlowRunResult = ipc_tool_call(
        &sock,
        Method::FlowRun,
        FlowRunParams {
            session_id,
            flow,
            variables: std::collections::BTreeMap::new(),
        },
    )
    .await
    .expect("flow click ok");
    let flow_click: ClickResult =
        serde_json::from_value(flow_result.completed_steps[0].output.clone()).unwrap();

    assert_eq!(direct.used_target, target);
    assert_eq!(flow_click.used_target, target);
    assert_eq!(*seen.lock().unwrap(), vec![target.clone(), target]);
    handle.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn navigate_back_round_trips_previous_url() {
    let (handle, sock) = spawn_daemon().await;
    let mut ws = connect_ext(handle.ws_addr()).await;
    let _ = do_handshake(&mut ws).await;

    run_extension(ws, |req| match &req.method {
        Method::ToolNavigateBack => {
            let _: NavigateBackParams =
                serde_json::from_value(req.params.clone().unwrap()).unwrap();
            ResponseBody::Ok(
                serde_json::to_value(NavigateBackResult {
                    tab_id: 4,
                    previous_url: Some("https://b.example/".into()),
                    final_url: Some("https://a.example/".into()),
                    reached: "load".into(),
                    error_text: None,
                    dialogs: vec![],
                })
                .unwrap(),
            )
        }
        Method::ToolNavigateForward => {
            let _: NavigateForwardParams =
                serde_json::from_value(req.params.clone().unwrap()).unwrap();
            ResponseBody::Ok(
                serde_json::to_value(NavigateForwardResult {
                    tab_id: 4,
                    previous_url: Some("https://a.example/".into()),
                    final_url: Some("https://b.example/".into()),
                    reached: "load".into(),
                    error_text: None,
                    dialogs: vec![],
                })
                .unwrap(),
            )
        }
        Method::ToolReload => {
            let p: ReloadParams = serde_json::from_value(req.params.clone().unwrap()).unwrap();
            assert_eq!(p.hard, Some(true));
            ResponseBody::Ok(
                serde_json::to_value(ReloadResult {
                    tab_id: 4,
                    previous_url: Some("https://b.example/".into()),
                    final_url: Some("https://b.example/".into()),
                    reached: "load".into(),
                    error_text: None,
                    dialogs: vec![],
                })
                .unwrap(),
            )
        }
        other => panic!("unexpected method {other:?}"),
    });

    let session_id = ipc_session_start(&sock).await;
    let back: NavigateBackResult = ipc_tool_call(
        &sock,
        Method::ToolNavigateBack,
        NavigateBackParams {
            session_id: session_id.clone(),
            tab_id: None,
            wait_until: Some(WaitUntil::Load),
            timeout_ms: Some(5_000),
        },
    )
    .await
    .expect("back ok");
    assert_eq!(back.previous_url.as_deref(), Some("https://b.example/"));
    let fwd: NavigateForwardResult = ipc_tool_call(
        &sock,
        Method::ToolNavigateForward,
        NavigateForwardParams {
            session_id: session_id.clone(),
            tab_id: None,
            wait_until: Some(WaitUntil::Load),
            timeout_ms: Some(5_000),
        },
    )
    .await
    .expect("forward ok");
    assert_eq!(fwd.final_url.as_deref(), Some("https://b.example/"));
    let reload: ReloadResult = ipc_tool_call(
        &sock,
        Method::ToolReload,
        ReloadParams {
            session_id,
            tab_id: None,
            wait_until: Some(WaitUntil::Load),
            timeout_ms: Some(5_000),
            hard: Some(true),
        },
    )
    .await
    .expect("reload ok");
    assert_eq!(reload.reached, "load");
    handle.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn click_round_trips_ref_and_modifiers() {
    let (handle, sock) = spawn_daemon().await;
    let mut ws = connect_ext(handle.ws_addr()).await;
    let _ = do_handshake(&mut ws).await;

    run_extension(ws, |req| {
        assert_eq!(req.method, Method::ToolClick);
        // Verify the nested target wire form: `ref` not `ref_`.
        let params = req.params.clone().unwrap();
        assert_eq!(params["target"]["ref"], "@e3");
        let p: ClickParams = serde_json::from_value(params).unwrap();
        assert_eq!(p.target.ref_.as_deref(), Some("@e3"));
        assert_eq!(p.button, Some(MouseButton::Right));
        assert_eq!(
            p.modifiers,
            Some(vec![KeyModifier::Ctrl, KeyModifier::Shift])
        );
        ResponseBody::Ok(
            serde_json::to_value(ClickResult {
                tab_id: 9,
                used_target: ref_locator("e3"),
                used_ref: Some("e3".into()),
                used_selector: None,
                x: 12.5,
                y: 34.0,
                dialogs: vec![],
            })
            .unwrap(),
        )
    });

    let session_id = ipc_session_start(&sock).await;
    let result: ClickResult = ipc_tool_call(
        &sock,
        Method::ToolClick,
        ClickParams {
            session_id,
            target: ref_locator("@e3"),
            tab_id: None,
            button: Some(MouseButton::Right),
            click_count: Some(1),
            modifiers: Some(vec![KeyModifier::Ctrl, KeyModifier::Shift]),
            timeout_ms: Some(5_000),
        },
    )
    .await
    .expect("click ok");
    assert_eq!(result.used_ref.as_deref(), Some("e3"));
    assert!((result.x - 12.5).abs() < f64::EPSILON);
    handle.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fill_round_trips_clear_before_default() {
    let (handle, sock) = spawn_daemon().await;
    let mut ws = connect_ext(handle.ws_addr()).await;
    let _ = do_handshake(&mut ws).await;

    run_extension(ws, |req| {
        assert_eq!(req.method, Method::ToolFill);
        let p: FillParams = serde_json::from_value(req.params.clone().unwrap()).unwrap();
        assert_eq!(p.value, "hello world");
        // clear_before omitted on the wire → None (extension applies default true).
        assert_eq!(p.clear_before, None);
        ResponseBody::Ok(
            serde_json::to_value(FillResult {
                tab_id: 12,
                used_target: css_locator(".search"),
                used_ref: None,
                used_selector: Some(".search".into()),
                value_length: 11,
                dialogs: vec![],
            })
            .unwrap(),
        )
    });

    let session_id = ipc_session_start(&sock).await;
    let result: FillResult = ipc_tool_call(
        &sock,
        Method::ToolFill,
        FillParams {
            session_id,
            value: "hello world".into(),
            target: css_locator(".search"),
            tab_id: None,
            clear_before: None,
            timeout_ms: Some(5_000),
        },
    )
    .await
    .expect("fill ok");
    assert_eq!(result.value_length, 11);
    assert_eq!(result.used_selector.as_deref(), Some(".search"));
    handle.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn press_round_trips_compound_key() {
    let (handle, sock) = spawn_daemon().await;
    let mut ws = connect_ext(handle.ws_addr()).await;
    let _ = do_handshake(&mut ws).await;

    run_extension(ws, |req| {
        assert_eq!(req.method, Method::ToolPress);
        let p: PressParams = serde_json::from_value(req.params.clone().unwrap()).unwrap();
        assert_eq!(p.key, "Ctrl+A");
        ResponseBody::Ok(
            serde_json::to_value(PressResult {
                tab_id: 4,
                key: "A".into(),
                code: "KeyA".into(),
                modifiers: vec![KeyModifier::Ctrl],
                used_target: None,
                dialogs: vec![],
            })
            .unwrap(),
        )
    });

    let session_id = ipc_session_start(&sock).await;
    let result: PressResult = ipc_tool_call(
        &sock,
        Method::ToolPress,
        PressParams {
            session_id,
            key: "Ctrl+A".into(),
            modifiers: None,
            target: None,
            tab_id: None,
            hold_ms: None,
            timeout_ms: Some(5_000),
        },
    )
    .await
    .expect("press ok");
    assert_eq!(result.code, "KeyA");
    assert_eq!(result.modifiers, vec![KeyModifier::Ctrl]);
    handle.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn select_round_trips_values() {
    let (handle, sock) = spawn_daemon().await;
    let mut ws = connect_ext(handle.ws_addr()).await;
    let _ = do_handshake(&mut ws).await;

    run_extension(ws, |req| {
        assert_eq!(req.method, Method::ToolSelect);
        let p: SelectParams = serde_json::from_value(req.params.clone().unwrap()).unwrap();
        assert_eq!(p.values, vec!["us".to_string(), "ca".to_string()]);
        ResponseBody::Ok(
            serde_json::to_value(SelectResult {
                tab_id: 12,
                used_target: ref_locator("e3"),
                used_ref: Some("e3".into()),
                used_selector: None,
                multiple: true,
                selected_values: vec!["us".into(), "ca".into()],
                selected_labels: vec!["United States".into(), "Canada".into()],
                dialogs: vec![],
            })
            .unwrap(),
        )
    });

    let session_id = ipc_session_start(&sock).await;
    let result: SelectResult = ipc_tool_call(
        &sock,
        Method::ToolSelect,
        SelectParams {
            session_id,
            values: vec!["us".into(), "ca".into()],
            target: ref_locator("@e3"),
            tab_id: None,
            timeout_ms: Some(5_000),
        },
    )
    .await
    .expect("select ok");
    assert!(result.multiple);
    assert_eq!(result.selected_values, vec!["us", "ca"]);
    assert_eq!(result.selected_labels, vec!["United States", "Canada"]);
    handle.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m7_tools_propagate_extension_errors() {
    let (handle, sock) = spawn_daemon().await;
    let mut ws = connect_ext(handle.ws_addr()).await;
    let _ = do_handshake(&mut ws).await;

    run_extension(ws, |req| match &req.method {
        Method::ToolNavigate
        | Method::ToolNavigateBack
        | Method::ToolNavigateForward
        | Method::ToolReload
        | Method::ToolClick
        | Method::ToolFill
        | Method::ToolPress
        | Method::ToolSelect => ResponseBody::Err(RpcError {
            code: ErrorCode::CdpFailed,
            message: format!("forced error for {:?}", req.method),
            data: None,
        }),
        other => panic!("unexpected method {other:?}"),
    });

    let session_id = ipc_session_start(&sock).await;

    let nav = ipc_tool_call::<_, Value>(
        &sock,
        Method::ToolNavigate,
        NavigateParams {
            session_id: session_id.clone(),
            url: "https://example.com/".into(),
            tab_id: None,
            wait_until: None,
            timeout_ms: Some(5_000),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(nav.code, ErrorCode::CdpFailed);

    let back = ipc_tool_call::<_, Value>(
        &sock,
        Method::ToolNavigateBack,
        NavigateBackParams {
            session_id: session_id.clone(),
            tab_id: None,
            wait_until: None,
            timeout_ms: Some(5_000),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(back.code, ErrorCode::CdpFailed);

    let forward = ipc_tool_call::<_, Value>(
        &sock,
        Method::ToolNavigateForward,
        NavigateForwardParams {
            session_id: session_id.clone(),
            tab_id: None,
            wait_until: None,
            timeout_ms: Some(5_000),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(forward.code, ErrorCode::CdpFailed);

    let reload = ipc_tool_call::<_, Value>(
        &sock,
        Method::ToolReload,
        ReloadParams {
            session_id: session_id.clone(),
            tab_id: None,
            wait_until: None,
            timeout_ms: Some(5_000),
            hard: None,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(reload.code, ErrorCode::CdpFailed);

    let click = ipc_tool_call::<_, Value>(
        &sock,
        Method::ToolClick,
        ClickParams {
            session_id: session_id.clone(),
            target: ref_locator("@e1"),
            tab_id: None,
            button: None,
            click_count: None,
            modifiers: None,
            timeout_ms: Some(5_000),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(click.code, ErrorCode::CdpFailed);

    let fill = ipc_tool_call::<_, Value>(
        &sock,
        Method::ToolFill,
        FillParams {
            session_id: session_id.clone(),
            value: "hello".into(),
            target: css_locator("input"),
            tab_id: None,
            clear_before: None,
            timeout_ms: Some(5_000),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(fill.code, ErrorCode::CdpFailed);

    let press = ipc_tool_call::<_, Value>(
        &sock,
        Method::ToolPress,
        PressParams {
            session_id: session_id.clone(),
            key: "Enter".into(),
            modifiers: None,
            target: None,
            tab_id: None,
            hold_ms: None,
            timeout_ms: Some(5_000),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(press.code, ErrorCode::CdpFailed);

    let select = ipc_tool_call::<_, Value>(
        &sock,
        Method::ToolSelect,
        SelectParams {
            session_id: session_id.clone(),
            values: vec!["a".into()],
            target: ref_locator("@e1"),
            tab_id: None,
            timeout_ms: Some(5_000),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(select.code, ErrorCode::CdpFailed);

    handle.shutdown().await;
}
