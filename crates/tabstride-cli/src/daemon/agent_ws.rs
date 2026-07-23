//! Persistent Agent WebSocket endpoint served at `/agent` on the same
//! localhost listener used by the browser extension.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, anyhow};
use futures_util::{SinkExt, StreamExt};
use semver::Version;
use serde_json::Value;
use tabstride_protocol::{
    AgentHandshakeParams, ErrorCode, Frame, HandshakeCompat, HandshakeResult, Method, RequestFrame,
    ResponseBody, ResponseFrame, RpcError, evaluate_handshake_compat,
};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::protocol::Message;
use tracing::{info, warn};
use uuid::Uuid;

use super::ipc::RpcHandler;
use super::state::{MIN_COMPATIBLE_PROTOCOL, PROTOCOL_VERSION, SERVER_NAME};

const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(5);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const DISCONNECT_CLEANUP_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn drive_connection(
    handler: RpcHandler,
    expected_token: &str,
    ws: WebSocketStream<TcpStream>,
    peer: SocketAddr,
) -> anyhow::Result<()> {
    let connection_id = Uuid::new_v4().to_string();
    let (mut writer, mut reader) = ws.split();
    let first = tokio::time::timeout(FIRST_FRAME_TIMEOUT, reader.next())
        .await
        .map_err(|_| anyhow!("agent handshake timeout"))?
        .ok_or_else(|| anyhow!("agent disconnected before handshake"))??;
    let Message::Text(first_text) = first else {
        return Err(anyhow!("agent first frame must be text"));
    };
    let request: RequestFrame =
        serde_json::from_str(&first_text).context("decode agent handshake request")?;
    if request.method != Method::SystemHandshake {
        send_error(
            &mut writer,
            request.id,
            ErrorCode::ProtocolError,
            "first agent frame must be system.handshake",
        )
        .await?;
        return Err(anyhow!("agent sent non-handshake first frame"));
    }
    let params: AgentHandshakeParams =
        serde_json::from_value(request.params.unwrap_or(Value::Null))
            .context("decode agent handshake params")?;
    if params.token != expected_token {
        send_error(
            &mut writer,
            request.id,
            ErrorCode::PermissionDenied,
            "invalid agent capability token",
        )
        .await?;
        return Err(anyhow!("agent capability token rejected"));
    }
    match evaluate_handshake_compat(
        &params.protocol_version,
        params.min_compatible_protocol.as_deref(),
        PROTOCOL_VERSION,
        MIN_COMPATIBLE_PROTOCOL,
    ) {
        HandshakeCompat::Reject { reason } => {
            send_error(
                &mut writer,
                request.id,
                ErrorCode::VersionTooOld,
                reason.clone(),
            )
            .await?;
            return Err(anyhow!(reason));
        }
        HandshakeCompat::Skew => warn!(
            connection = %connection_id,
            peer_protocol = %params.protocol_version,
            "agent protocol minor drift"
        ),
        HandshakeCompat::Ok => {}
    }
    let result = HandshakeResult {
        server: SERVER_NAME.to_string(),
        version: Version::parse(env!("CARGO_PKG_VERSION")).expect("valid package version"),
        protocol_version: PROTOCOL_VERSION.to_string(),
        min_compatible_peer: None,
        min_compatible_protocol: Some(MIN_COMPATIBLE_PROTOCOL.to_string()),
    };
    writer
        .send(Message::Text(serde_json::to_string(&ResponseFrame {
            id: request.id,
            body: ResponseBody::Ok(serde_json::to_value(result)?),
        })?))
        .await?;

    info!(connection = %connection_id, %peer, client = %params.client, "agent client connected");
    let active_ids = Arc::new(Mutex::new(HashSet::<String>::new()));
    let owned_sessions = Arc::new(Mutex::new(HashSet::<String>::new()));
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<Message>();
    let mut tasks = tokio::task::JoinSet::new();
    let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    heartbeat.tick().await;

    loop {
        tokio::select! {
            outbound = outbound_rx.recv() => {
                match outbound {
                    Some(message) => writer.send(message).await?,
                    None => break,
                }
            }
            incoming = reader.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        dispatch_text(
                            &handler,
                            &outbound_tx,
                            &active_ids,
                            &owned_sessions,
                            &mut tasks,
                            &text,
                        );
                    }
                    Some(Ok(Message::Ping(payload))) => writer.send(Message::Pong(payload)).await?,
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Binary(_) | Message::Frame(_))) => {
                        warn!(connection = %connection_id, "ignoring non-text agent frame");
                    }
                    Some(Err(err)) => {
                        warn!(connection = %connection_id, %err, "agent websocket read failed");
                        break;
                    }
                }
            }
            _ = heartbeat.tick() => {
                writer.send(Message::Ping(connection_id.clone().into_bytes())).await?;
            }
            Some(joined) = tasks.join_next(), if !tasks.is_empty() => {
                if let Err(err) = joined {
                    warn!(connection = %connection_id, %err, "agent request task failed");
                }
            }
        }
    }

    // Cancel active requests before stopping sessions owned by this client.
    let inflight: Vec<String> = active_ids.lock().unwrap().iter().cloned().collect();
    for rpc_id in inflight {
        let _ = handler(
            format!("agent-disconnect-cancel-{}", Uuid::new_v4()),
            Method::Cancel,
            serde_json::json!({ "rpc_id": rpc_id }),
        )
        .await;
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}

    let sessions: Vec<String> = owned_sessions.lock().unwrap().iter().cloned().collect();
    for session_id in sessions {
        let stop = handler(
            format!("agent-disconnect-stop-{}", Uuid::new_v4()),
            Method::SessionStop,
            serde_json::json!({ "session_id": session_id }),
        );
        if tokio::time::timeout(DISCONNECT_CLEANUP_TIMEOUT, stop)
            .await
            .is_err()
        {
            warn!(connection = %connection_id, "agent disconnect session cleanup timed out");
        }
    }
    info!(connection = %connection_id, %peer, "agent client disconnected");
    Ok(())
}

