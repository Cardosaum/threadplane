#![expect(
    clippy::redundant_pub_crate,
    reason = "Benchmark app wiring is crate-local orchestration."
)]

use std::{env, time::Instant};

use clap::Parser as _;
use reqwest::blocking::Client;
use snafu::ResultExt as _;

use threadplane_core::load_threadplane_config;

use crate::{
    command::{Cli, Command, ScenarioKind as CommandScenarioKind},
    error::{ConfigLoad, HttpClientBuild, JsonRender, Result},
    report::build_report,
    scenario::{run_benchmark, RunSettings, ScenarioKind},
};

pub(crate) fn run() -> Result<()> {
    drop(dotenvy::dotenv());

    let cli = Cli::parse();
    apply_config_override(&cli);

    let config = load_threadplane_config().context(ConfigLoad)?;
    let client = build_http_client()?;
    let server = cli.server.clone().unwrap_or_else(|| config.cli.url.clone());

    match cli.command {
        Command::Run(run_command) => {
            let started_at = Instant::now();
            let settings = RunSettings {
                actor_prefix: run_command.actor_prefix,
                concurrency: run_command.concurrency,
                operations: run_command.operations,
                scenario: match run_command.scenario {
                    CommandScenarioKind::Mixed => ScenarioKind::Mixed,
                    CommandScenarioKind::NoteWrites => ScenarioKind::NoteWrites,
                },
                server,
                workspace: run_command.workspace,
            };
            let samples = run_benchmark(&client, &settings)?;
            let report = build_report(
                &settings.workspace,
                settings.scenario,
                settings.concurrency,
                started_at.elapsed().as_secs_f64() * 1_000.0,
                samples,
            );
            let rendered = serde_json::to_string_pretty(&report).context(JsonRender)?;
            println!("{rendered}");
            Ok(())
        }
    }
}

fn apply_config_override(cli: &Cli) {
    if let Some(config_path) = &cli.config {
        env::set_var("THREADPLANE_CONFIG", config_path);
    }
}

fn build_http_client() -> Result<Client> {
    Client::builder().build().context(HttpClientBuild)
}
