//! Flow v1 executor built on the existing tool dispatch path.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;
use tabstride_protocol::{
    ErrorCode, FlowDefinition, FlowFailureData, FlowRunParams, FlowRunResult, FlowStep,
    FlowStepResult, Method, ResponseBody, RpcError, RpcId,
};

use super::state::DaemonState;

const DEFAULT_FLOW_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_FLOW_TIMEOUT: Duration = Duration::from_secs(10 * 60);

pub async fn handle_flow_run(
    state: &Arc<DaemonState>,
    rpc_id: RpcId,
    params: Value,
) -> ResponseBody {
    let params: FlowRunParams = match serde_json::from_value(params) {
        Ok(params) => params,
        Err(error) => return error_body(ErrorCode::InvalidParams, error.to_string()),
    };
    if params.session_id.trim().is_empty() {
        return error_body(
            ErrorCode::InvalidParams,
            "flow.run requires non-empty session_id",
        );
    }

    let flow = match expand_variables(params.flow, &params.variables) {
        Ok(flow) => flow,
        Err(error) => return ResponseBody::Err(error),
    };
    if let Err(error) = flow.validate() {
        return ResponseBody::Err(error);
    }
    let timeout = match parse_flow_timeout(flow.timeout.as_deref()) {
        Ok(timeout) => timeout,
        Err(error) => return ResponseBody::Err(error),
    };
    let guard = match state.abort_registry.register(rpc_id.clone()) {
        Ok(guard) => guard,
        Err(error) => {
            return error_body(
                ErrorCode::ProtocolError,
                format!("cannot register flow cancellation token: {error:?}"),
            );
        }
    };
    let token = guard.token().clone();
    let started = Instant::now();
    let name = flow.name.clone();
    let mut completed = Vec::with_capacity(flow.steps.len());

    for (index, step) in flow.steps.into_iter().enumerate() {
        let method = step.method();
        let method_name = method.as_str().to_string();
        if token.is_cancelled() {
            return flow_failure(
                &name,
                index,
                &method_name,
                started,
                completed,
                cancelled_error(),
            );
        }
        let Some(remaining) = timeout.checked_sub(started.elapsed()) else {
            return flow_failure(
                &name,
                index,
                &method_name,
                started,
                completed,
                timeout_error(timeout),
            );
        };

        let child_rpc_id = format!("{rpc_id}:step:{}", index + 1);
        let step_params = step.into_params(&params.session_id);
        let step_started = Instant::now();
        let dispatch = dispatch_step(state, child_rpc_id.clone(), method, step_params);
        tokio::pin!(dispatch);

        let body = tokio::select! {
            body = &mut dispatch => body,
            _ = token.cancelled() => {
                super::ipc::cancel_rpc(state, &child_rpc_id);
                let _ = tokio::time::timeout(Duration::from_secs(2), &mut dispatch).await;
                ResponseBody::Err(cancelled_error())
            }
            _ = tokio::time::sleep(remaining) => {
                super::ipc::cancel_rpc(state, &child_rpc_id);
                let _ = tokio::time::timeout(Duration::from_secs(2), &mut dispatch).await;
                ResponseBody::Err(timeout_error(timeout))
            }
        };

        match body {
            ResponseBody::Ok(output) => completed.push(FlowStepResult {
                index: index + 1,
                method: method_name,
                duration_ms: elapsed_ms(step_started),
                output,
            }),
            ResponseBody::Err(cause) => {
                return flow_failure(&name, index, &method_name, started, completed, cause);
            }
        }
    }

    ResponseBody::Ok(
        serde_json::to_value(FlowRunResult {
            name,
            duration_ms: elapsed_ms(started),
            completed_steps: completed,
        })
        .unwrap_or(Value::Null),
    )
}

async fn dispatch_step(
    state: &Arc<DaemonState>,
    rpc_id: RpcId,
    method: Method,
    params: Value,
) -> ResponseBody {
    if method == Method::ToolWaitMs {
        super::ipc::handle_wait_ms(&state.abort_registry, rpc_id, params).await
    } else {
        super::ipc::handle_tool_dispatch(state, rpc_id, method, params).await
    }
}

trait FlowStepExt {
    fn method(&self) -> Method;
    fn into_params(self, session_id: &str) -> Value;
}

impl FlowStepExt for FlowStep {
    fn method(&self) -> Method {
        match self {
            Self::Navigate(_) => Method::ToolNavigate,
            Self::Click(_) => Method::ToolClick,
            Self::Fill(_) => Method::ToolFill,
            Self::Press(_) => Method::ToolPress,
            Self::Snapshot(_) => Method::ToolSnapshot,
            Self::WaitMs(_) => Method::ToolWaitMs,
        }
    }

    fn into_params(self, session_id: &str) -> Value {
        match self {
            Self::Navigate(entry) => with_session(entry.navigate, session_id),
            Self::Click(entry) => with_session(entry.click, session_id),
            Self::Fill(entry) => with_session(entry.fill, session_id),
            Self::Press(entry) => with_session(entry.press, session_id),
            Self::Snapshot(entry) => with_session(entry.snapshot, session_id),
            Self::WaitMs(entry) => serde_json::to_value(entry.wait_ms).unwrap_or(Value::Null),
        }
    }
}

