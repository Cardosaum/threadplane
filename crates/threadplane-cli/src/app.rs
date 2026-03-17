#![expect(
    clippy::redundant_pub_crate,
    reason = "CLI modules are crate-internal namespaces with explicit visibility."
)]

use std::env;

use clap::Parser as _;
use snafu::ResultExt as _;

use threadplane_core::load_threadplane_config;

use crate::{
    command::{execute, Cli},
    error::{ConfigLoad, Result},
    http::build_http_client,
};

pub(crate) fn run() -> Result<()> {
    drop(dotenvy::dotenv());

    let cli = Cli::parse();
    apply_config_override(&cli);

    let config = load_threadplane_config().context(ConfigLoad)?;
    let client = build_http_client()?;

    execute(cli, &config, &client)
}

fn apply_config_override(cli: &Cli) {
    if let Some(config_path) = &cli.config {
        env::set_var("THREADPLANE_CONFIG", config_path);
    }
}
