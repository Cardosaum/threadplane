use rstest::rstest;

use crate::{
    report::summarize_latencies,
    scenario::{
        partition_operation_plan, OperationKind, OperationPlan, OperationSample, ScenarioKind,
    },
};

#[rstest]
#[case(vec![10.0_f64, 20.0_f64, 30.0_f64, 40.0_f64], 10.0_f64, 25.0_f64, 30.0_f64, 40.0_f64, 40.0_f64)]
#[case(vec![4.0_f64], 4.0_f64, 4.0_f64, 4.0_f64, 4.0_f64, 4.0_f64)]
fn summarize_latencies_reports_expected_percentiles(
    #[case] latencies_ms: Vec<f64>,
    #[case] expected_min_ms: f64,
    #[case] expected_average_ms: f64,
    #[case] expected_p50_ms: f64,
    #[case] expected_p95_ms: f64,
    #[case] expected_max_ms: f64,
) {
    let summary = summarize_latencies(latencies_ms);

    assert_eq!(summary.min_ms, expected_min_ms);
    assert_eq!(summary.average_ms, expected_average_ms);
    assert_eq!(summary.p50_ms, expected_p50_ms);
    assert_eq!(summary.p95_ms, expected_p95_ms);
    assert_eq!(summary.max_ms, expected_max_ms);
}

#[test]
fn partition_operation_plan_keeps_all_operations() {
    let mut plan = Vec::new();
    for index in 0..10 {
        plan.push(OperationPlan::new(index, OperationKind::CreateNote));
    }

    let chunks = partition_operation_plan(&plan, 3);

    let chunk_lengths: Vec<usize> = chunks.into_iter().map(|chunk| chunk.len()).collect();
    assert_eq!(chunk_lengths, vec![4, 4, 2]);
}

#[test]
fn operation_samples_sort_by_kind_rank() {
    let mut samples = [
        OperationSample {
            kind: OperationKind::OfferTask,
            latency_ms: 10.0_f64,
            succeeded: true,
        },
        OperationSample {
            kind: OperationKind::CreateNote,
            latency_ms: 5.0_f64,
            succeeded: true,
        },
    ];

    samples.sort_by_key(|sample| match sample.kind {
        OperationKind::CreateNote => 0_i32,
        OperationKind::ListEvents => 1_i32,
        OperationKind::ListOpenTasks => 2_i32,
        OperationKind::OfferTask => 3_i32,
        OperationKind::Scope => 4_i32,
    });

    let sorted_kinds: Vec<OperationKind> = samples.into_iter().map(|sample| sample.kind).collect();
    assert_eq!(sorted_kinds, vec![OperationKind::CreateNote, OperationKind::OfferTask]);
}

#[test]
fn scenario_kind_defaults_to_note_writes() {
    assert_eq!(ScenarioKind::default(), ScenarioKind::NoteWrites);
}
