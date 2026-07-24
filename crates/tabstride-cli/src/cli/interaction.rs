//! `tabstride click` / `tabstride fill` / `tabstride press` (M7 interaction tools).
//!
//! The compatibility `<target>` positional accepts either a snapshot ref
//! (`@e3`, `e3`) or CSS. Locator flags also support role + accessible name,
//! label, placeholder, visible text, and test id. Every locator must resolve
//! to exactly one element.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use clap::{Args, ValueEnum};
use tabstride_protocol::Method;
use tabstride_protocol::tools::{
    ClickParams, ClickResult, FillParams, FillResult, KeyModifier, Locator, MouseButton,
    PressParams, PressResult, SelectParams, SelectResult,
};

use crate::cli::dialogs::print_dialog_summaries;
use crate::cli::ensure_daemon::ensure_daemon;
use crate::cli::error::{CliError, Format};
use crate::cli::navigate::parse_timeout_ms;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CliMouseButton {
    Left,
    Middle,
    Right,
}

impl From<CliMouseButton> for MouseButton {
    fn from(v: CliMouseButton) -> Self {
        match v {
            CliMouseButton::Left => MouseButton::Left,
            CliMouseButton::Middle => MouseButton::Middle,
            CliMouseButton::Right => MouseButton::Right,
        }
    }
}

/// Decide whether a target string looks like a snapshot ref. Refs are
/// `@e<N>` or `e<N>` (decimal); anything else is treated as a CSS
/// selector.
pub(crate) fn looks_like_ref(target: &str) -> bool {
    let stripped = target.strip_prefix('@').unwrap_or(target);
    if let Some(rest) = stripped.strip_prefix('e') {
        return !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit());
    }
    false
}

/// Parse a CLI modifier list like `"ctrl,shift"` into the protocol
/// enum. Empty / blank string → empty vec.
pub(crate) fn parse_modifiers(input: &str) -> Result<Vec<KeyModifier>, String> {
    let mut out = Vec::new();
    for raw in input.split(',') {
        let m = raw.trim();
        if m.is_empty() {
            continue;
        }
        let normalised = match m.to_lowercase().as_str() {
            "alt" | "option" | "opt" => KeyModifier::Alt,
            "ctrl" | "control" => KeyModifier::Ctrl,
            "meta" | "cmd" | "command" | "super" => KeyModifier::Meta,
            "shift" => KeyModifier::Shift,
            other => return Err(format!("unknown modifier '{other}'")),
        };
        if !out.contains(&normalised) {
            out.push(normalised);
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, Args, Default)]
pub struct LocatorFlags {
    /// Locate by a snapshot ref from the latest snapshot.
    #[arg(long = "ref")]
    pub ref_: Option<String>,

    /// Locate by CSS with strict single-element matching.
    #[arg(long = "css", visible_alias = "selector")]
    pub css: Option<String>,

    /// Locate by ARIA role; requires --name.
    #[arg(long)]
    pub role: Option<String>,

    /// Accessible name paired with --role.
    #[arg(long)]
    pub name: Option<String>,

    /// Locate a control by its associated label.
    #[arg(long)]
    pub label: Option<String>,

    /// Locate by placeholder text.
    #[arg(long)]
    pub placeholder: Option<String>,

    /// Locate by visible text.
    #[arg(long)]
    pub text: Option<String>,

    /// Locate by data-testid.
    #[arg(long = "test-id")]
    pub test_id: Option<String>,

    /// Require an exact semantic string match instead of substring matching.
    #[arg(long)]
    pub exact: bool,
}

impl LocatorFlags {
    fn is_empty(&self) -> bool {
        self.ref_.is_none()
            && self.css.is_none()
            && self.role.is_none()
            && self.name.is_none()
            && self.label.is_none()
            && self.placeholder.is_none()
            && self.text.is_none()
            && self.test_id.is_none()
            && !self.exact
    }
}

fn build_locator(
    positional: Option<String>,
    flags: &LocatorFlags,
    required: bool,
) -> Result<Option<Locator>, CliError> {
    if positional.is_some() && !flags.is_empty() {
        return Err(CliError::Local(anyhow::anyhow!(
            "pass either positional <target> or one locator flag, not both"
        )));
    }
    let locator = if let Some(target) = positional {
        if looks_like_ref(&target) {
            Locator {
                ref_: Some(target),
                ..empty_locator()
            }
        } else {
            Locator {
                css: Some(target),
                ..empty_locator()
            }
        }
    } else if flags.is_empty() {
        if required {
            return Err(CliError::Local(anyhow::anyhow!(
                "missing target: pass <ref-or-css> or one of --ref/--css/--role/--label/--placeholder/--text/--test-id"
            )));
        }
        return Ok(None);
    } else {
        Locator {
            ref_: flags.ref_.clone(),
            css: flags.css.clone(),
            role: flags.role.clone(),
            name: flags.name.clone(),
            label: flags.label.clone(),
            placeholder: flags.placeholder.clone(),
            text: flags.text.clone(),
            test_id: flags.test_id.clone(),
            exact: flags.exact.then_some(true),
        }
    };
    locator
        .validate()
        .map_err(|error| CliError::Local(anyhow::anyhow!(error)))?;
    Ok(Some(locator))
}

fn empty_locator() -> Locator {
    Locator::default()
}

// ---------------------------------------------------------------------------
// tabstride click
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Args)]
pub struct ClickArgs {
    /// Compatibility target: snapshot ref (`@e3`, `e3`) or CSS.
    pub target: Option<String>,

