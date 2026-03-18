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
#[command(about = "Inspect and manage workspace policy, memberships, and public keys")]
struct WorkspaceCommand {
    #[command(subcommand)]
    command: WorkspaceSubcommand,
}

#[derive(Debug, Subcommand)]
enum WorkspaceSubcommand {
    #[command(about = "Add or update an actor public key for a workspace")]
    KeyAdd(WorkspaceKeyAdd),
    #[command(about = "List actor public keys registered for a workspace")]
    KeyList(WorkspaceKeyList),
    #[command(about = "Grant or update a workspace membership")]
    MemberGrant(WorkspaceMemberGrant),
    #[command(about = "List workspace memberships")]
    MemberList(WorkspaceMemberList),
    #[command(about = "Replace the workspace governance policy")]
    PolicySet(WorkspacePolicySet),
    #[command(about = "Show the effective workspace governance policy")]
    PolicyShow(WorkspacePolicyShow),
}

#[derive(Debug, Args)]
struct WorkspacePolicyShow {
    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args)]
struct WorkspacePolicySet {
    #[arg(long, help = "Admin actor updating the workspace policy")]
    actor: String,

    #[arg(
        long = "allowed-algorithm",
        help = "Allowed public-key algorithm. Repeat for multiple algorithms.",
        required = true
    )]
    allowed_algorithms: Vec<String>,

    #[arg(long, help = "Challenge TTL in seconds")]
    challenge_ttl_seconds: u32,

    #[arg(long, help = "Default task priority name")]
    default_priority: String,

    #[arg(
        long = "priority",
        help = "Priority definition as name:rank[:description]. Repeat for multiple priorities.",
        required = true
    )]
    priorities: Vec<String>,

    #[arg(long, help = "Require signed commands for workspace mutations")]
    signed_commands_required: bool,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args)]
struct WorkspaceMemberList {
    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args)]
struct WorkspaceMemberGrant {
    #[arg(long, help = "Admin actor granting the membership")]
    actor: String,

    #[arg(long, help = "Member actor ID")]
    member_actor_id: String,

    #[arg(long, help = "Workspace role to grant")]
    role: String,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args)]
struct WorkspaceKeyList {
    #[arg(long, help = "Optional actor ID filter")]
    actor_id: Option<String>,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args)]
struct WorkspaceKeyAdd {
    #[arg(long, help = "Admin actor registering the key")]
    actor: String,

    #[arg(long, help = "Public-key algorithm")]
    algorithm: String,

    #[arg(long, help = "Key ID")]
    key_id: String,

    #[arg(long, help = "Member actor ID")]
    member_actor_id: String,

    #[arg(long, help = "Public key material")]
    public_key: String,

    #[arg(long, help = "Workspace name")]
    workspace: String,
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
#[command(about = "Create, inspect, and update notes")]
struct NoteCommand {
    #[command(subcommand)]
    command: NoteSubcommand,
}

#[derive(Debug, Args)]
#[command(about = "Capture and recall durable memories for people and AI agents")]
struct MemoryCommand {
    #[command(subcommand)]
    command: MemorySubcommand,
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

#[derive(Debug, Subcommand)]
enum NoteSubcommand {
    #[command(about = "Create a new note in a workspace")]
    Add(AddNote),
    #[command(about = "List notes in a workspace")]
    List(ListNotes),
    #[command(about = "Search notes by title/body text")]
    Search(SearchNotes),
    #[command(about = "Fetch a note by ID")]
    Show(ShowNote),
    #[command(about = "Update a note and propagate through Xanadu links when present")]
    Update(UpdateNote),
}

#[derive(Debug, Subcommand)]
enum MemorySubcommand {
    #[command(about = "Create a new structured memory in a workspace")]
    Add(AddMemory),
    #[command(about = "List memories with structured filters")]
    List(ListMemories),
    #[command(about = "Recall the startup memories an agent or human should prime with")]
    Prime(PrimeMemories),
    #[command(about = "Fetch a memory by ID")]
    Show(ShowMemory),
}

#[derive(Debug, Args)]
struct AddMemory {
    #[arg(long, help = "Structured audience: agent, human, or both")]
    audience: String,

    #[arg(long, help = "Who is recording the memory")]
    author: String,

    #[arg(long, help = "Memory body")]
    body: String,

    #[arg(long, help = "Importance: normal, high, or critical")]
    importance: String,

    #[arg(long, help = "Memory kind, for example workflow, decision, or runbook")]
    kind: String,

    #[arg(
        long = "recall-trigger",
        help = "Recall trigger tag, for example session_start. Repeat for multiple triggers."
    )]
    recall_triggers: Vec<String>,

    #[arg(long, help = "Scope: workspace, repo, or global")]
    scope: String,

    #[arg(long = "tag", help = "Memory tag. Repeat for multiple tags.")]
    tags: Vec<String>,

    #[arg(long, help = "Memory title")]
    title: String,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args)]
struct ListMemories {
    #[arg(
        long,
        help = "Only include memories for this audience: agent, human, or both"
    )]
    audience: Option<String>,

    #[arg(
        long,
        default_value = "json",
        help = "Render JSON or a compact human-readable summary"
    )]
    format: OutputFormat,

    #[arg(long, help = "Only include memories with this importance")]
    importance: Option<String>,

    #[arg(long, help = "Only include memories with this kind")]
    kind: Option<String>,

    #[arg(long, help = "Maximum number of memories to return")]
    limit: Option<i64>,

    #[arg(long, help = "Search query matched against memory title and body")]
    query: Option<String>,

    #[arg(
        long = "recall-trigger",
        help = "Only include memories with this recall trigger"
    )]
    recall_trigger: Option<String>,

    #[arg(long, help = "Only include memories with this tag")]
    tag: Option<String>,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args)]
struct PrimeMemories {
    #[arg(
        long,
        default_value = "agent",
        help = "Recall memories for this audience: agent, human, or both"
    )]
    audience: String,

    #[arg(
        long,
        default_value = "json",
        help = "Render JSON or a compact human-readable summary"
    )]
    format: OutputFormat,

    #[arg(long, help = "Maximum number of memories to return")]
    limit: Option<i64>,

    #[arg(long = "recall-trigger", default_value = "session_start")]
    recall_trigger: String,

    #[arg(long, default_value = "prime")]
    tag: String,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args)]
struct ShowMemory {
    #[arg(long, help = "Memory UUID")]
    memory_id: Uuid,
}

#[derive(Debug, Args)]
struct AddNote {
    #[arg(long, help = "Note author")]
    author: String,

    #[arg(long, help = "Note body")]
    body: String,

    #[arg(long, help = "Note title")]
    title: String,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args)]
struct ListNotes {
    #[arg(long, help = "Only include notes from this author")]
    author: Option<String>,

    #[arg(
        long,
        default_value = "json",
        help = "Render JSON or a compact human-readable summary"
    )]
    format: OutputFormat,

    #[arg(long, help = "Maximum number of notes to return")]
    limit: Option<i64>,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args)]
struct SearchNotes {
    #[arg(long, help = "Only include notes from this author")]
    author: Option<String>,

    #[arg(
        long,
        default_value = "json",
        help = "Render JSON or a compact human-readable summary"
    )]
    format: OutputFormat,

    #[arg(long, help = "Maximum number of notes to return")]
    limit: Option<i64>,

    #[arg(long, help = "Search query matched against note title and body")]
    query: String,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args)]
struct ShowNote {
    #[arg(long, help = "Note UUID")]
    note_id: Uuid,
}

#[derive(Debug, Args)]
struct UpdateNote {
    #[arg(long, help = "Actor performing the update")]
    actor: String,

    #[arg(long, help = "Updated note body")]
    body: String,

