use super::super::*;

#[derive(Debug, Args)]
#[command(about = "Create and inspect first-class epics")]
pub(crate) struct EpicCommand {
    #[command(subcommand)]
    pub(crate) command: EpicSubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum EpicSubcommand {
    #[command(about = "Create a new epic")]
    Add(AddEpic),
    #[command(about = "List epics in a workspace")]
    List(ListEpics),
    #[command(about = "Fetch an epic by ID")]
    Show(ShowEpic),
}

#[derive(Debug, Args)]
pub(crate) struct AddEpic {
    #[arg(long, help = "Epic author")]
    pub(crate) author: String,

    #[arg(long, help = "Epic body")]
    pub(crate) body: String,

    #[arg(long, help = "Epic title")]
    pub(crate) title: String,

    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}

#[derive(Debug, Args)]
pub(crate) struct ListEpics {
    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}

#[derive(Debug, Args)]
pub(crate) struct ShowEpic {
    #[arg(long, help = "Epic UUID")]
    pub(crate) epic_id: Uuid,
}
