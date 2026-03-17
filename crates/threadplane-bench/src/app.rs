#![expect(
    clippy::redundant_pub_crate,
    reason = "Benchmark app wiring is crate-local orchestration."
)]

use chrono::Utc;
use std::time::Instant;

use clap::Parser as _;
use reqwest::blocking::Client;
use snafu::ResultExt as _;

use threadplane_core::{load_threadplane_config_with_overrides, BuildInfo, ServiceSnapshot};

use crate::{
    build_info::current_build_info,
    command::{Cli, Command, ScenarioKind as CommandScenarioKind},
    error::{ConfigLoad, HttpClientBuild, JsonRender, Result},
    report::{build_report, BenchmarkReportContext},
    scenario::{run_benchmark, RunSettings, ScenarioKind},
};

pub(crate) fn run() -> Result<()> {
    let cli = Cli::parse();
    let config_overrides = cli.config_overrides();
    let loaded_config = load_threadplane_config_with_overrides(
        cli.config.as_deref(),
        &config_overrides,
    )
    .context(ConfigLoad)?;
    let client = build_http_client()?;
    let server = loaded_config.config.cli.url;

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
                BenchmarkReportContext {
                    captured_at: Utc::now().to_rfc3339(),
                    client_build: current_build_info(),
                    concurrency: settings.concurrency,
                    scenario: settings.scenario,
                    server_build: fetch_server_build(&client, &settings.server),
                    server_url: settings.server.clone(),
                    total_duration_ms: started_at.elapsed().as_secs_f64() * 1_000.0_f64,
                    workspace: settings.workspace.clone(),
                },
                samples,
            );
            let rendered = serde_json::to_string_pretty(&report).context(JsonRender)?;
            println!("{rendered}");
            Ok(())
        }
    }
}

fn build_http_client() -> Result<Client> {
    Client::builder().build().context(HttpClientBuild)
}

fn fetch_server_build(client: &Client, server: &str) -> Option<BuildInfo> {
    let response = client
        .get(format!("{}/", server.trim_end_matches('/')))
        .send()
        .ok()?;
    if !response.status().is_success() {
        return None;
    }

    let snapshot = response.json::<ServiceSnapshot>().ok()?;
    Some(snapshot.build)
}
