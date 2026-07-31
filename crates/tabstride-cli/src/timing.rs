//! End-to-end request timing shared by the CLI, daemon and metrics reader.

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::daemon::paths;

pub const TIMING_FIELD: &str = "__tabstride_timing";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeCounters {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub cdp_calls: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub full_ax_tree_calls: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub locator_cache_hits: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub locator_cache_misses: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub snapshot_cache_hits: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub snapshot_cache_misses: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub overlay_cache_hits: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub overlay_cache_misses: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimingTrace {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_received_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serve_queue_entered_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serve_queue_started_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension_sent_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension_received_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cdp_started_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cdp_finished_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension_replied_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serve_replied_at: Option<u64>,
    #[serde(default, skip_serializing_if = "RuntimeCounters::is_empty")]
    pub counters: RuntimeCounters,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricRecord {
    pub recorded_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub method: String,
    pub outcome: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub timing: TimingTrace,
}

impl RuntimeCounters {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

impl TimingTrace {
    pub fn duration_us(
        &self,
        start: fn(&Self) -> Option<u64>,
        end: fn(&Self) -> Option<u64>,
    ) -> Option<u64> {
        end(self)?.checked_sub(start(self)?)
    }

    pub fn queue_wait_us(&self) -> Option<u64> {
        self.serve_queue_started_at?
            .checked_sub(self.serve_queue_entered_at?)
    }

    pub fn websocket_us(&self) -> Option<u64> {
        self.extension_received_at?
            .checked_sub(self.extension_sent_at?)
    }

    pub fn extension_dispatch_us(&self) -> Option<u64> {
        self.extension_replied_at?
            .checked_sub(self.extension_received_at?)
    }

    pub fn cdp_us(&self) -> Option<u64> {
        self.cdp_finished_at?.checked_sub(self.cdp_started_at?)
    }

    pub fn total_runtime_us(&self) -> Option<u64> {
        self.serve_replied_at?.checked_sub(self.agent_received_at?)
    }
}

pub fn epoch_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros()
        .min(u128::from(u64::MAX)) as u64
}

pub fn trace_from(value: &Value) -> TimingTrace {
    value
        .get(TIMING_FIELD)
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

pub fn put_trace(value: &mut Value, trace: &TimingTrace) {
    let Value::Object(map) = value else {
        return;
    };
    if let Ok(value) = serde_json::to_value(trace) {
        map.insert(TIMING_FIELD.to_string(), value);
    }
}

pub fn take_trace(value: &mut Value) -> Option<TimingTrace> {
    let Value::Object(map) = value else {
        return None;
    };
    map.remove(TIMING_FIELD)
        .and_then(|value| serde_json::from_value(value).ok())
}

pub fn metrics_path() -> anyhow::Result<std::path::PathBuf> {
    Ok(paths::tabstride_home()?.join("metrics.jsonl"))
}

pub fn append_metric(record: &MetricRecord) -> anyhow::Result<()> {
    static WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    paths::ensure_tabstride_home()?;
    let path = metrics_path()?;
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    serde_json::to_writer(&mut file, record)?;
    file.write_all(b"\n")?;
    Ok(())
}

pub fn read_metrics() -> anyhow::Result<Vec<MetricRecord>> {
    let path = metrics_path()?;
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    Ok(contents
        .lines()
        .filter_map(|line| serde_json::from_str::<MetricRecord>(line).ok())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_every_public_phase_from_schema_timestamps() {
        let trace = TimingTrace {
            run_id: Some("run-test".into()),
            agent_received_at: Some(100),
            serve_queue_entered_at: Some(110),
            serve_queue_started_at: Some(130),
            extension_sent_at: Some(140),
            extension_received_at: Some(160),
            cdp_started_at: Some(170),
            cdp_finished_at: Some(210),
            extension_replied_at: Some(220),
            serve_replied_at: Some(250),
            counters: RuntimeCounters {
                cdp_calls: 3,
                full_ax_tree_calls: 1,
                ..RuntimeCounters::default()
            },
        };
        assert_eq!(trace.queue_wait_us(), Some(20));
        assert_eq!(trace.websocket_us(), Some(20));
        assert_eq!(trace.extension_dispatch_us(), Some(60));
        assert_eq!(trace.cdp_us(), Some(40));
        assert_eq!(trace.total_runtime_us(), Some(150));
        assert_eq!(trace.run_id.as_deref(), Some("run-test"));
        assert_eq!(trace.counters.full_ax_tree_calls, 1);
    }

    #[test]
    fn hidden_trace_round_trips_without_touching_result_fields() {
        let trace = TimingTrace {
            agent_received_at: Some(42),
            ..TimingTrace::default()
        };
        let mut value = serde_json::json!({"tab_id": 7});
        put_trace(&mut value, &trace);
        assert_eq!(take_trace(&mut value), Some(trace));
        assert_eq!(value, serde_json::json!({"tab_id": 7}));
    }

    #[test]
    fn old_metrics_without_run_or_counters_remain_compatible() {
        let record: MetricRecord = serde_json::from_value(serde_json::json!({
            "recorded_at": 1,
            "method": "tool.click",
            "outcome": "ok",
            "timing": {"agent_received_at": 1, "serve_replied_at": 2}
        }))
        .unwrap();
        assert_eq!(record.run_id, None);
        assert!(record.timing.counters.is_empty());
    }
}
