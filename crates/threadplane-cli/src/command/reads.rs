#![allow(
    clippy::wildcard_imports,
    reason = "Read command definitions intentionally build on the command module prelude."
)]

use super::*;

#[derive(Debug, Args)]
#[command(about = "Explore entities and their graph-linked relations")]
pub(crate) struct EntityCommand {
    #[command(subcommand)]
    pub(crate) command: EntitySubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum EntitySubcommand {
    #[command(about = "List entities related to the selected entity")]
    Related(RelatedEntities),
    #[command(about = "Fetch an entity and its related graph neighborhood")]
    Show(ShowEntity),
}

#[derive(Debug, Args)]
pub(crate) struct ShowEntity {
    #[arg(long, help = "Entity ref, for example task:<uuid> or note:<uuid>")]
    pub(crate) entity_ref: String,

    #[arg(
        long,
        default_value = "json",
        help = "Render JSON or a compact human-readable summary"
    )]
    pub(crate) format: OutputFormat,
}

#[derive(Debug, Args)]
pub(crate) struct RelatedEntities {
    #[arg(long, help = "Entity ref, for example task:<uuid> or note:<uuid>")]
    pub(crate) entity_ref: String,

    #[arg(
        long,
        default_value = "json",
        help = "Render JSON or a compact human-readable summary"
    )]
    pub(crate) format: OutputFormat,
}

#[derive(Debug, Args)]
#[command(about = "Inspect workspace event history")]
pub(crate) struct EventsCommand {
    #[command(subcommand)]
    pub(crate) command: EventsSubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum EventsSubcommand {
    #[command(about = "List recent events for a workspace")]
    List(ListEvents),
    #[command(about = "Read workspace events incrementally and optionally follow for new changes")]
    Tail(TailEvents),
}

#[derive(Debug, Args)]
pub(crate) struct ListEvents {
    #[arg(
        long,
        default_value = "json",
        help = "Render JSON or a compact human-readable summary"
    )]
    pub(crate) format: OutputFormat,

    #[arg(
        long,
        default_value_t = 25,
        help = "Maximum number of events to return"
    )]
    pub(crate) limit: i64,

    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}

#[derive(Debug, Args)]
pub(crate) struct TailEvents {
    #[arg(long, help = "Resume after this event UUID")]
    pub(crate) after_event_id: Option<Uuid>,

    #[arg(long, help = "Keep polling for new events")]
    pub(crate) follow: bool,

    #[arg(
        long,
        default_value = "json",
        help = "Render JSON or a compact human-readable summary"
    )]
    pub(crate) format: OutputFormat,

    #[arg(
        long,
        default_value_t = 25,
        help = "Maximum number of events to return per poll"
    )]
    pub(crate) limit: i64,

    #[arg(
        long,
        default_value_t = 2,
        help = "Seconds to wait between follow polls"
    )]
    pub(crate) poll_seconds: u64,

    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}