fn dispatch_text(
    handler: &RpcHandler,
    outbound: &mpsc::UnboundedSender<Message>,
    active_ids: &Arc<Mutex<HashSet<String>>>,
    owned_sessions: &Arc<Mutex<HashSet<String>>>,
    tasks: &mut tokio::task::JoinSet<()>,
    text: &str,
) {
    let request = match serde_json::from_str::<Frame>(text) {
        Ok(Frame::Request(request)) => request,
        Ok(_) => {
            queue_error(outbound, "-", "agent endpoint accepts request frames only");
            return;
        }
        Err(err) => {
            queue_error(outbound, "-", format!("invalid agent request: {err}"));
            return;
        }
    };
    if request.method == Method::SystemHandshake {
        queue_error(outbound, request.id, "agent handshake is already complete");
        return;
    }
    {
        let mut ids = active_ids.lock().unwrap();
        if !ids.insert(request.id.clone()) {
            queue_error(outbound, request.id, "duplicate in-flight request id");
            return;
        }
    }
    let handler = Arc::clone(handler);
    let outbound = outbound.clone();
    let active_ids = Arc::clone(active_ids);
    let owned_sessions = Arc::clone(owned_sessions);
    tasks.spawn(async move {
        let id = request.id;
        let method = request.method;
        let body = handler(
            id.clone(),
            method.clone(),
            request.params.unwrap_or(Value::Null),
        )
        .await;
        update_owned_sessions(&owned_sessions, &method, &body);
        active_ids.lock().unwrap().remove(&id);
        if let Ok(encoded) = serde_json::to_string(&ResponseFrame { id, body }) {
            let _ = outbound.send(Message::Text(encoded));
        }
    });
}

fn update_owned_sessions(
    sessions: &Arc<Mutex<HashSet<String>>>,
    method: &Method,
    body: &ResponseBody,
) {
    let ResponseBody::Ok(value) = body else {
        return;
    };
    let mut sessions = sessions.lock().unwrap();
    match method {
        Method::SessionStart => {
            if let Some(id) = value.get("session_id").and_then(Value::as_str) {
                sessions.insert(id.to_string());
            }
        }
        Method::SessionStop | Method::SessionStopAll => {
            if let Some(stopped) = value.get("stopped").and_then(Value::as_array) {
                for id in stopped.iter().filter_map(Value::as_str) {
                    sessions.remove(id);
                }
            }
        }
        _ => {}
    }
}

fn queue_error(
    outbound: &mpsc::UnboundedSender<Message>,
    id: impl Into<String>,
    message: impl Into<String>,
) {
    let response = ResponseFrame {
        id: id.into(),
        body: ResponseBody::Err(RpcError {
            code: ErrorCode::ProtocolError,
            message: message.into(),
            data: None,
        }),
    };
    if let Ok(encoded) = serde_json::to_string(&response) {
        let _ = outbound.send(Message::Text(encoded));
    }
}

async fn send_error<S>(
    writer: &mut S,
    id: String,
    code: ErrorCode,
    message: impl Into<String>,
) -> anyhow::Result<()>
where
    S: futures_util::Sink<Message> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let response = ResponseFrame {
        id,
        body: ResponseBody::Err(RpcError {
            code,
            message: message.into(),
            data: None,
        }),
    };
    writer
        .send(Message::Text(serde_json::to_string(&response)?))
        .await
        .map_err(anyhow::Error::from)
}