    #[command(flatten)]
    pub locator: LocatorFlags,

    #[arg(long)]
    pub session: String,

    #[arg(long = "tab-id")]
    pub tab_id: Option<i64>,

    #[arg(long, value_enum, default_value_t = CliMouseButton::Left)]
    pub button: CliMouseButton,

    /// Number of consecutive presses (double-click = 2).
    #[arg(
        long = "click-count",
        alias = "count",
        default_value_t = 1,
        value_parser = clap::value_parser!(u32).range(1..)
    )]
    pub click_count: u32,

    /// Comma-separated modifiers (`alt,ctrl,shift,meta`).
    #[arg(long, default_value = "")]
    pub modifiers: String,

    #[arg(long, default_value = "30s", value_parser = parse_timeout_ms)]
    pub timeout: u32,
}

pub fn dispatch_click(args: ClickArgs, format: Format) -> Result<(), CliError> {
    let info = ensure_daemon().context("ensure daemon is running")?;
    let target = build_locator(args.target.clone(), &args.locator, true)?
        .expect("required locator must be present");
    let modifiers = parse_modifiers(&args.modifiers)
        .map_err(|e| CliError::Local(anyhow::anyhow!("--modifiers: {e}")))?;
    let params = ClickParams {
        session_id: args.session.clone(),
        target,
        tab_id: args.tab_id,
        button: Some(args.button.into()),
        click_count: Some(args.click_count),
        modifiers: if modifiers.is_empty() {
            None
        } else {
            Some(modifiers)
        },
        timeout_ms: Some(args.timeout),
    };
    let reply: ClickResult = call(
        info.sock_path,
        Method::ToolClick,
        params,
        "click-1",
        args.timeout,
    )?;
    match format {
        Format::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&reply)
                    .map_err(|e| CliError::Local(anyhow::anyhow!(e)))?
            );
        }
        Format::Human => {
            let target = format_locator(&reply.used_target);
            println!(
                "click ok tab={} target={target} at=({}, {})",
                reply.tab_id, reply.x, reply.y
            );
            print_dialog_summaries(&reply.dialogs);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// tabstride fill
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Args)]
pub struct FillArgs {
    /// Compatibility target: snapshot ref (`@e3`, `e3`) or CSS.
    #[arg(value_name = "TARGET")]
    pub target: Option<String>,

