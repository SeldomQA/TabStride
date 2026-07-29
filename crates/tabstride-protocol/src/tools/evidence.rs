use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tools::{ConsoleEntry, Locator};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FailureActionabilityAttempt {
    pub attempt: u32,
    pub elapsed_ms: u64,
    pub match_count: u32,
    pub failed_check: String,
    pub state: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
pub struct FailureTiming {
    pub locator_ms: u64,
    pub wait_ms: u64,
    pub cdp_ms: u64,
    pub evidence_ms: u64,
    pub total_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FailureSnapshot {
    pub text: String,
    pub ref_count: u32,
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FailureScreenshot {
    pub image_base64: String,
    pub width: u32,
    pub height: u32,
    pub format: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FailureEvidence {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locator: Option<Locator>,
    pub match_count: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actionability_history: Vec<FailureActionabilityAttempt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_failed_check: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<FailureSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<FailureScreenshot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent_console_errors: Vec<ConsoleEntry>,
    pub timing: FailureTiming,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collection_errors: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_evidence_round_trips_with_artifacts() {
        let evidence = FailureEvidence {
            locator: Some(Locator {
                css: Some("#save".into()),
                ..Locator::default()
            }),
            match_count: 1,
            actionability_history: vec![FailureActionabilityAttempt {
                attempt: 1,
                elapsed_ms: 25,
                match_count: 1,
                failed_check: "receives_events".into(),
                state: serde_json::json!({ "visible": true, "receives_events": false }),
            }],
            last_failed_check: Some("receives_events".into()),
            current_url: Some("https://example.test/".into()),
            snapshot: Some(FailureSnapshot {
                text: "RootWebArea".into(),
                ref_count: 0,
                truncated: false,
            }),
            screenshot: Some(FailureScreenshot {
                image_base64: "iVBORw0".into(),
                width: 1,
                height: 1,
                format: "png".into(),
            }),
            recent_console_errors: vec![],
            timing: FailureTiming {
                locator_ms: 2,
                wait_ms: 20,
                cdp_ms: 3,
                evidence_ms: 5,
                total_ms: 30,
            },
            collection_errors: vec![],
        };
        let value = serde_json::to_value(&evidence).unwrap();
        let round_trip: FailureEvidence = serde_json::from_value(value).unwrap();
        assert_eq!(round_trip, evidence);
    }
}
