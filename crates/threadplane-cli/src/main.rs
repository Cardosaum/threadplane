extern crate alloc;

pub(crate) mod app;
pub(crate) mod build_info;
pub(crate) mod command;
pub(crate) mod error;
pub(crate) mod http;

#[cfg(test)]
mod tests;

use std::process::ExitCode;

fn main() -> ExitCode {
    match app::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