    #[arg(long, help = "Note UUID")]
    note_id: Uuid,

    #[arg(long, help = "Updated note title")]
    title: String,

    #[arg(long, help = "Workspace name")]
    workspace: String,
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
enum OutputFormat {
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

pub(crate) fn execute<'cfg, 'ctx, A, O, S>(
    cli: Cli,
    config: &'cfg ThreadplaneConfig,
    discovery: &'cfg ConfigDiscovery,
    context: &'ctx mut CommandContext<'ctx, A, O, S>,
) -> Result<()>
where
    A: ApiClient,
    O: CommandOutput,
    S: Sleeper,
{
    CommandExecutor::new(config, discovery, context).execute(cli)
}

struct CommandExecutor<'cfg, 'ctx, A, O, S> {
    config: &'cfg ThreadplaneConfig,
    context: &'ctx mut CommandContext<'ctx, A, O, S>,
    discovery: &'cfg ConfigDiscovery,
}

impl<'cfg, 'ctx, A, O, S> CommandExecutor<'cfg, 'ctx, A, O, S>
where
    A: ApiClient,
    O: CommandOutput,
    S: Sleeper,
{
    const fn new(
        config: &'cfg ThreadplaneConfig,
        discovery: &'cfg ConfigDiscovery,
        context: &'ctx mut CommandContext<'ctx, A, O, S>,
    ) -> Self {
        Self {
            config,
            context,
            discovery,
        }
    }

    fn execute(&mut self, cli: Cli) -> Result<()> {
        let Cli {
            command: root_command,
            idempotency_key: command_idempotency_key,
            ..
        } = cli;
        let idempotency_key = command_idempotency_key.as_deref();

        match root_command {
            Command::Build(build_command) => self.handle_build(&build_command)?,
            Command::Config(config_command) => self.handle_config(&config_command)?,
            Command::Entity(entity_command) => self.handle_entity(entity_command)?,
            Command::Epic(epic_command) => self.handle_epic(idempotency_key, epic_command)?,
            Command::Events(events_command) => self.handle_events(events_command)?,
            Command::Link(link_command) => self.handle_link(idempotency_key, link_command)?,
            Command::Memory(memory_command) => self.handle_memory(idempotency_key, memory_command)?,
            Command::Note(note_command) => self.handle_note(idempotency_key, note_command)?,
            Command::Projection(projection_command) => self.handle_projection(&projection_command)?,
            Command::Scope => self.handle_scope()?,
            Command::Task(task_command) => self.handle_task(idempotency_key, task_command)?,
            Command::Workspace(workspace_command) => {
                self.handle_workspace(idempotency_key, workspace_command)?;
            }
        }

        Ok(())
    }

    fn handle_build(&mut self, command: &BuildCommand) -> Result<()> {
        match command.command {
            BuildSubcommand::Show => self.context.print_value(&current_build_info()),
            BuildSubcommand::Compare => {
                let snapshot: ServiceSnapshot = self.context.get_json("/")?;
                let comparison = compare_build_info(&current_build_info(), &snapshot.build);
                self.context.print_value(&comparison)
            }
        }
    }

    fn handle_epic(&mut self, idempotency_key: Option<&str>, command: EpicCommand) -> Result<()> {
        match command.command {
            EpicSubcommand::Add(epic) => {
                let request = CreateEpicRequest {
                    workspace: epic.workspace,
                    author: epic.author,
                    title: epic.title,
                    body: epic.body,
                };
                let response: serde_json::Value =
                    self.context
                        .post_json("/v1/epics", &request, idempotency_key)?;
                self.context.print_value(&response)
            }
            EpicSubcommand::List(epics) => {
                let path = format!("/v1/workspaces/{}/epics", epics.workspace);
                let response: serde_json::Value = self.context.get_json(&path)?;
                self.context.print_value(&response)
            }
            EpicSubcommand::Show(epic) => {
                let path = format!("/v1/epics/{}", epic.epic_id);
                let response: serde_json::Value = self.context.get_json(&path)?;
                self.context.print_value(&response)
            }
        }
    }

    fn handle_config(&mut self, command: &ConfigCommand) -> Result<()> {
        match command.command {
            ConfigSubcommand::Show => {
                let payload = json!({
                    "config": self.config,
                    "discovery": {
                        "search_order": self.discovery.search_order,
                        "selected_path": self.discovery.selected_path,
                        "explicit_override": self.discovery.explicit_override,
                        "env_override": self.discovery.env_override,
                        "env_prefix": self.discovery.env_prefix,
                    }
                });
                self.context.print_value(&payload)
            }
        }
    }

    fn handle_workspace(
        &mut self,
        idempotency_key: Option<&str>,
        command: WorkspaceCommand,
    ) -> Result<()> {
        match command.command {
            WorkspaceSubcommand::PolicyShow(workspace) => {
                let response: ApiEnvelope<WorkspacePolicy> =
                    self.context
                        .get_json(&workspace_policy_path(&workspace.workspace))?;
                self.context.print_value(&response)
            }
            WorkspaceSubcommand::PolicySet(workspace) => {
                let request = UpdateWorkspacePolicyRequest {
                    actor: workspace.actor,
                    auth: WorkspaceAuthPolicy {
                        allowed_algorithms: parse_public_key_algorithms(
                            &workspace.allowed_algorithms,
                        )?,
                        challenge_ttl_seconds: workspace.challenge_ttl_seconds,
                        signed_commands_required: workspace.signed_commands_required,
                    },
                    priorities: WorkspacePriorityPolicy {
                        default_priority: normalize_priority_name(&workspace.default_priority)?,
                        priorities: parse_workspace_priority_specs(&workspace.priorities)?,
                    },
                    workspace: workspace.workspace.clone(),
                };
                let response: ApiEnvelope<WorkspacePolicy> = self.context.put_json(
                    &workspace_policy_path(&workspace.workspace),
                    &request,
                    idempotency_key,
                )?;
                self.context.print_value(&response)
            }
            WorkspaceSubcommand::MemberList(workspace) => {
                let response: ApiEnvelope<Vec<WorkspaceMembership>> =
                    self.context.get_json(&workspace_memberships_path(&workspace.workspace))?;
                self.context.print_value(&response)
            }
            WorkspaceSubcommand::MemberGrant(workspace) => {
                let request = GrantWorkspaceMembershipRequest {
                    actor: workspace.actor,
                    member_actor_id: workspace.member_actor_id,
                    role: parse_workspace_role(&workspace.role)?,
                    workspace: workspace.workspace.clone(),
                };
                let response: ApiEnvelope<WorkspaceMembership> = self.context.post_json(
                    &workspace_memberships_path(&workspace.workspace),
                    &request,
                    idempotency_key,
                )?;
                self.context.print_value(&response)
            }
            WorkspaceSubcommand::KeyList(workspace) => {
                let response: ApiEnvelope<Vec<ActorPublicKey>> = self.context.get_json(
                    &workspace_keys_path(&workspace.workspace, workspace.actor_id.as_deref()),
                )?;
                self.context.print_value(&response)
            }
            WorkspaceSubcommand::KeyAdd(workspace) => {
                let request = AddWorkspacePublicKeyRequest {
                    actor: workspace.actor,
                    algorithm: parse_public_key_algorithm(&workspace.algorithm)?,
                    key_id: workspace.key_id,
                    member_actor_id: workspace.member_actor_id,
                    public_key: workspace.public_key,
                    workspace: workspace.workspace.clone(),
                };
                let response: ApiEnvelope<ActorPublicKey> = self.context.post_json(
                    &workspace_keys_path(&workspace.workspace, None),
                    &request,
                    idempotency_key,
                )?;
                self.context.print_value(&response)
            }
        }
    }

    fn handle_entity(&mut self, command: EntityCommand) -> Result<()> {
        match command.command {
            EntitySubcommand::Show(entity) => self.handle_show_entity(&entity),
            EntitySubcommand::Related(entity) => self.handle_related_entities(&entity),
        }
    }

    fn handle_show_entity(&mut self, entity: &ShowEntity) -> Result<()> {
        let path = entity_show_path(entity.entity_ref.as_str());
        let response: ApiEnvelope<EntityContext> = self.context.get_json(&path)?;

        match entity.format {
            OutputFormat::Compact => {
                self.context.print_compact(&render_entity_context_compact(&response.data));
                Ok(())
            }
            OutputFormat::Json => self.context.print_value(&response),
        }
    }

    fn handle_related_entities(&mut self, entity: &RelatedEntities) -> Result<()> {
        let path = entity_relations_path(entity.entity_ref.as_str());
        let response: ApiEnvelope<Vec<GraphRelation>> = self.context.get_json(&path)?;

        match entity.format {
            OutputFormat::Compact => {
                self.context
                    .print_compact(&render_graph_relations_compact(&response.data));
                Ok(())
            }
            OutputFormat::Json => self.context.print_value(&response),
        }
    }

    fn handle_events(&mut self, command: EventsCommand) -> Result<()> {
        match command.command {
            EventsSubcommand::List(events) => {
                let path = events_list_path(events.workspace.as_str(), events.limit);
                let response: ApiEnvelope<Vec<EventRecord>> = self.context.get_json(&path)?;
                match events.format {
                    OutputFormat::Compact => {
                        self.context
                            .print_compact(&render_event_list_compact(&response.data));
                        Ok(())
                    }
                    OutputFormat::Json => self.context.print_value(&response),
                }
            }
            EventsSubcommand::Tail(events) => self.handle_tail_events(&events),
        }
    }

    fn handle_scope(&mut self) -> Result<()> {
        let scope: serde_json::Value = self.context.get_json("/scope")?;
        let snapshot: ServiceSnapshot = self.context.get_json("/")?;
        let comparison = compare_build_info(&current_build_info(), &snapshot.build);

        if let Some(warning) = build_mismatch_warning(&comparison) {
            self.context.warn(&format!("warning: {warning}"));
        }

        self.context.print_value(&scope)
    }

    fn handle_link(
        &mut self,
        idempotency_key: Option<&str>,
        command: LinkCommand,
    ) -> Result<()> {
        match command.command {
            LinkSubcommand::Add(link) => {
                let request = AddLinkRequest {
                    workspace: link.workspace,
                    actor: link.actor,
                    from: link.from,
                    to: link.to,
                    relation: link.relation,
                };
                let response: serde_json::Value =
                    self.context
                        .post_json("/v1/links", &request, idempotency_key)?;
                self.context.print_value(&response)
            }
            LinkSubcommand::Xanadu(link) => {
                let request = CreateXanaduLinkRequest {
                    workspace: link.workspace,
                    actor: link.actor,
                    from: link.from,
                    to: link.to,
                };
                let response: serde_json::Value =
                    self.context
                        .post_json("/v1/links/xanadu", &request, idempotency_key)?;
                self.context.print_value(&response)
            }
        }
    }

    fn handle_note(
        &mut self,
        idempotency_key: Option<&str>,
        command: NoteCommand,
    ) -> Result<()> {
        match command.command {
            NoteSubcommand::Add(add) => {
                let request = CreateNoteRequest {
                    workspace: add.workspace,
                    author: add.author,
                    title: add.title,
                    body: add.body,
                };
                let response: serde_json::Value =
                    self.context
                        .post_json("/v1/notes", &request, idempotency_key)?;
                self.context.print_value(&response)
            }
            NoteSubcommand::List(list) => self.handle_list_notes(&list),
            NoteSubcommand::Search(search) => self.handle_search_notes(&search),
            NoteSubcommand::Show(show) => {
                let path = note_path(show.note_id);
                let response: serde_json::Value = self.context.get_json(&path)?;
                self.context.print_value(&response)
            }
            NoteSubcommand::Update(update) => {
                let path = note_path(update.note_id);
                let request = UpdateNoteRequest {
                    workspace: update.workspace,
                    actor: update.actor,
                    note_id: update.note_id,
                    title: update.title,
                    body: update.body,
                };
                let response: serde_json::Value =
                    self.context.patch_json(&path, &request, idempotency_key)?;
                self.context.print_value(&response)
            }
        }
    }

    fn handle_memory(
        &mut self,
        idempotency_key: Option<&str>,
        command: MemoryCommand,
    ) -> Result<()> {
        match command.command {
            MemorySubcommand::Add(add) => {
                let request = CreateMemoryRequest {
                    workspace: add.workspace,
                    author: add.author,
                    title: add.title,
                    body: add.body,
                    kind: parse_memory_kind_input(&add.kind)?,
                    scope: parse_memory_scope_input(&add.scope)?,
                    audience: parse_memory_audience_input(&add.audience)?,
                    importance: parse_memory_importance_input(&add.importance)?,
                    tags: add.tags,
                    recall_triggers: add.recall_triggers,
                };
                let response: serde_json::Value =
                    self.context
                        .post_json("/v1/memories", &request, idempotency_key)?;
                self.context.print_value(&response)
            }
            MemorySubcommand::List(list) => self.handle_list_memories(&list),
            MemorySubcommand::Prime(prime) => self.handle_prime_memories(&prime),
            MemorySubcommand::Show(show) => {
                let path = memory_path(show.memory_id);
                let response: serde_json::Value = self.context.get_json(&path)?;
                self.context.print_value(&response)
            }
        }
    }

    fn handle_list_memories(&mut self, memory: &ListMemories) -> Result<()> {
        let path = memory_list_path(MemoryListPathArgs {
            audience: memory.audience.as_deref(),
            importance: memory.importance.as_deref(),
            kind: memory.kind.as_deref(),
            limit: memory.limit,
            query: memory.query.as_deref(),
            recall_trigger: memory.recall_trigger.as_deref(),
            tag: memory.tag.as_deref(),
            workspace: memory.workspace.as_str(),
        })?;
        let response: ApiEnvelope<Vec<MemoryRecord>> = self.context.get_json(&path)?;

        match memory.format {
            OutputFormat::Compact => {
                self.context
                    .print_compact(&render_memory_list_compact(&response.data));
                Ok(())
            }
            OutputFormat::Json => self.context.print_value(&response),
        }
    }

    fn handle_prime_memories(&mut self, memory: &PrimeMemories) -> Result<()> {
        let path = memory_prime_path(memory)?;
        let response: ApiEnvelope<Vec<MemoryRecord>> = self.context.get_json(&path)?;

        match memory.format {
            OutputFormat::Compact => {
                self.context
                    .print_compact(&render_memory_list_compact(&response.data));
                Ok(())
            }
            OutputFormat::Json => self.context.print_value(&response),
        }
    }

    fn handle_list_notes(&mut self, note: &ListNotes) -> Result<()> {
        let path = note_list_path(
            note.workspace.as_str(),
            note.limit,
            note.author.as_deref(),
            None,
        );
        let response: ApiEnvelope<Vec<NoteRecord>> = self.context.get_json(&path)?;

        match note.format {
            OutputFormat::Compact => {
                self.context
                    .print_compact(&render_note_list_compact(&response.data));
                Ok(())
            }
            OutputFormat::Json => self.context.print_value(&response),
        }
    }

    fn handle_search_notes(&mut self, note: &SearchNotes) -> Result<()> {
        let path = note_list_path(
            note.workspace.as_str(),
            note.limit,
            note.author.as_deref(),
            Some(note.query.as_str()),
        );
        let response: ApiEnvelope<Vec<NoteRecord>> = self.context.get_json(&path)?;

        match note.format {
            OutputFormat::Compact => {
                self.context
                    .print_compact(&render_note_list_compact(&response.data));
                Ok(())
            }
            OutputFormat::Json => self.context.print_value(&response),
        }
    }

    fn handle_projection(&mut self, command: &ProjectionCommand) -> Result<()> {
        match command.command {
            ProjectionSubcommand::Status => {
                let response: ApiEnvelope<ProjectionStatus> =
                    self.context.get_json("/v1/projections/graph")?;
                self.context.print_value(&response)
            }
        }
    }
    fn handle_task(
        &mut self,
        idempotency_key: Option<&str>,
        command: TaskCommand,
    ) -> Result<()> {
        match command.command {
            TaskSubcommand::BlockedBy(task) => {
                self.handle_task_dependency_view(&task, TaskDependencyViewKind::BlockedBy)
            }
            TaskSubcommand::Blocks(task) => {
                self.handle_task_dependency_view(&task, TaskDependencyViewKind::Blocks)
            }
            TaskSubcommand::ClaimNext(task) => self.handle_claim_next_task(idempotency_key, task),
            TaskSubcommand::Claim(task) => self.handle_claim_task(idempotency_key, task),
            TaskSubcommand::Complete(task) => self.handle_complete_task(idempotency_key, task),
            TaskSubcommand::Context(task) => self.handle_task_context(&task),
            TaskSubcommand::Dag(task) => self.handle_task_dag(&task),
            TaskSubcommand::Depend(task) => self.handle_add_task_dependency(idempotency_key, task),
            TaskSubcommand::List(task) => self.handle_list_tasks(&task),
            TaskSubcommand::Next(task) => self.handle_next_task(&task),
            TaskSubcommand::Offer(task) => self.handle_offer_task(idempotency_key, task),
            TaskSubcommand::Release(task) => self.handle_release_task(idempotency_key, task),
            TaskSubcommand::Show(task) => self.handle_show_task(&task),
            TaskSubcommand::Triage(task) => {
                let response = self.triage_tasks(idempotency_key, &task)?;
                self.context.print_value(&response)
            }
            TaskSubcommand::Update(task) => self.handle_update_task(idempotency_key, task),
        }
    }

    fn handle_add_task_dependency(
        &mut self,
        idempotency_key: Option<&str>,
        task: AddTaskDependency,
    ) -> Result<()> {
        let request = AddTaskDependencyRequest {
            workspace: task.workspace,
            actor: task.actor,
            task_id: task.task_id,
            depends_on_task_id: task.depends_on,
        };
        let response: serde_json::Value = self.context.post_json(
            &task_dependencies_path(task.task_id),
            &request,
            idempotency_key,
        )?;
        self.context.print_value(&response)
    }

    fn handle_claim_task(
        &mut self,
        idempotency_key: Option<&str>,
        task: ClaimTask,
    ) -> Result<()> {
        let request = ClaimTaskRequest {
            workspace: task.workspace,
            actor: task.actor,
            task_id: task.task_id,
            lease_seconds: task.lease_seconds,
        };
        let path = task_claims_path(task.task_id);
        let response: serde_json::Value =
            self.context.post_json(&path, &request, idempotency_key)?;
        self.context.print_value(&response)
    }

    fn handle_claim_next_task(
        &mut self,
        idempotency_key: Option<&str>,
        task: ClaimNextTask,
    ) -> Result<()> {
        let request = ClaimNextTaskRequest {
            actor: task.actor,
            epic_id: task.epic_id,
            label: task
                .label
                .and_then(|value| normalize_task_labels(vec![value]).into_iter().next()),
            lease_seconds: task.lease_seconds,
            owner: normalize_task_owner(task.metadata_filters.owner),
            priority: task
                .metadata_filters
                .priority
                .as_deref()
                .map(parse_task_priority_input)
                .transpose()?,
            workspace: task.workspace,
        };
        let response: ApiEnvelope<Option<TaskClaimRecord>> =
            self.context
                .post_json("/v1/tasks/claims/next", &request, idempotency_key)?;
        self.context.print_value(&response)
    }

    fn handle_complete_task(
        &mut self,
        idempotency_key: Option<&str>,
        task: CompleteTask,
    ) -> Result<()> {
        let request = CompleteTaskRequest {
            workspace: task.workspace,
            actor: task.actor,
            task_id: task.task_id,
        };
        let path = task_completion_path(task.task_id);
        let response: serde_json::Value =
            self.context.post_json(&path, &request, idempotency_key)?;
        self.context.print_value(&response)
    }

    fn handle_list_tasks(&mut self, task: &ListTasks) -> Result<()> {
        let path = task_list_path(task)?;
        let response: ApiEnvelope<Vec<TaskListEntry>> = self.context.get_json(&path)?;

        match task.format {
            OutputFormat::Compact => {
                self.context
                    .print_compact(&render_task_list_compact(&response.data));
                Ok(())
            }
            OutputFormat::Json => self.context.print_value(&response),
        }
    }

    fn handle_next_task(&mut self, task: &NextTask) -> Result<()> {
        let path = task_next_path(task)?;
        let response: ApiEnvelope<Option<TaskListEntry>> = self.context.get_json(&path)?;

        match task.format {
            OutputFormat::Compact => {
                let rendered = response.data.map_or_else(
                    || "no tasks\n".to_owned(),
                    |entry| render_task_list_compact(&[entry]),
                );
                self.context.print_compact(&rendered);
                Ok(())
            }
            OutputFormat::Json => self.context.print_value(&response),
        }
    }

    fn handle_offer_task(
        &mut self,
        idempotency_key: Option<&str>,
        task: OfferTask,
    ) -> Result<()> {
        let workspace_policy = self.fetch_workspace_policy_summary(&task.workspace)?;
        let request = OfferTaskRequest {
            workspace: task.workspace,
            author: task.author,
            depends_on: task.depends_on,
            title: task.title,
            details: task.details,
            epic_id: task.epic_id,
            metadata: task_metadata_from_args(task.metadata, &workspace_policy)?,
        };
        let response: serde_json::Value =
            self.context.post_json("/v1/tasks", &request, idempotency_key)?;
        self.context.print_value(&response)
    }

    fn handle_release_task(
        &mut self,
        idempotency_key: Option<&str>,
        task: ReleaseTask,
    ) -> Result<()> {
        let request = ReleaseTaskRequest {
            workspace: task.workspace,
            actor: task.actor,
            task_id: task.task_id,
        };
        let path = task_claim_release_path(task.task_id);
        let response: serde_json::Value =
            self.context.post_json(&path, &request, idempotency_key)?;
        self.context.print_value(&response)
    }

    fn handle_show_task(&mut self, task: &ShowTask) -> Result<()> {
        let path = task_path(task.task_id);
        let response: serde_json::Value = self.context.get_json(&path)?;
        self.context.print_value(&response)
    }

    fn handle_task_context(&mut self, task: &TaskContextCommand) -> Result<()> {
        let path = task_context_path(task.task_id);
        let response: serde_json::Value = self.context.get_json(&path)?;
        self.context.print_value(&response)
    }

    fn handle_task_dag(&mut self, task: &TaskDagCommand) -> Result<()> {
        let path = task_dag_path(task.task_id);
        let response: serde_json::Value = self.context.get_json(&path)?;
        self.context.print_value(&response)
    }

    fn handle_update_task(
        &mut self,
        idempotency_key: Option<&str>,
        task: UpdateTask,
    ) -> Result<()> {
        let workspace_policy = self.fetch_workspace_policy_summary(&task.workspace)?;
        let request = UpdateTaskRequest {
            workspace: task.workspace,
            actor: task.actor,
            task_id: task.task_id,
            title: task.title,
            details: task.details,
            epic_id: task.epic_id,
            metadata: task_metadata_from_args(task.metadata, &workspace_policy)?,
        };
        let path = task_path(task.task_id);
        let response: serde_json::Value =
            self.context.patch_json(&path, &request, idempotency_key)?;
        self.context.print_value(&response)
    }

    fn fetch_task_context(&self, task_id: Uuid) -> Result<TaskContext> {
        let path = task_context_path(task_id);
        let response: ApiEnvelope<TaskContext> = self.context.get_json(&path)?;
        Ok(response.data)
    }

    fn handle_tail_events(&mut self, events: &TailEvents) -> Result<()> {
        let mut cursor = events.after_event_id;

        loop {
            let path = events_tail_path(events.workspace.as_str(), events.limit, cursor);
            let response: ApiEnvelope<Vec<EventRecord>> = self.context.get_json(&path)?;
            let latest_event_id = response.data.last().map(|event| event.event_id);

            match events.format {
                OutputFormat::Compact => {
                    if !response.data.is_empty() {
                        self.context
                            .print_compact(&render_event_list_compact(&response.data));
                    }
                    if response.data.is_empty() && !events.follow {
                        self.context.print_compact("no events\n");
                    }
                }
                OutputFormat::Json => self.context.print_value(&response)?,
            }

            cursor = latest_event_id.or(cursor);
            if !events.follow {
                return Ok(());
            }

            self.context.sleep(Duration::from_secs(events.poll_seconds));
        }
    }

    fn fetch_task_dag(&self, task_id: Uuid) -> Result<TaskDag> {
        let path = task_dag_path(task_id);
        let response: ApiEnvelope<TaskDag> = self.context.get_json(&path)?;
        Ok(response.data)
    }

    fn fetch_task_summary(&self, task_id: Uuid) -> Result<TaskRecord> {
        let path = task_path(task_id);
        let response: ApiEnvelope<TaskRecord> = self.context.get_json(&path)?;
        Ok(response.data)
    }

    fn handle_task_dependency_view(
        &mut self,
        task: &TaskDependencyViewCommand,
        kind: TaskDependencyViewKind,
    ) -> Result<()> {
        let data = if task.direct_only {
            let context = self.fetch_task_context(task.task_id)?;
            select_dependency_view_from_context(&context, kind).to_vec()
        } else {
            let dag = self.fetch_task_dag(task.task_id)?;
            select_dependency_view_from_dag(&dag, kind).to_vec()
        };

        match task.format {
            OutputFormat::Compact => {
                self.context
                    .print_compact(&render_task_dependency_compact(&data));
                Ok(())
            }
            OutputFormat::Json => self.context.print_value(&data),
        }
    }

    fn fetch_workspace_policy_summary(&self, workspace: &str) -> Result<WorkspacePolicy> {
        let response: ApiEnvelope<WorkspacePolicy> =
            self.context.get_json(&workspace_policy_path(workspace))?;
        Ok(response.data)
    }

    fn triage_tasks(
        &self,
        idempotency_key: Option<&str>,
        task: &TriageTasks,
    ) -> Result<TaskTriageSummary> {
        if !triage_has_changes(task.complete, task.epic_id, &task.metadata) {
            return Usage {
                message:
                    "task triage needs at least --epic-id, --complete, --priority, --owner, --clear-owner, --label, or --clear-labels"
                        .to_owned(),
            }
            .fail();
        }

        let task_ids = dedup_task_ids(&task.task_id);
        let mut completed_task_ids = Vec::new();
        let mut unchanged_task_ids = Vec::new();
        let mut updated_task_ids = Vec::new();

        for task_id in &task_ids {
            let task_record = self.fetch_task_summary(*task_id)?;
            let next_metadata = apply_metadata_patch(&task_record.metadata, &task.metadata)?;
            let outcome = self.triage_task_record(
                idempotency_key,
                task,
                *task_id,
                &task_record,
                &next_metadata,
            )?;

            if outcome.updated {
                updated_task_ids.push(*task_id);
            }
            if outcome.completed {
                completed_task_ids.push(*task_id);
            }
            if !outcome.changed {
                unchanged_task_ids.push(*task_id);
            }
        }

        Ok(TaskTriageSummary {
            clear_labels: task.metadata.clear_labels,
            clear_owner: task.metadata.clear_owner,
            completed_task_ids,
            epic_id: task.epic_id,
            labels: triage_summary_labels(&task.metadata),
            owner: triage_summary_owner(&task.metadata),
            priority: task
                .metadata
                .priority
                .as_deref()
                .map(parse_task_priority_input)
                .transpose()?,
            task_ids,
            unchanged_task_ids,
            updated_task_ids,
            workspace: task.workspace.clone(),
        })
    }

    fn triage_task_record(
        &self,
        idempotency_key: Option<&str>,
        task: &TriageTasks,
        task_id: Uuid,
        task_record: &TaskRecord,
        next_metadata: &TaskMetadata,
    ) -> Result<TaskTriageOutcome> {
        let mut outcome = TaskTriageOutcome::default();

        if let Some(epic_id) = task.epic_id {
            if task_record.epic_id != Some(epic_id) {
                let request = UpdateTaskRequest {
                    workspace: task.workspace.clone(),
                    actor: task.actor.clone(),
                    task_id,
                    title: task_record.title.clone(),
                    details: task_record.details.clone(),
                    epic_id: Some(epic_id),
                    metadata: next_metadata.clone(),
                };
                let request_key = idempotency_key
                    .map(|root_key| format!("{root_key}:triage-update-epic:{task_id}"));
                let _: serde_json::Value =
                    self.context.patch_json(&task_path(task_id), &request, request_key.as_deref())?;
                outcome.changed = true;
                outcome.updated = true;
            }
        }

        if !outcome.changed && task_metadata_changed(&task_record.metadata, next_metadata) {
            let request = UpdateTaskRequest {
                workspace: task.workspace.clone(),
                actor: task.actor.clone(),
                task_id,
                title: task_record.title.clone(),
                details: task_record.details.clone(),
                epic_id: task_record.epic_id,
                metadata: next_metadata.clone(),
            };
            let request_key =
                idempotency_key.map(|root_key| format!("{root_key}:triage-update-meta:{task_id}"));
            let _: serde_json::Value =
                self.context.patch_json(&task_path(task_id), &request, request_key.as_deref())?;
            outcome.changed = true;
            outcome.updated = true;
        }

        if task.complete && task_record.status != "completed" {
            let request = CompleteTaskRequest {
                workspace: task.workspace.clone(),
                actor: task.actor.clone(),
                task_id,
            };
            let request_key =
                idempotency_key.map(|root_key| format!("{root_key}:triage-complete:{task_id}"));
            let _: serde_json::Value = self.context.post_json(
                &task_completion_path(task_id),
                &request,
                request_key.as_deref(),
            )?;
            outcome.changed = true;
            outcome.completed = true;
        }

        Ok(outcome)
    }
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

fn compact_claim_label(entry: &TaskListEntry) -> String {
    entry.active_claim.as_ref().map_or_else(
        || "claim=open".to_owned(),
        |claim| format!("claim={}", claim.actor),
    )
}

fn compact_epic_label(entry: &TaskListEntry) -> String {
    entry.epic.as_ref().map_or_else(
        || "epic=none".to_owned(),
        |epic| format!("epic={}", epic.title),
    )
}

fn compact_owner_label(entry: &TaskListEntry) -> String {
    entry
        .task
        .metadata
        .owner
        .as_ref()
        .map_or_else(|| "owner=none".to_owned(), |owner| format!("owner={owner}"))
}

fn compact_labels_label(entry: &TaskListEntry) -> String {
    if entry.task.metadata.labels.is_empty() {
        return "labels=-".to_owned();
    }

    format!("labels={}", entry.task.metadata.labels.join(","))
}

pub(crate) fn render_task_list_compact(entries: &[TaskListEntry]) -> String {
    if entries.is_empty() {
        return "no tasks\n".to_owned();
    }

    let lines = entries
        .iter()
        .map(|entry| {
            format!(
                "{} | {} | status={} | priority={} | {} | deps={} | dependents={} | {} | {} | {} | {}",
                short_task_id(&entry.task.task_id),
                entry.task.title,
                entry.task.status,
                entry.task.metadata.priority,
                if entry.ready { "ready" } else { "blocked" },
                entry.dependencies.len(),
                entry.dependents.len(),
                compact_epic_label(entry),
                compact_owner_label(entry),
                compact_labels_label(entry),
                compact_claim_label(entry),
            )
        })
        .collect::<Vec<_>>();

    format!("{}\n", lines.join("\n"))
}

pub(crate) fn render_task_dependency_compact(entries: &[TaskDependencySummary]) -> String {
    if entries.is_empty() {
        return "no tasks\n".to_owned();
    }

    let lines = entries
        .iter()
        .map(|entry| {
            format!(
                "{} | {} | status={} | depth={}",
                short_task_id(&entry.task_id),
                entry.title,
                entry.status,
                entry.depth
            )
        })
        .collect::<Vec<_>>();

    format!("{}\n", lines.join("\n"))
}

pub(crate) fn render_note_list_compact(entries: &[NoteRecord]) -> String {
    if entries.is_empty() {
        return "no notes\n".to_owned();
    }

    let lines = entries
        .iter()
        .map(|entry| {
            format!(
                "{} | {} | author={} | updated_at={}",
                short_uuid(&entry.note_id),
                entry.title,
                entry.author,
                entry.updated_at
            )
        })
        .collect::<Vec<_>>();

    format!("{}\n", lines.join("\n"))
}

pub(crate) fn render_memory_list_compact(entries: &[MemoryRecord]) -> String {
    if entries.is_empty() {
        return "no memories\n".to_owned();
    }

    let lines = entries
        .iter()
        .map(|entry| {
            format!(
                "{} | {} | kind={} | importance={} | audience={} | tags={}",
                short_uuid(&entry.memory_id),
                entry.title,
                entry.kind,
                entry.importance,
                entry.audience,
                entry.tags.join(",")
            )
        })
        .collect::<Vec<_>>();

    format!("{}\n", lines.join("\n"))
}

pub(crate) fn render_event_list_compact(entries: &[EventRecord]) -> String {
    if entries.is_empty() {
        return "no events\n".to_owned();
    }

    let lines = entries
        .iter()
        .map(|entry| {
            format!(
                "{} | {} | actor={} | at={}",
                short_uuid(&entry.event_id),
                entry.kind,
                entry.actor,
                entry.created_at
            )
        })
        .collect::<Vec<_>>();

    format!("{}\n", lines.join("\n"))
}

pub(crate) fn render_entity_context_compact(context: &EntityContext) -> String {
    let mut rendered = compact_entity_summary(&context.entity);
    rendered.push('\n');
    rendered.push_str(&render_graph_relations_compact(&context.relations));
    rendered
}

pub(crate) fn render_graph_relations_compact(entries: &[GraphRelation]) -> String {
    if entries.is_empty() {
        return "no related entities\n".to_owned();
    }

    let lines = entries
        .iter()
        .map(|entry| {
            let title = entry.title.as_deref().unwrap_or("untitled");
            format!(
                "{} {} | {} | {}",
                entry.direction,
                entry.relation,
                short_entity_ref(entry.entity_ref.as_str()),
                title
            )
        })
        .collect::<Vec<_>>();

    format!("{}\n", lines.join("\n"))
}

fn compact_entity_summary(entity: &EntityRecord) -> String {
    match entity {
        EntityRecord::Epic(record) => format!(
            "epic {} | {} | author={} | workspace={}",
            short_uuid(&record.epic_id),
            record.title,
            record.author,
            record.workspace
        ),
        EntityRecord::Memory(record) => format!(
            "memory {} | {} | kind={} | importance={} | workspace={}",
            short_uuid(&record.memory_id),
            record.title,
            record.kind,
            record.importance,
            record.workspace
        ),
        EntityRecord::Note(record) => format!(
            "note {} | {} | author={} | workspace={}",
            short_uuid(&record.note_id),
            record.title,
            record.author,
            record.workspace
        ),
        EntityRecord::Task(record) => format!(
            "task {} | {} | status={} | priority={} | owner={} | workspace={}",
            short_uuid(&record.task_id),
            record.title,
            record.status,
            record.metadata.priority,
            record.metadata.owner.as_deref().unwrap_or("none"),
            record.workspace
        ),
    }
}

fn short_task_id(task_id: &Uuid) -> String {
    short_uuid(task_id)
}

fn short_entity_ref(entity_ref: &str) -> String {
    let Some((kind, raw_id)) = entity_ref.split_once(':') else {
        return entity_ref.to_owned();
    };
    let short_id = raw_id.split('-').next().unwrap_or(raw_id);
    format!("{kind}:{short_id}")
}

fn short_uuid(value: &Uuid) -> String {
    value
        .to_string()
        .split('-')
        .next()
        .unwrap_or_default()
        .to_owned()
}

fn task_list_path(task: &ListTasks) -> Result<String> {
    let suffix = task_query_suffix(
        task.status.map(TaskStatusValue::as_str),
        task.epic_id,
        task.limit,
        task.label.as_deref(),
        task.metadata_filters.owner.as_deref(),
        task.metadata_filters.priority.as_deref(),
        task.ready_only,
    )?;

    Ok(format!("/v1/workspaces/{}/tasks{}", task.workspace, suffix))
}

fn task_next_path(task: &NextTask) -> Result<String> {
    let suffix = task_query_suffix(
        Some("open"),
        task.epic_id,
        None,
        task.label.as_deref(),
        task.metadata_filters.owner.as_deref(),
        task.metadata_filters.priority.as_deref(),
        true,
    )?;

    Ok(format!(
        "/v1/workspaces/{}/tasks/next{}",
        task.workspace, suffix
    ))
}

fn memory_path(memory_id: Uuid) -> String {
    format!("/v1/memories/{memory_id}")
}

fn note_path(note_id: Uuid) -> String {
    format!("/v1/notes/{note_id}")
}

fn task_path(task_id: Uuid) -> String {
    format!("/v1/tasks/{task_id}")
}

fn task_claim_release_path(task_id: Uuid) -> String {
    format!("{}/claims/release", task_path(task_id))
}

fn task_claims_path(task_id: Uuid) -> String {
    format!("{}/claims", task_path(task_id))
}

fn task_completion_path(task_id: Uuid) -> String {
    format!("{}/completion", task_path(task_id))
}

fn task_context_path(task_id: Uuid) -> String {
    format!("{}/context", task_path(task_id))
}

fn task_dag_path(task_id: Uuid) -> String {
    format!("{}/dag", task_path(task_id))
}

fn task_dependencies_path(task_id: Uuid) -> String {
    format!("{}/dependencies", task_path(task_id))
}

fn task_query_suffix(
    status: Option<&str>,
    epic_id: Option<Uuid>,
    limit: Option<i64>,
    label: Option<&str>,
    owner: Option<&str>,
    priority: Option<&str>,
    ready_only: bool,
) -> Result<String> {
    let mut query = Vec::new();
    if let Some(status_filter) = status {
        query.push(format!("status={status_filter}"));
    }
    if let Some(selected_epic_id) = epic_id {
        query.push(format!("epic_id={selected_epic_id}"));
    }
    if let Some(query_limit) = limit {
        query.push(format!("limit={query_limit}"));
    }
    if let Some(selected_label) =
        normalize_task_labels(label.map(str::to_owned).into_iter().collect())
            .into_iter()
            .next()
    {
        query.push(format!("label={selected_label}"));
    }
    if let Some(selected_owner) = normalize_task_owner(owner.map(str::to_owned)) {
        query.push(format!("owner={selected_owner}"));
    }
    if let Some(selected_priority) = priority {
        query.push(format!(
            "priority={}",
            parse_task_priority_input(selected_priority)?
        ));
    }
    if ready_only {
        query.push("ready_only=true".to_owned());
    }

    if query.is_empty() {
        Ok(String::new())
    } else {
        Ok(format!("?{}", query.join("&")))
    }
}

pub(crate) fn note_list_path(
    workspace: &str,
    limit: Option<i64>,
    author: Option<&str>,
    query: Option<&str>,
) -> String {
    let mut params = Vec::new();
    if let Some(query_limit) = limit {
        params.push(format!("limit={query_limit}"));
    }
    if let Some(selected_author) = normalize_task_owner(author.map(str::to_owned)) {
        params.push(format!("author={selected_author}"));
    }
    if let Some(search_query) = query.map(str::trim).filter(|value| !value.is_empty()) {
        params.push(format!("query={search_query}"));
    }

    let suffix = if params.is_empty() {
        String::new()
    } else {
        format!("?{}", params.join("&"))
    };

    format!("/v1/workspaces/{workspace}/notes{suffix}")
}

pub(crate) fn memory_list_path(input: MemoryListPathArgs<'_>) -> Result<String> {
    let mut params = Vec::new();
    if let Some(query_limit) = input.limit {
        params.push(format!("limit={query_limit}"));
    }
    if let Some(selected_audience) = input.audience {
        params.push(format!(
            "audience={}",
            parse_memory_audience_input(selected_audience)?
        ));
    }
    if let Some(selected_importance) = input.importance {
        params.push(format!(
            "importance={}",
            parse_memory_importance_input(selected_importance)?
        ));
    }
    if let Some(selected_kind) = input.kind {
        params.push(format!("kind={}", parse_memory_kind_input(selected_kind)?));
    }
    if let Some(search_query) = input.query.map(str::trim).filter(|value| !value.is_empty()) {
        params.push(format!("query={search_query}"));
    }
    if let Some(selected_trigger) = input
        .recall_trigger
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        params.push(format!(
            "recall_trigger={}",
            normalize_memory_filter_name(selected_trigger)?
        ));
    }
    if let Some(selected_tag) = input.tag.map(str::trim).filter(|value| !value.is_empty()) {
        params.push(format!(
            "tag={}",
            normalize_memory_filter_name(selected_tag)?
        ));
    }

    let suffix = if params.is_empty() {
        String::new()
    } else {
        format!("?{}", params.join("&"))
    };

    Ok(format!(
        "/v1/workspaces/{}/memories{suffix}",
        input.workspace
    ))
}

fn memory_prime_path(memory: &PrimeMemories) -> Result<String> {
    let mut params = vec![
        format!(
            "audience={}",
            parse_memory_audience_input(&memory.audience)?
        ),
        format!(
            "recall_trigger={}",
            normalize_memory_filter_name(&memory.recall_trigger)?
        ),
        format!("tag={}", normalize_memory_filter_name(&memory.tag)?),
    ];
    if let Some(query_limit) = memory.limit {
        params.push(format!("limit={query_limit}"));
    }

    Ok(format!(
        "/v1/workspaces/{}/memories/prime?{}",
        memory.workspace,
        params.join("&")
    ))
}

pub(crate) fn entity_show_path(entity_ref: &str) -> String {
    format!("/v1/entities/{entity_ref}")
}

pub(crate) fn entity_relations_path(entity_ref: &str) -> String {
    format!("/v1/entities/{entity_ref}/relations")
}

pub(crate) fn events_list_path(workspace: &str, limit: i64) -> String {
    format!("/v1/workspaces/{workspace}/events?limit={limit}")
}

pub(crate) fn events_tail_path(
    workspace: &str,
    limit: i64,
    after_event_id: Option<Uuid>,
) -> String {
    let mut params = vec![format!("limit={limit}")];
    if let Some(event_id) = after_event_id {
        params.push(format!("after_event_id={event_id}"));
    }

    format!(
        "/v1/workspaces/{workspace}/events/tail?{}",
        params.join("&")
    )
}

fn workspace_policy_path(workspace: &str) -> String {
    format!("/v1/workspaces/{workspace}/policy")
}

fn workspace_memberships_path(workspace: &str) -> String {
    format!("/v1/workspaces/{workspace}/memberships")
}

fn workspace_keys_path(workspace: &str, actor_id: Option<&str>) -> String {
    if let Some(selected_actor_id) = normalize_task_owner(actor_id.map(str::to_owned)) {
        return format!("/v1/workspaces/{workspace}/keys?actor_id={selected_actor_id}");
    }

    format!("/v1/workspaces/{workspace}/keys")
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

#[cfg(test)]
mod execution_tests {
    #![expect(
        clippy::panic,
        reason = "Test-only fakes use explicit panic messages for fixture setup failures."
    )]

