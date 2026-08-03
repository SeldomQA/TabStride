//! Smoke tests for clap-derive command parsing.

use std::time::Duration;

use clap::Parser;
use tabstride::cli::client::ClientTransport;
use tabstride::cli::daemon::{DaemonCmd, parse_duration};
use tabstride::cli::flow::FlowCmd;
use tabstride::cli::interaction::CliPageUpdate;
use tabstride::cli::metrics::MetricsCmd;
use tabstride::cli::navigate::NavigateCmd;
use tabstride::cli::session::{CliSessionMode, CliTabTarget, SessionSub};
use tabstride::daemon::DaemonConfig;
use tabstride::{Cli, Command};

fn parse(args: &[&str]) -> Cli {
    Cli::try_parse_from(args).expect("clap parse should succeed")
}

#[test]
fn parses_daemon_start_with_defaults() {
    let cli = parse(&["tabstride", "daemon", "start"]);
    let Command::Daemon(DaemonCmd::Start(args)) = cli.command else {
        panic!("expected daemon start subcommand");
    };
    assert!(args.port.is_none());
    assert!(!args.foreground);
    assert_eq!(args.resolved_port(), 52800);
    assert_eq!(args.resolved_daemon_idle(), Duration::from_secs(600));
}

#[test]
fn parses_top_level_serve_with_defaults_and_flags() {
    let cli = parse(&["tabstride", "serve"]);
    let Command::Serve(args) = cli.command else {
        panic!("expected serve command");
    };
    assert_eq!(args.resolved_port(), 52800);
    assert_eq!(args.resolved_session_idle(), Duration::from_secs(300));
    assert_eq!(DaemonConfig::from(&args).daemon_idle, None);

    let cli = parse(&[
        "tabstride",
        "serve",
        "--port",
        "52900",
        "--session-idle",
        "30s",
    ]);
    let Command::Serve(args) = cli.command else {
        panic!("expected serve command");
    };
    assert_eq!(args.resolved_port(), 52900);
    assert_eq!(args.resolved_session_idle(), Duration::from_secs(30));
}

#[test]
fn parses_persistent_native_client_timeout() {
    let cli = parse(&["tabstride", "client", "--timeout", "2m"]);
    let Command::Client(args) = cli.command else {
        panic!("expected client command");
    };
    assert_eq!(args.timeout, Duration::from_secs(120));
}

#[test]
fn parses_skill_execution_path_commands() {
    let cli = parse(&[
        "tabstride",
        "session",
        "start",
        "--mode",
        "attach",
        "--tab",
        "active",
        "--snapshot",
    ]);
    let Command::Session(command) = cli.command else {
        panic!("expected session command");
    };
    assert!(matches!(command.sub, SessionSub::Start(_)));

    let cli = parse(&[
        "tabstride",
        "--json",
        "flow",
        "run",
        "task.yaml",
        "--session",
        "abcd",
        "--var",
        "task=write-code",
    ]);
    assert!(cli.flags.json);
    assert!(matches!(cli.command, Command::Flow(FlowCmd::Run(_))));

    let cli = parse(&[
        "tabstride",
        "client",
        "--transport",
        "websocket",
        "--timeout",
        "35s",
    ]);
    let Command::Client(args) = cli.command else {
        panic!("expected persistent client");
    };
    assert_eq!(args.transport, ClientTransport::Websocket);

    let cli = parse(&["tabstride", "session", "stop", "abcd"]);
    let Command::Session(command) = cli.command else {
        panic!("expected session command");
    };
    assert!(matches!(command.sub, SessionSub::Stop(_)));
}

#[test]
fn parses_flow_validate_and_run() {
    let cli = parse(&["tabstride", "flow", "validate", "demo.yaml"]);
    assert!(matches!(cli.command, Command::Flow(FlowCmd::Validate(_))));

    let cli = parse(&[
        "tabstride",
        "flow",
        "run",
        "demo.yaml",
        "--session",
        "abcd",
        "--var",
        "task=write-code",
    ]);
    let Command::Flow(FlowCmd::Run(args)) = cli.command else {
        panic!("expected flow run command");
    };
    assert_eq!(args.session, "abcd");
    assert_eq!(args.variables, vec![("task".into(), "write-code".into())]);
}

#[test]
fn parses_timing_and_metrics_commands() {
    let cli = parse(&[
        "tabstride",
        "flow",
        "run",
        "demo.yaml",
        "--session",
        "abcd",
        "--timing",
        "--run-id",
        "todo-baseline-001",
    ]);
    assert!(cli.flags.timing);
    assert_eq!(cli.flags.run_id.as_deref(), Some("todo-baseline-001"));
    assert!(matches!(cli.command, Command::Flow(FlowCmd::Run(_))));

    let cli = parse(&[
        "tabstride",
        "metrics",
        "summary",
        "--method",
        "tool.click",
        "--run-id",
        "todo-baseline-001",
        "--flow",
        "todo-demo",
        "--step-index",
        "3",
    ]);
    let Command::Metrics(MetricsCmd::Summary(filter)) = cli.command else {
        panic!("expected metrics summary");
    };
    assert_eq!(filter.flow.as_deref(), Some("todo-demo"));
    assert_eq!(filter.step_index, Some(3));
    let cli = parse(&["tabstride", "metrics", "export", "--out", "metrics.json"]);
    assert!(matches!(
        cli.command,
        Command::Metrics(MetricsCmd::Export(_))
    ));
}

