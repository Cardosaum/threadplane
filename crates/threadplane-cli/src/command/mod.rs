#![expect(
    clippy::arbitrary_source_item_ordering,
    clippy::redundant_pub_crate,
    reason = "CLI commands are crate-internal and keep explicit visibility and workflow grouping for readability."
)]

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
mod reads;
pub(crate) mod render;
mod task;
mod workspace;

#[cfg(test)]
mod tests_execution;

pub(crate) use content::{
    ListMemories, ListNotes, MemoryCommand, MemorySubcommand, NoteCommand, NoteSubcommand,
    PrimeMemories, SearchNotes,
};
pub(crate) use executor::execute;
pub(crate) use reads::{
    EntityCommand, EntitySubcommand, EventsCommand, EventsSubcommand, RelatedEntities, ShowEntity,
    TailEvents,
};
#[expect(
    unused_imports,
    reason = "The command module re-exports selected task types as the import seam for sibling modules and tests."
)]
pub(crate) use task::{
    apply_metadata_patch, dedup_task_ids, normalize_priority_name, parse_task_priority_input,
    select_dependency_view_from_context, select_dependency_view_from_dag, task_metadata_changed,
    task_metadata_from_args, triage_has_changes, triage_summary_labels, triage_summary_owner,
    AddTaskDependency, ClaimNextTask, ClaimTask, CompleteTask, ListTasks, NextTask, OfferTask,
    ReleaseTask, ShowTask, TaskCommand, TaskContextCommand, TaskDagCommand,
    TaskDependencyViewCommand, TaskDependencyViewKind, TaskMetadataArgs, TaskMetadataFilterArgs,
    TaskMetadataPatchArgs, TaskStatusValue, TaskSubcommand, TaskTriageOutcome, TaskTriageSummary,
    TriageTasks, UpdateTask,
};
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

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum OutputFormat {
    Compact,
    #[default]
    Json,
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
