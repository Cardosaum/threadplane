extern crate alloc;

pub(crate) mod app;
pub(crate) mod build_info;
pub(crate) mod error;
pub(crate) mod handlers;
pub(crate) mod idempotency;
pub(crate) mod lifecycle;
pub(crate) mod migration;
pub(crate) mod prelude;
pub(crate) mod projections;
pub(crate) mod replay;
pub(crate) mod storage;

#[cfg(test)]
mod tests;

#[tokio::main]
async fn main() -> error::ServerResult<()> {
    Box::pin(app::run()).await
}