#[test]
fn parses_page_update_for_every_interaction_command() {
    let cli = parse(&[
        "tabstride",
        "click",
        "@e1",
        "--session",
        "abcd",
        "--page-update",
        "delta",
    ]);
    let Command::Click(args) = cli.command else {
        panic!("expected click command");
    };
    assert_eq!(args.page_update, CliPageUpdate::Delta);

    let cli = parse(&[
        "tabstride",
        "fill",
        "@e1",
        "--value",
        "hello",
        "--session",
        "abcd",
        "--page-update",
        "none",
    ]);
    let Command::Fill(args) = cli.command else {
        panic!("expected fill command");
    };
    assert_eq!(args.page_update, CliPageUpdate::None);

    let cli = parse(&[
        "tabstride",
        "press",
        "Enter",
        "--session",
        "abcd",
        "--page-update",
        "signal",
    ]);
    let Command::Press(args) = cli.command else {
        panic!("expected press command");
    };
    assert_eq!(args.page_update, CliPageUpdate::Signal);

    let cli = parse(&[
        "tabstride",
        "select",
        "@e1",
        "--value",
        "SG",
        "--session",
        "abcd",
        "--page-update",
        "delta",
    ]);
    let Command::Select(args) = cli.command else {
        panic!("expected select command");
    };
    assert_eq!(args.page_update, CliPageUpdate::Delta);

    assert!(
        Cli::try_parse_from([
            "tabstride",
            "click",
            "@e1",
            "--session",
            "abcd",
            "--page-update",
            "invalid",
        ])
        .is_err()
    );
}

#[test]
fn parses_web_first_assertions() {
    let cli = parse(&[
        "tabstride",
        "assert",
        "--session",
        "abcd",
        "--text",
        "Write code",
        "--exact",
        "--visible",
        "--timeout",
        "5s",
    ]);
    let Command::Assert(args) = cli.command else {
        panic!("expected assert command");
    };
    assert!(args.visible);
    assert_eq!(args.locator.text.as_deref(), Some("Write code"));
    assert!(args.locator.exact);
    assert_eq!(args.timeout, 5_000);

    let cli = parse(&[
        "tabstride",
        "assert",
        "--session",
        "abcd",
        "--css",
        "#result",
        "--detached",
    ]);
    let Command::Assert(args) = cli.command else {
        panic!("expected assert command");
    };
    assert!(args.detached);

    assert!(
        Cli::try_parse_from([
            "tabstride",
            "assert",
            "--session",
            "abcd",
            "--css",
            "#save",
            "--visible",
            "--enabled",
        ])
        .is_err()
    );
}

#[test]
fn parses_daemon_start_with_flags() {
    let cli = parse(&[
        "tabstride",
        "daemon",
        "start",
        "--foreground",
        "--port",
        "52900",
        "--daemon-idle",
        "2s",
        "--session-idle",
        "30s",
    ]);
    let Command::Daemon(DaemonCmd::Start(args)) = cli.command else {
        panic!("expected daemon start subcommand");
    };
    assert!(args.foreground);
    assert_eq!(args.resolved_port(), 52900);
    assert_eq!(args.resolved_daemon_idle(), Duration::from_secs(2));
    assert_eq!(args.resolved_session_idle(), Duration::from_secs(30));
}

#[test]
fn parses_daemon_stop_and_restart() {
    let cli = parse(&["tabstride", "daemon", "stop"]);
    assert!(matches!(cli.command, Command::Daemon(DaemonCmd::Stop)));

    let cli = parse(&["tabstride", "daemon", "restart", "--foreground"]);
    let Command::Daemon(DaemonCmd::Restart(args)) = cli.command else {
        panic!("expected daemon restart subcommand");
    };
    assert!(args.foreground);
}

#[test]
fn parses_top_level_status_and_doctor() {
    let cli = parse(&["tabstride", "status"]);
    assert!(matches!(cli.command, Command::Status));

    let cli = parse(&["tabstride", "doctor"]);
    assert!(matches!(cli.command, Command::Doctor));
}

#[test]
fn top_level_help_hides_deprecated_daemon_commands() {
    let help = Cli::try_parse_from(["tabstride", "--help"])
        .unwrap_err()
        .to_string();
    assert!(help.contains("serve"));
    assert!(!help.contains("daemon  "));
}

