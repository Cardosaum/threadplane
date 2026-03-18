#![expect(
    clippy::redundant_pub_crate,
    reason = "Task handlers are crate-local endpoints with explicit visibility."
)]
#![allow(
    clippy::wildcard_imports,
    reason = "The task handler submodule intentionally builds on the handler prelude."
)]

mod claims;
mod mutations;

use super::*;

pub(crate) use claims::{claim_next_task, claim_task, complete_task, release_task};
pub(crate) use mutations::{add_task_dependency, offer_task, update_task};

pub(crate) fn task_selection_filters(query: &TaskListQuery) -> TaskListFilters<'_> {
    TaskListFilters {
        epic_id: query.epic_id,
        label: query.label.as_deref(),
        owner: query.owner.as_deref(),
        priority: query.priority.clone(),
        ready_only: query.ready_only.unwrap_or(false),
        status: query.status.as_deref(),
    }
}

pub(crate) fn task_next_filters(query: &TaskListQuery) -> TaskListFilters<'_> {
    TaskListFilters {
        ready_only: true,
        status: Some("open"),
        ..task_selection_filters(query)
    }
}
