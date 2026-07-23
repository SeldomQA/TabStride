//! Persistent native client: newline-delimited protocol frames on stdio,
//! one long-lived local IPC connection to `tabstride serve`.

use std::time::Duration;
use std::{
    collections::{HashMap, HashSet},
    time::Instant,
};

use anyhow::{Context, Result};
use clap::{Args, ValueEnum};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tabstride_protocol::{ErrorCode, Frame, RequestFrame, ResponseBody, ResponseFrame, RpcError};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use crate::agent_client::AgentClient;
use crate::cli::daemon::parse_duration;
use crate::ipc_client::Client;

const DEFAULT_CALL_TIMEOUT: Duration = Duration::from_secs(35);

#[derive(Debug, Clone, Args)]
pub struct ClientArgs {
    /// Maximum time allowed for each proxied request.
    #[arg(long, value_name = "DURATION", default_value = "35s", value_parser = parse_duration)]
    pub timeout: Duration,

    /// Persistent transport. WebSocket is the Agent API; IPC is retained for comparison.
    #[arg(long, value_enum, default_value_t = ClientTransport::Websocket)]
    pub transport: ClientTransport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ClientTransport {
    Websocket,
    Ipc,
}

impl Default for ClientArgs {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_CALL_TIMEOUT,
            transport: ClientTransport::Websocket,
        }
    }
}

/// Read request frames from stdin and write response frames to stdout.
///
/// The process deliberately emits no prompt or banner on stdout: every
/// non-empty output line is a valid protocol [`ResponseFrame`]. This lets an
/// Agent keep the child process alive and correlate responses by request id.
pub fn run(args: ClientArgs) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build native client runtime")?;
    runtime.block_on(run_stdio(args.timeout, args.transport))
}

async fn run_stdio(call_timeout: Duration, transport: ClientTransport) -> Result<()> {
    if transport == ClientTransport::Websocket {
        let client = AgentClient::connect()
            .await
            .context("connect persistent client to TabStride service")?;
        return run_websocket_stdio(client, call_timeout).await;
    }
    let mut client = Client::connect()
        .await
        .context("connect persistent client to TabStride service")?;
    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();
    let mut stdout = tokio::io::stdout();

    while let Some(line) = lines
        .next_line()
        .await
        .context("read native client stdin")?
    {
        if line.trim().is_empty() {
            continue;
        }
        let response = proxy_line(&mut client, &line, call_timeout).await;
        let mut encoded = serde_json::to_vec(&response).context("encode native client response")?;
        encoded.push(b'\n');
        stdout
            .write_all(&encoded)
            .await
            .context("write native client stdout")?;
        stdout.flush().await.context("flush native client stdout")?;
    }
    Ok(())
}

