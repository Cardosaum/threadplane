use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use serde_json::to_string_pretty;
use uuid::Uuid;

use threadplane_core::{
    AddLinkRequest, ApiEnvelope, ClaimTaskRequest, CreateNoteRequest, OfferTaskRequest,
    DEFAULT_SERVER_URL, SERVICE_NAME,
};

#[derive(Debug, Parser)]
#[command(name = SERVICE_NAME, version, about = "CLI for the threadplane POC")]
struct Cli {
    #[arg(long, env = "THREADPLANE_URL", default_value = DEFAULT_SERVER_URL)]
    server: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Scope,
    Note(NoteCommand),
    Task(TaskCommand),
    Link(LinkCommand),
    Events(EventsCommand),
}

#[derive(Debug, Args)]
struct NoteCommand {
    #[command(subcommand)]
    command: NoteSubcommand,
}

#[derive(Debug, Subcommand)]
enum NoteSubcommand {
    Add(AddNote),
}

#[derive(Debug, Args)]
struct AddNote {
    #[arg(long)]
    workspace: String,
    #[arg(long)]
    author: String,
    #[arg(long)]
    title: String,
    #[arg(long)]
    body: String,
}

#[derive(Debug, Args)]
struct TaskCommand {
    #[command(subcommand)]
    command: TaskSubcommand,
}

#[derive(Debug, Subcommand)]
enum TaskSubcommand {
    Offer(OfferTask),
    Claim(ClaimTask),
    ListOpen(ListOpenTasks),
    Context(TaskContextCommand),
}

#[derive(Debug, Args)]
struct OfferTask {
    #[arg(long)]
    workspace: String,
    #[arg(long)]
    author: String,
    #[arg(long)]
    title: String,
    #[arg(long)]
    details: String,
}

#[derive(Debug, Args)]
struct ClaimTask {
    #[arg(long)]
    workspace: String,
    #[arg(long)]
    actor: String,
    #[arg(long)]
    task_id: Uuid,
    #[arg(long)]
    lease_seconds: Option<i64>,
}

#[derive(Debug, Args)]
struct ListOpenTasks {
    #[arg(long)]
    workspace: String,
}

#[derive(Debug, Args)]
struct TaskContextCommand {
    #[arg(long)]
    task_id: Uuid,
}

#[derive(Debug, Args)]
struct LinkCommand {
    #[command(subcommand)]
    command: LinkSubcommand,
}

#[derive(Debug, Subcommand)]
enum LinkSubcommand {
    Add(AddLink),
}

#[derive(Debug, Args)]
struct AddLink {
    #[arg(long)]
    workspace: String,
    #[arg(long)]
    actor: String,
    #[arg(long)]
    from: String,
    #[arg(long)]
    to: String,
    #[arg(long)]
    relation: String,
}

#[derive(Debug, Args)]
struct EventsCommand {
    #[command(subcommand)]
    command: EventsSubcommand,
}

#[derive(Debug, Subcommand)]
enum EventsSubcommand {
    List(ListEvents),
}

#[derive(Debug, Args)]
struct ListEvents {
    #[arg(long)]
    workspace: String,
    #[arg(long, default_value_t = 25)]
    limit: i64,
}

fn main() -> Result<()> {
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();
    let client = Client::builder()
        .build()
        .context("failed to construct HTTP client")?;

    match cli.command {
        Command::Scope => print_value(&get_json::<serde_json::Value>(
            &client,
            &cli.server,
            "/scope",
        )?)?,
        Command::Note(command) => handle_note(&client, &cli.server, command)?,
        Command::Task(command) => handle_task(&client, &cli.server, command)?,
        Command::Link(command) => handle_link(&client, &cli.server, command)?,
        Command::Events(command) => handle_events(&client, &cli.server, command)?,
    }

    Ok(())
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
    }
}

fn handle_task(client: &Client, server: &str, command: TaskCommand) -> Result<()> {
    match command.command {
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
        TaskSubcommand::ListOpen(task) => {
            let path = format!("/v1/workspaces/{}/tasks/open", task.workspace);
            let response: serde_json::Value = get_json(client, server, &path)?;
            print_value(&response)
        }
        TaskSubcommand::Context(task) => {
            let path = format!("/v1/tasks/{}/context", task.task_id);
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

fn get_json<T: DeserializeOwned>(client: &Client, server: &str, path: &str) -> Result<T> {
    let response = client
        .get(url(server, path))
        .send()
        .with_context(|| format!("GET {} failed", url(server, path)))?;
    parse_response(response)
}

fn post_json<B: serde::Serialize, T: DeserializeOwned>(
    client: &Client,
    server: &str,
    path: &str,
    body: &B,
) -> Result<T> {
    let response = client
        .post(url(server, path))
        .json(body)
        .send()
        .with_context(|| format!("POST {} failed", url(server, path)))?;
    parse_response(response)
}

fn parse_response<T: DeserializeOwned>(response: reqwest::blocking::Response) -> Result<T> {
    let status = response.status();
    let body = response.text().context("failed to read response body")?;
    if !status.is_success() {
        anyhow::bail!("request failed with {}: {}", status, body);
    }

    serde_json::from_str(&body).context("failed to parse JSON response")
}

fn url(server: &str, path: &str) -> String {
    format!("{}{}", server.trim_end_matches('/'), path)
}

fn print_value<T: serde::Serialize>(value: &T) -> Result<()> {
    let rendered = to_string_pretty(value).context("failed to render JSON output")?;
    println!("{rendered}");
    Ok(())
}

#[allow(dead_code)]
fn _type_assertions() {
    let _: Option<ApiEnvelope<serde_json::Value>> = None;
}