fn with_session<T: Serialize>(step: T, session_id: &str) -> Value {
    let mut value = serde_json::to_value(step).unwrap_or(Value::Null);
    if let Value::Object(object) = &mut value {
        object.insert("session_id".into(), Value::String(session_id.into()));
    }
    value
}

fn expand_variables(
    flow: FlowDefinition,
    variables: &BTreeMap<String, String>,
) -> Result<FlowDefinition, RpcError> {
    let mut value = serde_json::to_value(flow).map_err(|error| invalid(error.to_string()))?;
    expand_value(&mut value, variables)?;
    serde_json::from_value(value).map_err(|error| invalid(error.to_string()))
}

fn expand_value(value: &mut Value, variables: &BTreeMap<String, String>) -> Result<(), RpcError> {
    match value {
        Value::String(text) => *text = expand_string(text, variables)?,
        Value::Array(values) => {
            for value in values {
                expand_value(value, variables)?;
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                expand_value(value, variables)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn expand_string(input: &str, variables: &BTreeMap<String, String>) -> Result<String, RpcError> {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("{{") {
        output.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            return Err(invalid(format!("unterminated variable in `{input}`")));
        };
        let key = after[..end].trim();
        let Some(value) = variables.get(key) else {
            return Err(invalid(format!("missing flow variable `{key}`")));
        };
        output.push_str(value);
        rest = &after[end + 2..];
    }
    output.push_str(rest);
    Ok(output)
}

fn parse_flow_timeout(raw: Option<&str>) -> Result<Duration, RpcError> {
    let Some(raw) = raw else {
        return Ok(DEFAULT_FLOW_TIMEOUT);
    };
    let (number, multiplier) = if let Some(value) = raw.strip_suffix("ms") {
        (value, 1_u64)
    } else if let Some(value) = raw.strip_suffix('s') {
        (value, 1_000)
    } else if let Some(value) = raw.strip_suffix('m') {
        (value, 60_000)
    } else {
        (raw, 1)
    };
    let millis = number
        .parse::<u64>()
        .ok()
        .and_then(|value| value.checked_mul(multiplier))
        .ok_or_else(|| invalid(format!("invalid flow timeout `{raw}`")))?;
    let timeout = Duration::from_millis(millis);
    if timeout.is_zero() || timeout > MAX_FLOW_TIMEOUT {
        return Err(invalid("flow timeout must be between 1ms and 10m"));
    }
    Ok(timeout)
}

fn flow_failure(
    name: &str,
    zero_based_index: usize,
    method: &str,
    started: Instant,
    completed_steps: Vec<FlowStepResult>,
    cause: RpcError,
) -> ResponseBody {
    let code = cause.code;
    let message = format!(
        "flow `{name}` failed at step {} ({method}): {}",
        zero_based_index + 1,
        cause.message
    );
    let data = FlowFailureData {
        flow_name: name.into(),
        failed_step: zero_based_index + 1,
        failed_method: method.into(),
        duration_ms: elapsed_ms(started),
        completed_steps,
        cause,
    };
    ResponseBody::Err(RpcError {
        code,
        message,
        data: serde_json::to_value(data).ok(),
    })
}

fn invalid(message: impl Into<String>) -> RpcError {
    RpcError {
        code: ErrorCode::InvalidParams,
        message: message.into(),
        data: None,
    }
}

fn cancelled_error() -> RpcError {
    RpcError {
        code: ErrorCode::Cancelled,
        message: "flow cancelled".into(),
        data: None,
    }
}

fn timeout_error(timeout: Duration) -> RpcError {
    RpcError {
        code: ErrorCode::Timeout,
        message: format!("flow exceeded total timeout of {}ms", timeout.as_millis()),
        data: None,
    }
}

fn error_body(code: ErrorCode, message: impl Into<String>) -> ResponseBody {
    ResponseBody::Err(RpcError {
        code,
        message: message.into(),
        data: None,
    })
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_multiple_variables() {
        let vars = BTreeMap::from([
            ("host".into(), "example.com".into()),
            ("task".into(), "write-code".into()),
        ]);
        assert_eq!(
            expand_string("https://{{host}}/{{ task }}", &vars).unwrap(),
            "https://example.com/write-code"
        );
    }

    #[test]
    fn rejects_missing_variable() {
        assert!(expand_string("{{missing}}", &BTreeMap::new()).is_err());
    }

    #[test]
    fn parses_flow_durations() {
        assert_eq!(
            parse_flow_timeout(Some("250ms")).unwrap(),
            Duration::from_millis(250)
        );
        assert_eq!(
            parse_flow_timeout(Some("30s")).unwrap(),
            Duration::from_secs(30)
        );
        assert_eq!(
            parse_flow_timeout(Some("2m")).unwrap(),
            Duration::from_secs(120)
        );
    }
}
