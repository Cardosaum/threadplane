#![expect(
    clippy::redundant_pub_crate,
    reason = "CLI commands are crate-internal and keep explicit visibility for readability."
)]

use std::{env, path::PathBuf};

use clap::{Args, Parser, Subcommand, ValueEnum};
use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::{json, to_string_pretty};
use snafu::ResultExt as _;
use uuid::Uuid;

use threadplane_core::{
    default_config_path, default_system_config_path, AddLinkRequest, AddTaskDependencyRequest,
    ClaimTaskRequest, CompleteTaskRequest, CreateEpicRequest, CreateNoteRequest,
    CreateXanaduLinkRequest, OfferTaskRequest, ReleaseTaskRequest, ThreadplaneConfig,
    UpdateNoteRequest, UpdateTaskRequest, SERVICE_NAME,
};

use crate::{
    error::{JsonRender, Result},
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
        help = "HTTP base URL for threadplane-server. Overrides cli.url from config."
    )]
    server: Option<String>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Config(ConfigCommand),
    Epic(EpicCommand),
    Events(EventsCommand),
    Link(LinkCommand),
    Note(NoteCommand),
    #[command(about = "Show the product and architecture summary exposed by the service")]
    Scope,
    Task(TaskCommand),
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
}

#[derive(Debug, Args)]
struct ListEvents {
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

#[derive(Debug, Subcommand)]
enum NoteSubcommand {
    #[command(about = "Create a new note in a workspace")]
    Add(AddNote),
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
    #[command(about = "Claim an open task with a lease")]
    Claim(ClaimTask),
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
    #[command(about = "Offer a new task into a workspace")]
    Offer(OfferTask),
    #[command(about = "Release an active claim and return the task to the pool")]
    Release(ReleaseTask),
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

    #[arg(long, help = "Only include tasks whose dependencies are all completed")]
    ready_only: bool,

    #[arg(long, help = "Optional workflow status filter")]
    status: Option<TaskStatusValue>,

    #[arg(long, help = "Workspace name")]
    workspace: String,
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

    #[arg(long, help = "Task UUID")]
    task_id: Uuid,

