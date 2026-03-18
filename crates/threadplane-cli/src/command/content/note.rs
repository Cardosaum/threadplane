use super::super::*;

#[derive(Debug, Args)]
#[command(about = "Create, inspect, and update notes")]
pub(crate) struct NoteCommand {
    #[command(subcommand)]
    pub(crate) command: NoteSubcommand,
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
