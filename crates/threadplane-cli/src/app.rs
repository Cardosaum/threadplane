#![expect(
    clippy::redundant_pub_crate,
    reason = "CLI modules are crate-internal namespaces with explicit visibility."
)]

use clap::Parser as _;
use snafu::ResultExt as _;

use threadplane_core::load_threadplane_config_with_overrides;

use crate::{
    command::{execute, Cli},
    error::{ConfigLoad, Result},
    http::build_http_client,
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

    execute(cli, &loaded_config.config, &loaded_config.discovery, &client)
}
