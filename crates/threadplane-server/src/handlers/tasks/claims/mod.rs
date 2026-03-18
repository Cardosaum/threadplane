#![allow(
    clippy::wildcard_imports,
    reason = "The task claim submodule intentionally builds on the task handler prelude."
)]

use super::*;

mod acquire;
mod lifecycle;
mod shared;

pub(crate) use acquire::{claim_next_task, claim_task};
pub(crate) use lifecycle::{complete_task, release_task};
