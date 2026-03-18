#![expect(
    clippy::arbitrary_source_item_ordering,
    clippy::redundant_pub_crate,
    reason = "CLI commands are crate-internal and keep explicit visibility and workflow grouping for readability."
)]

use alloc::collections::BTreeSet;
use core::time::Duration;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

use threadplane_core::{
    compare_build_info, normalize_task_labels, normalize_task_owner, ActorPublicKey,
    AddLinkRequest, AddTaskDependencyRequest, AddWorkspacePublicKeyRequest, ApiEnvelope,
    BuildComparison, ClaimNextTaskRequest, ClaimTaskRequest, CliConfigOverrides,
    CompleteTaskRequest, ConfigDiscovery, CreateEpicRequest, CreateMemoryRequest,
    CreateNoteRequest, CreateXanaduLinkRequest, EntityContext, EntityRecord, EventRecord,
    GrantWorkspaceMembershipRequest, GraphRelation, MemoryAudience, MemoryImportance, MemoryKind,
    MemoryRecord, MemoryScope, NoteRecord, OfferTaskRequest, ProjectionStatus, ReleaseTaskRequest,
    ServiceSnapshot, TaskClaimRecord, TaskContext, TaskDag, TaskDependencySummary, TaskListEntry,
    TaskMetadata, TaskPriority, TaskRecord, ThreadplaneConfig, ThreadplaneConfigOverrides,
    UpdateNoteRequest, UpdateTaskRequest, UpdateWorkspacePolicyRequest, WorkspaceAuthPolicy,
    WorkspaceMembership, WorkspacePolicy, WorkspacePriority, WorkspacePriorityPolicy,
    WorkspaceRole,
};

use crate::{
    build_info::current_build_info,
    error::{Result, Usage},
    runtime::{ApiClient, CommandContext, CommandOutput, Sleeper},
};

mod content;
mod executor;
pub(crate) mod paths;
pub(crate) mod render;
mod workspace;

#[cfg(test)]
mod tests_execution;

pub(crate) use content::{
    ListMemories, ListNotes, MemoryCommand, MemorySubcommand, NoteCommand, NoteSubcommand,
    PrimeMemories, SearchNotes,
};
pub(crate) use executor::execute;
pub(crate) use workspace::{WorkspaceCommand, WorkspaceSubcommand};

#[derive(Debug, Parser)]
#[command(
    name = "tplane",
    version,
    about = "Shared memory and coordination CLI for people and AI agents",
    long_about = "tplane talks to threadplane-server so people and agents can share tasks, notes, links, claims, and graph-backed context through one internet-reachable control plane."
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    command: Command,

    #[arg(
        long,
        help = "Path to a threadplane config TOML. Overrides config discovery order."
    )]
    pub(crate) config: Option<PathBuf>,

    #[arg(
        long,
        help = "Optional idempotency key for mutating commands. Batch commands derive child keys automatically."
    )]
    idempotency_key: Option<String>,

    #[arg(
        long,
        help = "HTTP base URL for threadplane-server. Overrides cli.url from config."
    )]
    server: Option<String>,
}

impl Cli {
    pub(crate) fn config_overrides(&self) -> ThreadplaneConfigOverrides {
        let cli = self.server.as_ref().map(|url| CliConfigOverrides {
            url: Some(url.clone()),
        });

        ThreadplaneConfigOverrides {
            cli,
            ..ThreadplaneConfigOverrides::default()
        }
    }
}

#[derive(Debug, Subcommand)]
enum Command {
    Build(BuildCommand),
    Config(ConfigCommand),
    Entity(EntityCommand),
    Epic(EpicCommand),
    Events(EventsCommand),
    Link(LinkCommand),
    Memory(MemoryCommand),
    Note(NoteCommand),
    Projection(ProjectionCommand),
    #[command(about = "Show the product and architecture summary exposed by the service")]
    Scope,
    Task(TaskCommand),
    Workspace(WorkspaceCommand),
}

#[derive(Debug, Args)]
#[command(about = "Explore entities and their graph-linked relations")]
struct EntityCommand {
    #[command(subcommand)]
    command: EntitySubcommand,
}

#[derive(Debug, Subcommand)]
enum EntitySubcommand {
    #[command(about = "List entities related to the selected entity")]
    Related(RelatedEntities),
    #[command(about = "Fetch an entity and its related graph neighborhood")]
    Show(ShowEntity),
}

