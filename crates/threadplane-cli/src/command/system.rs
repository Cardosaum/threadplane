#![allow(
    clippy::wildcard_imports,
    reason = "System command definitions intentionally build on the command module prelude."
)]

use super::*;

#[derive(Debug, Args)]
#[command(about = "Inspect and compare CLI/server build identity")]
pub(crate) struct BuildCommand {
    #[command(subcommand)]
    pub(crate) command: BuildSubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum BuildSubcommand {
    #[command(about = "Compare the local CLI build with the running server build")]
    Compare,
    #[command(about = "Show the local threadplane-cli build identity")]
    Show,
}

#[derive(Debug, Args)]
#[command(about = "Inspect configuration discovery and the resolved runtime config")]
pub(crate) struct ConfigCommand {
    #[command(subcommand)]
    pub(crate) command: ConfigSubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ConfigSubcommand {
    #[command(about = "Print the resolved config and where threadplane looks for it")]
    Show,
}

#[derive(Debug, Args)]
#[command(about = "Inspect graph projection replay status")]
pub(crate) struct ProjectionCommand {
    #[command(subcommand)]
    pub(crate) command: ProjectionSubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ProjectionSubcommand {
    #[command(about = "Show the persisted replay watermark for the graph projection")]
    Status,
}
