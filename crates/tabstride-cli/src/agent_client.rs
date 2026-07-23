//! WebSocket transport used by the persistent native Agent client.

use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use futures_util::{SinkExt, StreamExt};
use semver::Version;
use serde_json::Value;
use tabstride_protocol::{
    AgentHandshakeParams, Frame, HandshakeResult, Method, RequestFrame, ResponseBody, ResponseFrame,
};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use crate::daemon::info as daemon_info;
use crate::daemon::state::{MIN_COMPATIBLE_PROTOCOL, PROTOCOL_VERSION};
use crate::ipc_client::RpcOutcome;

pub struct AgentClient {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl AgentClient {
    pub async fn connect() -> Result<Self> {
        let info = daemon_info::read_valid()
            .context("read daemon.json")?
            .ok_or_else(|| anyhow!("no live service (daemon.json missing or stale)"))?;
        let url = format!("ws://127.0.0.1:{}/agent", info.ws_port);
        let (socket, _) = connect_async(&url)
            .await
            .with_context(|| format!("connect agent websocket {url}"))?;
        let mut client = Self { socket };
        let result = client
            .call_with_id::<_, HandshakeResult>(
                "agent-handshake".to_string(),
                Method::SystemHandshake,
                &AgentHandshakeParams {
                    client: "tabstride-native".to_string(),
                    version: Version::parse(env!("CARGO_PKG_VERSION"))
                        .expect("valid package version"),
                    protocol_version: PROTOCOL_VERSION.to_string(),
                    token: info.agent_token,
                    min_compatible_protocol: Some(MIN_COMPATIBLE_PROTOCOL.to_string()),
                },
                Duration::from_secs(5),
            )
            .await?
            .map_err(|error| anyhow!("agent handshake rejected: {}", error.message))?;
        if result.protocol_version.split('.').next()
            != Some(PROTOCOL_VERSION.split('.').next().unwrap())
        {
            return Err(anyhow!(
                "agent protocol mismatch: server={}, client={}",
                result.protocol_version,
                PROTOCOL_VERSION
            ));
        }
        Ok(client)
    }

    pub async fn call_with_id<P, R>(
        &mut self,
        id: String,
        method: Method,
        params: &P,
        call_timeout: Duration,
    ) -> Result<RpcOutcome<R>>
    where
        P: serde::Serialize,
        R: serde::de::DeserializeOwned,
    {
        let frame = Frame::Request(RequestFrame {
            id: id.clone(),
            method,
            params: Some(serde_json::to_value(params).context("serialize agent params")?),
        });
        self.socket
            .send(Message::Text(serde_json::to_string(&frame)?))
            .await
            .context("send agent request")?;
        timeout(call_timeout, async {
            loop {
                match self.socket.next().await {
                    Some(Ok(Message::Text(text))) => {
                        let frame: Frame =
                            serde_json::from_str(&text).context("decode agent response")?;
                        if let Frame::Response(ResponseFrame {
                            id: response_id,
                            body,
                        }) = frame
                        {
                            if response_id != id {
                                return Err(anyhow!(
                                    "agent response id mismatch: expected {id}, got {response_id}"
                                ));
                            }
                            return match body {
                                ResponseBody::Ok(value) => {
                                    Ok(Ok(serde_json::from_value(value)
                                        .context("decode agent result")?))
                                }
                                ResponseBody::Err(error) => Ok(Err(error)),
                            };
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        self.socket.send(Message::Pong(payload)).await?;
                    }
                    Some(Ok(Message::Pong(_) | Message::Binary(_) | Message::Frame(_))) => {}
                    Some(Ok(Message::Close(_))) | None => {
                        return Err(anyhow!("agent websocket closed"));
                    }
                    Some(Err(err)) => return Err(err.into()),
                }
            }
        })
        .await
        .context("agent request timed out")?
    }

    pub async fn call_value(
        &mut self,
        id: String,
        method: Method,
        params: &Value,
        call_timeout: Duration,
    ) -> Result<RpcOutcome<Value>> {
        self.call_with_id(id, method, params, call_timeout).await
    }

    pub fn into_socket(self) -> WebSocketStream<MaybeTlsStream<TcpStream>> {
        self.socket
    }
}
