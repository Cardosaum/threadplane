extern crate alloc;

mod app;
mod command;
mod error;
mod report;
mod scenario;
#[cfg(test)]
mod tests;

fn main() -> error::Result<()> {
    app::run()
}
