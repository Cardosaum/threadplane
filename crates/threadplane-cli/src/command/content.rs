#![allow(
    clippy::wildcard_imports,
    reason = "Content command definitions intentionally build on the command module prelude."
)]

use super::*;

#[derive(Debug, Args)]
#[command(about = "Create, inspect, and update notes")]
pub(crate) struct NoteCommand {
    #[command(subcommand)]
    pub(crate) command: NoteSubcommand,
}

#[derive(Debug, Args)]
#[command(about = "Capture and recall durable memories for people and AI agents")]
pub(crate) struct MemoryCommand {
    #[command(subcommand)]
    pub(crate) command: MemorySubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum NoteSubcommand {
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
pub(crate) enum MemorySubcommand {
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
pub(crate) struct AddMemory {
    #[arg(long, help = "Structured audience: agent, human, or both")]
    pub(crate) audience: String,

    #[arg(long, help = "Who is recording the memory")]
    pub(crate) author: String,

    #[arg(long, help = "Memory body")]
    pub(crate) body: String,

    #[arg(long, help = "Importance: normal, high, or critical")]
    pub(crate) importance: String,

    #[arg(long, help = "Memory kind, for example workflow, decision, or runbook")]
    pub(crate) kind: String,

    #[arg(
        long = "recall-trigger",
        help = "Recall trigger tag, for example session_start. Repeat for multiple triggers."
    )]
    pub(crate) recall_triggers: Vec<String>,

    #[arg(long, help = "Scope: workspace, repo, or global")]
    pub(crate) scope: String,

    #[arg(long = "tag", help = "Memory tag. Repeat for multiple tags.")]
    pub(crate) tags: Vec<String>,

    #[arg(long, help = "Memory title")]
    pub(crate) title: String,

    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}

#[derive(Debug, Args)]
pub(crate) struct ListMemories {
    #[arg(
        long,
        help = "Only include memories for this audience: agent, human, or both"
    )]
    pub(crate) audience: Option<String>,

    #[arg(
        long,
        default_value = "json",
        help = "Render JSON or a compact human-readable summary"
    )]
    pub(crate) format: OutputFormat,

    #[arg(long, help = "Only include memories with this importance")]
    pub(crate) importance: Option<String>,

    #[arg(long, help = "Only include memories with this kind")]
    pub(crate) kind: Option<String>,

    #[arg(long, help = "Maximum number of memories to return")]
    pub(crate) limit: Option<i64>,

    #[arg(long, help = "Search query matched against memory title and body")]
    pub(crate) query: Option<String>,

    #[arg(
        long = "recall-trigger",
        help = "Only include memories with this recall trigger"
    )]
    pub(crate) recall_trigger: Option<String>,

    #[arg(long, help = "Only include memories with this tag")]
    pub(crate) tag: Option<String>,

    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}

#[derive(Debug, Args)]
pub(crate) struct PrimeMemories {
    #[arg(
        long,
        default_value = "agent",
        help = "Recall memories for this audience: agent, human, or both"
    )]
    pub(crate) audience: String,

    #[arg(
        long,
        default_value = "json",
        help = "Render JSON or a compact human-readable summary"
    )]
    pub(crate) format: OutputFormat,

    #[arg(long, help = "Maximum number of memories to return")]
    pub(crate) limit: Option<i64>,

    #[arg(long = "recall-trigger", default_value = "session_start")]
    pub(crate) recall_trigger: String,

    #[arg(long, default_value = "prime")]
    pub(crate) tag: String,

    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}

#[derive(Debug, Args)]
pub(crate) struct ShowMemory {
    #[arg(long, help = "Memory UUID")]
    pub(crate) memory_id: Uuid,
}

#[derive(Debug, Args)]
pub(crate) struct AddNote {
    #[arg(long, help = "Note author")]
    pub(crate) author: String,

    #[arg(long, help = "Note body")]
    pub(crate) body: String,

    #[arg(long, help = "Note title")]
    pub(crate) title: String,

    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}

#[derive(Debug, Args)]
pub(crate) struct ListNotes {
    #[arg(long, help = "Only include notes from this author")]
    pub(crate) author: Option<String>,

    #[arg(
        long,
        default_value = "json",
        help = "Render JSON or a compact human-readable summary"
    )]
    pub(crate) format: OutputFormat,

    #[arg(long, help = "Maximum number of notes to return")]
    pub(crate) limit: Option<i64>,

    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}

#[derive(Debug, Args)]
pub(crate) struct SearchNotes {
    #[arg(long, help = "Only include notes from this author")]
    pub(crate) author: Option<String>,

    #[arg(
        long,
        default_value = "json",
        help = "Render JSON or a compact human-readable summary"
    )]
    pub(crate) format: OutputFormat,

    #[arg(long, help = "Maximum number of notes to return")]
    pub(crate) limit: Option<i64>,

    #[arg(long, help = "Search query matched against note title and body")]
    pub(crate) query: String,

    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}

#[derive(Debug, Args)]
pub(crate) struct ShowNote {
    #[arg(long, help = "Note UUID")]
    pub(crate) note_id: Uuid,
}

#[derive(Debug, Args)]
pub(crate) struct UpdateNote {
    #[arg(long, help = "Actor performing the update")]
    pub(crate) actor: String,

    #[arg(long, help = "Updated note body")]
    pub(crate) body: String,

    #[arg(long, help = "Note UUID")]
    pub(crate) note_id: Uuid,

    #[arg(long, help = "Updated note title")]
    pub(crate) title: String,

    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}
