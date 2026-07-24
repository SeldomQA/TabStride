//! DOM interaction tools (`tool.click`, `tool.fill`, `tool.press`,
//! `tool.select`).
//!
//! Element-targeted tools share one strict [`Locator`] protocol. Modifiers / mouse buttons are encoded as
//! lowercase JSON strings so the same wire shape works for CLI flags and
//! the extension's CDP bridge.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::JavaScriptDialogInfo;

/// Keyboard modifier flags. Multiple flags may be combined; the
/// extension folds them into CDP's bitfield (`alt=1, ctrl=2, meta=4,
/// shift=8`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum KeyModifier {
    Alt,
    Ctrl,
    Meta,
    Shift,
}

/// Mouse button selector for `tool.click`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MouseButton {
    #[default]
    Left,
    Middle,
    Right,
}

/// Unified element locator used by CLI, Flow, and extension interaction tools.
/// Exactly one primary strategy must be present. `role` additionally requires
/// `name`; `exact` applies to semantic string strategies only.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Locator {
    #[serde(
        rename = "ref",
        alias = "ref_",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub ref_: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub css: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(
        rename = "testId",
        alias = "test_id",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub test_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exact: Option<bool>,
}

impl Locator {
    pub fn validate(&self) -> Result<(), String> {
        let strategies = [
            self.ref_.as_deref(),
            self.css.as_deref(),
            self.role.as_deref(),
            self.label.as_deref(),
            self.placeholder.as_deref(),
            self.text.as_deref(),
            self.test_id.as_deref(),
        ];
        let selected = strategies
            .into_iter()
            .filter(|value| value.is_some_and(|value| !value.trim().is_empty()))
            .count();
        if selected != 1 {
            return Err("target requires exactly one non-empty locator strategy".into());
        }
        if self.role.is_some() != self.name.is_some() {
            return Err("role locators require both `role` and `name`".into());
        }
        if self
            .role
            .as_deref()
            .is_some_and(|role| role.trim().is_empty())
            || self
                .name
                .as_deref()
                .is_some_and(|name| name.trim().is_empty())
        {
            return Err("role locators require non-empty `role` and `name`".into());
        }
        if (self.ref_.is_some() || self.css.is_some()) && self.exact.is_some() {
            return Err("`exact` is only valid for semantic locators".into());
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// click
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ClickParams {
    pub session_id: String,
    pub target: Locator,
    /// Target tab. Defaults to the Agent Window's active tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub button: Option<MouseButton>,
    /// Number of consecutive mouse presses (double-click = 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub click_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modifiers: Option<Vec<KeyModifier>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub timeout_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ClickResult {
    pub tab_id: i64,
    pub used_target: Locator,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_selector: Option<String>,
    /// Viewport-relative click coordinates (CSS pixels). Reported so
    /// agents can correlate with a follow-up `tool.screenshot`.
    pub x: f64,
    pub y: f64,
    /// Native JS dialogs observed and auto-handled during this call.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dialogs: Vec<JavaScriptDialogInfo>,
}

// ---------------------------------------------------------------------------
// fill
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FillParams {
    pub session_id: String,
    pub value: String,
    pub target: Locator,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<i64>,
    /// Clear the field before typing. Defaults to `true`; pass `false`
    /// to append instead of replacing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clear_before: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub timeout_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FillResult {
    pub tab_id: i64,
    pub used_target: Locator,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_selector: Option<String>,
    /// UTF-16 code-unit length of the value that was finally typed
    /// (matches what `input.value.length` would report in the page).
    pub value_length: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dialogs: Vec<JavaScriptDialogInfo>,
}

// ---------------------------------------------------------------------------
// press
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PressParams {
    pub session_id: String,
    /// Logical key name. Accepts CDP `key` strings (`Enter`, `Escape`,
    /// `ArrowDown`, single characters like `a`), or a compound
    /// expression such as `Ctrl+A` / `Meta+Shift+P`. Modifiers in the
    /// compound form combine with anything supplied via `modifiers`.
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modifiers: Option<Vec<KeyModifier>>,
    /// Optional target to focus before dispatching the key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<Locator>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<i64>,
    /// Hold the key down for this many milliseconds between `keyDown`
    /// and `keyUp`. Useful for testing long-press handlers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub timeout_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PressResult {
    pub tab_id: i64,
    pub key: String,
    pub code: String,
    #[serde(default)]
    pub modifiers: Vec<KeyModifier>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_target: Option<Locator>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dialogs: Vec<JavaScriptDialogInfo>,
}

// ---------------------------------------------------------------------------
// select
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SelectParams {
    pub session_id: String,
    /// Option `value` strings to set as the final selection. For a
    /// single-select `<select>` exactly one value is required; for
    /// `<select multiple>` the list replaces the current selection
    /// (an empty list clears all selections).
    pub values: Vec<String>,
    pub target: Locator,
    /// Target tab. Defaults to the Agent Window's active tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<i64>,
    /// Maximum time the daemon waits for the tool call before timing out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub timeout_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SelectResult {
    pub tab_id: i64,
    pub used_target: Locator,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_selector: Option<String>,
    /// Whether the target `<select>` had the `multiple` attribute.
    pub multiple: bool,
    /// Final selected option `value` attributes after the call.
    pub selected_values: Vec<String>,
    /// Visible labels of the selected options (same order as
    /// `selected_values`).
    pub selected_labels: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dialogs: Vec<JavaScriptDialogInfo>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ref_locator(value: &str) -> Locator {
        Locator {
            ref_: Some(value.into()),
            css: None,
            role: None,
            name: None,
            label: None,
            placeholder: None,
            text: None,
            test_id: None,
            exact: None,
        }
    }

    #[test]
    fn click_params_serialise_nested_target() {
        let p = ClickParams {
            session_id: "abcd".into(),
            target: ref_locator("@e3"),
            tab_id: Some(42),
            button: Some(MouseButton::Left),
            click_count: Some(1),
            modifiers: Some(vec![KeyModifier::Ctrl]),
            timeout_ms: Some(5_000),
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["target"]["ref"], "@e3");
        let round: ClickParams = serde_json::from_value(v).unwrap();
        assert_eq!(round, p);
    }

    #[test]
    fn locator_accepts_rust_ref_alias() {
        let p: ClickParams = serde_json::from_value(json!({
            "session_id": "a",
            "target": { "ref_": "e1" },
        }))
        .unwrap();
        assert_eq!(p.target.ref_.as_deref(), Some("e1"));
    }

    #[test]
    fn modifiers_render_as_lowercase_strings() {
        let v = serde_json::to_value(KeyModifier::Ctrl).unwrap();
        assert_eq!(v, json!("ctrl"));
        let v = serde_json::to_value(KeyModifier::Meta).unwrap();
        assert_eq!(v, json!("meta"));
    }

    #[test]
    fn press_result_round_trips() {
        let r = PressResult {
            tab_id: 5,
            key: "a".into(),
            code: "KeyA".into(),
            modifiers: vec![KeyModifier::Ctrl, KeyModifier::Shift],
            used_target: None,
            dialogs: vec![],
        };
        let v = serde_json::to_value(&r).unwrap();
        let round: PressResult = serde_json::from_value(v).unwrap();
        assert_eq!(round, r);
    }

    #[test]
    fn fill_params_default_clear_before_is_omitted() {
        let p = FillParams {
            session_id: "abcd".into(),
            value: "hello".into(),
            target: ref_locator("@e1"),
            tab_id: None,
            clear_before: None,
            timeout_ms: None,
        };
        let v = serde_json::to_value(&p).unwrap();
        assert!(v.get("clear_before").is_none());
    }

    #[test]
    fn select_params_round_trips_values() {
        let p = SelectParams {
            session_id: "abcd".into(),
            values: vec!["us".into(), "ca".into()],
            target: ref_locator("@e3"),
            tab_id: Some(12),
            timeout_ms: Some(5_000),
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v.get("values").cloned(), Some(json!(["us", "ca"])));
        let round: SelectParams = serde_json::from_value(v).unwrap();
        assert_eq!(round, p);
    }

    #[test]
    fn locator_validates_every_strategy_and_role_name_pair() {
        for locator in [
            ref_locator("@e1"),
            Locator {
                css: Some("#save".into()),
                ..Locator::default()
            },
            Locator {
                role: Some("button".into()),
                name: Some("Save".into()),
                exact: Some(true),
                ..Locator::default()
            },
            Locator {
                label: Some("Email".into()),
                ..Locator::default()
            },
            Locator {
                placeholder: Some("name@example.com".into()),
                ..Locator::default()
            },
            Locator {
                text: Some("Welcome".into()),
                ..Locator::default()
            },
            Locator {
                test_id: Some("save".into()),
                ..Locator::default()
            },
        ] {
            locator.validate().unwrap();
        }
        assert!(
            Locator {
                role: Some("button".into()),
                ..Locator::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            Locator {
                role: Some("button".into()),
                name: Some("   ".into()),
                ..Locator::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            Locator {
                css: Some("button".into()),
                text: Some("Save".into()),
                ..Locator::default()
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn locator_uses_test_id_wire_name() {
        let locator = Locator {
            test_id: Some("save".into()),
            exact: Some(true),
            ..Locator::default()
        };
        let value = serde_json::to_value(&locator).unwrap();
        assert_eq!(value, json!({"testId": "save", "exact": true}));
        let round: Locator = serde_json::from_value(value).unwrap();
        assert_eq!(round, locator);
    }
}
