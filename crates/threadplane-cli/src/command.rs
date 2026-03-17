#![expect(
    clippy::redundant_pub_crate,
    reason = "CLI commands are crate-internal and keep explicit visibility for readability."
)]

use alloc::collections::BTreeSet;
use core::time::Duration;
use std::{path::PathBuf, thread};

use clap::{Args, Parser, Subcommand, ValueEnum};
use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::{json, to_string_pretty};
use snafu::ResultExt as _;
use uuid::Uuid;

use threadplane_core::{
    compare_build_info, normalize_task_labels, normalize_task_owner, AddLinkRequest,
    AddTaskDependencyRequest, ApiEnvelope, BuildComparison, ClaimNextTaskRequest,
    ClaimTaskRequest, CliConfigOverrides, CompleteTaskRequest, ConfigDiscovery,
    CreateEpicRequest, CreateNoteRequest, CreateXanaduLinkRequest, EventRecord,
    NoteRecord, OfferTaskRequest, ProjectionStatus, ReleaseTaskRequest, ServiceSnapshot,
    TaskClaimRecord, TaskContext, TaskDag, TaskDependencySummary, TaskListEntry, TaskMetadata,
    TaskPriority, TaskRecord, ThreadplaneConfig, ThreadplaneConfigOverrides,
    UpdateNoteRequest, UpdateTaskRequest, SERVICE_NAME,
};

use crate::{
    build_info::current_build_info,
    error::{JsonRender, Result, Usage},
    http::{get_json, post_json},
};

#[derive(Debug, Parser)]
#[command(
    name = SERVICE_NAME,
    version,
    about = "Shared memory and coordination CLI for people and AI agents",
    long_about = "threadplane-cli talks to threadplane-server so people and agents can share tasks, notes, links, claims, and graph-backed context through one internet-reachable control plane."
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
    Epic(EpicCommand),
    Events(EventsCommand),
    Link(LinkCommand),
    Note(NoteCommand),
    Projection(ProjectionCommand),
    #[command(about = "Show the product and architecture summary exposed by the service")]
    Scope,
    Task(TaskCommand),
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

    #[arg(long, default_value_t = 2, help = "Seconds to wait between follow polls")]
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
    #[arg(long, help = "Only return direct relationships instead of the transitive chain")]
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

#[derive(Debug, Clone, Copy, ValueEnum, strum::Display)]
#[strum(serialize_all = "snake_case")]
pub(crate) enum TaskPriorityValue {
    High,
    Low,
    Medium,
    Urgent,
}

#[derive(Debug, Args, Clone)]
struct TaskMetadataArgs {
    #[arg(long, help = "Durable label. Repeat for multiple labels")]
    label: Vec<String>,

    #[arg(long, help = "Durable owner, distinct from the temporary claim actor")]
    owner: Option<String>,

    #[arg(
        long,
        default_value_t = TaskPriorityValue::Medium,
        help = "Priority used for backlog sorting and filtering"
    )]
    priority: TaskPriorityValue,
}

#[derive(Debug, Args, Clone, Default)]
struct TaskMetadataFilterArgs {
    #[arg(long, help = "Only include tasks owned by this durable owner")]
    owner: Option<String>,

    #[arg(long, help = "Only include tasks with this priority")]
    priority: Option<TaskPriorityValue>,
}

#[derive(Debug, Args, Clone, Default)]
pub(crate) struct TaskMetadataPatchArgs {
    #[arg(long, help = "Clear all durable labels")]
    pub(crate) clear_labels: bool,

    #[arg(long, help = "Clear any durable owner")]
    pub(crate) clear_owner: bool,

    #[arg(long, help = "Replace labels with this set. Repeat for multiple labels")]
    pub(crate) label: Vec<String>,

    #[arg(long, help = "Replace the durable owner")]
    pub(crate) owner: Option<String>,

