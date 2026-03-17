extern crate alloc;

pub(crate) mod app;
pub(crate) mod error;
pub(crate) mod handlers;
pub(crate) mod lifecycle;
pub(crate) mod projections;
pub(crate) mod storage;

#[cfg(test)]
mod tests;

#[tokio::main]
async fn main() -> error::ServerResult<()> {
    app::run().await
}