    use std::path::PathBuf;

    use alloc::collections::BTreeMap;
    use core::{cell::RefCell, time::Duration};
    use serde::{de::DeserializeOwned, Serialize};
    use serde_json::{json, Value};

    use super::*;
    use crate::runtime::{ApiClient, CommandContext, CommandOutput, Sleeper};
    use threadplane_core::{
        build_info, CliConfig, PublicKeyAlgorithm, ServerConfig, ServiceSnapshot,
        TaskSummary, WorkspaceAuthPolicy, WorkspaceBootstrapConfig,
        WorkspaceBootstrapMembershipConfig, WorkspaceBootstrapPublicKeyConfig,
    };

    #[derive(Default)]
    struct FakeApi {
        gets: BTreeMap<String, Value>,
        requests: RefCell<Vec<String>>,
    }

    impl FakeApi {
        fn with_get_response<T>(mut self, path: &str, value: &T) -> Self
        where
            T: Serialize,
        {
            let serialized = match serde_json::to_value(value) {
                Ok(serialized) => serialized,
                Err(error) => panic!("serializable fake response: {error}"),
            };
            self.gets.insert(path.to_owned(), serialized);
            self
        }

        fn requests(&self) -> Vec<String> {
            self.requests.borrow().clone()
        }
    }

    impl ApiClient for FakeApi {
        fn get_json<T>(&self, path: &str) -> Result<T>
        where
            T: DeserializeOwned,
        {
            self.requests.borrow_mut().push(format!("GET {path}"));
            let value = self.gets.get(path).cloned().ok_or_else(|| {
                Usage {
                    message: format!("missing fake GET response for {path}"),
                }
                .build()
            })?;

            serde_json::from_value(value).map_err(|source| {
                Usage {
                    message: format!("failed to deserialize fake GET response for {path}: {source}"),
                }
                .build()
            })
        }

