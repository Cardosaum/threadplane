#![expect(
    clippy::redundant_pub_crate,
    reason = "CLI modules are crate-internal namespaces with explicit visibility."
)]

use clap::Parser as _;
use reqwest::blocking::Client;
use snafu::ResultExt as _;

use serde::{de::DeserializeOwned, Serialize};
use threadplane_core::load_threadplane_config_with_overrides;

use crate::{
    command::{execute, Cli},
    error::{ConfigLoad, Result},
    http::{build_http_client, get_json, patch_json, post_json, put_json},
    runtime::{ApiClient, CommandContext, StdCommandOutput, ThreadSleeper},
};

struct HttpApi<'client> {
    client: &'client Client,
    server: &'client str,
}

impl<'client> HttpApi<'client> {
    const fn new(client: &'client Client, server: &'client str) -> Self {
        Self { client, server }
    }
}

impl ApiClient for HttpApi<'_> {
    fn get_json<T>(&self, path: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        get_json(self.client, self.server, path)
    }

    fn patch_json<B, T>(&self, path: &str, body: &B, idempotency_key: Option<&str>) -> Result<T>
    where
        B: Serialize,
        T: DeserializeOwned,
    {
        patch_json(self.client, self.server, path, body, idempotency_key)
    }

    fn post_json<B, T>(&self, path: &str, body: &B, idempotency_key: Option<&str>) -> Result<T>
    where
        B: Serialize,
        T: DeserializeOwned,
    {
        post_json(self.client, self.server, path, body, idempotency_key)
    }

    fn put_json<B, T>(&self, path: &str, body: &B, idempotency_key: Option<&str>) -> Result<T>
    where
        B: Serialize,
        T: DeserializeOwned,
    {
        put_json(self.client, self.server, path, body, idempotency_key)
    }
}

pub(crate) fn run() -> Result<()> {
    let cli = Cli::parse();
    let config_overrides = cli.config_overrides();
    let loaded_config =
        load_threadplane_config_with_overrides(cli.config.as_deref(), &config_overrides)
            .context(ConfigLoad)?;
    let client = build_http_client()?;
    let api = HttpApi::new(&client, loaded_config.config.cli.url.as_str());
    let mut output = StdCommandOutput;
    let sleeper = ThreadSleeper;
    let mut context = CommandContext::builder()
        .api(&api)
        .output(&mut output)
        .sleeper(&sleeper)
        .build();

    execute(
        cli,
        &loaded_config.config,
        &loaded_config.discovery,
        &mut context,
    )
}
