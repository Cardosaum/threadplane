#![expect(
    clippy::redundant_pub_crate,
    reason = "HTTP helpers are crate-local adapters with explicit visibility."
)]

use bon::Builder;
use core::marker::PhantomData;
use reqwest::{
    blocking::{Client, RequestBuilder, Response},
    Method,
};
use serde::{de::DeserializeOwned, Serialize};
use snafu::{ensure, IntoError as _, ResultExt as _};
use url::Url;

use crate::build_info::current_build_info;
use crate::error::{
    ContractMismatchDetails, HttpClientBuild, JsonContractMismatch, JsonParse, NonSuccessStatus,
    RequestSend, ResponseBodyRead, Result, UrlJoin, UrlParse,
};
use threadplane_core::{compare_build_info, BuildComparison, ServiceSnapshot};

struct RequestTarget {
    base_url: Url,
}

impl RequestTarget {
    fn new(server: &str) -> Result<Self> {
        let mut base_url = Url::parse(server).context(UrlParse {
            server: server.to_owned(),
        })?;
        if !base_url.path().ends_with('/') {
            base_url.set_path(&format!("{}/", base_url.path()));
        }
        Ok(Self { base_url })
    }

    fn request_url(&self, path: &str) -> Result<Url> {
        self.base_url
            .join(path.trim_start_matches('/'))
            .context(UrlJoin {
                base_url: self.base_url.to_string(),
                path: path.to_owned(),
            })
    }

    fn root_url(&self) -> Url {
        self.base_url.clone()
    }
}

#[derive(Builder)]
struct RequestMetadata<'request> {
    idempotency_key: Option<&'request str>,
    method: Method,
}

struct JsonRequest<'request, Body, ResponseType> {
    body: Option<&'request Body>,
    metadata: RequestMetadata<'request>,
    path: &'request str,
    response: PhantomData<fn() -> ResponseType>,
}

impl<'request, ResponseType> JsonRequest<'request, (), ResponseType> {
    fn get(path: &'request str) -> Self {
        Self {
            body: None,
            metadata: RequestMetadata::builder().method(Method::GET).build(),
            path,
            response: PhantomData,
        }
    }
}

impl<'request, Body, ResponseType> JsonRequest<'request, Body, ResponseType> {
    fn patch(
        path: &'request str,
        body: &'request Body,
        idempotency_key: Option<&'request str>,
    ) -> Self {
        Self::with_body(Method::PATCH, path, body, idempotency_key)
    }

    fn post(
        path: &'request str,
        body: &'request Body,
        idempotency_key: Option<&'request str>,
    ) -> Self {
        Self::with_body(Method::POST, path, body, idempotency_key)
    }

    fn put(
        path: &'request str,
        body: &'request Body,
        idempotency_key: Option<&'request str>,
    ) -> Self {
        Self::with_body(Method::PUT, path, body, idempotency_key)
    }

    fn with_body(
        method: Method,
        path: &'request str,
        body: &'request Body,
        idempotency_key: Option<&'request str>,
    ) -> Self {
        let metadata_builder = RequestMetadata::builder().method(method);
        let metadata = match idempotency_key {
            Some(command_idempotency_key) => metadata_builder
                .idempotency_key(command_idempotency_key)
                .build(),
            None => metadata_builder.build(),
        };

        Self {
            body: Some(body),
            metadata,
            path,
            response: PhantomData,
        }
    }
}

struct ServerTransport<'client> {
    client: &'client Client,
    target: RequestTarget,
}

impl<'client> ServerTransport<'client> {
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

    fn get_json<T>(&self, path: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        JsonRequest::<(), T>::get(path).send(self)
    }

    fn new(client: &'client Client, server: &str) -> Result<Self> {
        Ok(Self {
            client,
            target: RequestTarget::new(server)?,
        })
    }

