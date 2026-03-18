use super::super::*;

#[derive(Debug, Args)]
#[command(about = "Create semantic and Xanadu links between entities")]
pub(crate) struct LinkCommand {
    #[command(subcommand)]
    pub(crate) command: LinkSubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum LinkSubcommand {
    #[command(about = "Create a semantic graph link between two entities")]
    Add(AddLink),
    #[command(about = "Create a Xanadu transclusion link between two text entities")]
    Xanadu(AddXanaduLink),
}

#[derive(Debug, Args)]
pub(crate) struct AddLink {
    #[arg(long, help = "Actor creating the link")]
    pub(crate) actor: String,

    #[arg(long, help = "Source entity ref, for example task:<uuid>")]
    pub(crate) from: String,

    #[arg(long, help = "Relationship name, for example depends_on")]
    pub(crate) relation: String,

    #[arg(long, help = "Target entity ref, for example note:<uuid>")]
    pub(crate) to: String,

    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}

#[derive(Debug, Args)]
pub(crate) struct AddXanaduLink {
    #[arg(long, help = "Actor creating the Xanadu link")]
    pub(crate) actor: String,

    #[arg(long, help = "Source entity ref, for example task:<uuid>")]
    pub(crate) from: String,

    #[arg(long, help = "Target entity ref, for example note:<uuid>")]
    pub(crate) to: String,

    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}
