//! `tabstride metrics` — inspect/export persisted end-to-end timings.

use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::{Args, Subcommand};
use serde::Serialize;

use crate::cli::error::{CliError, Format};
use crate::timing::{MetricRecord, read_metrics};

#[derive(Debug, Clone, Subcommand)]
pub enum MetricsCmd {
    /// Print count and P50/P95/P99 latency by method.
    Summary(MetricsFilter),
    /// Export matching raw metric records as JSON.
    Export(MetricsExportArgs),
}

#[derive(Debug, Clone, Args, Default)]
pub struct MetricsFilter {
    /// Only include one wire method, for example `tool.click`.
    #[arg(long)]
    pub method: Option<String>,
    /// Only include the newest N records.
    #[arg(long)]
    pub last: Option<usize>,
}

#[derive(Debug, Clone, Args)]
pub struct MetricsExportArgs {
    #[command(flatten)]
    pub filter: MetricsFilter,
    /// Destination JSON file.
    #[arg(long)]
    pub out: PathBuf,
}

#[derive(Debug, Serialize)]
struct SummaryRow {
    method: String,
    count: usize,
    p50_us: u64,
    p95_us: u64,
    p99_us: u64,
    queue_p95_us: Option<u64>,
    websocket_p95_us: Option<u64>,
    extension_p95_us: Option<u64>,
    cdp_p95_us: Option<u64>,
}

pub fn dispatch(command: MetricsCmd, format: Format) -> Result<(), CliError> {
    match command {
        MetricsCmd::Summary(filter) => summary(filter, format),
        MetricsCmd::Export(args) => export(args),
    }
}

fn filtered(filter: &MetricsFilter) -> Result<Vec<MetricRecord>, CliError> {
    let mut records = read_metrics().map_err(CliError::Local)?;
    if let Some(method) = &filter.method {
        records.retain(|record| record.method == *method);
    }
    if let Some(last) = filter.last
        && records.len() > last
    {
        records.drain(..records.len() - last);
    }
    Ok(records)
}

fn summary(filter: MetricsFilter, format: Format) -> Result<(), CliError> {
    let records = filtered(&filter)?;
    let mut groups: BTreeMap<String, Vec<MetricRecord>> = BTreeMap::new();
    for record in records {
        groups
            .entry(record.method.clone())
            .or_default()
            .push(record);
    }
    let rows: Vec<SummaryRow> = groups
        .into_iter()
        .filter_map(|(method, records)| {
            let total: Vec<u64> = records
                .iter()
                .filter_map(|record| record.timing.total_runtime_us())
                .collect();
            if total.is_empty() {
                return None;
            }
            Some(SummaryRow {
                method,
                count: total.len(),
                p50_us: percentile(&total, 50),
                p95_us: percentile(&total, 95),
                p99_us: percentile(&total, 99),
                queue_p95_us: phase_percentile(&records, |r| r.timing.queue_wait_us()),
                websocket_p95_us: phase_percentile(&records, |r| r.timing.websocket_us()),
                extension_p95_us: phase_percentile(&records, |r| r.timing.extension_dispatch_us()),
                cdp_p95_us: phase_percentile(&records, |r| r.timing.cdp_us()),
            })
        })
        .collect();
    match format {
        Format::Json => println!(
            "{}",
            serde_json::to_string_pretty(&rows).map_err(|e| CliError::Local(e.into()))?
        ),
        Format::Human => {
            if rows.is_empty() {
                println!("No timing metrics recorded yet. Run a browser command first.");
            } else {
                println!(
                    "{:<28} {:>7} {:>10} {:>10} {:>10} {:>10}",
                    "method", "count", "p50", "p95", "p99", "cdp p95"
                );
                for row in rows {
                    println!(
                        "{:<28} {:>7} {:>9.2}ms {:>9.2}ms {:>9.2}ms {:>9}",
                        row.method,
                        row.count,
                        row.p50_us as f64 / 1000.0,
                        row.p95_us as f64 / 1000.0,
                        row.p99_us as f64 / 1000.0,
                        row.cdp_p95_us
                            .map(|v| format!("{:.2}ms", v as f64 / 1000.0))
                            .unwrap_or_else(|| "-".into())
                    );
                }
            }
        }
    }
    Ok(())
}

fn export(args: MetricsExportArgs) -> Result<(), CliError> {
    let records = filtered(&args.filter)?;
    let bytes = serde_json::to_vec_pretty(&records).map_err(|e| CliError::Local(e.into()))?;
    std::fs::write(&args.out, bytes).map_err(|e| CliError::Local(e.into()))?;
    println!(
        "exported {} metric(s) to {}",
        records.len(),
        args.out.display()
    );
    Ok(())
}

fn phase_percentile(
    records: &[MetricRecord],
    value: impl Fn(&MetricRecord) -> Option<u64>,
) -> Option<u64> {
    let values: Vec<u64> = records.iter().filter_map(value).collect();
    (!values.is_empty()).then(|| percentile(&values, 95))
}

fn percentile(values: &[u64], percentile: usize) -> u64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let index = ((sorted.len() - 1) * percentile).div_ceil(100);
    sorted[index]
}

#[cfg(test)]
mod tests {
    use super::percentile;

    #[test]
    fn percentile_uses_nearest_rank() {
        assert_eq!(percentile(&[1, 2, 3, 4, 5], 50), 3);
        assert_eq!(percentile(&[1, 2, 3, 4, 5], 95), 5);
    }
}
