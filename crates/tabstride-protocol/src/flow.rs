//! Declarative Flow v1 protocol types.
//!
//! Flow deliberately composes existing tool RPCs. It does not expose arbitrary
//! JavaScript or a second browser execution surface.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tools::{KeyModifier, Locator, WaitUntil};
use crate::{ErrorCode, RpcError};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlowDefinition {
    pub name: String,
    /// Total timeout. Accepts `250ms`, `30s`, `2m`, or an integer number of ms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    pub steps: Vec<FlowStep>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum FlowStep {
    Navigate(FlowNavigateEntry),
    Click(FlowClickEntry),
    Fill(FlowFillEntry),
    Press(FlowPressEntry),
    Snapshot(FlowSnapshotEntry),
    WaitMs(FlowWaitMsEntry),
}

macro_rules! flow_entry {
    ($name:ident, $field:ident, $step:ty) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
        #[serde(deny_unknown_fields)]
        pub struct $name {
            pub $field: $step,
        }
    };
}

flow_entry!(FlowNavigateEntry, navigate, FlowNavigateStep);
flow_entry!(FlowClickEntry, click, FlowClickStep);
flow_entry!(FlowFillEntry, fill, FlowFillStep);
flow_entry!(FlowPressEntry, press, FlowPressStep);
flow_entry!(FlowSnapshotEntry, snapshot, FlowSnapshotStep);
flow_entry!(FlowWaitMsEntry, wait_ms, FlowWaitMsStep);

pub type FlowTarget = Locator;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlowNavigateStep {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_until: Option<WaitUntil>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlowClickStep {
    pub target: FlowTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlowFillStep {
    pub target: FlowTarget,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clear_before: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlowPressStep {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<FlowTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modifiers: Option<Vec<KeyModifier>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlowSnapshotStep {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlowWaitMsStep {
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlowRunParams {
    pub session_id: String,
    pub flow: FlowDefinition,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FlowStepResult {
    pub index: usize,
    pub method: String,
    pub duration_ms: u64,
    pub output: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FlowRunResult {
    pub name: String,
    pub duration_ms: u64,
    pub completed_steps: Vec<FlowStepResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FlowFailureData {
    pub flow_name: String,
    pub failed_step: usize,
    pub failed_method: String,
    pub duration_ms: u64,
    pub completed_steps: Vec<FlowStepResult>,
    pub cause: RpcError,
}

impl FlowDefinition {
    pub fn validate(&self) -> Result<(), RpcError> {
        if self.name.trim().is_empty() {
            return Err(invalid("flow name must not be empty"));
        }
        if self.steps.is_empty() {
            return Err(invalid("flow must contain at least one step"));
        }
        for (index, step) in self.steps.iter().enumerate() {
            let target = match step {
                FlowStep::Click(entry) => Some(&entry.click.target),
                FlowStep::Fill(entry) => Some(&entry.fill.target),
                FlowStep::Press(entry) => entry.press.target.as_ref(),
                _ => None,
            };
            if let Some(target) = target {
                target
                    .validate()
                    .map_err(|message| invalid(format!("step {}: {message}", index + 1)))?;
            }
        }
        Ok(())
    }
}

fn invalid(message: impl Into<String>) -> RpcError {
    RpcError {
        code: ErrorCode::InvalidParams,
        message: message.into(),
        data: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_shape_and_validation() {
        let flow: FlowDefinition = serde_yaml::from_str(
            r#"name: demo
timeout: 30s
steps:
  - navigate:
      url: https://example.com
  - fill:
      target: { css: input }
      value: "{{value}}"
  - press:
      key: Enter
      target:
        role: textbox
        name: Task
        exact: true
  - snapshot: {}
"#,
        )
        .unwrap();
        assert_eq!(flow.steps.len(), 4);
        flow.validate().unwrap();
        let FlowStep::Press(entry) = &flow.steps[2] else {
            panic!("expected press step");
        };
        assert_eq!(
            entry.press.target.as_ref().unwrap().role.as_deref(),
            Some("textbox")
        );
    }

    #[test]
    fn target_requires_one_strategy() {
        let target = FlowTarget {
            ref_: Some("@e1".into()),
            css: Some("button".into()),
            role: None,
            name: None,
            label: None,
            placeholder: None,
            text: None,
            test_id: None,
            exact: None,
        };
        assert!(target.validate().is_err());
    }
}
