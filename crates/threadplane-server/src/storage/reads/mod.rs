#![expect(
    clippy::redundant_pub_crate,
    reason = "Read persistence is shared only inside this crate."
)]
#![allow(
    clippy::wildcard_imports,
    reason = "The reads submodule intentionally builds on the storage prelude."
)]

mod claims;
mod entities;
mod epics;
mod tasks;
mod text;

use super::*;

pub(crate) use claims::{fetch_active_claim, fetch_active_claim_tx, fetch_claim_by_event_id};
pub(crate) use entities::fetch_entity_record;
pub(crate) use epics::{
    fetch_epic_by_event_id, fetch_epic_by_id, fetch_epic_by_id_tx, fetch_epic_for_task,
    fetch_epic_rows_for_workspace,
};
pub(crate) use tasks::{fetch_task_by_event_id, fetch_task_by_id, fetch_task_by_id_tx};
pub(crate) use text::{
    fetch_link_by_event_id, fetch_memory_by_event_id, fetch_memory_by_id, fetch_memory_by_id_tx,
    fetch_note_by_event_id, fetch_note_by_id, fetch_note_by_id_tx,
};
