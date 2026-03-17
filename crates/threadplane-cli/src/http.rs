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
use core::marker::PhantomData;

use crate::build_info::current_build_info;
use crate::error::{
    ContractMismatchDetails, HttpClientBuild, JsonContractMismatch, JsonParse, NonSuccessStatus,
    RequestSend, ResponseBodyRead, Result,
};
use threadplane_core::{compare_build_info, BuildComparison, ServiceSnapshot};

struct RequestTarget<'request> {
    path: &'request str,
    server: &'request str,
}

impl<'request> RequestTarget<'request> {
    const fn new(server: &'request str, path: &'request str) -> Self {
        Self { path, server }
    }

    fn request_url(&self) -> String {
        format!("{}{}", self.server.trim_end_matches('/'), self.path)
    }

    fn root_url(&self) -> String {
        format!("{}/", self.server.trim_end_matches('/'))
    }
}

struct RequestMetadata<'request> {
    idempotency_key: Option<&'request str>,
    method: Method,
}

impl<'request> RequestMetadata<'request> {
    const fn new(method: Method) -> Self {
        Self {
            idempotency_key: None,
            method,
        }
    }

    const fn with_idempotency_key(mut self, idempotency_key: Option<&'request str>) -> Self {
        self.idempotency_key = idempotency_key;
        self
    }
}

struct JsonRequest<'request, Body, ResponseType> {
    body: Option<&'request Body>,
    metadata: RequestMetadata<'request>,
    response: PhantomData<fn() -> ResponseType>,
    target: RequestTarget<'request>,
}

impl<'request, ResponseType> JsonRequest<'request, (), ResponseType> {
    const fn get(server: &'request str, path: &'request str) -> Self {
        Self {
            body: None,
            metadata: RequestMetadata::new(Method::GET),
            response: PhantomData,
            target: RequestTarget::new(server, path),
        }
    }
}

impl<'request, Body, ResponseType> JsonRequest<'request, Body, ResponseType> {
    const fn patch(server: &'request str, path: &'request str, body: &'request Body) -> Self {
        Self::with_body(Method::PATCH, server, path, body)
    }

    const fn post(server: &'request str, path: &'request str, body: &'request Body) -> Self {
        Self::with_body(Method::POST, server, path, body)
    }

    const fn put(server: &'request str, path: &'request str, body: &'request Body) -> Self {
        Self::with_body(Method::PUT, server, path, body)
    }

    const fn with_body(
        method: Method,
        server: &'request str,
        path: &'request str,
        body: &'request Body,
    ) -> Self {
        Self {
            body: Some(body),
            metadata: RequestMetadata::new(method),
            response: PhantomData,
            target: RequestTarget::new(server, path),
        }
    }

    fn with_idempotency_key(mut self, idempotency_key: Option<&'request str>) -> Self {
        self.metadata = self.metadata.with_idempotency_key(idempotency_key);
        self
    }
}

struct ServerTransport<'client, 'server> {
    client: &'client Client,
    target: RequestTarget<'server>,
}