#[derive(Debug, Args)]
struct ShowEntity {
    #[arg(long, help = "Entity ref, for example task:<uuid> or note:<uuid>")]
    entity_ref: String,

    #[arg(
        long,
        default_value = "json",
        help = "Render JSON or a compact human-readable summary"
    )]
    format: OutputFormat,
}

#[derive(Debug, Args)]
struct RelatedEntities {
    #[arg(long, help = "Entity ref, for example task:<uuid> or note:<uuid>")]
    entity_ref: String,

    #[arg(
        long,
        default_value = "json",
        help = "Render JSON or a compact human-readable summary"
    )]
    format: OutputFormat,
}

#[derive(Debug, Args)]
#[command(about = "Inspect and compare CLI/server build identity")]
struct BuildCommand {
    #[command(subcommand)]
    command: BuildSubcommand,
}

#[derive(Debug, Subcommand)]
enum BuildSubcommand {
    #[command(about = "Compare the local CLI build with the running server build")]
    Compare,
    #[command(about = "Show the local threadplane-cli build identity")]
    Show,
}

#[derive(Debug, Args)]
#[command(about = "Create and inspect first-class epics")]
struct EpicCommand {
    #[command(subcommand)]
    command: EpicSubcommand,
}

#[derive(Debug, Subcommand)]
enum EpicSubcommand {
    #[command(about = "Create a new epic")]
    Add(AddEpic),
    #[command(about = "List epics in a workspace")]
    List(ListEpics),
    #[command(about = "Fetch an epic by ID")]
    Show(ShowEpic),
}

#[derive(Debug, Args)]
struct AddEpic {
    #[arg(long, help = "Epic author")]
    author: String,

    #[arg(long, help = "Epic body")]
    body: String,

    #[arg(long, help = "Epic title")]
    title: String,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args)]
struct ListEpics {
    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args)]
struct ShowEpic {
    #[arg(long, help = "Epic UUID")]
    epic_id: Uuid,
}

#[derive(Debug, Args)]
#[command(about = "Inspect configuration discovery and the resolved runtime config")]
struct ConfigCommand {
    #[command(subcommand)]
    command: ConfigSubcommand,
}

#[derive(Debug, Subcommand)]
enum ConfigSubcommand {
    #[command(about = "Print the resolved config and where threadplane looks for it")]
    Show,
}

#[derive(Debug, Args)]
#[command(about = "Inspect workspace event history")]
struct EventsCommand {
    #[command(subcommand)]
    command: EventsSubcommand,
}

#[derive(Debug, Subcommand)]
enum EventsSubcommand {
    #[command(about = "List recent events for a workspace")]
    List(ListEvents),
    #[command(about = "Read workspace events incrementally and optionally follow for new changes")]
    Tail(TailEvents),
}

#[derive(Debug, Args)]
struct ListEvents {
    #[arg(
        long,
        default_value = "json",
        help = "Render JSON or a compact human-readable summary"
    )]
    format: OutputFormat,

    #[arg(
        long,
        default_value_t = 25,
        help = "Maximum number of events to return"
    )]
    limit: i64,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args)]
struct TailEvents {
    #[arg(long, help = "Resume after this event UUID")]
    after_event_id: Option<Uuid>,

    #[arg(long, help = "Keep polling for new events")]
    follow: bool,

    #[arg(
        long,
        default_value = "json",
        help = "Render JSON or a compact human-readable summary"
    )]
    format: OutputFormat,

    #[arg(
        long,
        default_value_t = 25,
        help = "Maximum number of events to return per poll"
    )]
    limit: i64,

    #[arg(
        long,
        default_value_t = 2,
        help = "Seconds to wait between follow polls"
    )]
    poll_seconds: u64,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args)]
#[command(about = "Create semantic and Xanadu links between entities")]
struct LinkCommand {
    #[command(subcommand)]
    command: LinkSubcommand,
}

#[derive(Debug, Subcommand)]
enum LinkSubcommand {
    #[command(about = "Create a semantic graph link between two entities")]
    Add(AddLink),
    #[command(about = "Create a Xanadu transclusion link between two text entities")]
    Xanadu(AddXanaduLink),
}

#[derive(Debug, Args)]
struct AddLink {
    #[arg(long, help = "Actor creating the link")]
    actor: String,

    #[arg(long, help = "Source entity ref, for example task:<uuid>")]
    from: String,