        fn patch_json<B, T>(&self, path: &str, _body: &B, _idempotency_key: Option<&str>) -> Result<T>
        where
            B: Serialize,
            T: DeserializeOwned,
        {
            Err(Usage {
                message: format!("unexpected fake PATCH {path}"),
            }
            .build())
        }

        fn post_json<B, T>(&self, path: &str, _body: &B, _idempotency_key: Option<&str>) -> Result<T>
        where
            B: Serialize,
            T: DeserializeOwned,
        {
            Err(Usage {
                message: format!("unexpected fake POST {path}"),
            }
            .build())
        }

        fn put_json<B, T>(&self, path: &str, _body: &B, _idempotency_key: Option<&str>) -> Result<T>
        where
            B: Serialize,
            T: DeserializeOwned,
        {
            Err(Usage {
                message: format!("unexpected fake PUT {path}"),
            }
            .build())
        }
    }

    #[derive(Default)]
    struct FakeOutput {
        rendered: String,
        warnings: Vec<String>,
    }

    impl CommandOutput for FakeOutput {
        fn print(&mut self, text: &str) {
            self.rendered.push_str(text);
        }

        fn print_warning(&mut self, text: &str) {
            self.warnings.push(text.to_owned());
        }
    }

    #[derive(Default)]
    struct RecordingSleeper {
        sleeps: RefCell<Vec<Duration>>,
    }

