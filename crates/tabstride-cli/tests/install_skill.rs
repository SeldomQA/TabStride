//! Tests for `tabstride install-skill` parsing and harness helpers.

use tabstride::skill_install::{HarnessId, interactive_candidates, parse_harness_id};
use tabstride::{Cli, Command};
use clap::Parser;

#[test]
fn parses_install_skill_flags() {
    let cli = Cli::try_parse_from([
        "tabstride",
        "install-skill",
        "--harness",
        "cursor",
        "--harness",
        "codex-internal",
        "-y",
        "--force",
    ])
    .expect("parse install-skill");
    let Command::InstallSkill(args) = cli.command else {
        panic!("expected install-skill");
    };
    assert_eq!(args.harness, vec!["cursor", "codex-internal"]);
    assert!(args.yes);
    assert!(args.force);
}

#[test]
fn parse_harness_aliases_include_codebody() {
    assert_eq!(parse_harness_id("codebody").unwrap(), HarnessId::Codebuddy);
}

#[test]
fn interactive_candidates_only_include_detected() {
    let reports = vec![
        sample_report(HarnessId::Cursor, true),
        sample_report(HarnessId::Codex, false),
    ];
    let candidates = interactive_candidates(&reports);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].id, HarnessId::Cursor);
}

fn sample_report(id: HarnessId, detected: bool) -> tabstride::skill_install::HarnessReport {
    tabstride::skill_install::HarnessReport {
        id,
        skills_dir: std::path::PathBuf::from("/tmp/skills"),
        detected,
        detection_detail: None,
        installed: false,
    }
}
