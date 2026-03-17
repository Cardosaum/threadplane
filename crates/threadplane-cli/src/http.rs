#![expect(
    clippy::redundant_pub_crate,
    reason = "HTTP helpers are crate-local adapters with explicit visibility."
)]

use reqwest::blocking::{Client, Response};
use serde::{de::DeserializeOwned, Serialize};
use snafu::{ensure, IntoError as _, ResultExt as _};

use crate::build_info::current_build_info;
use crate::error::{
    ContractMismatchDetails, HttpClientBuild, JsonContractMismatch, JsonParse, NonSuccessStatus,
    RequestSend, ResponseBodyRead, Result,
};
use threadplane_core::{compare_build_info, BuildComparison, ServiceSnapshot};

pub(crate) fn build_http_client() -> Result<Client> {
    Client::builder().build().context(HttpClientBuild)
}

pub(crate) fn get_json<T: DeserializeOwned>(
    client: &Client,
    server: &str,
    path: &str,
) -> Result<T> {
    let request_url = url(server, path);
    let response = client.get(&request_url).send().context(RequestSend {
        method: "GET",
        url: request_url.clone(),
    })?;
    parse_response(client, "GET", server, request_url, response)
}

pub(crate) fn post_json<B: Serialize, T: DeserializeOwned>(
    client: &Client,
    server: &str,
    path: &str,
    body: &B,
    idempotency_key: Option<&str>,
) -> Result<T> {
    let request_url = url(server, path);
    let mut request_builder = client.post(&request_url).json(body);
    if let Some(command_idempotency_key) = idempotency_key {
        request_builder =
            request_builder.header("Idempotency-Key", command_idempotency_key);
    }
    let response = request_builder.send().context(RequestSend {
        method: "POST",
        url: request_url.clone(),
    })?;
    parse_response(client, "POST", server, request_url, response)
}

pub(crate) fn patch_json<B: Serialize, T: DeserializeOwned>(
    client: &Client,
    server: &str,
    path: &str,
    body: &B,
    idempotency_key: Option<&str>,
) -> Result<T> {
    let request_url = url(server, path);
    let mut request_builder = client.patch(&request_url).json(body);
    if let Some(command_idempotency_key) = idempotency_key {
        request_builder = request_builder.header("Idempotency-Key", command_idempotency_key);
    }
    let response = request_builder.send().context(RequestSend {
        method: "PATCH",
        url: request_url.clone(),
    })?;
    parse_response(client, "PATCH", server, request_url, response)
}

fn parse_response<T: DeserializeOwned>(
    client: &Client,
    method: &'static str,
    server: &str,
    url: String,
    response: Response,
) -> Result<T> {
    let status = response.status();
    let body = response
        .text()
        .context(ResponseBodyRead { url: url.clone() })?;
    ensure!(
        status.is_success(),
        NonSuccessStatus {
            method,
            url,
            status,
            body,
        }
    );

    serde_json::from_str(&body).map_err(|source| {
        if let Some(details) = contract_mismatch_details(client, server) {
            return JsonContractMismatch {
                details: Box::new(details),
                url,
            }
            .into_error(source);
        }

        JsonParse { url }.into_error(source)
    })
}

fn contract_mismatch_details(client: &Client, server: &str) -> Option<ContractMismatchDetails> {
    let comparison = fetch_build_comparison(client, server)?;
    (!comparison.matches).then(|| ContractMismatchDetails {
        changed_fields: comparison
            .differences
            .iter()
            .map(|difference| difference.field.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        cli_commit: comparison
            .client
            .git_commit
            .as_deref()
            .unwrap_or("unknown")
            .to_owned(),
        cli_version: comparison.client.version,
        server_commit: comparison
            .server
            .git_commit
            .as_deref()
            .unwrap_or("unknown")
            .to_owned(),
        server_version: comparison.server.version,
    })
}

fn fetch_build_comparison(client: &Client, server: &str) -> Option<BuildComparison> {
    let request_url = root_url(server);
    let response = client.get(&request_url).send().ok()?;
    if !response.status().is_success() {
        return None;
    }

    let body = response.text().ok()?;
    let snapshot = serde_json::from_str::<ServiceSnapshot>(&body).ok()?;

    Some(compare_build_info(&current_build_info(), &snapshot.build))
}

fn root_url(server: &str) -> String {
    format!("{}/", server.trim_end_matches('/'))
}

fn url(server: &str, path: &str) -> String {
    format!("{}{}", server.trim_end_matches('/'), path)
}