    #[arg(long, help = "Updated task title")]
    title: String,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

pub(crate) fn execute(cli: Cli, config: &ThreadplaneConfig, client: &Client) -> Result<()> {
    let server = cli.server.unwrap_or_else(|| config.cli.url.clone());

    match cli.command {
        Command::Config(command) => handle_config(&command, config)?,
        Command::Epic(command) => handle_epic(client, &server, command)?,
        Command::Events(command) => handle_events(client, &server, command)?,
        Command::Link(command) => handle_link(client, &server, command)?,
        Command::Note(command) => handle_note(client, &server, command)?,
        Command::Scope => print_value(&get_json::<serde_json::Value>(client, &server, "/scope")?)?,
        Command::Task(command) => handle_task(client, &server, command)?,
    }

    Ok(())
}

fn handle_epic(client: &Client, server: &str, command: EpicCommand) -> Result<()> {
    match command.command {
        EpicSubcommand::Add(epic) => {
            let request = CreateEpicRequest {
                workspace: epic.workspace,
                author: epic.author,
                title: epic.title,
                body: epic.body,
            };
            let response: serde_json::Value = post_json(client, server, "/v1/epics", &request)?;
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

fn handle_config(command: &ConfigCommand, config: &ThreadplaneConfig) -> Result<()> {
    match command.command {
        ConfigSubcommand::Show => {
            let env_override = env::var("THREADPLANE_CONFIG")
                .ok()
                .filter(|value| !value.is_empty());
            let search_order = env_override.as_ref().map_or_else(
                || {
                    vec![
                        default_config_path().display().to_string(),
                        default_system_config_path().display().to_string(),
                    ]
                },
                |override_path| vec![override_path.clone()],
            );

            let payload = json!({
                "config": config,
                "discovery": {
                    "search_order": search_order,
                    "env_override": env_override,
                    "env_prefix": "THREADPLANE__",
                }
            });
            print_value(&payload)
        }
    }
}

fn handle_events(client: &Client, server: &str, command: EventsCommand) -> Result<()> {
    match command.command {
        EventsSubcommand::List(events) => {
            let path = format!(
                "/v1/workspaces/{}/events?limit={}",
                events.workspace, events.limit
            );
            let response: serde_json::Value = get_json(client, server, &path)?;
            print_value(&response)
        }
    }
}

fn handle_link(client: &Client, server: &str, command: LinkCommand) -> Result<()> {
    match command.command {
        LinkSubcommand::Add(link) => {
            let request = AddLinkRequest {
                workspace: link.workspace,
                actor: link.actor,
                from: link.from,
                to: link.to,
                relation: link.relation,
            };
            let response: serde_json::Value = post_json(client, server, "/v1/links", &request)?;
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
                post_json(client, server, "/v1/links/xanadu", &request)?;
            print_value(&response)
        }
    }
}

fn handle_note(client: &Client, server: &str, command: NoteCommand) -> Result<()> {
    match command.command {
        NoteSubcommand::Add(add) => {
            let request = CreateNoteRequest {
                workspace: add.workspace,
                author: add.author,
                title: add.title,
                body: add.body,
            };
            let response: serde_json::Value = post_json(client, server, "/v1/notes", &request)?;
            print_value(&response)
        }
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
                post_json(client, server, "/v1/notes/update", &request)?;
            print_value(&response)
        }
    }
}

fn handle_task(client: &Client, server: &str, command: TaskCommand) -> Result<()> {
    match command.command {
        TaskSubcommand::Claim(task) => {
            let request = ClaimTaskRequest {
                workspace: task.workspace,
                actor: task.actor,
                task_id: task.task_id,
                lease_seconds: task.lease_seconds,
            };
            let response: serde_json::Value =
                post_json(client, server, "/v1/tasks/claim", &request)?;
            print_value(&response)
        }
        TaskSubcommand::Complete(task) => {
            let request = CompleteTaskRequest {
                workspace: task.workspace,
                actor: task.actor,
                task_id: task.task_id,
            };
            let response: serde_json::Value =
                post_json(client, server, "/v1/tasks/complete", &request)?;
            print_value(&response)
        }
        TaskSubcommand::Context(task) => {
            let path = format!("/v1/tasks/{}/context", task.task_id);
            let response: serde_json::Value = get_json(client, server, &path)?;
            print_value(&response)
        }
        TaskSubcommand::Dag(task) => {
            let path = format!("/v1/tasks/{}/dag", task.task_id);
            let response: serde_json::Value = get_json(client, server, &path)?;
            print_value(&response)
        }
        TaskSubcommand::Depend(task) => {
            let request = AddTaskDependencyRequest {
                workspace: task.workspace,
                actor: task.actor,
                task_id: task.task_id,
                depends_on_task_id: task.depends_on,
            };
            let response: serde_json::Value =
                post_json(client, server, "/v1/tasks/dependencies", &request)?;
            print_value(&response)
        }
        TaskSubcommand::List(task) => {
            let mut query = Vec::new();
            if let Some(status) = task.status {
                query.push(format!("status={}", status.as_str()));
            }
            if let Some(epic_id) = task.epic_id {
                query.push(format!("epic_id={epic_id}"));
            }
            if task.ready_only {
                query.push("ready_only=true".to_owned());
            }

            let suffix = if query.is_empty() {
                String::new()
            } else {
                format!("?{}", query.join("&"))
            };
            let path = format!("/v1/workspaces/{}/tasks{}", task.workspace, suffix);
            let response: serde_json::Value = get_json(client, server, &path)?;
            print_value(&response)
        }
        TaskSubcommand::Offer(task) => {
            let request = OfferTaskRequest {
                workspace: task.workspace,
                author: task.author,
                depends_on: task.depends_on,
                title: task.title,
                details: task.details,
                epic_id: task.epic_id,
            };
            let response: serde_json::Value =
                post_json(client, server, "/v1/tasks/offers", &request)?;
            print_value(&response)
        }
        TaskSubcommand::Release(task) => {
            let request = ReleaseTaskRequest {
                workspace: task.workspace,
                actor: task.actor,
                task_id: task.task_id,
            };
            let response: serde_json::Value =
                post_json(client, server, "/v1/tasks/release", &request)?;
            print_value(&response)
        }
        TaskSubcommand::Update(task) => {
            let request = UpdateTaskRequest {
                workspace: task.workspace,
                actor: task.actor,
                task_id: task.task_id,
                title: task.title,
                details: task.details,
                epic_id: task.epic_id,
            };
            let response: serde_json::Value =
                post_json(client, server, "/v1/tasks/update", &request)?;
            print_value(&response)
        }
    }
}

fn print_value<T: Serialize>(value: &T) -> Result<()> {
    let rendered = to_string_pretty(value).context(JsonRender)?;
    println!("{rendered}");
    Ok(())
}
