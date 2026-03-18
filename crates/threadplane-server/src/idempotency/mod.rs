#![expect(
    clippy::redundant_pub_crate,
    reason = "Idempotency helpers are crate-local orchestration around command receipts."
)]

mod flow;
mod types;

pub(crate) use flow::{begin_idempotent_command, complete_idempotent_command};
pub(crate) use types::{CommandExecution, IdempotencyContext, IDEMPOTENCY_KEY_HEADER};
