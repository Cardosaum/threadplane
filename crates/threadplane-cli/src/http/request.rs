use bon::Builder;
use core::marker::PhantomData;
use reqwest::Method;

#[derive(Builder)]
pub(super) struct RequestMetadata<'request> {
    pub(super) idempotency_key: Option<&'request str>,
    pub(super) method: Method,
}

pub(super) struct JsonRequest<'request, Body, ResponseType> {
    pub(super) body: Option<&'request Body>,
    pub(super) metadata: RequestMetadata<'request>,
    pub(super) path: &'request str,
    response: PhantomData<fn() -> ResponseType>,
}

impl<'request, Body, ResponseType> JsonRequest<'request, Body, ResponseType> {
    pub(super) fn new(method: Method, path: &'request str) -> Self {
        Self {
            body: None,
            metadata: RequestMetadata::builder().method(method).build(),
            path,
            response: PhantomData,
        }
    }

    pub(super) const fn with_body(mut self, body: &'request Body) -> Self {
        self.body = Some(body);
        self
    }

    pub(super) fn with_idempotency_key(mut self, idempotency_key: Option<&'request str>) -> Self {
        self.metadata = RequestMetadata::builder()
            .method(self.metadata.method)
            .maybe_idempotency_key(idempotency_key)
            .build();
        self
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::panic,
        reason = "Test failures should print the underlying builder error clearly."
    )]

    use proptest::prelude::*;
    use reqwest::{blocking::Client, header::HeaderValue, Method};
    use rstest::rstest;

    use super::{JsonRequest, RequestMetadata};
    use crate::http::{target::RequestTarget, transport::ServerTransport};

    macro_rules! patch_request {
        ($body:expr) => {{
            JsonRequest::<_, serde_json::Value>::new(Method::PATCH, "/v1/tasks/123")
                .with_body(&$body)
                .with_idempotency_key(Some("command-1"))
        }};
    }

    #[rstest]
    #[case(Method::PATCH, Some("command-1"))]
    #[case(Method::POST, None)]
    #[case(Method::PUT, Some("command-2"))]
    fn json_request_keeps_optional_body_and_metadata(
        #[case] method: Method,
        #[case] idempotency_key: Option<&str>,
    ) {
        let body = serde_json::json!({"title":"test"});
        let request = JsonRequest::<_, serde_json::Value>::new(method.clone(), "/v1/tasks/123")
            .with_body(&body)
            .with_idempotency_key(idempotency_key);

        assert!(request.body.is_some());
        assert_eq!(request.metadata.idempotency_key, idempotency_key);
        assert_eq!(request.metadata.method, method);
    }

    #[test]
    fn request_builder_preserves_method_and_idempotency_metadata() {
        let body = serde_json::json!({"title":"test"});
        let request = patch_request!(body);
        let client = Client::builder()
            .build()
            .unwrap_or_else(|error| panic!("client should build: {error}"));
        let transport = ServerTransport::new(&client, "http://127.0.0.1:4000")
            .unwrap_or_else(|error| panic!("transport should build: {error}"));
        let target = RequestTarget::new("http://127.0.0.1:4000")
            .unwrap_or_else(|error| panic!("target should build: {error}"));
        let url = target
            .request_url(request.path)
            .unwrap_or_else(|error| panic!("url should build: {error}"));
        let built = transport
            .request_builder(&request, url)
            .build()
            .unwrap_or_else(|error| panic!("request should build: {error}"));

        assert_eq!(built.method(), Method::PATCH);
        assert_eq!(
            built.headers().get("Idempotency-Key"),
            Some(&HeaderValue::from_static("command-1"))
        );
    }

    #[rstest]
    #[case(Some("abc"))]
    #[case(None)]
    fn request_metadata_keeps_optional_idempotency_key(#[case] idempotency_key: Option<&str>) {
        let metadata = RequestMetadata::builder()
            .method(Method::POST)
            .maybe_idempotency_key(idempotency_key)
            .build();

        assert_eq!(metadata.idempotency_key, idempotency_key);
    }

    #[test]
    fn request_uses_paths_resolved_by_targets() {
        let request = JsonRequest::<(), serde_json::Value>::new(Method::GET, "/v1/tasks");
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

    proptest! {
        #[test]
        fn request_preserves_user_supplied_path(
            segment in "[a-z0-9]{1,8}",
            suffix in proptest::option::of("[a-z0-9]{1,8}"),
        ) {
            let path = match suffix {
                Some(suffix) => format!("/{segment}/{suffix}"),
                None => format!("/{segment}"),
            };
            let request = JsonRequest::<(), serde_json::Value>::new(Method::GET, &path);

            prop_assert_eq!(request.path, path.as_str());
            prop_assert_eq!(request.metadata.method, Method::GET);
            prop_assert!(request.body.is_none());
        }
    }
}