async fn run_websocket_stdio(client: AgentClient, call_timeout: Duration) -> Result<()> {
    let (mut writer, mut reader) = client.into_socket().split();
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    let mut deadlines = HashMap::<String, Instant>::new();
    let mut internal_cancel_ids = HashSet::<String>::new();
    let mut stdin_open = true;
    let mut timeout_tick = tokio::time::interval(Duration::from_millis(100));
    timeout_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        if !stdin_open && deadlines.is_empty() {
            let _ = writer.send(Message::Close(None)).await;
            return Ok(());
        }
        tokio::select! {
            line = lines.next_line(), if stdin_open => {
                match line.context("read native client stdin")? {
                    Some(line) if line.trim().is_empty() => {}
                    Some(line) => {
                        let request = match serde_json::from_str::<Frame>(&line) {
                            Ok(Frame::Request(request)) => request,
                            Ok(_) => {
                                write_response(&mut stdout, error_response(
                                    "-", ErrorCode::ProtocolError,
                                    "native client accepts request frames only",
                                )).await?;
                                continue;
                            }
                            Err(err) => {
                                write_response(&mut stdout, error_response(
                                    extract_id(&line).as_deref().unwrap_or("-"),
                                    ErrorCode::ProtocolError,
                                    format!("invalid request frame: {err}"),
                                )).await?;
                                continue;
                            }
                        };
                        if deadlines.contains_key(&request.id) {
                            write_response(&mut stdout, error_response(
                                request.id, ErrorCode::ProtocolError,
                                "duplicate in-flight request id",
                            )).await?;
                            continue;
                        }
                        deadlines.insert(request.id.clone(), Instant::now() + call_timeout);
                        writer.send(Message::Text(serde_json::to_string(&Frame::Request(request))?)).await
                            .context("send agent request")?;
                    }
                    None => stdin_open = false,
                }
            }
            incoming = reader.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        let frame: Frame = serde_json::from_str(&text).context("decode agent response")?;
                        if let Frame::Response(response) = frame {
                            if internal_cancel_ids.remove(&response.id) {
                                continue;
                            }
                            deadlines.remove(&response.id);
                            write_response(&mut stdout, response).await?;
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => writer.send(Message::Pong(payload)).await?,
                    Some(Ok(Message::Pong(_) | Message::Binary(_) | Message::Frame(_))) => {}
                    Some(Ok(Message::Close(_))) | None => {
                        if deadlines.is_empty() { return Ok(()); }
                        return Err(anyhow::anyhow!("agent websocket closed with requests in flight"));
                    }
                    Some(Err(err)) => return Err(err.into()),
                }
            }
            _ = timeout_tick.tick(), if !deadlines.is_empty() => {
                let now = Instant::now();
                let expired: Vec<String> = deadlines
                    .iter()
                    .filter(|(_, deadline)| **deadline <= now)
                    .map(|(id, _)| id.clone())
                    .collect();
                for rpc_id in expired {
                    // Move the deadline forward so a slow cancellation does not
                    // emit an unbounded stream of cancel frames.
                    deadlines.insert(rpc_id.clone(), now + call_timeout);
                    let cancel_id = format!("client-timeout-{}", Uuid::new_v4());
                    internal_cancel_ids.insert(cancel_id.clone());
                    let frame = Frame::Request(RequestFrame {
                        id: cancel_id,
                        method: tabstride_protocol::Method::Cancel,
                        params: Some(serde_json::json!({ "rpc_id": rpc_id })),
                    });
                    writer.send(Message::Text(serde_json::to_string(&frame)?)).await
                        .context("send timed-out request cancellation")?;
                }
            }
        }
    }
}

async fn write_response(stdout: &mut tokio::io::Stdout, response: ResponseFrame) -> Result<()> {
    let mut encoded = serde_json::to_vec(&response).context("encode native client response")?;
    encoded.push(b'\n');
    stdout
        .write_all(&encoded)
        .await
        .context("write native client stdout")?;
    stdout.flush().await.context("flush native client stdout")?;
    Ok(())
}

async fn proxy_line(client: &mut Client, line: &str, call_timeout: Duration) -> ResponseFrame {
    let request = match serde_json::from_str::<Frame>(line) {
        Ok(Frame::Request(request)) => request,
        Ok(_) => {
            return error_response(
                "-",
                ErrorCode::ProtocolError,
                "native client accepts request frames only",
            );
        }
        Err(err) => {
            return error_response(
                extract_id(line).as_deref().unwrap_or("-"),
                ErrorCode::ProtocolError,
                format!("invalid request frame: {err}"),
            );
        }
    };
    proxy_request(client, request, call_timeout).await
}

async fn proxy_request(
    client: &mut Client,
    request: RequestFrame,
    call_timeout: Duration,
) -> ResponseFrame {
    let id = request.id;
    let params = request.params.unwrap_or(Value::Null);
    match client
        .call_with_id::<_, Value>(id.clone(), request.method, &params, call_timeout)
        .await
    {
        Ok(Ok(result)) => ResponseFrame {
            id,
            body: ResponseBody::Ok(result),
        },
        Ok(Err(error)) => ResponseFrame {
            id,
            body: ResponseBody::Err(error),
        },
        Err(err) => error_response(
            &id,
            ErrorCode::ProtocolError,
            format!("persistent IPC request failed: {err:#}"),
        ),
    }
}

fn error_response(
    id: impl Into<String>,
    code: ErrorCode,
    message: impl Into<String>,
) -> ResponseFrame {
    ResponseFrame {
        id: id.into(),
        body: ResponseBody::Err(RpcError {
            code,
            message: message.into(),
            data: None,
        }),
    }
}

fn extract_id(line: &str) -> Option<String> {
    serde_json::from_str::<Value>(line)
        .ok()?
        .get("id")?
        .as_str()
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_id_from_malformed_request_shape() {
        assert_eq!(
            extract_id(r#"{"id":"req-7","method":9}"#),
            Some("req-7".into())
        );
        assert_eq!(extract_id("not-json"), None);
    }

    #[test]
    fn default_timeout_matches_business_rpc_budget() {
        assert_eq!(ClientArgs::default().timeout, Duration::from_secs(35));
    }
}
