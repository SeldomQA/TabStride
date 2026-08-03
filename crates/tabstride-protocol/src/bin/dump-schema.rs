//! Emit JSON Schema files for handshake + §7 tool params/results (`schema/`).

use std::env;
use std::fs;
use std::path::PathBuf;

use schemars::schema_for;
use tabstride_protocol::system::{
    BrowserListParams, HandshakeParams, HandshakeResult, PingParams, PingResult, StatusParams,
    StatusResult,
};
use tabstride_protocol::tools::*;
use tabstride_protocol::{
    CancelParams, CancelResult, FlowDefinition, FlowFailureData, FlowRunParams, FlowRunResult,
};

fn write_schema(name: &str, schema: impl serde::Serialize, check: bool) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("schema");
    fs::create_dir_all(&dir).expect("create schema dir");
    let path = dir.join(format!("{name}.json"));
    let json = serde_json::to_string_pretty(&schema).expect("serialize schema");
    let mut json = json;
    json.push('\n');
    if check {
        let existing = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {} for schema drift check: {e}", path.display()));
        assert_eq!(
            existing,
            json,
            "JSON Schema drift detected in {}; run `cargo run -p tabstride-protocol --bin dump-schema --locked`",
            path.display()
        );
    } else {
        fs::write(&path, json).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    }
}

macro_rules! dump {
    ($ty:ty, $file:literal, $check:expr) => {
        write_schema($file, schema_for!($ty), $check);
    };
}

fn main() {
    let mut args = env::args().skip(1);
    let check = match args.next().as_deref() {
        None => false,
        Some("--check") => true,
        Some(arg) => panic!("unknown argument `{arg}`; expected `--check`"),
    };
    assert!(args.next().is_none(), "expected at most one argument");

    macro_rules! emit {
        ($ty:ty, $file:literal) => {
            dump!($ty, $file, check);
        };
    }

    emit!(HandshakeParams, "handshake_params");
    emit!(HandshakeResult, "handshake_result");

    emit!(PingParams, "system_ping_params");
    emit!(PingResult, "system_ping_result");
    emit!(StatusParams, "system_status_params");
    emit!(StatusResult, "system_status_result");
    emit!(BrowserListParams, "browser_list_params");

    emit!(CancelParams, "cancel_params");
    emit!(CancelResult, "cancel_result");

    emit!(SessionStartParams, "tool_session_start_params");
    emit!(SessionStartResult, "tool_session_start_result");
    emit!(SessionStopParams, "tool_session_stop_params");
    emit!(SessionStopResult, "tool_session_stop_result");

    emit!(TabListParams, "tool_tab_list_params");
    emit!(TabListResult, "tool_tab_list_result");
    emit!(TabCreateParams, "tool_tab_create_params");
    emit!(TabCreateResult, "tool_tab_create_result");
    emit!(TabCloseParams, "tool_tab_close_params");
    emit!(TabCloseResult, "tool_tab_close_result");
    emit!(TabBorrowParams, "tool_tab_borrow_params");
    emit!(TabBorrowResult, "tool_tab_borrow_result");
    emit!(TabReturnParams, "tool_tab_return_params");
    emit!(TabReturnResult, "tool_tab_return_result");
    emit!(TabSelectParams, "tool_tab_select_params");
    emit!(TabSelectResult, "tool_tab_select_result");

    emit!(NavigateParams, "tool_navigate_params");
    emit!(NavigateResult, "tool_navigate_result");
    emit!(NavigateBackParams, "tool_navigate_back_params");
    emit!(NavigateBackResult, "tool_navigate_back_result");
    emit!(NavigateForwardParams, "tool_navigate_forward_params");
    emit!(NavigateForwardResult, "tool_navigate_forward_result");
    emit!(ReloadParams, "tool_reload_params");
    emit!(ReloadResult, "tool_reload_result");

    emit!(ClickParams, "tool_click_params");
    emit!(ClickResult, "tool_click_result");
    emit!(FillParams, "tool_fill_params");
    emit!(FillResult, "tool_fill_result");
    emit!(PressParams, "tool_press_params");
    emit!(PressResult, "tool_press_result");
    emit!(SelectParams, "tool_select_params");
    emit!(SelectResult, "tool_select_result");
    emit!(AssertionSpec, "assertion_spec");
    emit!(AssertParams, "tool_assert_params");
    emit!(AssertResult, "tool_assert_result");
    emit!(FailureEvidence, "failure_evidence");
    emit!(Locator, "locator");

    emit!(SnapshotParams, "tool_snapshot_params");
    emit!(SnapshotResult, "tool_snapshot_result");
    emit!(GetHtmlParams, "tool_get_html_params");
    emit!(GetHtmlResult, "tool_get_html_result");
    emit!(ScreenshotParams, "tool_screenshot_params");
    emit!(ScreenshotResult, "tool_screenshot_result");
    emit!(ConsoleParams, "tool_console_params");
    emit!(ConsoleResult, "tool_console_result");
    emit!(ConsoleEntry, "tool_console_entry");
    emit!(ConsoleStackFrame, "tool_console_stack_frame");

    emit!(EvaluateParams, "tool_evaluate_params");
    emit!(EvaluateResult, "tool_evaluate_result");
    emit!(EvaluateError, "tool_evaluate_error");

    emit!(WaitForNavigationParams, "tool_wait_for_navigation_params");
    emit!(WaitForNavigationResult, "tool_wait_for_navigation_result");
    emit!(WaitMsParams, "tool_wait_ms_params");
    emit!(WaitMsResult, "tool_wait_ms_result");
    emit!(RequestHelpParams, "tool_request_help_params");
    emit!(RequestHelpResult, "tool_request_help_result");

    emit!(FlowDefinition, "flow_definition");
    emit!(FlowRunParams, "flow_run_params");
    emit!(FlowRunResult, "flow_run_result");
    emit!(FlowFailureData, "flow_failure_data");
}
