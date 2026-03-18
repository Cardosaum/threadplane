#![expect(
    clippy::redundant_pub_crate,
    reason = "Read handlers are grouped by query surface rather than alphabetically."
)]

mod content;
mod entities;
mod events;
mod system;
mod tasks;
mod workspace;

pub(crate) use content::{
    list_epics, list_memories, list_notes, prime_memories, show_epic, show_memory, show_note,
};
pub(crate) use entities::{related_entities, show_entity};
pub(crate) use events::{list_events, tail_events};
pub(crate) use system::{healthz, projection_status, root, scope};
pub(crate) use tasks::{list_open_tasks, list_tasks, next_task, show_task, task_context, task_dag};
pub(crate) use workspace::{
    list_workspace_memberships, list_workspace_public_keys, show_workspace_policy,
};