    #[arg(long, help = "Relationship name, for example depends_on")]
    relation: String,

    #[arg(long, help = "Target entity ref, for example note:<uuid>")]
    to: String,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args)]
struct AddXanaduLink {
    #[arg(long, help = "Actor creating the Xanadu link")]
    actor: String,

    #[arg(long, help = "Source entity ref, for example task:<uuid>")]
    from: String,

    #[arg(long, help = "Target entity ref, for example note:<uuid>")]
    to: String,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args)]
#[command(about = "Inspect graph projection replay status")]
struct ProjectionCommand {
    #[command(subcommand)]
    command: ProjectionSubcommand,
}

#[derive(Debug, Subcommand)]
enum ProjectionSubcommand {
    #[command(about = "Show the persisted replay watermark for the graph projection")]
    Status,
}

#[derive(Debug, Args)]
#[command(about = "Offer, claim, and inspect shared tasks")]
struct TaskCommand {
    #[command(subcommand)]
    command: TaskSubcommand,
}

#[derive(Debug, Subcommand)]
enum TaskSubcommand {
    #[command(about = "Show tasks blocking the selected task")]
    BlockedBy(TaskDependencyViewCommand),
    #[command(about = "Show tasks that are blocked by the selected task")]
    Blocks(TaskDependencyViewCommand),
    #[command(about = "Claim an open task with a lease")]
    Claim(ClaimTask),
    #[command(about = "Claim the next best ready task in the workspace")]
    ClaimNext(ClaimNextTask),
    #[command(about = "Mark a task complete and release any active claim")]
    Complete(CompleteTask),
    #[command(about = "Fetch a task plus graph-backed related context")]
    Context(TaskContextCommand),
    #[command(about = "Show the task dependency DAG around a task")]
    Dag(TaskDagCommand),
    #[command(about = "Declare that one task depends on another")]
    Depend(AddTaskDependency),
    #[command(about = "List tasks with workflow filters")]
    List(ListTasks),
    #[command(about = "Show the next best ready task in the workspace")]
    Next(NextTask),
    #[command(about = "Offer a new task into a workspace")]
    Offer(OfferTask),
    #[command(about = "Release an active claim and return the task to the pool")]
    Release(ReleaseTask),
    #[command(about = "Fetch a task by ID without graph context")]
    Show(ShowTask),
    #[command(about = "Apply the same epic assignment and/or completion to multiple tasks")]
    Triage(TriageTasks),
    #[command(about = "Update a task and propagate through Xanadu links when present")]
    Update(UpdateTask),
}

#[derive(Debug, Args)]
struct ClaimTask {
    #[arg(long, help = "Actor claiming the task")]
    actor: String,

    #[arg(long, help = "Lease duration in seconds")]
    lease_seconds: Option<i64>,

    #[arg(long, help = "Task UUID")]
    task_id: Uuid,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args)]
struct ClaimNextTask {
    #[arg(long, help = "Actor claiming the task")]
    actor: String,

    #[arg(long, help = "Optional epic filter")]
    epic_id: Option<Uuid>,

    #[arg(long, help = "Durable label filter")]
    label: Option<String>,

    #[arg(long, help = "Lease duration in seconds")]
    lease_seconds: Option<i64>,

    #[command(flatten)]
    metadata_filters: TaskMetadataFilterArgs,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args)]
struct TaskDependencyViewCommand {
    #[arg(
        long,
        help = "Only return direct relationships instead of the transitive chain"
    )]
    direct_only: bool,

    #[arg(
        long,
        default_value = "compact",
        help = "Render JSON or a compact human-readable summary"
    )]
    format: OutputFormat,

    #[arg(long, help = "Task UUID")]
    task_id: Uuid,
}

#[derive(Debug, Args)]
struct CompleteTask {
    #[arg(long, help = "Actor completing the task")]
    actor: String,

    #[arg(long, help = "Task UUID")]
    task_id: Uuid,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args)]
struct TaskContextCommand {
    #[arg(long, help = "Task UUID")]
    task_id: Uuid,
}

#[derive(Debug, Args)]
struct TaskDagCommand {
    #[arg(long, help = "Task UUID")]
    task_id: Uuid,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum TaskStatusValue {
    Claimed,
    Completed,
    Open,
}

impl TaskStatusValue {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Claimed => "claimed",
            Self::Completed => "completed",
        }
    }
}

