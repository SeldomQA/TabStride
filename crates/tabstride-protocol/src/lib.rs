//! tabstride wire protocol — frames, methods, tool payloads, handshake.

pub mod cancel;
pub mod error;
pub mod flow;
pub mod frame;
pub mod method;
pub mod system;
pub mod tools;

pub use cancel::{CancelParams, CancelResult};
pub use error::{DecodeError, ErrorCode, RpcError};
pub use flow::*;
pub use frame::{EventFrame, EventKind, Frame, RequestFrame, ResponseBody, ResponseFrame, RpcId};
pub use method::Method;
pub use system::{
    AgentHandshakeParams, BrowserListParams, BrowserPeerInfo, BrowserStatusEntry, HandshakeCompat,
    HandshakeParams, HandshakeResult, PingParams, PingResult, SessionStatusEntry, StatusParams,
    StatusResult, VersionSkewEntry, compare_protocol, evaluate_handshake_compat,
};
pub use tools::*;
