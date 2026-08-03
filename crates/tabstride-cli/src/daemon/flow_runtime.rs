//! Flow v1 executor built on the existing tool dispatch path.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;
use tabstride_protocol::{
    ErrorCode, FlowAssertEntry, FlowDefinition, FlowFailureData, FlowRunParams, FlowRunResult,
    FlowStep, FlowStepResult, HelpOutcome, Method, RequestHelpResult, ResponseBody, RpcError,
    RpcId, StepTiming,
};
use tracing::debug;

use super::state::DaemonState;
use crate::timing::{MetricRecord, TimingTrace, append_metric, epoch_us, put_trace, take_trace};

const DEFAULT_FLOW_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_FLOW_TIMEOUT: Duration = Duration::from_secs(10 * 60);

pub async fn handle_flow_run(
    state: &Arc<DaemonState>,
    rpc_id: RpcId,
    params: Value,
    parent_timing: TimingTrace,
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
    let mut steps = flow.steps;
    steps.extend(
        flow.assertions
            .into_iter()
            .map(|assertion| FlowStep::Assert(FlowAssertEntry { assertion })),
    );
    let mut completed = Vec::with_capacity(steps.len());

    for (index, step) in steps.into_iter().enumerate() {
        let method = step.method();
        let method_name = method.as_str().to_string();
        if token.is_cancelled() {
            return flow_failure(
                &name,
                index,
                &method_name,
                started,
                completed,
                None,
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
                None,
                timeout_error(timeout),
            );
        };

        let child_rpc_id = format!("{rpc_id}:step:{}", index + 1);
        let mut step_params = step.into_params(&params.session_id);
        let step_started = Instant::now();
        let is_local = method == Method::ToolWaitMs;
        if !is_local {
            put_trace(
                &mut step_params,
                &TimingTrace {
                    run_id: parent_timing.run_id.clone(),
                    flow_name: Some(name.clone()),
                    flow_step_index: Some(index + 1),
                    agent_received_at: Some(epoch_us()),
                    // Flow owns semantic outcome recording after request_help
                    // and cancellation normalization, so child dispatch must
                    // never persist a duplicate transport-level metric.
                    skip_metric: true,
                    ..TimingTrace::default()
                },
            );
        }
        let dispatch = dispatch_step(state, child_rpc_id.clone(), method.clone(), step_params);
        tokio::pin!(dispatch);

        let body = tokio::select! {
            body = &mut dispatch => body,
            _ = token.cancelled() => {
                super::ipc::cancel_rpc(state, &child_rpc_id);
                let observed = tokio::time::timeout(Duration::from_secs(2), &mut dispatch).await.ok();
                terminal_error_with_observed_data(cancelled_error(), observed)
            }
            _ = tokio::time::sleep(remaining) => {
                super::ipc::cancel_rpc(state, &child_rpc_id);
                let observed = tokio::time::timeout(Duration::from_secs(2), &mut dispatch).await.ok();
                terminal_error_with_observed_data(timeout_error(timeout), observed)
            }
        };

        match normalize_step_body(&method, body) {
            ResponseBody::Ok(mut output) => {
                let duration_us = elapsed_us(step_started);
                let (timing, trace) = take_step_timing(&mut output, is_local, duration_us);
                if parent_timing.agent_received_at.is_some() {
                    record_step_metric(
                        &parent_timing,
                        &name,
                        index + 1,
                        &method_name,
                        &params.session_id,
                        "ok",
                        duration_us,
                        trace,
                    );
                }
                completed.push(FlowStepResult {
                    index: index + 1,
                    method: method_name,
                    duration_ms: elapsed_ms(step_started),
                    timing: timing.filter(|t| !t.is_empty()),
                    output,
                });
            }
            ResponseBody::Err(mut cause) => {
                let duration_us = elapsed_us(step_started);
                let mut output = cause.data.take().unwrap_or(Value::Null);
                let (timing, trace) = take_step_timing(&mut output, is_local, duration_us);
                if !output.is_null() {
                    cause.data = Some(output.clone());
                }
                if parent_timing.agent_received_at.is_some() {
                    record_step_metric(
                        &parent_timing,
                        &name,
                        index + 1,
                        &method_name,
                        &params.session_id,
                        "error",
                        duration_us,
                        trace,
                    );
                }
                let failed_step_result = FlowStepResult {
                    index: index + 1,
                    method: method_name.clone(),
                    duration_ms: elapsed_ms(step_started),
                    timing,
                    output,
                };
                return flow_failure(
                    &name,
                    index,
                    &method_name,
                    started,
                    completed,
                    Some(failed_step_result),
                    cause,
                );
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

fn normalize_step_body(method: &Method, body: ResponseBody) -> ResponseBody {
    if *method != Method::ToolRequestHelp {
        return body;
    }
    let ResponseBody::Ok(output) = body else {
        return body;
    };
    let result: RequestHelpResult = match serde_json::from_value(output.clone()) {
        Ok(result) => result,
        Err(error) => {
            return ResponseBody::Err(RpcError {
                code: ErrorCode::ProtocolError,
                message: format!("request_help returned an invalid result: {error}"),
                data: Some(output),
            });
        }
    };
    match result.outcome {
        HelpOutcome::Continued => ResponseBody::Ok(output),
        HelpOutcome::Cancelled => ResponseBody::Err(RpcError {
            code: ErrorCode::UserAborted,
            message: "user cancelled the Flow request_help step".into(),
            data: Some(output),
        }),
        HelpOutcome::TimedOut => ResponseBody::Err(RpcError {
            code: ErrorCode::Timeout,
            message: "Flow request_help step timed out".into(),
            data: Some(output),
        }),
        HelpOutcome::Navigated => ResponseBody::Err(RpcError {
            code: ErrorCode::Cancelled,
            message: "page navigated during the Flow request_help step".into(),
            data: Some(output),
        }),
    }
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
            Self::Select(_) => Method::ToolSelect,
            Self::WaitFor(_) => Method::ToolAssert,
            Self::RequestHelp(_) => Method::ToolRequestHelp,
            Self::Assert(_) => Method::ToolAssert,
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
            Self::Select(entry) => with_session(entry.select, session_id),
            Self::WaitFor(entry) => with_session(entry.wait_for.into_assertion(), session_id),
            Self::RequestHelp(entry) => with_session(entry.request_help, session_id),
            Self::Assert(entry) => with_session(entry.assertion, session_id),
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
    failed_step_result: Option<FlowStepResult>,
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
        failed_step_result,
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

fn elapsed_us(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}

fn take_step_timing(
    output: &mut Value,
    is_local: bool,
    duration_us: u64,
) -> (Option<StepTiming>, Option<TimingTrace>) {
    if is_local {
        return (
            Some(StepTiming {
                local_us: Some(duration_us),
                ..StepTiming::default()
            }),
            None,
        );
    }
    let mut trace = take_trace(output);
    if let Some(trace) = &mut trace {
        trace.serve_replied_at = Some(epoch_us());
    }
    if trace.is_none() {
        // Cancellation can win before the extension's response (and its
        // embedded phase trace) reaches the daemon. Preserve the duration we
        // did observe instead of returning a failed step with no Timing.
        return (
            Some(StepTiming {
                local_us: Some(duration_us),
                ..StepTiming::default()
            }),
            None,
        );
    }
    let timing = trace
        .as_ref()
        .map(|trace| StepTiming {
            queue_us: trace.queue_wait_us(),
            websocket_us: trace.websocket_us(),
            websocket_roundtrip_us: trace.websocket_roundtrip_us(),
            extension_us: trace.extension_dispatch_us(),
            extension_non_cdp_us: trace.extension_non_cdp_us(),
            cdp_us: trace.cdp_us(),
            cdp_span_us: trace.cdp_span_us(),
            ..StepTiming::default()
        })
        .filter(|timing| !timing.is_empty());
    (timing, trace)
}

fn terminal_error_with_observed_data(
    mut desired: RpcError,
    observed: Option<ResponseBody>,
) -> ResponseBody {
    desired.data = observed.and_then(|body| match body {
        ResponseBody::Ok(value) => Some(value),
        ResponseBody::Err(error) => error.data,
    });
    ResponseBody::Err(desired)
}

#[allow(clippy::too_many_arguments)]
fn record_step_metric(
    parent_timing: &TimingTrace,
    flow_name: &str,
    step_index: usize,
    method: &str,
    session_id: &str,
    outcome: &str,
    local_us: u64,
    trace: Option<TimingTrace>,
) {
    let now = epoch_us();
    let mut trace = match trace {
        Some(trace) => trace,
        None => TimingTrace {
            agent_received_at: now.checked_sub(local_us),
            local_us: Some(local_us),
            serve_replied_at: Some(now),
            ..TimingTrace::default()
        },
    };
    trace.run_id = parent_timing.run_id.clone();
    trace.flow_name = Some(flow_name.into());
    trace.flow_step_index = Some(step_index);
    trace.skip_metric = false;
    let record = MetricRecord {
        recorded_at: now,
        run_id: trace.run_id.clone(),
        flow_name: trace.flow_name.clone(),
        step_index: trace.flow_step_index,
        method: method.into(),
        outcome: outcome.into(),
        session_id: Some(session_id.into()),
        timing: trace,
    };
    if let Err(error) = append_metric(&record) {
        debug!(?error, "failed to persist local Flow step metric");
    }
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

    fn help_result(outcome: HelpOutcome) -> ResponseBody {
        ResponseBody::Ok(
            serde_json::to_value(RequestHelpResult {
                outcome,
                note: None,
                tab_id: 7,
                resolved_targets: None,
            })
            .unwrap(),
        )
    }

    #[test]
    fn request_help_continue_allows_the_flow_to_resume() {
        let body = normalize_step_body(
            &Method::ToolRequestHelp,
            help_result(HelpOutcome::Continued),
        );
        assert!(matches!(body, ResponseBody::Ok(_)));
    }

    #[test]
    fn request_help_non_continue_outcomes_stop_the_flow() {
        for (outcome, code) in [
            (HelpOutcome::Cancelled, ErrorCode::UserAborted),
            (HelpOutcome::TimedOut, ErrorCode::Timeout),
            (HelpOutcome::Navigated, ErrorCode::Cancelled),
        ] {
            let body = normalize_step_body(&Method::ToolRequestHelp, help_result(outcome));
            let ResponseBody::Err(error) = body else {
                panic!("expected request_help outcome {outcome:?} to stop the flow");
            };
            assert_eq!(error.code, code);
            assert!(error.data.is_some());
        }
    }

    #[test]
    fn malformed_request_help_result_is_a_protocol_error() {
        let body = normalize_step_body(
            &Method::ToolRequestHelp,
            ResponseBody::Ok(serde_json::json!({"outcome": "continued"})),
        );
        let ResponseBody::Err(error) = body else {
            panic!("expected malformed result to fail");
        };
        assert_eq!(error.code, ErrorCode::ProtocolError);
    }

    #[test]
    fn extracts_step_timing_from_embedded_trace() {
        let mut output = serde_json::json!({
            "tab_id": 9,
            "__tabstride_timing": {
                "serve_queue_entered_at": 100,
                "serve_queue_started_at": 150,
                "extension_sent_at": 160,
                "extension_received_at": 200,
                "cdp_started_at": 210,
                "cdp_finished_at": 350,
                "extension_replied_at": 380,
                "extension_response_sent_at": 390,
                "serve_extension_received_at": 400,
                "cdp_us": 90
            }
        });
        let timing = take_trace(&mut output).map(|trace| StepTiming {
            queue_us: trace.queue_wait_us(),
            websocket_us: trace.websocket_us(),
            websocket_roundtrip_us: trace.websocket_roundtrip_us(),
            extension_us: trace.extension_dispatch_us(),
            extension_non_cdp_us: trace.extension_non_cdp_us(),
            cdp_us: trace.cdp_us(),
            cdp_span_us: trace.cdp_span_us(),
            ..StepTiming::default()
        });
        let timing = timing.expect("timing should be present");
        assert_eq!(timing.queue_us, Some(50));
        assert_eq!(timing.websocket_us, Some(50));
        assert_eq!(timing.websocket_roundtrip_us, Some(240));
        assert_eq!(timing.extension_us, Some(180));
        assert_eq!(timing.extension_non_cdp_us, Some(90));
        assert_eq!(timing.cdp_us, Some(90));
        assert_eq!(timing.cdp_span_us, Some(140));
        // The embedded trace is removed from output.
        assert!(output.get("__tabstride_timing").is_none());
        assert_eq!(output["tab_id"], 9);
    }

    #[test]
    fn step_without_transport_trace_preserves_observed_duration() {
        let mut output = serde_json::json!({"tab_id": 3});
        let (timing, trace) = take_step_timing(&mut output, false, 17);
        assert_eq!(
            timing,
            Some(StepTiming {
                local_us: Some(17),
                ..StepTiming::default()
            })
        );
        assert!(trace.is_none());
    }
}
