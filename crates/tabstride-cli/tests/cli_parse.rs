//! Smoke tests for clap-derive command parsing.

use std::time::Duration;

use clap::Parser;
use tabstride::cli::daemon::{DaemonCmd, parse_duration};
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