#[derive(Debug, Args)]
struct AddTaskDependency {
    #[arg(long, help = "Actor adding the dependency edge")]
    actor: String,

    #[arg(long, help = "Task UUID that must complete first")]
    depends_on: Uuid,

    #[arg(long, help = "Task UUID that will wait")]
    task_id: Uuid,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args)]
struct ListTasks {
    #[arg(long, help = "Optional epic filter")]
    epic_id: Option<Uuid>,

    #[arg(
        long,
        default_value = "json",
        help = "Render JSON or a compact human-readable summary"
    )]
    format: OutputFormat,

    #[arg(long, help = "Only include tasks with the selected normalized label")]
    label: Option<String>,

    #[arg(long, help = "Maximum number of tasks to return")]
    limit: Option<i64>,

    #[command(flatten)]
    metadata_filters: TaskMetadataFilterArgs,

    #[arg(long, help = "Only include tasks whose dependencies are all completed")]
    ready_only: bool,

    #[arg(long, help = "Optional workflow status filter")]
    status: Option<TaskStatusValue>,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum OutputFormat {
    Compact,
    #[default]
    Json,
}

#[derive(Debug, Args)]
struct NextTask {
    #[arg(long, help = "Optional epic filter")]
    epic_id: Option<Uuid>,

    #[arg(
        long,
        default_value = "json",
        help = "Render JSON or a compact human-readable summary"
    )]
    format: OutputFormat,

    #[arg(long, help = "Durable label filter")]
    label: Option<String>,

    #[command(flatten)]
    metadata_filters: TaskMetadataFilterArgs,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args, Clone)]
struct TaskMetadataArgs {
    #[arg(long, help = "Durable label. Repeat for multiple labels")]
    label: Vec<String>,

    #[arg(long, help = "Durable owner, distinct from the temporary claim actor")]
    owner: Option<String>,

    #[arg(long, help = "Priority used for backlog sorting and filtering")]
    priority: Option<String>,
}

#[derive(Debug, Args, Clone, Default)]
struct TaskMetadataFilterArgs {
    #[arg(long, help = "Only include tasks owned by this durable owner")]
    owner: Option<String>,

    #[arg(long, help = "Only include tasks with this priority")]
    priority: Option<String>,
}

#[derive(Debug, Args, Clone, Default)]
pub(crate) struct TaskMetadataPatchArgs {
    #[arg(long, help = "Clear all durable labels")]
    pub(crate) clear_labels: bool,

    #[arg(long, help = "Clear any durable owner")]
    pub(crate) clear_owner: bool,

    #[arg(
        long,
        help = "Replace labels with this set. Repeat for multiple labels"
    )]
    pub(crate) label: Vec<String>,

    #[arg(long, help = "Replace the durable owner")]
    pub(crate) owner: Option<String>,

    #[arg(long, help = "Replace the task priority")]
    pub(crate) priority: Option<String>,
}

#[derive(Debug, Args)]
struct OfferTask {
    #[arg(long, help = "Task author")]
    author: String,

    #[arg(long, help = "Dependency task UUID. Repeat for multiple dependencies")]
    depends_on: Vec<Uuid>,

    #[arg(long, help = "Task details")]
    details: String,

    #[arg(long, help = "Optional epic UUID to attach this task to")]
    epic_id: Option<Uuid>,

    #[command(flatten)]
    metadata: TaskMetadataArgs,

    #[arg(long, help = "Task title")]
    title: String,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args)]
struct ReleaseTask {
    #[arg(long, help = "Actor releasing the task")]
    actor: String,

    #[arg(long, help = "Task UUID")]
    task_id: Uuid,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args)]
struct ShowTask {
    #[arg(long, help = "Task UUID")]
    task_id: Uuid,
}

#[derive(Debug, Args)]
struct TriageTasks {
    #[arg(long, help = "Actor performing the triage")]
    actor: String,

    #[arg(
        long,
        help = "Mark every listed task completed after any metadata updates"
    )]
    complete: bool,

    #[arg(long, help = "Optional epic UUID to assign to every listed task")]
    epic_id: Option<Uuid>,

    #[command(flatten)]
    metadata: TaskMetadataPatchArgs,

    #[arg(
        long,
        help = "Task UUID to triage. Repeat for multiple tasks",
        num_args = 1..,
        required = true
    )]
    task_id: Vec<Uuid>,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args)]