    /// Text to type into the element.
    #[arg(long)]
    pub value: String,

    #[command(flatten)]
    pub locator: LocatorFlags,

    #[arg(long)]
    pub session: String,

    #[arg(long = "tab-id")]
    pub tab_id: Option<i64>,

    /// Skip the default "wipe the field first" pass.
    #[arg(long)]
    pub no_clear: bool,

    #[arg(long, default_value = "30s", value_parser = parse_timeout_ms)]
    pub timeout: u32,
}

pub fn dispatch_fill(args: FillArgs, format: Format) -> Result<(), CliError> {
    let info = ensure_daemon().context("ensure daemon is running")?;
    let target = build_locator(args.target.clone(), &args.locator, true)?
        .expect("required locator must be present");
    let params = FillParams {
        session_id: args.session.clone(),
        value: args.value.clone(),
        target,
        tab_id: args.tab_id,
        clear_before: if args.no_clear { Some(false) } else { None },
        timeout_ms: Some(args.timeout),
    };
    let reply: FillResult = call(
        info.sock_path,
        Method::ToolFill,
        params,
        "fill-1",
        args.timeout,
    )?;
    match format {
        Format::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&reply)
                    .map_err(|e| CliError::Local(anyhow::anyhow!(e)))?
            );
        }
        Format::Human => {
            let target = format_locator(&reply.used_target);
            println!(
                "fill ok tab={} target={target} length={}",
                reply.tab_id, reply.value_length
            );
            print_dialog_summaries(&reply.dialogs);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// tabstride press
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Args)]
pub struct PressArgs {
    /// Key spec (`Enter`, `Ctrl+A`, `ArrowLeft`, `a`).
    pub key: String,

    #[arg(long)]
    pub session: String,

    #[arg(long = "tab-id")]
    pub tab_id: Option<i64>,

    /// Comma-separated modifiers in addition to anything baked into `<key>`.
    #[arg(long, default_value = "")]
    pub modifiers: String,

    /// Optional locator to focus before dispatching the key.
    #[command(flatten)]
    pub locator: LocatorFlags,

    /// Hold the key down for N milliseconds between keyDown and keyUp.
    #[arg(long = "hold-ms")]
    pub hold_ms: Option<u32>,

    #[arg(long, default_value = "30s", value_parser = parse_timeout_ms)]
    pub timeout: u32,
}

pub fn dispatch_press(args: PressArgs, format: Format) -> Result<(), CliError> {
    let info = ensure_daemon().context("ensure daemon is running")?;
    let modifiers = parse_modifiers(&args.modifiers)
        .map_err(|e| CliError::Local(anyhow::anyhow!("--modifiers: {e}")))?;
    let target = build_locator(None, &args.locator, false)?;
    let params = PressParams {
        session_id: args.session.clone(),
        key: args.key.clone(),
        modifiers: if modifiers.is_empty() {
            None
        } else {
            Some(modifiers)
        },
        target,
        tab_id: args.tab_id,
        hold_ms: args.hold_ms,
        timeout_ms: Some(args.timeout),
    };
    let reply: PressResult = call(
        info.sock_path,
        Method::ToolPress,
        params,
        "press-1",
        args.timeout,
    )?;
    match format {
        Format::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&reply)
                    .map_err(|e| CliError::Local(anyhow::anyhow!(e)))?
            );
        }
        Format::Human => {
            let mods = if reply.modifiers.is_empty() {
                String::new()
            } else {
                format!(
                    " modifiers=[{}]",
                    reply
                        .modifiers
                        .iter()
                        .map(modifier_label)
                        .collect::<Vec<_>>()
                        .join(",")
                )
            };
            println!(
                "press ok tab={} key={} code={}{mods}",
                reply.tab_id, reply.key, reply.code
            );
            print_dialog_summaries(&reply.dialogs);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// tabstride select
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Args)]
pub struct SelectArgs {
    /// Compatibility target: snapshot ref (`@e3`, `e3`) or CSS.
    #[arg(value_name = "TARGET")]
    pub target: Option<String>,

