extern crate alloc;

pub(crate) mod app;
pub(crate) mod build_info;
pub(crate) mod command;
pub(crate) mod error;
pub(crate) mod http;

#[cfg(test)]
mod tests;

fn main() -> error::Result<()> {
    app::run()
}
