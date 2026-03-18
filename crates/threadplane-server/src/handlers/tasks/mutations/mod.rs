#![allow(
    clippy::wildcard_imports,
    reason = "The task mutation submodule intentionally builds on the task handler prelude."
)]

use super::*;

mod dependencies;
mod offer;
mod shared;
mod update;

pub(crate) use dependencies::add_task_dependency;
pub(crate) use offer::offer_task;
pub(crate) use shared::{ensure_supported_task_priority, project_task_record};
pub(crate) use update::update_task;
