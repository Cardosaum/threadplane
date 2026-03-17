#![expect(
    clippy::redundant_pub_crate,
    reason = "HTTP helpers are crate-local adapters with explicit visibility."
)]

mod request;
mod target;
mod transport;

use reqwest::{blocking::Client, Method};
use serde::{de::DeserializeOwned, Serialize};
use snafu::ResultExt as _;

use crate::error::{HttpClientBuild, Result};
use request::JsonRequest;
use transport::ServerTransport;

macro_rules! define_json_mutation {
    ($fn_name:ident, $method:expr) => {
        pub(crate) fn $fn_name<B, T>(
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
            send_json(
                client,
                server,
                &JsonRequest::<B, T>::new($method, path)
                    .with_body(body)
                    .with_idempotency_key(idempotency_key),
            )
        }
    };
}

pub(crate) fn build_http_client() -> Result<Client> {
    Client::builder().build().context(HttpClientBuild)
}

pub(crate) fn get_json<T>(client: &Client, server: &str, path: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    send_json(client, server, &JsonRequest::<(), T>::new(Method::GET, path))
}

define_json_mutation!(patch_json, Method::PATCH);
define_json_mutation!(post_json, Method::POST);
define_json_mutation!(put_json, Method::PUT);

fn send_json<Body, ResponseType>(
    client: &Client,
    server: &str,
    request: &JsonRequest<'_, Body, ResponseType>,
) -> Result<ResponseType>
where
    Body: Serialize,
    ResponseType: DeserializeOwned,
{
    ServerTransport::new(client, server)?.send(request)
}