    #[arg(long, help = "Replace the task priority")]
    pub(crate) priority: Option<TaskPriorityValue>,
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

    #[arg(long, help = "Mark every listed task completed after any metadata updates")]
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

#[derive(Debug, Clone, Copy)]
enum TaskDependencyViewKind {
    BlockedBy,
    Blocks,
}

pub(crate) fn execute(
    cli: Cli,
    config: &ThreadplaneConfig,
    discovery: &ConfigDiscovery,
    client: &Client,
) -> Result<()> {
    let Cli {
        command: root_command,
        idempotency_key: command_idempotency_key,
        ..
    } = cli;
    let server = config.cli.url.clone();
    let idempotency_key = command_idempotency_key.as_deref();

    match root_command {
        Command::Build(build_command) => handle_build(client, &server, &build_command)?,
        Command::Config(config_command) => handle_config(&config_command, config, discovery)?,
        Command::Epic(epic_command) => {
            handle_epic(client, &server, idempotency_key, epic_command)?;
        }
        Command::Events(events_command) => handle_events(client, &server, events_command)?,
        Command::Link(link_command) => {
            handle_link(client, &server, idempotency_key, link_command)?;
        }
        Command::Note(note_command) => {
            handle_note(client, &server, idempotency_key, note_command)?;
        }
        Command::Projection(projection_command) => {
            handle_projection(client, &server, &projection_command)?;
        }
        Command::Scope => handle_scope(client, &server)?,
        Command::Task(task_command) => {
            handle_task(client, &server, idempotency_key, task_command)?;
        }
    }

    Ok(())
}
fn handle_build(client: &Client, server: &str, command: &BuildCommand) -> Result<()> {
    match command.command {
        BuildSubcommand::Show => print_value(&current_build_info()),
        BuildSubcommand::Compare => {
            let snapshot: ServiceSnapshot = get_json(client, server, "/")?;
            let comparison = compare_build_info(&current_build_info(), &snapshot.build);
            print_value(&comparison)
        }
    }
}

fn handle_epic(
    client: &Client,
    server: &str,
    idempotency_key: Option<&str>,
    command: EpicCommand,
) -> Result<()> {
    match command.command {
        EpicSubcommand::Add(epic) => {
            let request = CreateEpicRequest {
                workspace: epic.workspace,
                author: epic.author,
                title: epic.title,
                body: epic.body,
            };
            let response: serde_json::Value =
                post_json(client, server, "/v1/epics", &request, idempotency_key)?;
            print_value(&response)
        }
        EpicSubcommand::List(epics) => {
            let path = format!("/v1/workspaces/{}/epics", epics.workspace);
            let response: serde_json::Value = get_json(client, server, &path)?;
            print_value(&response)
        }
        EpicSubcommand::Show(epic) => {
            let path = format!("/v1/epics/{}", epic.epic_id);
            let response: serde_json::Value = get_json(client, server, &path)?;
            print_value(&response)
        }
    }
}

fn handle_config(
    command: &ConfigCommand,
    config: &ThreadplaneConfig,
    discovery: &ConfigDiscovery,
) -> Result<()> {
    match command.command {
        ConfigSubcommand::Show => {
            let payload = json!({
                "config": config,
                "discovery": {
                    "search_order": discovery.search_order,
                    "selected_path": discovery.selected_path,
                    "explicit_override": discovery.explicit_override,
                    "env_override": discovery.env_override,
                    "env_prefix": discovery.env_prefix,
                }
            });
            print_value(&payload)
        }
    }
}

fn handle_events(client: &Client, server: &str, command: EventsCommand) -> Result<()> {
    match command.command {
        EventsSubcommand::List(events) => {
            let path = events_list_path(events.workspace.as_str(), events.limit);
            let response: ApiEnvelope<Vec<EventRecord>> = get_json(client, server, &path)?;
            match events.format {
                OutputFormat::Compact => {
                    print!("{}", render_event_list_compact(&response.data));
                    Ok(())
                }
                OutputFormat::Json => print_value(&response),
            }
        }
        EventsSubcommand::Tail(events) => handle_tail_events(client, server, &events),
    }
}

fn handle_scope(client: &Client, server: &str) -> Result<()> {
    let scope: serde_json::Value = get_json(client, server, "/scope")?;
    let snapshot: ServiceSnapshot = get_json(client, server, "/")?;
    let comparison = compare_build_info(&current_build_info(), &snapshot.build);

    if let Some(warning) = build_mismatch_warning(&comparison) {
        eprintln!("warning: {warning}");
    }

    print_value(&scope)
}

fn handle_link(
    client: &Client,
    server: &str,
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
                post_json(client, server, "/v1/links", &request, idempotency_key)?;
            print_value(&response)
        }
        LinkSubcommand::Xanadu(link) => {
            let request = CreateXanaduLinkRequest {
                workspace: link.workspace,
                actor: link.actor,
                from: link.from,
                to: link.to,
            };
            let response: serde_json::Value =
                post_json(client, server, "/v1/links/xanadu", &request, idempotency_key)?;
            print_value(&response)
        }
    }
}