struct UpdateTask {
    #[arg(long, help = "Actor performing the update")]
    actor: String,

    #[arg(long, help = "Updated task details")]
    details: String,

    #[arg(
        long,
        help = "Optional epic UUID. When provided, the task is attached to that epic"
    )]
    epic_id: Option<Uuid>,

    #[command(flatten)]
    metadata: TaskMetadataArgs,

    #[arg(long, help = "Task UUID")]
    task_id: Uuid,

    #[arg(long, help = "Updated task title")]
    title: String,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Serialize)]
struct TaskTriageSummary {
    clear_labels: bool,
    clear_owner: bool,
    completed_task_ids: Vec<Uuid>,
    epic_id: Option<Uuid>,
    labels: Option<Vec<String>>,
    owner: Option<String>,
    priority: Option<TaskPriority>,
    task_ids: Vec<Uuid>,
    unchanged_task_ids: Vec<Uuid>,
    updated_task_ids: Vec<Uuid>,
    workspace: String,
}

#[derive(Debug, Default)]
struct TaskTriageOutcome {
    changed: bool,
    completed: bool,
    updated: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct MemoryListPathArgs<'input> {
    pub(crate) audience: Option<&'input str>,
    pub(crate) importance: Option<&'input str>,
    pub(crate) kind: Option<&'input str>,
    pub(crate) limit: Option<i64>,
    pub(crate) query: Option<&'input str>,
    pub(crate) recall_trigger: Option<&'input str>,
    pub(crate) tag: Option<&'input str>,
    pub(crate) workspace: &'input str,
}

#[derive(Debug, Clone, Copy)]
enum TaskDependencyViewKind {
    BlockedBy,
    Blocks,
}

