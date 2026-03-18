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
mod parse;
pub(crate) mod paths;
mod reads;
pub(crate) mod render;
mod shared;
mod system;
mod task;
mod workspace;

#[cfg(test)]
mod tests_execution;

pub(crate) use content::{
    EpicCommand, EpicSubcommand, LinkCommand, LinkSubcommand, ListMemories, ListNotes,
    MemoryCommand, MemorySubcommand, NoteCommand, NoteSubcommand, PrimeMemories, SearchNotes,
};
pub(crate) use executor::execute;
pub(crate) use parse::{
    normalize_memory_filter_name, parse_memory_audience_input, parse_memory_importance_input,
    parse_memory_kind_input, parse_memory_scope_input, parse_public_key_algorithm,
    parse_public_key_algorithms, parse_workspace_priority_specs, parse_workspace_role,
};
pub(crate) use reads::{
    EntityCommand, EntitySubcommand, EventsCommand, EventsSubcommand, RelatedEntities, ShowEntity,
    TailEvents,
};
pub(crate) use shared::{build_mismatch_warning, MemoryListPathArgs, OutputFormat};
pub(crate) use system::{
    BuildCommand, BuildSubcommand, ConfigCommand, ConfigSubcommand, ProjectionCommand,
    ProjectionSubcommand,
};
#[cfg(test)]
pub(crate) use task::TaskMetadataFilterArgs;
pub(crate) use task::{
    apply_metadata_patch, dedup_task_ids, normalize_priority_name, parse_task_priority_input,
    select_dependency_view_from_context, select_dependency_view_from_dag, task_metadata_changed,
    task_metadata_from_args, triage_has_changes, triage_summary_labels, triage_summary_owner,
    AddTaskDependency, ClaimNextTask, ClaimTask, CompleteTask, ListTasks, NextTask, OfferTask,
    ReleaseTask, ShowTask, TaskCommand, TaskContextCommand, TaskDagCommand,
    TaskDependencyViewCommand, TaskDependencyViewKind, TaskMetadataArgs, TaskMetadataPatchArgs,
    TaskStatusValue, TaskSubcommand, TaskTriageOutcome, TaskTriageSummary, TriageTasks, UpdateTask,
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
