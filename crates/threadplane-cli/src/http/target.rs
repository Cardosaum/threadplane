use snafu::ResultExt as _;
use url::Url;

use crate::error::{Result, UrlJoin, UrlParse};

pub(super) struct RequestTarget {
    base_url: Url,
}

impl RequestTarget {
    pub(super) fn new(server: &str) -> Result<Self> {
        let mut base_url = Url::parse(server).context(UrlParse {
            server: server.to_owned(),
        })?;
        if !base_url.path().ends_with('/') {
            base_url.set_path(&format!("{}/", base_url.path()));
        }
        Ok(Self { base_url })
    }

    pub(super) fn request_url(&self, path: &str) -> Result<Url> {
        self.base_url
            .join(path.trim_start_matches('/'))
            .context(UrlJoin {
                base_url: self.base_url.to_string(),
                path: path.to_owned(),
            })
    }

    pub(super) fn root_url(&self) -> Url {
        self.base_url.clone()
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::panic,
        reason = "Test failures should print the underlying builder error clearly."
    )]

    use proptest::{option, prelude::*};
    use rstest::rstest;

    use super::RequestTarget;

    #[rstest]
    #[case(
        "http://127.0.0.1:4000",
        "http://127.0.0.1:4000/",
        "http://127.0.0.1:4000/v1/tasks"
    )]
    #[case(
        "http://127.0.0.1:4000/",
        "http://127.0.0.1:4000/",
        "http://127.0.0.1:4000/v1/tasks"
    )]
    #[case(
        "http://127.0.0.1:4000/api",
        "http://127.0.0.1:4000/api/",
        "http://127.0.0.1:4000/api/v1/tasks"
    )]
    fn request_target_normalizes_base_urls(
        #[case] server: &str,
        #[case] expected_root: &str,
        #[case] expected_request: &str,
    ) {
        let target = RequestTarget::new(server)
            .unwrap_or_else(|error| panic!("target should build: {error}"));

        assert_eq!(
            target
                .request_url("/v1/tasks")
                .unwrap_or_else(|error| panic!("url should build: {error}"))
                .as_str(),
            expected_request
        );
        assert_eq!(target.root_url().as_str(), expected_root);
    }

    proptest! {
        #[test]
        fn request_target_joins_paths_without_double_slashes(
            first in "[a-z0-9]{1,8}",
            second in option::of("[a-z0-9]{1,8}"),
        ) {
            let target = RequestTarget::new("http://127.0.0.1:4000/")
                .unwrap_or_else(|error| panic!("target should build: {error}"));
            let path = second.map_or_else(
                || format!("/{first}"),
                |second_segment| format!("/{first}/{second_segment}"),
            );
            let joined = target
                .request_url(&path)
                .unwrap_or_else(|error| panic!("url should build: {error}"));

            prop_assert!(joined.as_str().starts_with("http://127.0.0.1:4000/"));
            prop_assert_eq!(joined.path(), path);
            prop_assert!(!joined.as_str().contains("//v1"));
        }
    }
}