fn handle_note(
    client: &Client,
    server: &str,
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
                post_json(client, server, "/v1/notes", &request, idempotency_key)?;
            print_value(&response)
        }
        NoteSubcommand::List(list) => handle_list_notes(client, server, &list),
        NoteSubcommand::Search(search) => handle_search_notes(client, server, &search),
        NoteSubcommand::Show(show) => {
            let path = format!("/v1/notes/{}", show.note_id);
            let response: serde_json::Value = get_json(client, server, &path)?;
            print_value(&response)
        }
        NoteSubcommand::Update(update) => {
            let request = UpdateNoteRequest {
                workspace: update.workspace,
                actor: update.actor,
                note_id: update.note_id,
                title: update.title,
                body: update.body,
            };
            let response: serde_json::Value =
                post_json(client, server, "/v1/notes/update", &request, idempotency_key)?;
            print_value(&response)
        }
    }
}

fn handle_list_notes(client: &Client, server: &str, note: &ListNotes) -> Result<()> {
    let path = note_list_path(
        note.workspace.as_str(),
        note.limit,
        note.author.as_deref(),
        None,
    );
    let response: ApiEnvelope<Vec<NoteRecord>> = get_json(client, server, &path)?;

    match note.format {
        OutputFormat::Compact => {
            print!("{}", render_note_list_compact(&response.data));
            Ok(())
        }
        OutputFormat::Json => print_value(&response),
    }
}

fn handle_search_notes(client: &Client, server: &str, note: &SearchNotes) -> Result<()> {
    let path = note_list_path(
        note.workspace.as_str(),
        note.limit,
        note.author.as_deref(),
        Some(note.query.as_str()),
    );
    let response: ApiEnvelope<Vec<NoteRecord>> = get_json(client, server, &path)?;

    match note.format {
        OutputFormat::Compact => {
            print!("{}", render_note_list_compact(&response.data));
            Ok(())
        }
        OutputFormat::Json => print_value(&response),
    }
}

fn handle_projection(client: &Client, server: &str, command: &ProjectionCommand) -> Result<()> {
    match command.command {
        ProjectionSubcommand::Status => {
            let response: ApiEnvelope<ProjectionStatus> =
                get_json(client, server, "/v1/projections/graph")?;
            print_value(&response)
        }
    }
}