#[test]
fn parses_console_command_with_context_safety_flags() {
    let cli = parse(&[
        "tabstride",
        "console",
        "--session",
        "s1",
        "--tab-id",
        "9",
        "--since",
        "12",
        "--limit",
        "75",
        "--max-text-chars",
        "2048",
        "--include-stack",
    ]);
    let Command::Console(args) = cli.command else {
        panic!("expected console command");
    };
    assert_eq!(args.session, "s1");
    assert_eq!(args.tab_id, Some(9));
    assert_eq!(args.since, Some(12));
    assert_eq!(args.limit, Some(75));
    assert_eq!(args.max_text_chars, Some(2048));
    assert!(args.include_stack);
}

#[test]
fn rejects_zero_console_bounds() {
    assert!(
        Cli::try_parse_from(["tabstride", "console", "--session", "s1", "--limit", "0"]).is_err()
    );
    assert!(
        Cli::try_parse_from([
            "tabstride",
            "console",
            "--session",
            "s1",
            "--max-text-chars",
            "0"
        ])
        .is_err()
    );
}

#[test]
fn parses_install_skill_subcommand() {
    let cli = parse(&["tabstride", "install-skill", "--list"]);
    assert!(matches!(cli.command, Command::InstallSkill(_)));
}

#[test]
fn parses_attach_session_targeting_active_tab() {
    let cli = parse(&[
        "tabstride",
        "session",
        "start",
        "--mode",
        "attach",
        "--tab",
        "active",
    ]);
    let Command::Session(cmd) = cli.command else {
        panic!("expected session command");
    };
    let SessionSub::Start(args) = cmd.sub else {
        panic!("expected session start");
    };
    assert_eq!(args.mode, CliSessionMode::Attach);
    assert_eq!(args.tab, Some(CliTabTarget::Active));
    assert_eq!(args.tab_id, None);
}

#[test]
fn parses_attach_session_targeting_explicit_tab_id() {
    let cli = parse(&[
        "tabstride",
        "session",
        "start",
        "--mode",
        "attach",
        "--tab-id",
        "77",
    ]);
    let Command::Session(cmd) = cli.command else {
        panic!("expected session command");
    };
    let SessionSub::Start(args) = cmd.sub else {
        panic!("expected session start");
    };
    assert_eq!(args.mode, CliSessionMode::Attach);
    assert_eq!(args.tab_id, Some(77));
}

#[test]
fn parses_update_subcommand_with_flags() {
    let cli = parse(&[
        "tabstride",
        "update",
        "--check",
        "--yes",
        "--no-restart-daemon",
    ]);
    let Command::Update(args) = cli.command else {
        panic!("expected update subcommand");
    };
    assert!(args.check);
    assert!(args.yes);
    assert!(args.no_restart_daemon);
}

#[test]
fn duration_parser_accepts_units() {
    assert_eq!(parse_duration("750ms").unwrap(), Duration::from_millis(750));
    assert_eq!(parse_duration("2m").unwrap(), Duration::from_secs(120));
}

#[test]
fn parses_nested_navigate_back_and_forward() {
    let cli = parse(&["tabstride", "navigate", "back", "--session", "s1"]);
    let Command::Navigate(cmd) = cli.command else {
        panic!("expected navigate command");
    };
    assert!(matches!(cmd.command, Some(NavigateCmd::Back(_))));

    let cli = parse(&["tabstride", "navigate", "forward", "--session", "s1"]);
    let Command::Navigate(cmd) = cli.command else {
        panic!("expected navigate command");
    };
    assert!(matches!(cmd.command, Some(NavigateCmd::Forward(_))));
}

#[test]
fn parses_click_count_alias() {
    let cli = parse(&[
        "tabstride",
        "click",
        "@e1",
        "--session",
        "s1",
        "--count",
        "2",
    ]);
    let Command::Click(args) = cli.command else {
        panic!("expected click command");
    };
    assert_eq!(args.click_count, 2);
}

#[test]
fn parses_semantic_locator_flags() {
    let cli = parse(&[
        "tabstride",
        "click",
        "--role",
        "button",
        "--name",
        "Save",
        "--exact",
        "--session",
        "abcd",
    ]);
    let Command::Click(args) = cli.command else {
        panic!("expected click command");
    };
    assert_eq!(args.locator.role.as_deref(), Some("button"));
    assert_eq!(args.locator.name.as_deref(), Some("Save"));
    assert!(args.locator.exact);

    let cli = parse(&[
        "tabstride",
        "fill",
        "--label",
        "Email",
        "--value",
        "user@example.com",
        "--session",
        "abcd",
    ]);
    let Command::Fill(args) = cli.command else {
        panic!("expected fill command");
    };
    assert_eq!(args.locator.label.as_deref(), Some("Email"));
}

#[test]
fn rejects_zero_click_count() {
    assert!(
        Cli::try_parse_from([
            "tabstride",
            "click",
            "@e1",
            "--session",
            "s1",
            "--count",
            "0"
        ])
        .is_err()
    );
}