    impl RecordingSleeper {
        fn sleeps(&self) -> Vec<Duration> {
            self.sleeps.borrow().clone()
        }
    }

    impl Sleeper for RecordingSleeper {
        fn sleep(&self, duration: Duration) {
            self.sleeps.borrow_mut().push(duration);
        }
    }

    #[test]
    fn execute_scope_uses_fake_ports_and_emits_build_warning() {
        let api = FakeApi::default()
            .with_get_response("/scope", &json!({"ok": true, "summary": "scope"}))
            .with_get_response(
                "/",
                &ServiceSnapshot {
                    build: build_info(
                        "threadplane-server",
                        "9.9.9",
                        "release",
                        Some("bbbbbbbbbbbb"),
                        false,
                    ),
                    event_kinds: Vec::new(),
                    graph_projection: "neo4j".to_owned(),
                    name: "threadplane".to_owned(),
                    source_of_truth: "postgres".to_owned(),
                    summary: "shared memory".to_owned(),
                    tuple_space: "lease-backed".to_owned(),
                },
            );
        let mut output = FakeOutput::default();
        let sleeper = RecordingSleeper::default();
        let config = sample_config();
        let discovery = sample_discovery();
        let mut context = CommandContext::builder()
            .api(&api)
            .output(&mut output)
            .sleeper(&sleeper)
            .build();

        if let Err(error) = execute(
            Cli {
                command: Command::Scope,
                config: None,
                idempotency_key: None,
                server: None,
            },
            &config,
            &discovery,
            &mut context,
        ) {
            panic!("scope command succeeds: {error}");
        }

        assert_eq!(api.requests(), vec!["GET /scope", "GET /"]);
        assert!(output.rendered.contains("\"summary\": \"scope\""));
        assert_eq!(output.warnings.len(), 1);
        let first_warning = output.warnings.first().cloned().unwrap_or_default();
        assert!(first_warning.contains("warning:"));
        assert!(sleeper.sleeps().is_empty());
    }

