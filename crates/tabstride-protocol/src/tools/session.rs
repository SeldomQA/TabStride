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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionStartResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_window_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attached_tab_id: Option<i64>,
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
        };
        let encoded = serde_json::to_value(params).unwrap();
        assert_eq!(encoded["mode"], "attach");
        assert_eq!(encoded["tab"], "active");

        let result = SessionStartResult {
            agent_window_id: None,
            attached_tab_id: Some(77),
        };
        assert_eq!(serde_json::to_value(result).unwrap()["attached_tab_id"], 77);
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
