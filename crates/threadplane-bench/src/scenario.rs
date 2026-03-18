#![expect(
    clippy::redundant_pub_crate,
    reason = "Benchmark scenarios are crate-local orchestration building blocks."
)]

use core::time::Duration;
use std::{sync::mpsc, thread, time::Instant};

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::error::{Config, Result};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OperationKind {
    CreateNote,
    ListEvents,
    ListOpenTasks,
    OfferTask,
    Scope,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ScenarioKind {
    Mixed,
    #[default]
    NoteWrites,
}

#[derive(Clone, Debug)]
pub(crate) struct RunSettings {
    pub(crate) actor_prefix: String,
    pub(crate) concurrency: usize,
    pub(crate) operations: usize,
    pub(crate) scenario: ScenarioKind,
    pub(crate) server: String,
    pub(crate) workspace: String,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct OperationSample {
    pub(crate) kind: OperationKind,
    pub(crate) latency_ms: f64,
    pub(crate) succeeded: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct OperationPlan {
    index: usize,
    kind: OperationKind,
}

impl OperationPlan {
    pub(crate) const fn new(index: usize, kind: OperationKind) -> Self {
        Self { index, kind }
    }
}

#[derive(Clone)]
struct WorkerContext {
    actor_prefix: String,
    server: String,
    workspace: String,
}

pub(crate) fn run_benchmark(
    client: &Client,
    settings: &RunSettings,
) -> Result<Vec<OperationSample>> {
    if settings.operations == 0 {
        return Err(Config {
            message: "operations must be greater than zero".to_owned(),
        }
        .build());
    }
    if settings.concurrency == 0 {
        return Err(Config {
            message: "concurrency must be greater than zero".to_owned(),
        }
        .build());
    }

    let operation_plan = build_operation_plan(settings.scenario, settings.operations);
    let worker_count = settings.concurrency.min(operation_plan.len());
    let plan_chunks = partition_operation_plan(&operation_plan, worker_count);
    let worker_context = WorkerContext {
        actor_prefix: settings.actor_prefix.clone(),
        server: settings.server.clone(),
        workspace: settings.workspace.clone(),
    };

    let (sender, receiver) = mpsc::channel::<Vec<OperationSample>>();
    thread::scope(|scope| {
        for plan_chunk in plan_chunks {
            let thread_sender = sender.clone();
            let thread_client = client.clone();
            let thread_context = worker_context.clone();
            scope.spawn(move || {
                let samples = execute_plan_chunk(&thread_client, &thread_context, plan_chunk);
                drop(thread_sender.send(samples));
            });
        }
    });
    drop(sender);

    let mut samples = Vec::with_capacity(operation_plan.len());
    while let Ok(mut worker_samples) = receiver.recv() {
        samples.append(&mut worker_samples);
    }
    samples.sort_by_key(|sample| operation_kind_rank(sample.kind));
    Ok(samples)
}

fn execute_plan_chunk(
    client: &Client,
    worker_context: &WorkerContext,
    plan_chunk: Vec<OperationPlan>,
) -> Vec<OperationSample> {
    let mut samples = Vec::with_capacity(plan_chunk.len());
    for plan in plan_chunk {
        let started_at = Instant::now();
        let succeeded = execute_operation(client, worker_context, plan);
        let elapsed = started_at.elapsed();
        samples.push(OperationSample {
            kind: plan.kind,
            latency_ms: elapsed.as_secs_f64() * 1_000.0,
            succeeded,
        });
    }
    samples
}

fn execute_operation(client: &Client, worker_context: &WorkerContext, plan: OperationPlan) -> bool {
    match plan.kind {
        OperationKind::CreateNote => {
            let request_body = json!({
                "workspace": worker_context.workspace,
                "author": format!("{}-writer", worker_context.actor_prefix),
                "title": format!("bench-note-{}", unique_suffix(plan.index)),
                "body": "repeatable benchmark note write",
            });
            post_json(client, &worker_context.server, "/v1/notes", &request_body)
        }
        OperationKind::OfferTask => {
            let request_body = json!({
                "workspace": worker_context.workspace,
                "author": format!("{}-writer", worker_context.actor_prefix),
                "title": format!("bench-task-{}", unique_suffix(plan.index)),
                "details": "repeatable benchmark task write",
                "depends_on": [],
                "priority": "medium",
                "owner": null,
                "labels": ["benchmark"],
                "epic_id": null,
            });
            post_json(client, &worker_context.server, "/v1/tasks", &request_body)
        }
        OperationKind::ListEvents => get_ok(
            client,
            &worker_context.server,
            &format!(
                "/v1/workspaces/{}/events?limit=10",
                worker_context.workspace
            ),
        ),
        OperationKind::ListOpenTasks => get_ok(
            client,
            &worker_context.server,
            &format!("/v1/workspaces/{}/tasks/open", worker_context.workspace),
        ),
        OperationKind::Scope => get_ok(client, &worker_context.server, "/scope"),
    }
}

fn build_operation_plan(scenario: ScenarioKind, operations: usize) -> Vec<OperationPlan> {
    let mut plan = Vec::with_capacity(operations);
    for index in 0..operations {
        let kind = match scenario {
            ScenarioKind::NoteWrites => OperationKind::CreateNote,
            ScenarioKind::Mixed => mixed_operation_kind(index),
        };
        plan.push(OperationPlan::new(index, kind));
    }
    plan
}

const fn mixed_operation_kind(index: usize) -> OperationKind {
    match index % 4 {
        0 => OperationKind::CreateNote,
        1 => OperationKind::OfferTask,
        2 => OperationKind::ListEvents,
        _ => OperationKind::ListOpenTasks,
    }
}

pub(crate) fn partition_operation_plan(
    operation_plan: &[OperationPlan],
    worker_count: usize,
) -> Vec<Vec<OperationPlan>> {
    let mut chunks = Vec::with_capacity(worker_count);
    let chunk_size = operation_plan.len().div_ceil(worker_count);

    for chunk_index in 0..worker_count {
        let chunk_start = chunk_index.saturating_mul(chunk_size);
        if chunk_start >= operation_plan.len() {
            break;
        }

        let chunk_end = chunk_start
            .checked_add(chunk_size)
            .unwrap_or(operation_plan.len())
            .min(operation_plan.len());
        let Some(chunk_slice) = operation_plan.get(chunk_start..chunk_end) else {
            continue;
        };
        chunks.push(chunk_slice.to_vec());
    }

    chunks
}

fn get_ok(client: &Client, server: &str, path: &str) -> bool {
    let request_url = url(server, path);
    let send_result = client.get(&request_url).timeout(timeout()).send();
    let Ok(response) = send_result else {
        return false;
    };

    response.status().is_success()
}

fn post_json(client: &Client, server: &str, path: &str, body: &serde_json::Value) -> bool {
    let request_url = url(server, path);
    let send_result = client
        .post(&request_url)
        .json(body)
        .timeout(timeout())
        .send();
    let Ok(response) = send_result else {
        return false;
    };

    response.status().is_success()
}

const fn timeout() -> Duration {
    Duration::from_secs(15)
}

fn url(server: &str, path: &str) -> String {
    format!("{}{}", server.trim_end_matches('/'), path)
}

fn unique_suffix(index: usize) -> String {
    format!("{index:08}-{}", Uuid::new_v4())
}

const fn operation_kind_rank(kind: OperationKind) -> u8 {
    match kind {
        OperationKind::CreateNote => 0,
        OperationKind::ListEvents => 1,
        OperationKind::ListOpenTasks => 2,
        OperationKind::OfferTask => 3,
        OperationKind::Scope => 4,
    }
}
