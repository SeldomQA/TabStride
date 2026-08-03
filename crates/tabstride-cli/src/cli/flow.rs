//! `tabstride flow validate|run`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, anyhow};
use clap::{Args, Subcommand};
use tabstride_protocol::{FlowDefinition, FlowRunParams, FlowRunResult, Method, StepTiming};

use crate::cli::daemon::parse_duration;
use crate::cli::ensure_daemon::ensure_daemon;
use crate::cli::error::{CliError, Format};

#[derive(Debug, Clone, Subcommand)]
pub enum FlowCmd {
    /// Parse and validate a flow file without running it.
    Validate(FlowFileArgs),
    /// Submit the complete flow to the running TabStride service.
    Run(FlowRunArgs),
}

#[derive(Debug, Clone, Args)]
pub struct FlowFileArgs {
    /// YAML flow file.
    pub file: PathBuf,
}

#[derive(Debug, Clone, Args)]
pub struct FlowRunArgs {
    /// YAML flow file.
    pub file: PathBuf,
    /// Existing session id, usually created in attach mode.
    #[arg(long)]
    pub session: String,
    /// Flow variable in KEY=VALUE form. May be repeated.
    #[arg(long = "var", value_parser = parse_variable)]
    pub variables: Vec<(String, String)>,
}

pub fn dispatch(command: FlowCmd, format: Format) -> Result<(), CliError> {
    match command {
        FlowCmd::Validate(args) => validate(args, format),
        FlowCmd::Run(args) => run(args, format),
    }
}

fn validate(args: FlowFileArgs, format: Format) -> Result<(), CliError> {
    let flow = load_flow(&args.file)?;
    validate_flow(&flow)?;
    match format {
        Format::Human => println!(
            "ok    flow `{}` is valid ({} steps)",
            flow.name,
            flow.steps.len()
        ),
        Format::Json => println!(
            "{}",
            serde_json::json!({"valid": true, "name": flow.name, "steps": flow.steps.len()})
        ),
    }
    Ok(())
}

fn run(args: FlowRunArgs, format: Format) -> Result<(), CliError> {
    let flow = load_flow(&args.file)?;
    validate_flow(&flow)?;
    let call_timeout = flow
        .timeout
        .as_deref()
        .map(parse_duration)
        .transpose()
        .map_err(|error| CliError::Local(anyhow!(error)))?
        .unwrap_or(Duration::from_secs(30))
        .saturating_add(Duration::from_secs(5));
    let info = ensure_daemon().context("connect to TabStride service")?;
    let params = FlowRunParams {
        session_id: args.session,
        flow,
        variables: args.variables.into_iter().collect::<BTreeMap<_, _>>(),
    };
    let result: FlowRunResult = crate::cli::business_rpc::call(
        info.sock_path,
        "flow",
        Method::FlowRun,
        Some(params),
        call_timeout,
    )?;
    match format {
        Format::Json => println!(
            "{}",
            serde_json::to_string_pretty(&result).map_err(|error| CliError::Local(error.into()))?
        ),
        Format::Human => {
            println!(
                "flow `{}` completed: {} steps in {}ms",
                result.name,
                result.completed_steps.len(),
                result.duration_ms
            );
            for step in result.completed_steps {
                println!(
                    "ok    {:>2}  {:<24} {}ms",
                    step.index, step.method, step.duration_ms
                );
                if let Some(timing) = &step.timing {
                    println!("      {:>24}  {}", "", format_step_timing(timing));
                }
            }
        }
    }
    Ok(())
}

fn load_flow(path: &Path) -> Result<FlowDefinition, CliError> {
    let text =
        fs::read_to_string(path).with_context(|| format!("read flow file {}", path.display()))?;
    serde_yaml::from_str(&text)
        .with_context(|| format!("parse flow file {}", path.display()))
        .map_err(CliError::Local)
}

fn validate_flow(flow: &FlowDefinition) -> Result<(), CliError> {
    flow.validate().map_err(CliError::from_rpc)?;
    if let Some(timeout) = flow.timeout.as_deref() {
        let duration = parse_duration(timeout).map_err(|error| CliError::Local(anyhow!(error)))?;
        if duration.is_zero() || duration > Duration::from_secs(10 * 60) {
            return Err(CliError::Local(anyhow!(
                "flow timeout must be between 1ms and 10m"
            )));
        }
    }
    Ok(())
}

fn parse_variable(raw: &str) -> Result<(String, String), String> {
    let Some((key, value)) = raw.split_once('=') else {
        return Err("expected KEY=VALUE".into());
    };
    if key.trim().is_empty() {
        return Err("variable name must not be empty".into());
    }
    Ok((key.trim().into(), value.into()))
}

pub(crate) fn format_step_timing(timing: &StepTiming) -> String {
    let mut parts = Vec::new();
    if let Some(v) = timing.local_us {
        parts.push(format!("local={:.2}ms", v as f64 / 1000.0));
    }
    if let Some(v) = timing.queue_us {
        parts.push(format!("queue={:.2}ms", v as f64 / 1000.0));
    }
    if let Some(v) = timing.websocket_us {
        parts.push(format!("ws={:.2}ms", v as f64 / 1000.0));
    }
    if let Some(v) = timing.websocket_roundtrip_us {
        parts.push(format!("ws_rtt={:.2}ms", v as f64 / 1000.0));
    }
    if let Some(v) = timing.extension_us {
        parts.push(format!("ext={:.2}ms", v as f64 / 1000.0));
    }
    if let Some(v) = timing.extension_non_cdp_us {
        parts.push(format!("ext_non_cdp={:.2}ms", v as f64 / 1000.0));
    }
    if let Some(v) = timing.cdp_us {
        parts.push(format!("cdp={:.2}ms", v as f64 / 1000.0));
    }
    if let Some(v) = timing.cdp_span_us {
        parts.push(format!("cdp_span={:.2}ms", v as f64 / 1000.0));
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variable_keeps_equals_in_value() {
        assert_eq!(
            parse_variable("token=a=b").unwrap(),
            ("token".into(), "a=b".into())
        );
    }

    #[test]
    fn formats_step_timing_phases() {
        let timing = StepTiming {
            local_us: None,
            queue_us: Some(1500),
            websocket_us: Some(200),
            websocket_roundtrip_us: Some(4500),
            extension_us: Some(4200),
            extension_non_cdp_us: Some(1100),
            cdp_us: Some(3100),
            cdp_span_us: Some(3600),
        };
        let text = format_step_timing(&timing);
        assert_eq!(
            text,
            "queue=1.50ms ws=0.20ms ws_rtt=4.50ms ext=4.20ms ext_non_cdp=1.10ms cdp=3.10ms cdp_span=3.60ms"
        );
    }

    #[test]
    fn formats_partial_timing() {
        let timing = StepTiming {
            cdp_us: Some(900),
            ..StepTiming::default()
        };
        assert_eq!(format_step_timing(&timing), "cdp=0.90ms");
    }
}
