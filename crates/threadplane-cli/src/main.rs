pub(crate) mod app;
pub(crate) mod command;
pub(crate) mod error;
pub(crate) mod http;

fn main() -> error::Result<()> {
    app::run()
}