    /// Option `value` to select. Repeat for multi-select (`--value a --value b`).
    #[arg(long = "value", required = true)]
    pub values: Vec<String>,

    #[command(flatten)]
    pub locator: LocatorFlags,

    #[arg(long)]
    pub session: String,

    #[arg(long = "tab-id")]
    pub tab_id: Option<i64>,

    #[arg(long, default_value = "30s", value_parser = parse_timeout_ms)]
    pub timeout: u32,
}

pub fn dispatch_select(args: SelectArgs, format: Format) -> Result<(), CliError> {
    let info = ensure_daemon().context("ensure daemon is running")?;
    let target = build_locator(args.target.clone(), &args.locator, true)?
        .expect("required locator must be present");
    let params = SelectParams {
        session_id: args.session.clone(),
        values: args.values.clone(),
        target,
        tab_id: args.tab_id,
        timeout_ms: Some(args.timeout),
    };
    let reply: SelectResult = call(
        info.sock_path,
        Method::ToolSelect,
        params,
        "select-1",
        args.timeout,
    )?;
    match format {
        Format::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&reply)
                    .map_err(|e| CliError::Local(anyhow::anyhow!(e)))?
            );
        }
        Format::Human => {
            let target = format_locator(&reply.used_target);
            let values = reply.selected_values.join(",");
            let labels = reply.selected_labels.join(",");
            println!(
                "select ok tab={} target={target} multiple={} values=[{values}] labels=[{labels}]",
                reply.tab_id, reply.multiple
            );
            print_dialog_summaries(&reply.dialogs);
        }
    }
    Ok(())
}

fn format_locator(locator: &Locator) -> String {
    if let Some(value) = &locator.ref_ {
        return format!("ref={value}");
    }
    if let Some(value) = &locator.css {
        return format!("css={value}");
    }
    if let (Some(role), Some(name)) = (&locator.role, &locator.name) {
        return format!("role={role} name={name}");
    }
    for (kind, value) in [
        ("label", locator.label.as_deref()),
        ("placeholder", locator.placeholder.as_deref()),
        ("text", locator.text.as_deref()),
        ("testId", locator.test_id.as_deref()),
    ] {
        if let Some(value) = value {
            return format!("{kind}={value}");
        }
    }
    "?".into()
}

fn modifier_label(m: &KeyModifier) -> &'static str {
    match m {
        KeyModifier::Alt => "alt",
        KeyModifier::Ctrl => "ctrl",
        KeyModifier::Meta => "meta",
        KeyModifier::Shift => "shift",
    }
}

fn call<P, R>(
    sock: PathBuf,
    method: Method,
    params: P,
    id: &'static str,
    timeout_ms: u32,
) -> Result<R, CliError>
where
    P: serde::Serialize + Send + 'static,
    R: serde::de::DeserializeOwned + Send + 'static,
{
    crate::cli::business_rpc::call::<P, R>(
        sock,
        id,
        method,
        Some(params),
        interaction_ipc_timeout(timeout_ms),
    )
}