fn handle_task(
    client: &Client,
    server: &str,
    idempotency_key: Option<&str>,
    command: TaskCommand,
) -> Result<()> {
    match command.command {
        TaskSubcommand::BlockedBy(task) => handle_task_dependency_view(
            client,
            server,
            &task,
            TaskDependencyViewKind::BlockedBy,
        ),
        TaskSubcommand::Blocks(task) => {
            handle_task_dependency_view(client, server, &task, TaskDependencyViewKind::Blocks)
        }
        TaskSubcommand::ClaimNext(task) => {
            handle_claim_next_task(client, server, idempotency_key, task)
        }
        TaskSubcommand::Claim(task) => handle_claim_task(client, server, idempotency_key, task),
        TaskSubcommand::Complete(task) => {
            handle_complete_task(client, server, idempotency_key, task)
        }
        TaskSubcommand::Context(task) => handle_task_context(client, server, &task),
        TaskSubcommand::Dag(task) => handle_task_dag(client, server, &task),
        TaskSubcommand::Depend(task) => {
            handle_add_task_dependency(client, server, idempotency_key, task)
        }
        TaskSubcommand::List(task) => handle_list_tasks(client, server, &task),
        TaskSubcommand::Next(task) => handle_next_task(client, server, &task),
        TaskSubcommand::Offer(task) => handle_offer_task(client, server, idempotency_key, task),
        TaskSubcommand::Release(task) => {
            handle_release_task(client, server, idempotency_key, task)
        }
        TaskSubcommand::Show(task) => handle_show_task(client, server, &task),
        TaskSubcommand::Triage(task) => {
            let response = triage_tasks(client, server, idempotency_key, &task)?;
            print_value(&response)
        }
        TaskSubcommand::Update(task) => handle_update_task(client, server, idempotency_key, task),
    }
}

fn handle_add_task_dependency(
    client: &Client,
    server: &str,
    idempotency_key: Option<&str>,
    task: AddTaskDependency,
) -> Result<()> {
    let request = AddTaskDependencyRequest {
        workspace: task.workspace,
        actor: task.actor,
        task_id: task.task_id,
        depends_on_task_id: task.depends_on,
    };
    let response: serde_json::Value = post_json(
        client,
        server,
        "/v1/tasks/dependencies",
        &request,
        idempotency_key,
    )?;
    print_value(&response)
}

fn handle_claim_task(
    client: &Client,
    server: &str,
    idempotency_key: Option<&str>,
    task: ClaimTask,
) -> Result<()> {
    let request = ClaimTaskRequest {
        workspace: task.workspace,
        actor: task.actor,
        task_id: task.task_id,
        lease_seconds: task.lease_seconds,
    };
    let response: serde_json::Value =
        post_json(client, server, "/v1/tasks/claim", &request, idempotency_key)?;
    print_value(&response)
}

fn handle_claim_next_task(
    client: &Client,
    server: &str,
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
        priority: task.metadata_filters.priority.map(task_priority_from_cli),
        workspace: task.workspace,
    };
    let response: ApiEnvelope<Option<TaskClaimRecord>> =
        post_json(client, server, "/v1/tasks/claim-next", &request, idempotency_key)?;
    print_value(&response)
}

fn handle_complete_task(
    client: &Client,
    server: &str,
    idempotency_key: Option<&str>,
    task: CompleteTask,
) -> Result<()> {
    let request = CompleteTaskRequest {
        workspace: task.workspace,
        actor: task.actor,
        task_id: task.task_id,
    };
    let response: serde_json::Value =
        post_json(client, server, "/v1/tasks/complete", &request, idempotency_key)?;
    print_value(&response)
}

fn handle_list_tasks(client: &Client, server: &str, task: &ListTasks) -> Result<()> {
    let path = task_list_path(task);
    let response: ApiEnvelope<Vec<TaskListEntry>> = get_json(client, server, &path)?;

    match task.format {
        OutputFormat::Compact => {
            print!("{}", render_task_list_compact(&response.data));
            Ok(())
        }
        OutputFormat::Json => print_value(&response),
    }
}

