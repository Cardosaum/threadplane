#![expect(
    clippy::cast_possible_truncation,
    reason = "Benchmark percentile indexes intentionally round and clamp into bounded usize slots."
)]
#![expect(
    clippy::cast_precision_loss,
    reason = "Benchmark output intentionally uses floating-point throughput and latency values."
)]
#![expect(
    clippy::cast_sign_loss,
    reason = "Percentile indexes are clamped to non-negative values before conversion."
)]
#![expect(
    clippy::redundant_pub_crate,
    reason = "Benchmark report helpers are crate-local data-shaping utilities."
)]
#![expect(
    clippy::struct_field_names,
    reason = "Latency fields keep explicit millisecond suffixes in benchmark JSON."
)]

use alloc::collections::BTreeMap;

use serde::Serialize;

use crate::scenario::{OperationKind, OperationSample, ScenarioKind};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct BenchmarkReport {
    pub(crate) concurrency: usize,
    pub(crate) failed_operations: usize,
    pub(crate) operation_breakdown: Vec<OperationBreakdown>,
    pub(crate) operations: usize,
    pub(crate) scenario: ScenarioKind,
    pub(crate) successful_operations: usize,
    pub(crate) throughput_ops_per_second: f64,
    pub(crate) total_duration_ms: f64,
    pub(crate) workspace: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LatencySummary {
    pub(crate) average_ms: f64,
    pub(crate) max_ms: f64,
    pub(crate) min_ms: f64,
    pub(crate) p50_ms: f64,
    pub(crate) p95_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OperationBreakdown {
    pub(crate) failed_operations: usize,
    pub(crate) kind: OperationKind,
    pub(crate) latency_ms: LatencySummary,
    pub(crate) successful_operations: usize,
}

pub(crate) fn build_report(
    workspace: &str,
    scenario: ScenarioKind,
    concurrency: usize,
    total_duration_ms: f64,
    samples: Vec<OperationSample>,
) -> BenchmarkReport {
    let operations = samples.len();
    let successful_operations = samples.iter().filter(|sample| sample.succeeded).count();
    let failed_operations = operations.saturating_sub(successful_operations);
    let throughput_ops_per_second = if total_duration_ms > 0.0_f64 {
        (operations as f64) / (total_duration_ms / 1_000.0_f64)
    } else {
        0.0_f64
    };

    BenchmarkReport {
        concurrency,
        failed_operations,
        operation_breakdown: build_operation_breakdown(samples),
        operations,
        scenario,
        successful_operations,
        throughput_ops_per_second,
        total_duration_ms,
        workspace: workspace.to_owned(),
    }
}

fn build_operation_breakdown(samples: Vec<OperationSample>) -> Vec<OperationBreakdown> {
    let mut grouped_samples: BTreeMap<OperationKind, Vec<OperationSample>> = BTreeMap::new();
    for sample in samples {
        grouped_samples
            .entry(sample.kind)
            .or_default()
            .push(sample);
    }

    let mut breakdown = Vec::with_capacity(grouped_samples.len());
    for (kind, kind_samples) in grouped_samples {
        let successful_operations = kind_samples
            .iter()
            .filter(|sample| sample.succeeded)
            .count();
        let failed_operations = kind_samples.len().saturating_sub(successful_operations);
        let latencies_ms = kind_samples
            .into_iter()
            .map(|sample| sample.latency_ms)
            .collect();
        breakdown.push(OperationBreakdown {
            failed_operations,
            kind,
            latency_ms: summarize_latencies(latencies_ms),
            successful_operations,
        });
    }

    breakdown
}

pub(crate) fn summarize_latencies(mut latencies_ms: Vec<f64>) -> LatencySummary {
    if latencies_ms.is_empty() {
        return LatencySummary {
            average_ms: 0.0,
            max_ms: 0.0,
            min_ms: 0.0,
            p50_ms: 0.0,
            p95_ms: 0.0,
        };
    }

    latencies_ms.sort_by(f64::total_cmp);
    let sample_count = latencies_ms.len();
    let total_latency_ms: f64 = latencies_ms.iter().sum();
    let min_ms = latencies_ms.first().copied().unwrap_or(0.0_f64);
    let max_ms = latencies_ms.last().copied().unwrap_or(0.0_f64);

    LatencySummary {
        average_ms: total_latency_ms / (sample_count as f64),
        max_ms,
        min_ms,
        p50_ms: percentile(&latencies_ms, 0.50),
        p95_ms: percentile(&latencies_ms, 0.95),
    }
}

fn percentile(sorted_latencies_ms: &[f64], percentile: f64) -> f64 {
    let last_index = sorted_latencies_ms.len().saturating_sub(1);
    let percentile_index = ((last_index as f64) * percentile).round();
    let bounded_index = percentile_index.clamp(0.0_f64, last_index as f64) as usize;
    sorted_latencies_ms
        .get(bounded_index)
        .copied()
        .unwrap_or(0.0_f64)
}
