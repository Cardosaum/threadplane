use reqwest::{
    blocking::{Client, RequestBuilder, Response},
    StatusCode,
};
use serde::{de::DeserializeOwned, Serialize};
use snafu::{ensure, IntoError as _, ResultExt as _};

use crate::build_info::current_build_info;
use crate::error::{
    ContractMismatchDetails, JsonContractMismatch, JsonParse, NonSuccessStatus, RequestSend,
    ResponseBodyRead, Result,
};
use crate::http::{request::JsonRequest, target::RequestTarget};
use threadplane_core::{compare_build_info, BuildComparison, ServiceSnapshot};

pub(super) struct ServerTransport<'client> {
    client: &'client Client,
    target: RequestTarget,
}

impl<'client> ServerTransport<'client> {
    fn build_comparison(&self) -> Option<BuildComparison> {
        let response = self.client.get(self.target.root_url()).send().ok()?;
        if !response.status().is_success() {
            return None;
        }

        let body = response.text().ok()?;
        let snapshot = serde_json::from_str::<ServiceSnapshot>(&body).ok()?;

        Some(compare_build_info(&current_build_info(), &snapshot.build))
    }

    fn contract_mismatch_details(&self) -> Option<ContractMismatchDetails> {
        let comparison = self.build_comparison()?;

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

    pub(super) fn new(client: &'client Client, server: &str) -> Result<Self> {
        Ok(Self {
            client,
            target: RequestTarget::new(server)?,
        })
    }

    pub(super) fn request_builder<Body, ResponseType>(
        &self,
        request: &JsonRequest<'_, Body, ResponseType>,
        url: url::Url,
    ) -> RequestBuilder
    where
        Body: Serialize,
    {
        let mut request_builder = self.client.request(request.metadata.method.clone(), url);

        if let Some(body) = request.body {
            request_builder = request_builder.json(body);
        }
        if let Some(idempotency_key) = request.metadata.idempotency_key {
            request_builder = request_builder.header("Idempotency-Key", idempotency_key);
        }

        request_builder
    }

    pub(super) fn send<Body, ResponseType>(
        &self,
        request: &JsonRequest<'_, Body, ResponseType>,
    ) -> Result<ResponseType>
    where
        Body: Serialize,
        ResponseType: DeserializeOwned,
    {
        let url = self.target.request_url(request.path)?;
        let response = self
            .request_builder(request, url.clone())
            .send()
            .context(RequestSend {
                method: request.metadata.method.to_string(),
                url: url.to_string(),
            })?;

        parse_json_response(self, request, response, url.as_str())
    }
}

fn parse_json_response<Body, ResponseType>(
    transport: &ServerTransport<'_>,
    request: &JsonRequest<'_, Body, ResponseType>,
    response: Response,
    url: &str,
) -> Result<ResponseType>
where
    Body: Serialize,
    ResponseType: DeserializeOwned,
{
    let status = response.status();
    let body = response
        .text()
        .context(ResponseBodyRead { url: url.to_owned() })?;
    ensure_success(status, request.metadata.method.as_str(), url, &body)?;
    deserialize_response(transport, &body, url)
}

fn ensure_success(status: StatusCode, method: &str, url: &str, body: &str) -> Result<()> {
    ensure!(
        status.is_success(),
        NonSuccessStatus {
            body: body.to_owned(),
            method: method.to_owned(),
            status,
            url: url.to_owned(),
        }
    );

    Ok(())
}

fn deserialize_response<ResponseType>(
    transport: &ServerTransport<'_>,
    body: &str,
    url: &str,
) -> Result<ResponseType>
where
    ResponseType: DeserializeOwned,
{
    let mut deserializer = serde_json::Deserializer::from_str(body);
    serde_path_to_error::deserialize(&mut deserializer).map_err(|error| {
        let json_path = match error.path().to_string() {
            path if path.is_empty() => "<root>".to_owned(),
            path => path,
        };
        let source = error.into_inner();

        if let Some(details) = transport.contract_mismatch_details() {
            return JsonContractMismatch {
                details: Box::new(details),
                json_path,
                url: url.to_owned(),
            }
            .into_error(source);
        }

        JsonParse {
            json_path,
            url: url.to_owned(),
        }
        .into_error(source)
    })
}
