//! Session-scoped tools (`tool.session_*`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ErrorCode;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionMode {
    #[default]
    Isolated,
    Attach,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionStartParams {
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser_instance_id: Option<String>,
    #[serde(default)]
    pub mode: SessionMode,
    /// `active` for the last-focused user window. Kept separate from
    /// `tab_id` so future selectors can be added without overloading ids.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<i64>,
    /// When `true`, the extension will capture an initial accessibility
    /// snapshot of the attached/Agent Window tab and return it alongside
    /// the session metadata so the agent can skip a separate
    /// `tool.snapshot` round-trip (A-2: merged attach+snapshot).
    #[serde(default)]
    pub snapshot: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionStartResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_window_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attached_tab_id: Option<i64>,
    /// Tab URL at session creation time (only populated when
    /// `snapshot` was requested).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Tab title at session creation time (only populated when
    /// `snapshot` was requested).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// CDP document version at session creation time (lets the agent
    /// track when a page has changed without a full re-snapshot).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_version: Option<u64>,
    /// Indented aria-snapshot text captured during session creation
    /// (only populated when `snapshot` was requested and the tab has
    /// a loaded page).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_text: Option<String>,
    /// Number of `@e<N>` refs registered for this session by the
    /// initial snapshot.
    #[serde(default)]
    pub snapshot_ref_count: u32,
    /// Whether the initial snapshot was truncated by depth/token caps.
    #[serde(default)]
    pub snapshot_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionStopParams {
    pub session_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReturnFailure {
    pub tab_id: i64,
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionStopResult {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub returned_tab_ids: Vec<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub return_failures: Vec<ReturnFailure>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn session_start_defaults_to_isolated_for_wire_compatibility() {
        let params: SessionStartParams = serde_json::from_value(json!({
            "session_id": "abcd"
        }))
        .unwrap();
        assert_eq!(params.mode, SessionMode::Isolated);
        assert_eq!(params.tab, None);
        assert_eq!(params.tab_id, None);
    }

    #[test]
    fn attach_session_round_trips_target_and_result() {
        let params = SessionStartParams {
            session_id: "abcd".into(),
            browser_instance_id: Some("browser-1".into()),
            mode: SessionMode::Attach,
            tab: Some("active".into()),
            tab_id: None,
            snapshot: false,
        };
        let encoded = serde_json::to_value(params).unwrap();
        assert_eq!(encoded["mode"], "attach");
        assert_eq!(encoded["tab"], "active");

        let result = SessionStartResult {
            agent_window_id: None,
            attached_tab_id: Some(77),
            url: None,
            title: None,
            document_version: None,
            snapshot_text: None,
            snapshot_ref_count: 0,
            snapshot_truncated: false,
        };
        assert_eq!(serde_json::to_value(result).unwrap()["attached_tab_id"], 77);
    }

    #[test]
    fn session_start_with_snapshot_round_trips() {
        let params = SessionStartParams {
            session_id: "snap".into(),
            browser_instance_id: None,
            mode: SessionMode::Attach,
            tab: Some("active".into()),
            tab_id: None,
            snapshot: true,
        };
        let encoded = serde_json::to_value(params).unwrap();
        assert_eq!(encoded["snapshot"], true);

        let result = SessionStartResult {
            agent_window_id: None,
            attached_tab_id: Some(42),
            url: Some("https://example.com".into()),
            title: Some("Example".into()),
            document_version: Some(3),
            snapshot_text: Some("root\n  @e1 heading \"Welcome\"\n".into()),
            snapshot_ref_count: 1,
            snapshot_truncated: false,
        };
        let encoded = serde_json::to_value(&result).unwrap();
        assert_eq!(encoded["url"], "https://example.com");
        assert_eq!(encoded["title"], "Example");
        assert_eq!(encoded["document_version"], 3);
        assert_eq!(
            encoded["snapshot_text"],
            "root\n  @e1 heading \"Welcome\"\n"
        );
        assert_eq!(encoded["snapshot_ref_count"], 1);
    }

    #[test]
    fn session_start_defaults_snapshot_to_false() {
        let params: SessionStartParams = serde_json::from_value(json!({
            "session_id": "abcd"
        }))
        .unwrap();
        assert!(!params.snapshot);
    }

    #[test]
    fn session_stop_result_round_trips_auto_return_payload() {
        let result: SessionStopResult = serde_json::from_value(json!({
            "returned_tab_ids": [7, 8],
            "return_failures": [
                { "tab_id": 9, "code": "cdp_failed", "message": "move failed" }
            ]
        }))
        .unwrap();

        assert_eq!(result.returned_tab_ids, vec![7, 8]);
        assert_eq!(result.return_failures[0].tab_id, 9);
        assert_eq!(result.return_failures[0].code, ErrorCode::CdpFailed);
        let encoded = serde_json::to_value(result).unwrap();
        assert_eq!(encoded["returned_tab_ids"], json!([7, 8]));
        assert_eq!(encoded["return_failures"][0]["code"], "cdp_failed");
    }
}
