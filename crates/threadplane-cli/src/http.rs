#![expect(
    clippy::redundant_pub_crate,
    reason = "HTTP helpers are crate-local adapters with explicit visibility."
)]

use reqwest::blocking::{Client, Response};
use serde::{de::DeserializeOwned, Serialize};
use snafu::{ensure, ResultExt as _};

use crate::error::{
    HttpClientBuild, JsonParse, NonSuccessStatus, RequestSend, ResponseBodyRead, Result,
};

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
    parse_response("GET", request_url, response)
}

pub(crate) fn post_json<B: Serialize, T: DeserializeOwned>(
    client: &Client,
    server: &str,
    path: &str,
    body: &B,
) -> Result<T> {
    let request_url = url(server, path);
    let response = client
        .post(&request_url)
        .json(body)
        .send()
        .context(RequestSend {
            method: "POST",
            url: request_url.clone(),
        })?;
    parse_response("POST", request_url, response)
}

fn parse_response<T: DeserializeOwned>(
    method: &'static str,
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

    serde_json::from_str(&body).context(JsonParse { url })
}

fn url(server: &str, path: &str) -> String {
    format!("{}{}", server.trim_end_matches('/'), path)
}