pub(crate) fn build_mismatch_warning(comparison: &BuildComparison) -> Option<String> {
    if comparison.matches {
        return None;
    }

    let changed_fields = comparison
        .differences
        .iter()
        .map(|difference| difference.field.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    Some(format!(
        "threadplane-cli {} ({}) differs from server {} ({}); changed fields: {}. Run `threadplane build compare` for details.",
        comparison.client.version,
        comparison.client.git_commit.as_deref().unwrap_or("unknown"),
        comparison.server.version,
        comparison.server.git_commit.as_deref().unwrap_or("unknown"),
        changed_fields,
    ))
}

fn parse_task_priority_input(input: &str) -> Result<TaskPriority> {
    TaskPriority::new(input).ok_or_else(|| {
        Usage {
            message: "priority cannot be empty".to_owned(),
        }
        .build()
    })
}

fn normalize_priority_name(input: &str) -> Result<String> {
    Ok(parse_task_priority_input(input)?.to_string())
}

fn parse_memory_kind_input(input: &str) -> Result<MemoryKind> {
    MemoryKind::new(input).ok_or_else(|| {
        Usage {
            message: "memory kind cannot be empty".to_owned(),
        }
        .build()
    })
}

fn parse_memory_audience_input(input: &str) -> Result<MemoryAudience> {
    input.parse().map_err(|_error| {
        Usage {
            message: format!("unsupported memory audience `{input}`"),
        }
        .build()
    })
}

fn parse_memory_importance_input(input: &str) -> Result<MemoryImportance> {
    input.parse().map_err(|_error| {
        Usage {
            message: format!("unsupported memory importance `{input}`"),
        }
        .build()
    })
}

fn parse_memory_scope_input(input: &str) -> Result<MemoryScope> {
    input.parse().map_err(|_error| {
        Usage {
            message: format!("unsupported memory scope `{input}`"),
        }
        .build()
    })
}

fn normalize_memory_filter_name(input: &str) -> Result<String> {
    let normalized = threadplane_core::normalize_memory_kind_name(input);
    if normalized.is_empty() {
        return Err(Usage {
            message: "memory filters cannot be empty".to_owned(),
        }
        .build());
    }

    Ok(normalized)
}

fn parse_public_key_algorithm(input: &str) -> Result<threadplane_core::PublicKeyAlgorithm> {
    input.parse().map_err(|_error| {
        Usage {
            message: format!("unsupported public-key algorithm `{input}`"),
        }
        .build()
    })
}

fn parse_public_key_algorithms(
    inputs: &[String],
) -> Result<Vec<threadplane_core::PublicKeyAlgorithm>> {
    inputs
        .iter()
        .map(String::as_str)
        .map(parse_public_key_algorithm)
        .collect()
}

fn parse_workspace_priority_specs(inputs: &[String]) -> Result<Vec<WorkspacePriority>> {
    inputs
        .iter()
        .map(String::as_str)
        .map(parse_workspace_priority_spec)
        .collect()
}

fn parse_workspace_role(input: &str) -> Result<WorkspaceRole> {
    input.parse().map_err(|_error| {
        Usage {
            message: format!("unsupported workspace role `{input}`"),
        }
        .build()
    })
}

fn parse_workspace_priority_spec(input: &str) -> Result<WorkspacePriority> {
    let mut parts = input.splitn(3, ':');
    let raw_name = parts.next().unwrap_or_default();
    let raw_rank = parts.next().ok_or_else(|| {
        Usage {
            message: format!(
                "priority definition `{input}` must look like name:rank[:description]"
            ),
        }
        .build()
    })?;
    let description = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let rank = raw_rank.parse::<u16>().map_err(|_error| {
        Usage {
            message: format!("priority rank `{raw_rank}` must be an unsigned integer"),
        }
        .build()
    })?;

    Ok(WorkspacePriority {
        description,
        name: normalize_priority_name(raw_name)?,
        rank,
    })
}

fn select_dependency_view_from_context(
    context: &TaskContext,
    kind: TaskDependencyViewKind,
) -> &[TaskDependencySummary] {
    match kind {
        TaskDependencyViewKind::BlockedBy => &context.dependencies,
        TaskDependencyViewKind::Blocks => &context.dependents,
    }
}

fn select_dependency_view_from_dag(
    dag: &TaskDag,
    kind: TaskDependencyViewKind,
) -> &[TaskDependencySummary] {
    match kind {
        TaskDependencyViewKind::BlockedBy => &dag.dependencies,
        TaskDependencyViewKind::Blocks => &dag.dependents,
    }
}

pub(crate) fn dedup_task_ids(task_ids: &[Uuid]) -> Vec<Uuid> {
    task_ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn task_metadata_from_args(
    metadata: TaskMetadataArgs,
    workspace_policy: &WorkspacePolicy,
) -> Result<TaskMetadata> {
    let priority = metadata
        .priority
        .as_deref()
        .map(parse_task_priority_input)
        .transpose()?
        .or_else(|| workspace_policy.priorities.default_task_priority())
        .ok_or_else(|| {
            Usage {
                message: "workspace policy does not define a usable default priority".to_owned(),
            }
            .build()
        })?;

    Ok(TaskMetadata {
        labels: normalize_task_labels(metadata.label),
        owner: normalize_task_owner(metadata.owner),
        priority,
    })
}

fn apply_metadata_patch(
    current: &TaskMetadata,
    patch: &TaskMetadataPatchArgs,
) -> Result<TaskMetadata> {
    let labels = if patch.clear_labels {
        Vec::new()
    } else if patch.label.is_empty() {
        current.labels.clone()
    } else {
        normalize_task_labels(patch.label.clone())
    };
    let owner = if patch.clear_owner {
        None
    } else if patch.owner.is_some() {
        normalize_task_owner(patch.owner.clone())
    } else {
        current.owner.clone()
    };

    Ok(TaskMetadata {
        labels,
        owner,
        priority: patch
            .priority
            .as_deref()
            .map(parse_task_priority_input)
            .transpose()?
            .unwrap_or_else(|| current.priority.clone()),
    })
}

fn task_metadata_changed(current: &TaskMetadata, next: &TaskMetadata) -> bool {
    current != next
}

fn triage_summary_labels(metadata: &TaskMetadataPatchArgs) -> Option<Vec<String>> {
    if metadata.clear_labels {
        return Some(Vec::new());
    }
    (!metadata.label.is_empty()).then(|| normalize_task_labels(metadata.label.clone()))
}

fn triage_summary_owner(metadata: &TaskMetadataPatchArgs) -> Option<String> {
    normalize_task_owner(metadata.owner.clone())
}

pub(crate) fn triage_has_changes(
    complete: bool,
    epic_id: Option<Uuid>,
    metadata: &TaskMetadataPatchArgs,
) -> bool {
    complete
        || epic_id.is_some()
        || metadata.priority.is_some()
        || metadata.clear_owner
        || metadata.owner.is_some()
        || metadata.clear_labels
        || !metadata.label.is_empty()
}
