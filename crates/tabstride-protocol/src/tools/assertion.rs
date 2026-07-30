use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Locator;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssertionSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<Locator>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attached: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detached: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_equals: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_contains: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_equals: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unchecked: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub populated: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url_equals: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url_matches: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u32>,
}

impl AssertionSpec {
    pub fn validate(&self) -> Result<(), String> {
        let predicates = [
            self.visible.is_some(),
            self.hidden.is_some(),
            self.attached.is_some(),
            self.detached.is_some(),
            self.text_equals.is_some(),
            self.text_contains.is_some(),
            self.value_equals.is_some(),
            self.enabled.is_some(),
            self.disabled.is_some(),
            self.editable.is_some(),
            self.checked.is_some(),
            self.unchecked.is_some(),
            self.count.is_some(),
            self.populated.is_some(),
            self.url_equals.is_some(),
            self.url_matches.is_some(),
        ]
        .into_iter()
        .filter(|present| *present)
        .count();

        if predicates != 1 {
            return Err("assertion must specify exactly one expectation".to_string());
        }
        if self.timeout_ms == Some(0) {
            return Err("assertion timeout_ms must be greater than zero".to_string());
        }

        let is_url_assertion = self.url_equals.is_some() || self.url_matches.is_some();
        if is_url_assertion {
            if self.target.is_some() {
                return Err("URL assertions do not accept a target".to_string());
            }
        } else {
            let target = self
                .target
                .as_ref()
                .ok_or_else(|| "element assertions require a target".to_string())?;
            target.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssertParams {
    pub session_id: String,
    #[serde(flatten)]
    pub assertion: AssertionSpec,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AssertResult {
    pub tab_id: i64,
    pub assertion: String,
    pub passed: bool,
    pub elapsed_ms: u64,
    pub expected: Value,
    pub actual: Value,
    pub match_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_target: Option<Locator>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visible_spec() -> AssertionSpec {
        AssertionSpec {
            target: Some(Locator {
                css: Some("#save".to_string()),
                ..Locator::default()
            }),
            tab_id: None,
            visible: Some(true),
            timeout_ms: Some(1_000),
            ..AssertionSpec::default()
        }
    }

    #[test]
    fn validation_requires_one_expectation_and_correct_target_scope() {
        assert!(visible_spec().validate().is_ok());

        let mut missing = visible_spec();
        missing.visible = None;
        assert!(missing.validate().is_err());

        let mut multiple = visible_spec();
        multiple.hidden = Some(true);
        assert!(multiple.validate().is_err());

        let mut url = visible_spec();
        url.visible = None;
        url.target = None;
        url.url_matches = Some("^https://example\\.com".to_string());
        assert!(url.validate().is_ok());

        url.target = visible_spec().target;
        assert!(url.validate().is_err());
    }
}