fn handle_next_task(client: &Client, server: &str, task: &NextTask) -> Result<()> {
    let path = task_next_path(task);
    let response: ApiEnvelope<Option<TaskListEntry>> = get_json(client, server, &path)?;

    match task.format {
        OutputFormat::Compact => {
            let rendered = response
                .data
                .map_or_else(|| "no tasks\n".to_owned(), |entry| render_task_list_compact(&[entry]));
            print!("{rendered}");
            Ok(())
        }
        OutputFormat::Json => print_value(&response),
    }
}

fn handle_offer_task(
    client: &Client,
    server: &str,
    idempotency_key: Option<&str>,
    task: OfferTask,
) -> Result<()> {
    let request = OfferTaskRequest {
        workspace: task.workspace,
        author: task.author,
        depends_on: task.depends_on,
        title: task.title,
        details: task.details,
        epic_id: task.epic_id,
        metadata: task_metadata_from_args(task.metadata),
    };
    let response: serde_json::Value =
        post_json(client, server, "/v1/tasks/offers", &request, idempotency_key)?;
    print_value(&response)
}

fn handle_release_task(
    client: &Client,
    server: &str,
    idempotency_key: Option<&str>,
    task: ReleaseTask,
) -> Result<()> {
    let request = ReleaseTaskRequest {
        workspace: task.workspace,
        actor: task.actor,
        task_id: task.task_id,
    };
    let response: serde_json::Value =
        post_json(client, server, "/v1/tasks/release", &request, idempotency_key)?;
    print_value(&response)
}

fn handle_show_task(client: &Client, server: &str, task: &ShowTask) -> Result<()> {
    let path = format!("/v1/tasks/{}", task.task_id);
    let response: serde_json::Value = get_json(client, server, &path)?;
    print_value(&response)
}

fn handle_task_context(client: &Client, server: &str, task: &TaskContextCommand) -> Result<()> {
    let path = format!("/v1/tasks/{}/context", task.task_id);
    let response: serde_json::Value = get_json(client, server, &path)?;
    print_value(&response)
}

fn handle_task_dag(client: &Client, server: &str, task: &TaskDagCommand) -> Result<()> {
    let path = format!("/v1/tasks/{}/dag", task.task_id);
    let response: serde_json::Value = get_json(client, server, &path)?;
    print_value(&response)
}

fn handle_update_task(
    client: &Client,
    server: &str,
    idempotency_key: Option<&str>,
    task: UpdateTask,
) -> Result<()> {
    let request = UpdateTaskRequest {
        workspace: task.workspace,
        actor: task.actor,
        task_id: task.task_id,
        title: task.title,
        details: task.details,
        epic_id: task.epic_id,
        metadata: task_metadata_from_args(task.metadata),
    };
    let response: serde_json::Value =
        post_json(client, server, "/v1/tasks/update", &request, idempotency_key)?;
    print_value(&response)
}

fn fetch_task_context(client: &Client, server: &str, task_id: Uuid) -> Result<TaskContext> {
    let path = format!("/v1/tasks/{task_id}/context");
    let response: ApiEnvelope<TaskContext> = get_json(client, server, &path)?;
    Ok(response.data)
}

fn handle_tail_events(client: &Client, server: &str, events: &TailEvents) -> Result<()> {
    let mut cursor = events.after_event_id;

    loop {
        let path = events_tail_path(events.workspace.as_str(), events.limit, cursor);
        let response: ApiEnvelope<Vec<EventRecord>> = get_json(client, server, &path)?;
        let latest_event_id = response.data.last().map(|event| event.event_id);

        match events.format {
            OutputFormat::Compact => {
                if !response.data.is_empty() {
                    print!("{}", render_event_list_compact(&response.data));
                }
                if response.data.is_empty() && !events.follow {
                    print!("no events\n");
                }
            }
            OutputFormat::Json => print_value(&response)?,
        }

        cursor = latest_event_id.or(cursor);
        if !events.follow {
            return Ok(());
        }

        thread::sleep(Duration::from_secs(events.poll_seconds));
    }
}

