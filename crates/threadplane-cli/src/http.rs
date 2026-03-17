#![expect(
    clippy::redundant_pub_crate,
    reason = "HTTP helpers are crate-local adapters with explicit visibility."
)]

use reqwest::{
    blocking::{Client, RequestBuilder, Response},
    Method,
};
use serde::{de::DeserializeOwned, Serialize};
use snafu::{ensure, IntoError as _, ResultExt as _};

use crate::build_info::current_build_info;
use crate::error::{
    ContractMismatchDetails, HttpClientBuild, JsonContractMismatch, JsonParse, NonSuccessStatus,
    RequestSend, ResponseBodyRead, Result,
};
use threadplane_core::{compare_build_info, BuildComparison, ServiceSnapshot};

struct ApiRequest<'request, Body> {
    body: Option<&'request Body>,
    idempotency_key: Option<&'request str>,
    method: Method,
    path: &'request str,
    server: &'request str,
}

impl<'request> ApiRequest<'request, ()> {
    const fn get(server: &'request str, path: &'request str) -> Self {
        Self {
            body: None,
            idempotency_key: None,
            method: Method::GET,
            path,
            server,
        }
    }
}

impl<'request, Body> ApiRequest<'request, Body> {
    const fn patch(server: &'request str, path: &'request str, body: &'request Body) -> Self {
        Self::with_body(Method::PATCH, server, path, body)
    }

    const fn post(server: &'request str, path: &'request str, body: &'request Body) -> Self {
        Self::with_body(Method::POST, server, path, body)
    }

    const fn put(server: &'request str, path: &'request str, body: &'request Body) -> Self {
        Self::with_body(Method::PUT, server, path, body)
    }

    fn request_builder(&self, client: &Client) -> RequestBuilder
    where
        Body: Serialize,
    {
        let request_url = self.request_url();
        let mut request_builder = client.request(self.method.clone(), request_url);

        if let Some(body) = self.body {
            request_builder = request_builder.json(body);
        }
        if let Some(idempotency_key) = self.idempotency_key {
            request_builder = request_builder.header("Idempotency-Key", idempotency_key);
        }

        request_builder
    }

    fn request_url(&self) -> String {
        url(self.server, self.path)
    }

    const fn with_body(
        method: Method,
        server: &'request str,
        path: &'request str,
        body: &'request Body,
    ) -> Self {
        Self {
            body: Some(body),
            idempotency_key: None,
            method,
            path,
            server,
        }
    }

    const fn with_idempotency_key(mut self, idempotency_key: Option<&'request str>) -> Self {
        self.idempotency_key = idempotency_key;
        self
    }
}

pub(crate) fn build_http_client() -> Result<Client> {
    Client::builder().build().context(HttpClientBuild)
}

pub(crate) fn get_json<T>(client: &Client, server: &str, path: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    let request = ApiRequest::<()>::get(server, path);
    send_json(client, &request)
}

pub(crate) fn patch_json<B, T>(
    client: &Client,
    server: &str,
    path: &str,
    body: &B,
    idempotency_key: Option<&str>,
) -> Result<T>
where
    B: Serialize,
    T: DeserializeOwned,
{
    let request = ApiRequest::patch(server, path, body).with_idempotency_key(idempotency_key);
    send_json(client, &request)
}

pub(crate) fn post_json<B, T>(
    client: &Client,
    server: &str,
    path: &str,
    body: &B,
    idempotency_key: Option<&str>,
) -> Result<T>
where
    B: Serialize,
    T: DeserializeOwned,
{
    let request = ApiRequest::post(server, path, body).with_idempotency_key(idempotency_key);
    send_json(client, &request)
}

pub(crate) fn put_json<B, T>(
    client: &Client,
    server: &str,
    path: &str,
    body: &B,
    idempotency_key: Option<&str>,
) -> Result<T>
where
    B: Serialize,
    T: DeserializeOwned,
{
    let request = ApiRequest::put(server, path, body).with_idempotency_key(idempotency_key);
    send_json(client, &request)
}

fn send_json<T, Body>(client: &Client, request: &ApiRequest<'_, Body>) -> Result<T>
where
    Body: Serialize,
    T: DeserializeOwned,
{
    let response = send_request(client, request)?;
    parse_json_response(
        client,
        request.server,
        &request.method,
        request.request_url(),
        response,
    )
}

fn send_request<Body>(client: &Client, request: &ApiRequest<'_, Body>) -> Result<Response>
where
    Body: Serialize,
{
    let request_url = request.request_url();
    request.request_builder(client).send().context(RequestSend {
        method: request.method.to_string(),
        url: request_url,
    })
}

fn parse_json_response<T>(
    client: &Client,
    server: &str,
    method: &Method,
    url: String,
    response: Response,
) -> Result<T>
where
    T: DeserializeOwned,
{
    let status = response.status();
    let body = response
        .text()
        .context(ResponseBodyRead { url: url.clone() })?;
    ensure!(
        status.is_success(),
        NonSuccessStatus {
            method: method.to_string(),
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

#[cfg(test)]
mod tests {
    use super::{url, ApiRequest};

    #[test]
    fn api_request_builds_urls_without_double_slashes() {
        let body = serde_json::json!({});
        let request = ApiRequest::post("http://127.0.0.1:4000/", "/v1/tasks", &body);
        assert_eq!(request.request_url(), "http://127.0.0.1:4000/v1/tasks");
    }

    #[test]
    fn url_trims_the_server_suffix_only() {
        assert_eq!(
            url("http://127.0.0.1:4000/", "/v1/tasks"),
            "http://127.0.0.1:4000/v1/tasks"
        );
    }
}
