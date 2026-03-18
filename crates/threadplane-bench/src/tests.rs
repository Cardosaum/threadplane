use proptest::proptest;
use proptest::{prop_assert, prop_assert_eq};
use rstest::rstest;
use threadplane_core::build_info;

use crate::{
    report::{build_report, summarize_latencies, BenchmarkReportContext},
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

proptest! {
    #[test]
    fn partition_operation_plan_preserves_original_order(
        operations in 1_usize..128,
        workers in 1_usize..32,
    ) {
        let plan: Vec<_> = (0..operations)
            .map(|index| OperationPlan::new(index, OperationKind::CreateNote))
            .collect();

        let chunks = partition_operation_plan(&plan, workers);
        let flattened: Vec<_> = chunks.into_iter().flatten().collect();

        prop_assert_eq!(flattened.len(), plan.len());
        for (expected_index, entry) in flattened.iter().enumerate() {
            let Some(expected) = plan.get(expected_index) else {
                prop_assert!(false, "flattened plan must not exceed original plan length");
                return Ok(());
            };
            prop_assert_eq!(format!("{entry:?}"), format!("{expected:?}"));
        }
    }
}

proptest! {
    #[test]
    fn partition_operation_plan_never_creates_empty_chunks(
        operations in 1_usize..128,
        workers in 1_usize..32,
    ) {
        let plan: Vec<_> = (0..operations)
            .map(|index| OperationPlan::new(index, OperationKind::OfferTask))
            .collect();

        let chunks = partition_operation_plan(&plan, workers);

        for chunk in &chunks {
            prop_assert!(!chunk.is_empty());
        }
        prop_assert!(chunks.len() <= workers);
        prop_assert!(chunks.len() <= operations);
    }
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
    assert_eq!(
        sorted_kinds,
        vec![OperationKind::CreateNote, OperationKind::OfferTask]
    );
}

#[test]
fn scenario_kind_defaults_to_note_writes() {
    assert_eq!(ScenarioKind::default(), ScenarioKind::NoteWrites);
}

#[test]
fn build_report_carries_capture_metadata() {
    let report = build_report(
        BenchmarkReportContext {
            captured_at: "2026-03-17T12:00:00Z".to_owned(),
            client_build: build_info(
                "threadplane-bench",
                "0.1.0",
                "debug",
                Some("abc123def456"),
                false,
            ),
            concurrency: 4,
            scenario: ScenarioKind::Mixed,
            server_build: Some(build_info(
                "threadplane-server",
                "0.1.0",
                "debug",
                Some("fed654cba321"),
                true,
            )),
            server_url: "http://127.0.0.1:4000".to_owned(),
            total_duration_ms: 500.0_f64,
            workspace: "bench-lab".to_owned(),
        },
        vec![OperationSample {
            kind: OperationKind::CreateNote,
            latency_ms: 25.0_f64,
            succeeded: true,
        }],
    );

    assert_eq!(report.captured_at, "2026-03-17T12:00:00Z");
    assert_eq!(report.client_build.service, "threadplane-bench");
    assert_eq!(
        report
            .server_build
            .as_ref()
            .map(|build| build.service.as_str()),
        Some("threadplane-server")
    );
    assert_eq!(report.server_url, "http://127.0.0.1:4000");
    assert_eq!(report.workspace, "bench-lab");
    assert_eq!(report.operations, 1);
}
