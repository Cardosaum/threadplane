#![expect(
    clippy::redundant_pub_crate,
    reason = "Benchmark CLI command types are crate-local."
)]

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "threadplane-bench",
    about = "Repeatable benchmark harness for threadplane",
    version
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,

    #[arg(long, global = true, help = "Load a specific threadplane config file.")]
    pub(crate) config: Option<PathBuf>,

    #[arg(
        long,
        global = true,
        help = "HTTP base URL for threadplane-server. Overrides cli.url from config."
    )]
    pub(crate) server: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    Run(RunCommand),
}

#[derive(Debug, clap::Args)]
pub(crate) struct RunCommand {
    #[arg(
        long,
        default_value = "bench",
        help = "Prefix used for generated author and title values."
    )]
    pub(crate) actor_prefix: String,

    #[arg(long, default_value_t = 8, help = "Number of worker threads to use.")]
    pub(crate) concurrency: usize,

    #[arg(long, default_value_t = 100, help = "Total operations to execute.")]
    pub(crate) operations: usize,

    #[arg(
        long,
        default_value = "mixed",
        help = "Scenario profile to execute."
    )]
    pub(crate) scenario: ScenarioKind,

    #[arg(long, help = "Workspace used for benchmark writes and reads.")]
    pub(crate) workspace: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub(crate) enum ScenarioKind {
    Mixed,
    #[default]
    NoteWrites,
}
