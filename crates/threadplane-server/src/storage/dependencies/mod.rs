#![expect(
    clippy::redundant_pub_crate,
    reason = "Dependency persistence is shared only inside this crate."
)]
#![allow(
    clippy::wildcard_imports,
    reason = "The dependency submodule intentionally builds on the storage prelude."
)]

mod reads;
mod writes;

use super::*;

pub(crate) use reads::{
    fetch_dependency_chain, fetch_dependent_chain, fetch_direct_dependencies,
    fetch_direct_dependents, task_is_ready,
};
pub(crate) use writes::{append_task_dependency, fetch_task_dependency_by_event_id};
