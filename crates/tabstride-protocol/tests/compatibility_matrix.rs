//! Wire-compatibility fixtures for the A-plan optional protocol additions.
//!
//! The legacy structs intentionally model the 0.2.0 payload shape before
//! merged Snapshot and page-update fields were added. Serde's default unknown
//! field handling models the JavaScript extension and older CLI consumers.

use serde::{Deserialize, Serialize};
use serde_json::json;
use tabstride_protocol::tools::{
    ClickParams, ClickResult, Locator, PageUpdateMode, SessionMode, SessionStartParams,
    SessionStartResult,
};

#[derive(Debug, Deserialize)]
struct LegacySessionStartParams {
    session_id: String,
    mode: SessionMode,
    tab: Option<String>,
}

#[derive(Debug, Serialize)]
struct LegacySessionStartParamsOut {
    session_id: String,
    mode: SessionMode,
    tab: Option<String>,
}

#[derive(Debug, Serialize)]
struct LegacySessionStartResultOut {
    attached_tab_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct LegacySessionStartResult {
    attached_tab_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct LegacyClickParams {
    session_id: String,
    target: Locator,
}

#[derive(Debug, Serialize)]
struct LegacyClickResultOut {
    tab_id: i64,
    used_target: Locator,
    x: f64,
    y: f64,
}

#[derive(Debug, Deserialize)]
struct LegacyClickResult {
    tab_id: i64,
    x: f64,
    y: f64,
}

fn button() -> Locator {
    Locator {
        role: Some("button".into()),
        name: Some("Save".into()),
        exact: Some(true),
        ..Locator::default()
    }
}

#[test]
fn new_cli_requests_remain_readable_by_legacy_extension() {
    let start = SessionStartParams {
        session_id: "compat".into(),
        browser_instance_id: None,
        mode: SessionMode::Attach,
        tab: Some("active".into()),
        tab_id: None,
        snapshot: true,
    };
    let legacy: LegacySessionStartParams =
        serde_json::from_value(serde_json::to_value(start).unwrap()).unwrap();
    assert_eq!(legacy.session_id, "compat");
    assert_eq!(legacy.mode, SessionMode::Attach);
    assert_eq!(legacy.tab.as_deref(), Some("active"));

    let click = ClickParams {
        session_id: "compat".into(),
        target: button(),
        tab_id: None,
        button: None,
        click_count: None,
        modifiers: None,
        timeout_ms: None,
        page_update: Some(PageUpdateMode::Delta),
    };
    let legacy: LegacyClickParams =
        serde_json::from_value(serde_json::to_value(click).unwrap()).unwrap();
    assert_eq!(legacy.session_id, "compat");
    assert_eq!(legacy.target, button());
}

#[test]
fn legacy_cli_requests_keep_new_extension_defaults() {
    let legacy = LegacySessionStartParamsOut {
        session_id: "compat".into(),
        mode: SessionMode::Attach,
        tab: Some("active".into()),
    };
    let current: SessionStartParams =
        serde_json::from_value(serde_json::to_value(legacy).unwrap()).unwrap();
    assert!(!current.snapshot);

    let current: ClickParams = serde_json::from_value(json!({
        "session_id": "compat",
        "target": { "role": "button", "name": "Save", "exact": true }
    }))
    .unwrap();
    assert_eq!(current.page_update, None);
}

#[test]
fn legacy_extension_results_keep_new_daemon_defaults() {
    let legacy = LegacySessionStartResultOut {
        attached_tab_id: Some(42),
    };
    let current: SessionStartResult =
        serde_json::from_value(serde_json::to_value(legacy).unwrap()).unwrap();
    assert_eq!(current.attached_tab_id, Some(42));
    assert_eq!(current.snapshot_available, None);
    assert_eq!(current.snapshot_text, None);

    let legacy = LegacyClickResultOut {
        tab_id: 42,
        used_target: button(),
        x: 10.0,
        y: 20.0,
    };
    let current: ClickResult =
        serde_json::from_value(serde_json::to_value(legacy).unwrap()).unwrap();
    assert!(!current.document_changed);
    assert!(!current.document_change_known);
    assert_eq!(current.snapshot_delta, None);
}

#[test]
fn new_extension_results_remain_readable_by_legacy_daemon_and_cli() {
    let start = SessionStartResult {
        attached_tab_id: Some(42),
        snapshot_available: Some(true),
        snapshot_text: Some("@e1 button \"Save\"".into()),
        snapshot_ref_count: 1,
        ..SessionStartResult::default()
    };
    let legacy: LegacySessionStartResult =
        serde_json::from_value(serde_json::to_value(start).unwrap()).unwrap();
    assert_eq!(legacy.attached_tab_id, Some(42));

    let current = ClickResult {
        tab_id: 42,
        used_target: button(),
        used_ref: None,
        used_selector: None,
        x: 10.0,
        y: 20.0,
        dialogs: vec![],
        document_changed: true,
        document_change_known: true,
        document_version: Some(7),
        snapshot_delta: None,
    };
    let legacy: LegacyClickResult =
        serde_json::from_value(serde_json::to_value(current).unwrap()).unwrap();
    assert_eq!(legacy.tab_id, 42);
    assert_eq!((legacy.x, legacy.y), (10.0, 20.0));
}
