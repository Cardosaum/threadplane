use core::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use threadplane_core::{
    normalize_memory_recall_triggers, normalize_memory_tags, normalize_task_labels, parse_entity_ref,
    relation_type, validate_workspace_policy, PublicKeyAlgorithm, WorkspaceAuthPolicy,
    WorkspacePolicy, WorkspacePriority, WorkspacePriorityPolicy,
};

fn bench_normalize_task_labels(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("normalize_task_labels");
    for size in [4_usize, 16, 64] {
        let labels: Vec<_> = (0..size)
            .map(|index| format!("  Label {}  ", index % 8))
            .collect();
        group.bench_with_input(BenchmarkId::from_parameter(size), &labels, |bencher, input| {
            bencher.iter(|| normalize_task_labels(black_box(input.clone())));
        });
    }
    group.finish();
}

fn bench_normalize_memory_tags(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("normalize_memory_tags");
    for size in [4_usize, 16, 64] {
        let tags: Vec<_> = (0..size)
            .map(|index| format!("  Prime {}  ", index % 8))
            .collect();
        group.bench_with_input(BenchmarkId::from_parameter(size), &tags, |bencher, input| {
            bencher.iter(|| normalize_memory_tags(black_box(input.clone())));
        });
    }
    group.finish();
}

fn bench_normalize_memory_recall_triggers(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("normalize_memory_recall_triggers");
    for size in [4_usize, 16, 64] {
        let triggers: Vec<_> = (0..size)
            .map(|index| format!("  Session Start {}  ", index % 8))
            .collect();
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &triggers,
            |bencher, input| {
                bencher.iter(|| normalize_memory_recall_triggers(black_box(input.clone())));
            },
        );
    }
    group.finish();
}

fn bench_parse_entity_ref(criterion: &mut Criterion) {
    let refs = [
        "task:11111111-2222-3333-4444-555555555555",
        "note:11111111-2222-3333-4444-555555555555",
        "epic:11111111-2222-3333-4444-555555555555",
        "memory:11111111-2222-3333-4444-555555555555",
    ];

    criterion.bench_function("parse_entity_ref", |bencher| {
        bencher.iter(|| {
            for entity_ref in refs {
                black_box(parse_entity_ref(black_box(entity_ref)));
            }
        });
    });
}

fn bench_relation_type(criterion: &mut Criterion) {
    let inputs = [
        "implements epic",
        "depends-on",
        "Xanadu Link",
        "documents",
        "shared_context",
    ];

    criterion.bench_function("relation_type", |bencher| {
        bencher.iter(|| {
            for input in inputs {
                black_box(relation_type(black_box(input)));
            }
        });
    });
}

fn bench_validate_workspace_policy(criterion: &mut Criterion) {
    let policy = WorkspacePolicy {
        auth: WorkspaceAuthPolicy {
            allowed_algorithms: vec![PublicKeyAlgorithm::Ed25519, PublicKeyAlgorithm::SshEd25519],
            challenge_ttl_seconds: 300,
            signed_commands_required: true,
        },
        priorities: WorkspacePriorityPolicy {
            default_priority: "medium".to_owned(),
            priorities: vec![
                WorkspacePriority {
                    description: Some("background cleanup".to_owned()),
                    name: "low".to_owned(),
                    rank: 40,
                },
                WorkspacePriority {
                    description: Some("default flow".to_owned()),
                    name: "medium".to_owned(),
                    rank: 30,
                },
                WorkspacePriority {
                    description: Some("important work".to_owned()),
                    name: "high".to_owned(),
                    rank: 20,
                },
                WorkspacePriority {
                    description: Some("immediate attention".to_owned()),
                    name: "urgent".to_owned(),
                    rank: 10,
                },
            ],
        },
        workspace: "bench-lab".to_owned(),
    };

    criterion.bench_function("validate_workspace_policy", |bencher| {
        bencher.iter(|| black_box(validate_workspace_policy(black_box(&policy))));
    });
}

criterion_group!(
    benches,
    bench_normalize_memory_recall_triggers,
    bench_normalize_memory_tags,
    bench_normalize_task_labels,
    bench_parse_entity_ref,
    bench_relation_type,
    bench_validate_workspace_policy
);
criterion_main!(benches);
