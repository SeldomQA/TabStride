//! Persistent Agent WebSocket protocol tests.

use std::collections::HashMap;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use semver::Version;
use tabstride::daemon::{self, DaemonConfig};
use tabstride_protocol::{
    AgentHandshakeParams, Frame, Method, RequestFrame, ResponseBody, ResponseFrame,
};
use tokio_tungstenite::tungstenite::Message;

async fn connect_agent() -> (
    tabstride::daemon::DaemonHandle,
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
) {
    let daemon = daemon::run(DaemonConfig::new(0), None).await.unwrap();
    let agent_token = daemon.state().agent_token.clone();
    let url = format!("ws://{}/agent", daemon.ws_addr());
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    let handshake = Frame::Request(RequestFrame {
        id: "hello".into(),
        method: Method::SystemHandshake,
        params: Some(
            serde_json::to_value(AgentHandshakeParams {
                client: "integration-test".into(),
                version: Version::parse("0.2.0").unwrap(),
                protocol_version: "1.0".into(),
                token: agent_token,
                min_compatible_protocol: Some("1.0".into()),
            })
            .unwrap(),
        ),
    });
    socket
        .send(Message::Text(serde_json::to_string(&handshake).unwrap()))
        .await
        .unwrap();
    let response = socket.next().await.unwrap().unwrap();
    let Message::Text(text) = response else {
        panic!("expected text handshake response")
    };
    let frame: Frame = serde_json::from_str(&text).unwrap();
    assert!(matches!(
        frame,
        Frame::Response(ResponseFrame {
            body: ResponseBody::Ok(_),
            ..
        })
    ));
    (daemon, socket)
}

#[tokio::test]
async fn rejects_agent_with_wrong_capability_token() {
    let daemon = daemon::run(DaemonConfig::new(0), None).await.unwrap();
    let url = format!("ws://{}/agent", daemon.ws_addr());
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    let request = Frame::Request(RequestFrame {
        id: "bad-token".into(),
        method: Method::SystemHandshake,
        params: Some(
            serde_json::to_value(AgentHandshakeParams {
                client: "integration-test".into(),
                version: Version::parse("0.2.0").unwrap(),
                protocol_version: "1.0".into(),
                token: "wrong".into(),
                min_compatible_protocol: Some("1.0".into()),
            })
            .unwrap(),
        ),
    });
    socket
        .send(Message::Text(serde_json::to_string(&request).unwrap()))
        .await
        .unwrap();
    let Message::Text(text) = socket.next().await.unwrap().unwrap() else {
        panic!("expected rejection response")
    };
    let Frame::Response(response) = serde_json::from_str(&text).unwrap() else {
        panic!("expected response frame")
    };
    assert!(matches!(response.body, ResponseBody::Err(_)));
    daemon.shutdown().await;
}

#[tokio::test]
async fn one_connection_handles_one_hundred_pipelined_requests() {
    let (daemon, mut socket) = connect_agent().await;
    for index in 0..100 {
        let request = Frame::Request(RequestFrame {
            id: format!("ping-{index}"),
            method: Method::SystemPing,
            params: Some(serde_json::json!({})),
        });
        socket
            .send(Message::Text(serde_json::to_string(&request).unwrap()))
            .await
            .unwrap();
    }

    let mut seen = HashMap::new();
    while seen.len() < 100 {
        let message = tokio::time::timeout(Duration::from_secs(5), socket.next())
            .await
            .expect("response timeout")
            .expect("socket closed")
            .expect("websocket error");
        if let Message::Text(text) = message {
            let Frame::Response(response) = serde_json::from_str(&text).unwrap() else {
                continue;
            };
            seen.insert(response.id, response.body);
        }
    }
    assert_eq!(seen.len(), 100);
    assert!(
        seen.values()
            .all(|body| matches!(body, ResponseBody::Ok(_)))
    );

    socket.close(None).await.unwrap();
    daemon.shutdown().await;
}

#[tokio::test]
async fn cancel_can_overtake_a_long_running_request_on_same_connection() {
    let (daemon, mut socket) = connect_agent().await;
    for request in [
        RequestFrame {
            id: "wait-long".into(),
            method: Method::ToolWaitMs,
            params: Some(serde_json::json!({ "duration_ms": 30_000 })),
        },
        RequestFrame {
            id: "cancel-wait".into(),
            method: Method::Cancel,
            params: Some(serde_json::json!({ "rpc_id": "wait-long" })),
        },
    ] {
        socket
            .send(Message::Text(
                serde_json::to_string(&Frame::Request(request)).unwrap(),
            ))
            .await
            .unwrap();
    }

    let mut responses = HashMap::new();
    while responses.len() < 2 {
        let message = tokio::time::timeout(Duration::from_secs(5), socket.next())
            .await
            .expect("cancel response timeout")
            .expect("socket closed")
            .expect("websocket error");
        if let Message::Text(text) = message {
            let Frame::Response(response) = serde_json::from_str(&text).unwrap() else {
                continue;
            };
            responses.insert(response.id, response.body);
        }
    }
    assert!(matches!(responses["cancel-wait"], ResponseBody::Ok(_)));
    assert!(matches!(responses["wait-long"], ResponseBody::Err(_)));

    socket.close(None).await.unwrap();
    daemon.shutdown().await;
}
