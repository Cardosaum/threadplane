#![expect(
    clippy::arbitrary_source_item_ordering,
    clippy::redundant_pub_crate,
    reason = "CLI runtime ports are crate-internal boundaries with explicit visibility and grouped method order."
)]

use core::time::Duration;
use std::thread;

use bon::Builder;
use serde::{de::DeserializeOwned, Serialize};
use snafu::ResultExt as _;

use crate::error::{JsonRender, Result};

pub(crate) trait ApiClient {
    fn get_json<T>(&self, path: &str) -> Result<T>
    where
        T: DeserializeOwned;

    fn patch_json<B, T>(&self, path: &str, body: &B, idempotency_key: Option<&str>) -> Result<T>
    where
        B: Serialize,
        T: DeserializeOwned;

    fn post_json<B, T>(&self, path: &str, body: &B, idempotency_key: Option<&str>) -> Result<T>
    where
        B: Serialize,
        T: DeserializeOwned;

    fn put_json<B, T>(&self, path: &str, body: &B, idempotency_key: Option<&str>) -> Result<T>
    where
        B: Serialize,
        T: DeserializeOwned;
}

pub(crate) trait CommandOutput {
    fn print(&mut self, text: &str);
    fn print_warning(&mut self, text: &str);
}

pub(crate) trait Sleeper {
    fn sleep(&self, duration: Duration);
}

#[derive(Builder)]
pub(crate) struct CommandContext<'context, A, O, S> {
    api: &'context A,
    output: &'context mut O,
    sleeper: &'context S,
}

impl<A, O, S> CommandContext<'_, A, O, S>
where
    A: ApiClient,
    O: CommandOutput,
    S: Sleeper,
{
    pub(crate) fn get_json<T>(&self, path: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.api.get_json(path)
    }

    pub(crate) fn patch_json<B, T>(
        &self,
        path: &str,
        body: &B,
        idempotency_key: Option<&str>,
    ) -> Result<T>
    where
        B: Serialize,
        T: DeserializeOwned,
    {
        self.api.patch_json(path, body, idempotency_key)
    }

    pub(crate) fn post_json<B, T>(
        &self,
        path: &str,
        body: &B,
        idempotency_key: Option<&str>,
    ) -> Result<T>
    where
        B: Serialize,
        T: DeserializeOwned,
    {
        self.api.post_json(path, body, idempotency_key)
    }

    pub(crate) fn put_json<B, T>(
        &self,
        path: &str,
        body: &B,
        idempotency_key: Option<&str>,
    ) -> Result<T>
    where
        B: Serialize,
        T: DeserializeOwned,
    {
        self.api.put_json(path, body, idempotency_key)
    }

    pub(crate) fn print_compact(&mut self, text: &str) {
        self.output.print(text);
    }

    pub(crate) fn print_value<T>(&mut self, value: &T) -> Result<()>
    where
        T: Serialize,
    {
        let rendered = serde_json::to_string_pretty(value).context(JsonRender)?;
        self.output.print(&format!("{rendered}\n"));
        Ok(())
    }

    pub(crate) fn warn(&mut self, text: &str) {
        self.output.print_warning(text);
    }

    pub(crate) fn sleep(&self, duration: Duration) {
        self.sleeper.sleep(duration);
    }
}

pub(crate) struct StdCommandOutput;

impl CommandOutput for StdCommandOutput {
    fn print(&mut self, text: &str) {
        print!("{text}");
    }

    fn print_warning(&mut self, text: &str) {
        eprintln!("{text}");
    }
}

pub(crate) struct ThreadSleeper;

impl Sleeper for ThreadSleeper {
    fn sleep(&self, duration: Duration) {
        thread::sleep(duration);
    }
}