    #[test]
    fn execute_next_task_renders_compact_output_through_runtime() {
        let task_id = Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap_or_default();
        let api = FakeApi::default().with_get_response(
            "/v1/workspaces/threadplane-dev/tasks/next?status=open&ready_only=true",
            &ApiEnvelope {
                ok: true,
                data: Some(TaskListEntry {
                    active_claim: None,
                    dependencies: Vec::new(),
                    dependents: Vec::new(),
                    epic: None,
                    ready: true,
                    task: TaskSummary {
                        author: "codex".to_owned(),
                        created_at: "2026-03-18T00:00:00Z".to_owned(),
                        details: "Keep work flowing.".to_owned(),
                        entity_ref: format!("task:{task_id}"),
                        epic_id: None,
                        metadata: TaskMetadata {
                            labels: vec!["workflow".to_owned()],
                            owner: Some("codex".to_owned()),
                            priority: TaskPriority::from_lossy("high"),
                        },
                        status: "open".to_owned(),
                        task_id,
                        title: "Pick next ready task".to_owned(),
                        transclusion_id: None,
                        updated_at: "2026-03-18T00:00:00Z".to_owned(),
                        workspace: "threadplane-dev".to_owned(),
                    },
                }),
                receipt: None,
            },
        );
        let mut output = FakeOutput::default();
        let sleeper = RecordingSleeper::default();
        let config = sample_config();
        let discovery = sample_discovery();
        let mut context = CommandContext::builder()
            .api(&api)
            .output(&mut output)
            .sleeper(&sleeper)
            .build();

        if let Err(error) = execute(
            Cli {
                command: Command::Task(TaskCommand {
                    command: TaskSubcommand::Next(NextTask {
                        epic_id: None,
                        format: OutputFormat::Compact,
                        label: None,
                        metadata_filters: TaskMetadataFilterArgs::default(),
                        workspace: "threadplane-dev".to_owned(),
                    }),
                }),
                config: None,
                idempotency_key: None,
                server: None,
            },
            &config,
            &discovery,
            &mut context,
        ) {
            panic!("next task command succeeds: {error}");
        }

        assert_eq!(
            api.requests(),
            vec!["GET /v1/workspaces/threadplane-dev/tasks/next?status=open&ready_only=true"]
        );
        assert!(output.rendered.contains("Pick next ready task"));
        assert!(output.rendered.contains("priority=high"));
        assert!(sleeper.sleeps().is_empty());
    }

