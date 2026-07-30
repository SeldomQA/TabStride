//! Web-first assertion CLI. The daemon forwards the same `tool.assert`
//! payload used by declarative Flow steps, so both entry points share the
//! extension's retry and inspection engine.

use std::time::Duration;

use anyhow::Context;
use clap::{ArgGroup, Args};
use tabstride_protocol::Method;
use tabstride_protocol::tools::{AssertParams, AssertResult, AssertionSpec};

use crate::cli::ensure_daemon::ensure_daemon;
use crate::cli::error::{CliError, Format};
use crate::cli::interaction::{LocatorFlags, build_locator};
use crate::cli::navigate::parse_timeout_ms;

#[derive(Debug, Clone, Args)]
#[command(group(
    ArgGroup::new("expectation")
        .required(true)
        .multiple(false)
        .args([
            "visible",
            "hidden",
            "attached",
            "detached",
            "text_equals",
            "text_contains",
            "value_equals",
            "enabled",
            "disabled",
            "editable",
            "checked",
            "unchecked",
            "count",
            "populated",
            "url_equals",
            "url_matches",
        ])
))]
pub struct AssertArgs {
    /// Compatibility target: snapshot ref (`@e3`, `e3`) or CSS.
    #[arg(value_name = "TARGET")]
    pub target: Option<String>,

    #[command(flatten)]
    pub locator: LocatorFlags,

    #[arg(long)]
    pub session: String,

    #[arg(long = "tab-id")]
    pub tab_id: Option<i64>,

    /// Assert that the target is visible.
    #[arg(long)]
    pub visible: bool,

    /// Assert that the target is absent or hidden.
    #[arg(long)]
    pub hidden: bool,

    /// Assert that exactly one target is attached to the document.
    #[arg(long)]
    pub attached: bool,

    /// Assert that no target is attached to the document.
    #[arg(long)]
    pub detached: bool,

    /// Assert normalized visible text equality.
    #[arg(long = "text-equals")]
    pub text_equals: Option<String>,

    /// Assert normalized visible text contains this value.
    #[arg(long = "text-contains")]
    pub text_contains: Option<String>,

    /// Assert the target's current form value.
    #[arg(long = "value-equals")]
    pub value_equals: Option<String>,

    #[arg(long)]
    pub enabled: bool,

    #[arg(long)]
    pub disabled: bool,

    /// Assert that the target accepts user input.
    #[arg(long)]
    pub editable: bool,

    #[arg(long)]
    pub checked: bool,

    #[arg(long)]
    pub unchecked: bool,

    /// Assert the number of locator matches. Count assertions are not strict.
    #[arg(long)]
    pub count: Option<u32>,

    /// Assert that the target's form value is non-empty.
    #[arg(long)]
    pub populated: bool,

    #[arg(long = "url-equals")]
    pub url_equals: Option<String>,

    /// Assert the current URL with a JavaScript regular expression.
    #[arg(long = "url-matches")]
    pub url_matches: Option<String>,

    #[arg(long, default_value = "30s", value_parser = parse_timeout_ms)]
    pub timeout: u32,
}

pub fn dispatch(args: AssertArgs, format: Format) -> Result<(), CliError> {
    let info = ensure_daemon().context("ensure daemon is running")?;
    let url_assertion = args.url_equals.is_some() || args.url_matches.is_some();
    let target = build_locator(args.target.clone(), &args.locator, !url_assertion)?;
    let assertion = AssertionSpec {
        target,
        tab_id: args.tab_id,
        visible: args.visible.then_some(true),
        hidden: args.hidden.then_some(true),
        attached: args.attached.then_some(true),
        detached: args.detached.then_some(true),
        text_equals: args.text_equals,
        text_contains: args.text_contains,
        value_equals: args.value_equals,
        enabled: args.enabled.then_some(true),
        disabled: args.disabled.then_some(true),
        editable: args.editable.then_some(true),
        checked: args.checked.then_some(true),
        unchecked: args.unchecked.then_some(true),
        count: args.count,
        populated: args.populated.then_some(true),
        url_equals: args.url_equals,
        url_matches: args.url_matches,
        timeout_ms: Some(args.timeout),
    };
    assertion
        .validate()
        .map_err(|error| CliError::Local(anyhow::anyhow!(error)))?;

    let params = AssertParams {
        session_id: args.session,
        assertion,
    };
    let reply: AssertResult = crate::cli::business_rpc::call(
        info.sock_path,
        "assert",
        Method::ToolAssert,
        Some(params),
        Duration::from_millis(u64::from(args.timeout))
            .checked_add(Duration::from_secs(15))
            .unwrap_or(Duration::from_secs(45)),
    )?;

    match format {
        Format::Json => println!(
            "{}",
            serde_json::to_string_pretty(&reply)
                .map_err(|error| CliError::Local(anyhow::anyhow!(error)))?
        ),
        Format::Human => println!(
            "assert ok tab={} assertion={} actual={} elapsed={}ms",
            reply.tab_id, reply.assertion, reply.actual, reply.elapsed_ms
        ),
    }
    Ok(())
}