impl<'client, 'server> ServerTransport<'client, 'server> {
    fn contract_mismatch_details(&self) -> Option<ContractMismatchDetails> {
        let comparison = self.fetch_build_comparison()?;
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

    fn fetch_build_comparison(&self) -> Option<BuildComparison> {
        let response = self.client.get(self.target.root_url()).send().ok()?;
        if !response.status().is_success() {
            return None;
        }

        let body = response.text().ok()?;
        let snapshot = serde_json::from_str::<ServiceSnapshot>(&body).ok()?;

        Some(compare_build_info(&current_build_info(), &snapshot.build))
    }

    fn get_json<T>(&self, path: &'server str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        JsonRequest::<(), T>::get(self.target.server, path).send(self)
    }

    const fn new(client: &'client Client, server: &'server str) -> Self {
        Self {
            client,
            target: RequestTarget::new(server, ""),
        }
    }

    fn parse_json_response<T>(&self, request: &JsonRequest<'_, impl Serialize, T>, response: Response) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let status = response.status();
        let url = request.target.request_url();
        let body = response
            .text()
            .context(ResponseBodyRead { url: url.clone() })?;
        ensure!(
            status.is_success(),
            NonSuccessStatus {
                method: request.metadata.method.to_string(),
                url,
                status,
                body,
            }
        );

        serde_json::from_str(&body).map_err(|source| {
            if let Some(details) = self.contract_mismatch_details() {
                return JsonContractMismatch {
                    details: Box::new(details),
                    url,
                }
                .into_error(source);
            }

            JsonParse { url }.into_error(source)
        })
    }

    fn request_builder<Body>(
        &self,
        request: &JsonRequest<'_, Body, impl DeserializeOwned>,
    ) -> RequestBuilder
    where
        Body: Serialize,
    {
        let mut request_builder = self
            .client
            .request(request.metadata.method.clone(), request.target.request_url());

        if let Some(body) = request.body {
            request_builder = request_builder.json(body);
        }
        if let Some(idempotency_key) = request.metadata.idempotency_key {
            request_builder = request_builder.header("Idempotency-Key", idempotency_key);
        }

        request_builder
    }

    fn send<Body, ResponseType>(&self, request: &JsonRequest<'_, Body, ResponseType>) -> Result<ResponseType>
    where
        Body: Serialize,
        ResponseType: DeserializeOwned,
    {
        let response = self
            .request_builder(request)
            .send()
            .context(RequestSend {
                method: request.metadata.method.to_string(),
                url: request.target.request_url(),
            })?;
        self.parse_json_response(request, response)
    }
}

impl<Body, ResponseType> JsonRequest<'_, Body, ResponseType>
where
    Body: Serialize,
    ResponseType: DeserializeOwned,
{
    fn send(&self, transport: &ServerTransport<'_, '_>) -> Result<ResponseType> {
        transport.send(self)
    }
}

pub(crate) fn build_http_client() -> Result<Client> {
    Client::builder().build().context(HttpClientBuild)
}

pub(crate) fn get_json<T>(client: &Client, server: &str, path: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    ServerTransport::new(client, server).get_json(path)
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
    JsonRequest::<B, T>::patch(server, path, body)
        .with_idempotency_key(idempotency_key)
        .send(&ServerTransport::new(client, server))
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
    JsonRequest::<B, T>::post(server, path, body)
        .with_idempotency_key(idempotency_key)
        .send(&ServerTransport::new(client, server))
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
    JsonRequest::<B, T>::put(server, path, body)
        .with_idempotency_key(idempotency_key)
        .send(&ServerTransport::new(client, server))
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::panic,
        reason = "Test failures should print the underlying builder error clearly."
    )]

    use super::{JsonRequest, RequestMetadata, RequestTarget, ServerTransport};
    use reqwest::{blocking::Client, header::HeaderValue, Method};

    #[test]
    fn json_request_builds_urls_without_double_slashes() {
        let body = serde_json::json!({});
        let request =
            JsonRequest::<_, serde_json::Value>::post("http://127.0.0.1:4000/", "/v1/tasks", &body);

        assert_eq!(request.target.request_url(), "http://127.0.0.1:4000/v1/tasks");
    }

    #[test]
    fn request_builder_preserves_method_and_idempotency_metadata() {
        let body = serde_json::json!({"title":"test"});
        let request = JsonRequest::<_, serde_json::Value>::patch(
            "http://127.0.0.1:4000",
            "/v1/tasks/123",
            &body,
        )
        .with_idempotency_key(Some("command-1"));
        let client = Client::builder()
            .build()
            .unwrap_or_else(|error| panic!("client should build: {error}"));
        let transport = ServerTransport::new(&client, "http://127.0.0.1:4000");
        let built = transport
            .request_builder(&request)
            .build()
            .unwrap_or_else(|error| panic!("request should build: {error}"));

        assert_eq!(built.method(), Method::PATCH);
        assert_eq!(
            built.headers().get("Idempotency-Key"),
            Some(&HeaderValue::from_static("command-1"))
        );
    }

    #[test]
    fn request_metadata_keeps_optional_idempotency_key() {
        let metadata = RequestMetadata::new(Method::POST).with_idempotency_key(Some("abc"));

        assert_eq!(metadata.idempotency_key, Some("abc"));
    }

    #[test]
    fn request_target_trims_the_server_suffix_only() {
        let target = RequestTarget::new("http://127.0.0.1:4000/", "/v1/tasks");

        assert_eq!(target.request_url(), "http://127.0.0.1:4000/v1/tasks");
        assert_eq!(target.root_url(), "http://127.0.0.1:4000/");
    }
}