fn fetch_task_dag(client: &Client, server: &str, task_id: Uuid) -> Result<TaskDag> {
    let path = format!("/v1/tasks/{task_id}/dag");
    let response: ApiEnvelope<TaskDag> = get_json(client, server, &path)?;
    Ok(response.data)
}

fn fetch_task_summary(client: &Client, server: &str, task_id: Uuid) -> Result<TaskRecord> {
    let path = format!("/v1/tasks/{task_id}");
    let response: ApiEnvelope<TaskRecord> = get_json(client, server, &path)?;
    Ok(response.data)
}

fn print_value<T: Serialize>(value: &T) -> Result<()> {
    let rendered = to_string_pretty(value).context(JsonRender)?;
    println!("{rendered}");
    Ok(())
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
    entry
        .active_claim
        .as_ref()
        .map_or_else(|| "claim=open".to_owned(), |claim| format!("claim={}", claim.actor))
}

fn compact_epic_label(entry: &TaskListEntry) -> String {
    entry
        .epic
        .as_ref()
        .map_or_else(|| "epic=none".to_owned(), |epic| format!("epic={}", epic.title))
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

fn handle_task_dependency_view(
    client: &Client,
    server: &str,
    task: &TaskDependencyViewCommand,
    kind: TaskDependencyViewKind,
) -> Result<()> {
    let data = if task.direct_only {
        let context = fetch_task_context(client, server, task.task_id)?;
        select_dependency_view_from_context(&context, kind).to_vec()
    } else {
        let dag = fetch_task_dag(client, server, task.task_id)?;
        select_dependency_view_from_dag(&dag, kind).to_vec()
    };

    match task.format {
        OutputFormat::Compact => {
            print!("{}", render_task_dependency_compact(&data));
            Ok(())
        }
        OutputFormat::Json => print_value(&data),
    }
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

fn short_task_id(task_id: &Uuid) -> String {
    short_uuid(task_id)
}

fn short_uuid(value: &Uuid) -> String {
    value
        .to_string()
        .split('-')
        .next()
        .unwrap_or_default()
        .to_owned()
}

fn task_list_path(task: &ListTasks) -> String {
    let suffix = task_query_suffix(
        task.status.map(TaskStatusValue::as_str),
        task.epic_id,
        task.limit,
        task.label.as_deref(),
        task.metadata_filters.owner.as_deref(),
        task.metadata_filters.priority,
        task.ready_only,
    );

    format!("/v1/workspaces/{}/tasks{}", task.workspace, suffix)
}

fn task_next_path(task: &NextTask) -> String {
    let suffix = task_query_suffix(
        Some("open"),
        task.epic_id,
        None,
        task.label.as_deref(),
        task.metadata_filters.owner.as_deref(),
        task.metadata_filters.priority,
        true,
    );

    format!("/v1/workspaces/{}/tasks/next{}", task.workspace, suffix)
}

fn task_query_suffix(
    status: Option<&str>,
    epic_id: Option<Uuid>,
    limit: Option<i64>,
    label: Option<&str>,
    owner: Option<&str>,
    priority: Option<TaskPriorityValue>,
    ready_only: bool,
) -> String {
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
    if let Some(selected_label) = normalize_task_labels(label.map(str::to_owned).into_iter().collect())
        .into_iter()
        .next()
    {
        query.push(format!("label={selected_label}"));
    }
    if let Some(selected_owner) = normalize_task_owner(owner.map(str::to_owned)) {
        query.push(format!("owner={selected_owner}"));
    }
    if let Some(selected_priority) = priority {
        query.push(format!("priority={}", task_priority_from_cli(selected_priority)));
    }
    if ready_only {
        query.push("ready_only=true".to_owned());
    }

    if query.is_empty() {
        String::new()
    } else {
        format!("?{}", query.join("&"))
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

pub(crate) fn events_list_path(workspace: &str, limit: i64) -> String {
    format!("/v1/workspaces/{workspace}/events?limit={limit}")
}

pub(crate) fn events_tail_path(workspace: &str, limit: i64, after_event_id: Option<Uuid>) -> String {
    let mut params = vec![format!("limit={limit}")];
    if let Some(event_id) = after_event_id {
        params.push(format!("after_event_id={event_id}"));
    }

    format!("/v1/workspaces/{workspace}/events/tail?{}", params.join("&"))
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

fn triage_tasks(
    client: &Client,
    server: &str,
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
        let task_record = fetch_task_summary(client, server, *task_id)?;
        let mut changed = false;
        let next_metadata = apply_metadata_patch(&task_record.metadata, &task.metadata);

        if let Some(epic_id) = task.epic_id {
            if task_record.epic_id != Some(epic_id) {
                let request = UpdateTaskRequest {
                    workspace: task.workspace.clone(),
                    actor: task.actor.clone(),
                    task_id: *task_id,
                    title: task_record.title.clone(),
                    details: task_record.details.clone(),
                    epic_id: Some(epic_id),
                    metadata: next_metadata.clone(),
                };
                let request_key = idempotency_key
                    .map(|root_key| format!("{root_key}:triage-update-epic:{task_id}"));
                let _: serde_json::Value =
                    post_json(client, server, "/v1/tasks/update", &request, request_key.as_deref())?;
                updated_task_ids.push(*task_id);
                changed = true;
            }
        }

        if !changed && task_metadata_changed(&task_record.metadata, &next_metadata) {
            let request = UpdateTaskRequest {
                workspace: task.workspace.clone(),
                actor: task.actor.clone(),
                task_id: *task_id,
                title: task_record.title.clone(),
                details: task_record.details.clone(),
                epic_id: task_record.epic_id,
                metadata: next_metadata,
            };
            let request_key = idempotency_key
                .map(|root_key| format!("{root_key}:triage-update-meta:{task_id}"));
            let _: serde_json::Value =
                post_json(client, server, "/v1/tasks/update", &request, request_key.as_deref())?;
            updated_task_ids.push(*task_id);
            changed = true;
        }

        if task.complete && task_record.status != "completed" {
            let request = CompleteTaskRequest {
                workspace: task.workspace.clone(),
                actor: task.actor.clone(),
                task_id: *task_id,
            };
            let request_key = idempotency_key
                .map(|root_key| format!("{root_key}:triage-complete:{task_id}"));
            let _: serde_json::Value =
                post_json(client, server, "/v1/tasks/complete", &request, request_key.as_deref())?;
            completed_task_ids.push(*task_id);
            changed = true;
        }

        if !changed {
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
        priority: task.metadata.priority.map(task_priority_from_cli),
        task_ids,
        unchanged_task_ids,
        updated_task_ids,
        workspace: task.workspace.clone(),
    })
}

pub(crate) fn dedup_task_ids(task_ids: &[Uuid]) -> Vec<Uuid> {
    task_ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn task_metadata_from_args(metadata: TaskMetadataArgs) -> TaskMetadata {
    TaskMetadata {
        labels: normalize_task_labels(metadata.label),
        owner: normalize_task_owner(metadata.owner),
        priority: task_priority_from_cli(metadata.priority),
    }
}

fn apply_metadata_patch(current: &TaskMetadata, patch: &TaskMetadataPatchArgs) -> TaskMetadata {
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

    TaskMetadata {
        labels,
        owner,
        priority: patch.priority.map_or(current.priority, task_priority_from_cli),
    }
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

const fn task_priority_from_cli(priority: TaskPriorityValue) -> TaskPriority {
    match priority {
        TaskPriorityValue::Low => TaskPriority::Low,
        TaskPriorityValue::Medium => TaskPriority::Medium,
        TaskPriorityValue::High => TaskPriority::High,
        TaskPriorityValue::Urgent => TaskPriority::Urgent,
    }
}
