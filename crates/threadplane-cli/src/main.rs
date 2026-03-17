use core::result::Result as CoreResult;
use std::{env, path::PathBuf};

use clap::{Args, Parser, Subcommand};
use reqwest::blocking::{Client, Response};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, to_string_pretty};
use snafu::{ensure, ResultExt as _, Snafu};
use uuid::Uuid;

use threadplane_core::{
    default_config_path, default_system_config_path, load_threadplane_config, AddLinkRequest,
    ClaimTaskRequest, CreateNoteRequest, CreateXanaduLinkRequest, OfferTaskRequest,
    ThreadplaneConfig, ThreadplaneError, UpdateNoteRequest, UpdateTaskRequest, SERVICE_NAME,
};

type Result<T, E = CliError> = CoreResult<T, E>;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(false)))]
enum CliError {
    #[snafu(display("failed to load threadplane config: {source}"))]
    ConfigLoad {
        source: ThreadplaneError,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("failed to construct HTTP client: {source}"))]
    HttpClientBuild {
        source: reqwest::Error,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("failed to parse JSON response from {url}: {source}"))]
    JsonParse {
        url: String,
        source: serde_json::Error,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("failed to render JSON output: {source}"))]
    JsonRender {
        source: serde_json::Error,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("{method} {url} returned {status}: {body}"))]
    NonSuccessStatus {
        method: &'static str,
        url: String,
        status: reqwest::StatusCode,
        body: String,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("{method} {url} failed: {source}"))]
    RequestSend {
        method: &'static str,
        url: String,
        source: reqwest::Error,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("failed to read response body from {url}: {source}"))]
    ResponseBodyRead {
        url: String,
        source: reqwest::Error,
        #[snafu(implicit)]
        location: snafu::Location,
    },
}

#[derive(Debug, Parser)]
#[command(
    name = SERVICE_NAME,
    version,
    about = "Shared memory and coordination CLI for people and AI agents",
    long_about = "threadplane-cli talks to threadplane-server so people and agents can share tasks, notes, links, claims, and graph-backed context through one internet-reachable control plane."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    #[arg(long, help = "Path to a threadplane config TOML. Overrides config discovery order.")]
    config: Option<PathBuf>,

    #[arg(long, help = "HTTP base URL for threadplane-server. Overrides cli.url from config.")]
    server: Option<String>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Config(ConfigCommand),
    Events(EventsCommand),
    Link(LinkCommand),
    Note(NoteCommand),
    #[command(about = "Show the product and architecture summary exposed by the service")]
    Scope,
    Task(TaskCommand),
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
    #[arg(long, default_value_t = 25, help = "Maximum number of events to return")]
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
    #[command(about = "Fetch a task plus graph-backed related context")]
    Context(TaskContextCommand),
    #[command(about = "List open tasks for a workspace")]
    ListOpen(ListOpenTasks),
    #[command(about = "Offer a new task into a workspace")]
    Offer(OfferTask),
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
struct TaskContextCommand {
    #[arg(long, help = "Task UUID")]
    task_id: Uuid,
}

#[derive(Debug, Args)]
struct ListOpenTasks {
    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args)]
struct OfferTask {
    #[arg(long, help = "Task author")]
    author: String,

    #[arg(long, help = "Task details")]
    details: String,

    #[arg(long, help = "Task title")]
    title: String,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

#[derive(Debug, Args)]
struct UpdateTask {
    #[arg(long, help = "Actor performing the update")]
    actor: String,

    #[arg(long, help = "Updated task details")]
    details: String,

    #[arg(long, help = "Task UUID")]
    task_id: Uuid,

    #[arg(long, help = "Updated task title")]
    title: String,

    #[arg(long, help = "Workspace name")]
    workspace: String,
}

fn main() -> Result<()> {
    drop(dotenvy::dotenv());

    let cli = Cli::parse();
    if let Some(config_path) = &cli.config {
        env::set_var("THREADPLANE_CONFIG", config_path);
    }
    let config = load_threadplane_config().context(ConfigLoad)?;
    let server = cli.server.unwrap_or_else(|| config.cli.url.clone());
    let client = build_http_client()?;

    match cli.command {
        Command::Config(command) => handle_config(&command, &config)?,
        Command::Events(command) => handle_events(&client, &server, command)?,
        Command::Link(command) => handle_link(&client, &server, command)?,
        Command::Note(command) => handle_note(&client, &server, command)?,
        Command::Scope => print_value(&get_json::<serde_json::Value>(&client, &server, "/scope")?)?,
        Command::Task(command) => handle_task(&client, &server, command)?,
    }

    Ok(())
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

fn build_http_client() -> Result<Client> {
    Client::builder().build().context(HttpClientBuild)
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
        TaskSubcommand::Context(task) => {
            let path = format!("/v1/tasks/{}/context", task.task_id);
            let response: serde_json::Value = get_json(client, server, &path)?;
            print_value(&response)
        }
        TaskSubcommand::ListOpen(task) => {
            let path = format!("/v1/workspaces/{}/tasks/open", task.workspace);
            let response: serde_json::Value = get_json(client, server, &path)?;
            print_value(&response)
        }
        TaskSubcommand::Offer(task) => {
            let request = OfferTaskRequest {
                workspace: task.workspace,
                author: task.author,
                title: task.title,
                details: task.details,
            };
            let response: serde_json::Value =
                post_json(client, server, "/v1/tasks/offers", &request)?;
            print_value(&response)
        }
        TaskSubcommand::Update(task) => {
            let request = UpdateTaskRequest {
                workspace: task.workspace,
                actor: task.actor,
                task_id: task.task_id,
                title: task.title,
                details: task.details,
            };
            let response: serde_json::Value =
                post_json(client, server, "/v1/tasks/update", &request)?;
            print_value(&response)
        }
    }
}

fn get_json<T: DeserializeOwned>(client: &Client, server: &str, path: &str) -> Result<T> {
    let request_url = url(server, path);
    let response = client.get(&request_url).send().context(RequestSend {
        method: "GET",
        url: request_url.clone(),
    })?;
    parse_response("GET", request_url, response)
}

fn post_json<B: Serialize, T: DeserializeOwned>(
    client: &Client,
    server: &str,
    path: &str,
    body: &B,
) -> Result<T> {
    let request_url = url(server, path);
    let response = client
        .post(&request_url)
        .json(body)
        .send()
        .context(RequestSend {
            method: "POST",
            url: request_url.clone(),
        })?;
    parse_response("POST", request_url, response)
}

fn parse_response<T: DeserializeOwned>(
    method: &'static str,
    url: String,
    response: Response,
) -> Result<T> {
    let status = response.status();
    let body = response
        .text()
        .context(ResponseBodyRead { url: url.clone() })?;
    ensure!(
        status.is_success(),
        NonSuccessStatus {
            method,
            url,
            status,
            body,
        }
    );

    serde_json::from_str(&body).context(JsonParse { url })
}

fn print_value<T: Serialize>(value: &T) -> Result<()> {
    let rendered = to_string_pretty(value).context(JsonRender)?;
    println!("{rendered}");
    Ok(())
}

fn url(server: &str, path: &str) -> String {
    format!("{}{}", server.trim_end_matches('/'), path)
}
