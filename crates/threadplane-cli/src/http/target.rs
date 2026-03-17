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

    use super::RequestTarget;

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
