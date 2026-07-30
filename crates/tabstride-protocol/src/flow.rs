//! Declarative Flow v1 protocol types.
//!
//! Flow deliberately composes existing tool RPCs. It does not expose arbitrary
//! JavaScript or a second browser execution surface.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tools::{AssertionSpec, HelpTarget, KeyModifier, Locator, WaitUntil};
use crate::{ErrorCode, RpcError};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlowDefinition {
    pub name: String,
    /// Total timeout. Accepts `250ms`, `30s`, `2m`, or an integer number of ms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    pub steps: Vec<FlowStep>,
    /// Assertions that run only after every action step has succeeded.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assertions: Vec<AssertionSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum FlowStep {
    Navigate(FlowNavigateEntry),
    Click(FlowClickEntry),
    Fill(FlowFillEntry),
    Press(FlowPressEntry),
    Select(FlowSelectEntry),
    WaitFor(FlowWaitForEntry),
    RequestHelp(FlowRequestHelpEntry),
    Assert(FlowAssertEntry),
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
flow_entry!(FlowSelectEntry, select, FlowSelectStep);
flow_entry!(FlowWaitForEntry, wait_for, FlowWaitForStep);
flow_entry!(FlowRequestHelpEntry, request_help, FlowRequestHelpStep);
flow_entry!(FlowSnapshotEntry, snapshot, FlowSnapshotStep);
flow_entry!(FlowWaitMsEntry, wait_ms, FlowWaitMsStep);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlowAssertEntry {
    #[serde(rename = "assert")]
    pub assertion: AssertionSpec,
}

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
pub struct FlowSelectStep {
    pub target: FlowTarget,
    pub values: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FlowWaitForState {
    Attached,
    Detached,
    Visible,
    Hidden,
    Enabled,
    Disabled,
    Editable,
    Checked,
    Unchecked,
    Populated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlowWaitForStep {
    pub target: FlowTarget,
    pub state: FlowWaitForState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u32>,
}

impl FlowWaitForStep {
    pub fn into_assertion(self) -> AssertionSpec {
        let mut assertion = AssertionSpec {
            target: Some(self.target),
            tab_id: self.tab_id,
            timeout_ms: self.timeout_ms,
            ..AssertionSpec::default()
        };
        match self.state {
            FlowWaitForState::Attached => assertion.attached = Some(true),
            FlowWaitForState::Detached => assertion.detached = Some(true),
            FlowWaitForState::Visible => assertion.visible = Some(true),
            FlowWaitForState::Hidden => assertion.hidden = Some(true),
            FlowWaitForState::Enabled => assertion.enabled = Some(true),
            FlowWaitForState::Disabled => assertion.disabled = Some(true),
            FlowWaitForState::Editable => assertion.editable = Some(true),
            FlowWaitForState::Checked => assertion.checked = Some(true),
            FlowWaitForState::Unchecked => assertion.unchecked = Some(true),
            FlowWaitForState::Populated => assertion.populated = Some(true),
        }
        assertion
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlowRequestHelpStep {
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub targets: Option<Vec<HelpTarget>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<i64>,
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
            if let FlowStep::Assert(entry) = step {
                entry
                    .assertion
                    .validate()
                    .map_err(|message| invalid(format!("step {}: {message}", index + 1)))?;
            }
            let target = match step {
                FlowStep::Click(entry) => Some(&entry.click.target),
                FlowStep::Fill(entry) => Some(&entry.fill.target),
                FlowStep::Press(entry) => entry.press.target.as_ref(),
                FlowStep::Select(entry) => Some(&entry.select.target),
                FlowStep::WaitFor(entry) => Some(&entry.wait_for.target),
                FlowStep::Assert(_) => None,
                _ => None,
            };
            if let Some(target) = target {
                target
                    .validate()
                    .map_err(|message| invalid(format!("step {}: {message}", index + 1)))?;
            }
            if let FlowStep::RequestHelp(entry) = step {
                validate_request_help(&entry.request_help)
                    .map_err(|message| invalid(format!("step {}: {message}", index + 1)))?;
            }
        }
        for (index, assertion) in self.assertions.iter().enumerate() {
            assertion
                .validate()
                .map_err(|message| invalid(format!("final assertion {}: {message}", index + 1)))?;
        }
        Ok(())
    }
}

fn validate_request_help(step: &FlowRequestHelpStep) -> Result<(), String> {
    if step.prompt.trim().is_empty() {
        return Err("request_help requires a non-empty prompt".into());
    }
    if step.timeout_ms == Some(0) {
        return Err("request_help timeout_ms must be greater than zero".into());
    }
    for target in step.targets.iter().flatten() {
        let refs = target
            .ref_
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
        let selectors = target
            .selector
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
        if refs == selectors {
            return Err(
                "each request_help target requires exactly one non-empty ref or selector".into(),
            );
        }
    }
    Ok(())
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
            r##"name: demo
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
  - select:
      target:
        testId: country
      values: [SG]
  - wait_for:
      target:
        label: Carriers
      state: populated
      timeout_ms: 5000
  - request_help:
      prompt: Complete the captcha, then continue
      targets:
        - selector: "#captcha"
      timeout_ms: 30000
  - assert:
      target:
        text: Write code
        exact: true
      visible: true
  - snapshot: {}
assertions:
  - url_matches: "/todos$"
"##,
        )
        .unwrap();
        assert_eq!(flow.steps.len(), 8);
        assert_eq!(flow.assertions.len(), 1);
        flow.validate().unwrap();
        let FlowStep::Press(entry) = &flow.steps[2] else {
            panic!("expected press step");
        };
        assert_eq!(
            entry.press.target.as_ref().unwrap().role.as_deref(),
            Some("textbox")
        );
        let FlowStep::Select(entry) = &flow.steps[3] else {
            panic!("expected select step");
        };
        assert_eq!(entry.select.values, ["SG"]);
        let FlowStep::WaitFor(entry) = &flow.steps[4] else {
            panic!("expected wait_for step");
        };
        assert_eq!(entry.wait_for.state, FlowWaitForState::Populated);
        assert_eq!(
            entry.wait_for.clone().into_assertion().populated,
            Some(true)
        );
        let FlowStep::RequestHelp(entry) = &flow.steps[5] else {
            panic!("expected request_help step");
        };
        assert_eq!(
            entry.request_help.prompt,
            "Complete the captcha, then continue"
        );
        let FlowStep::Assert(entry) = &flow.steps[6] else {
            panic!("expected assert step");
        };
        assert_eq!(entry.assertion.visible, Some(true));
        assert_eq!(
            entry.assertion.target.as_ref().unwrap().text.as_deref(),
            Some("Write code")
        );
        assert_eq!(flow.assertions[0].url_matches.as_deref(), Some("/todos$"));
    }

    #[test]
    fn assertion_step_rejects_multiple_expectations() {
        let flow: FlowDefinition = serde_yaml::from_str(
            r##"name: invalid assertion
steps:
  - assert:
      target: { css: "#save" }
      visible: true
      enabled: true
"##,
        )
        .unwrap();
        assert!(flow.validate().is_err());
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

    #[test]
    fn request_help_requires_prompt_and_one_target_strategy() {
        let missing_prompt: FlowDefinition = serde_yaml::from_str(
            r#"name: invalid help
steps:
  - request_help:
      prompt: ""
"#,
        )
        .unwrap();
        assert!(
            missing_prompt
                .validate()
                .unwrap_err()
                .message
                .contains("non-empty prompt")
        );

        let flow: FlowDefinition = serde_yaml::from_str(
            r##"name: invalid help
steps:
  - request_help:
      prompt: Complete the captcha
      targets:
        - ref: "@e1"
          selector: "#captcha"
"##,
        )
        .unwrap();
        let error = flow.validate().unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidParams);
        assert!(error.message.contains("exactly one"));
    }

    #[test]
    fn final_assertions_are_validated() {
        let flow: FlowDefinition = serde_yaml::from_str(
            r#"name: invalid final assertion
steps:
  - wait_ms:
      duration_ms: 1
assertions:
  - visible: true
"#,
        )
        .unwrap();
        let error = flow.validate().unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidParams);
        assert!(error.message.contains("final assertion 1"));
    }
}
