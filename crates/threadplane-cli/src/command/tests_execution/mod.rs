#![expect(
    clippy::panic,
    reason = "Test-only fakes use explicit panic messages for fixture setup failures."
)]

mod cases;
mod support;