fn interaction_ipc_timeout(timeout_ms: u32) -> Duration {
    Duration::from_millis(u64::from(timeout_ms))
        .checked_add(Duration::from_secs(15))
        .unwrap_or(Duration::from_secs(u64::from(timeout_ms / 1_000) + 15))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_like_ref_handles_both_forms() {
        assert!(looks_like_ref("@e3"));
        assert!(looks_like_ref("e42"));
        assert!(!looks_like_ref("button"));
        assert!(!looks_like_ref(".btn"));
        assert!(!looks_like_ref("@"));
        assert!(!looks_like_ref("e"));
    }

    #[test]
    fn parse_modifiers_round_trips() {
        assert!(parse_modifiers("").unwrap().is_empty());
        let v = parse_modifiers("ctrl,Shift").unwrap();
        assert_eq!(v, vec![KeyModifier::Ctrl, KeyModifier::Shift]);
        let v = parse_modifiers("cmd").unwrap();
        assert_eq!(v, vec![KeyModifier::Meta]);
        let v = parse_modifiers("ctrl,ctrl").unwrap();
        assert_eq!(v, vec![KeyModifier::Ctrl]);
        assert!(parse_modifiers("garbage").is_err());
    }

    #[test]
    fn build_locator_picks_ref_path_for_ref_strings() {
        let locator = build_locator(Some("@e3".into()), &LocatorFlags::default(), true)
            .unwrap()
            .unwrap();
        assert_eq!(locator.ref_.as_deref(), Some("@e3"));
    }

    #[test]
    fn build_locator_picks_css_for_anything_else() {
        let locator = build_locator(Some(".btn".into()), &LocatorFlags::default(), true)
            .unwrap()
            .unwrap();
        assert_eq!(locator.css.as_deref(), Some(".btn"));
    }

    #[test]
    fn build_locator_accepts_role_name_and_exact() {
        let flags = LocatorFlags {
            role: Some("button".into()),
            name: Some("Save".into()),
            exact: true,
            ..LocatorFlags::default()
        };
        let locator = build_locator(None, &flags, true).unwrap().unwrap();
        assert_eq!(locator.role.as_deref(), Some("button"));
        assert_eq!(locator.name.as_deref(), Some("Save"));
        assert_eq!(locator.exact, Some(true));
    }

    #[test]
    fn build_locator_rejects_missing_role_name_and_multiple_strategies() {
        let missing_name = LocatorFlags {
            role: Some("button".into()),
            ..LocatorFlags::default()
        };
        assert!(build_locator(None, &missing_name, true).is_err());

        let multiple = LocatorFlags {
            label: Some("Email".into()),
            placeholder: Some("name@example.com".into()),
            ..LocatorFlags::default()
        };
        assert!(build_locator(None, &multiple, true).is_err());
    }

    #[test]
    fn build_locator_rejects_positional_target_with_locator_flags() {
        let flags = LocatorFlags {
            text: Some("Save".into()),
            ..LocatorFlags::default()
        };
        assert!(build_locator(Some("button".into()), &flags, true).is_err());
    }

    #[test]
    fn interaction_ipc_timeout_tracks_user_timeout_with_grace() {
        assert_eq!(
            interaction_ipc_timeout(60_000),
            Duration::from_secs(60) + Duration::from_secs(15)
        );
    }

    #[test]
    fn fill_clap_accepts_target_positional_with_value_flag() {
        use clap::Parser;

        #[derive(Parser)]
        #[command(name = "fill")]
        struct Wrapper {
            #[command(flatten)]
            args: FillArgs,
        }

        let parsed =
            Wrapper::try_parse_from(["fill", "@e138", "--value", "deepseek", "--session", "ohli"])
                .expect("fill args should parse");
        assert_eq!(parsed.args.target.as_deref(), Some("@e138"));
        assert_eq!(parsed.args.value, "deepseek");
        assert_eq!(parsed.args.session, "ohli");
    }

    #[test]
    fn select_clap_accepts_target_positional_with_repeated_value_flags() {
        use clap::Parser;

        #[derive(Parser)]
        #[command(name = "select")]
        struct Wrapper {
            #[command(flatten)]
            args: SelectArgs,
        }

        let parsed = Wrapper::try_parse_from([
            "select",
            "@e138",
            "--value",
            "us",
            "--value",
            "ca",
            "--session",
            "ohli",
        ])
        .expect("select args should parse");
        assert_eq!(parsed.args.target.as_deref(), Some("@e138"));
        assert_eq!(parsed.args.values, vec!["us", "ca"]);
        assert_eq!(parsed.args.session, "ohli");
    }
}