    fn parse_json_response<T>(&self, request: &JsonRequest<'_, impl Serialize, T>, response: Response) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let status = response.status();
        let url = self.target.request_url(request.path)?.to_string();
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

        let mut deserializer = serde_json::Deserializer::from_str(&body);
        serde_path_to_error::deserialize(&mut deserializer).map_err(|error| {
            let json_path = match error.path().to_string() {
                path if path.is_empty() => "<root>".to_owned(),
                path => path,
            };
            let source = error.into_inner();

            if let Some(details) = self.contract_mismatch_details() {
                return JsonContractMismatch {
                    details: Box::new(details),
                    json_path,
                    url,
                }
                .into_error(source);
            }

            JsonParse { json_path, url }.into_error(source)
        })
    }

    fn request_builder<Body, ResponseType>(
        &self,
        request: &JsonRequest<'_, Body, ResponseType>,
    ) -> Result<RequestBuilder>
    where
        Body: Serialize,
    {
        let mut request_builder = self
            .client
            .request(
                request.metadata.method.clone(),
                self.target.request_url(request.path)?,
            );

        if let Some(body) = request.body {
            request_builder = request_builder.json(body);
        }
        if let Some(idempotency_key) = request.metadata.idempotency_key {
            request_builder = request_builder.header("Idempotency-Key", idempotency_key);
        }

        Ok(request_builder)
    }

    fn send<Body, ResponseType>(&self, request: &JsonRequest<'_, Body, ResponseType>) -> Result<ResponseType>
    where
        Body: Serialize,
        ResponseType: DeserializeOwned,
    {
        let url = self.target.request_url(request.path)?.to_string();
        let response = self.request_builder(request)?.send().context(RequestSend {
            method: request.metadata.method.to_string(),
            url,
        })?;
        self.parse_json_response(request, response)
    }
}

impl<Body, ResponseType> JsonRequest<'_, Body, ResponseType>
where
    Body: Serialize,
    ResponseType: DeserializeOwned,
{
    fn send(&self, transport: &ServerTransport<'_>) -> Result<ResponseType> {
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
    ServerTransport::new(client, server)?.get_json(path)
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
    JsonRequest::<B, T>::patch(path, body, idempotency_key).send(&ServerTransport::new(client, server)?)
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
    JsonRequest::<B, T>::post(path, body, idempotency_key).send(&ServerTransport::new(client, server)?)
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
    JsonRequest::<B, T>::put(path, body, idempotency_key).send(&ServerTransport::new(client, server)?)
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
            JsonRequest::<_, serde_json::Value>::post("/v1/tasks", &body, None);
        let target = RequestTarget::new("http://127.0.0.1:4000/")
            .unwrap_or_else(|error| panic!("target should build: {error}"));

        assert_eq!(
            target
                .request_url(request.path)
                .unwrap_or_else(|error| panic!("url should build: {error}"))
                .as_str(),
            "http://127.0.0.1:4000/v1/tasks"
        );
    }

    #[test]
    fn request_builder_preserves_method_and_idempotency_metadata() {
        let body = serde_json::json!({"title":"test"});
        let request =
            JsonRequest::<_, serde_json::Value>::patch("/v1/tasks/123", &body, Some("command-1"));
        let client = Client::builder()
            .build()
            .unwrap_or_else(|error| panic!("client should build: {error}"));
        let transport = ServerTransport::new(&client, "http://127.0.0.1:4000")
            .unwrap_or_else(|error| panic!("transport should build: {error}"));
        let built = transport
            .request_builder(&request)
            .unwrap_or_else(|error| panic!("request builder should build: {error}"))
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
        let metadata = RequestMetadata::builder()
            .idempotency_key("abc")
            .method(Method::POST)
            .build();

        assert_eq!(metadata.idempotency_key, Some("abc"));
    }

    #[test]
    fn request_target_trims_the_server_suffix_only() {
        let target = RequestTarget::new("http://127.0.0.1:4000/")
            .unwrap_or_else(|error| panic!("target should build: {error}"));

        assert_eq!(
            target
                .request_url("/v1/tasks")
                .unwrap_or_else(|error| panic!("url should build: {error}"))
                .as_str(),
            "http://127.0.0.1:4000/v1/tasks"
        );
        assert_eq!(target.root_url().as_str(), "http://127.0.0.1:4000/");
    }
}