    fn sample_config() -> ThreadplaneConfig {
        ThreadplaneConfig {
            cli: CliConfig {
                url: "http://127.0.0.1:4000".to_owned(),
            },
            server: ServerConfig {
                bind: "127.0.0.1:4000".to_owned(),
                database_url: "postgres://threadplane:test@127.0.0.1/threadplane".to_owned(),
                default_lease_seconds: 120,
                neo4j_password: "test".to_owned(),
                neo4j_uri: "bolt://127.0.0.1:7687".to_owned(),
                neo4j_user: "neo4j".to_owned(),
                workspace_bootstrap: WorkspaceBootstrapConfig {
                    auth: WorkspaceAuthPolicy {
                        allowed_algorithms: vec![
                            PublicKeyAlgorithm::Ed25519,
                            PublicKeyAlgorithm::SshEd25519,
                        ],
                        challenge_ttl_seconds: 300,
                        signed_commands_required: false,
                    },
                    memberships: vec![WorkspaceBootstrapMembershipConfig {
                        actor_id: "operator".to_owned(),
                        role: WorkspaceRole::Admin,
                    }],
                    priorities: WorkspacePriorityPolicy {
                        default_priority: "medium".to_owned(),
                        priorities: vec![
                            WorkspacePriority {
                                description: Some("Important".to_owned()),
                                name: "high".to_owned(),
                                rank: 10,
                            },
                            WorkspacePriority {
                                description: Some("Normal".to_owned()),
                                name: "medium".to_owned(),
                                rank: 20,
                            },
                        ],
                    },
                    public_keys: vec![WorkspaceBootstrapPublicKeyConfig {
                        actor_id: "operator".to_owned(),
                        algorithm: PublicKeyAlgorithm::Ed25519,
                        key_id: "operator-main".to_owned(),
                        public_key: "ed25519:test".to_owned(),
                    }],
                },
            },
        }
    }

    fn sample_discovery() -> ConfigDiscovery {
        ConfigDiscovery {
            env_override: None,
            env_prefix: "THREADPLANE",
            explicit_override: None,
            search_order: vec![PathBuf::from("/tmp/threadplane/config.toml")],
            selected_path: Some(PathBuf::from("/tmp/threadplane/config.toml")),
        }
    }
}
