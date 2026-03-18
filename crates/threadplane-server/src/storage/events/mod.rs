#![expect(
    clippy::redundant_pub_crate,
    reason = "Event log persistence is shared only inside this crate."
)]
#![allow(
    clippy::wildcard_imports,
    reason = "The event log submodule intentionally builds on the storage prelude."
)]

use super::*;

mod append;
mod cursor;
mod queries;
mod row;
mod status;

pub(crate) use append::append_event;
pub(crate) use cursor::{fetch_projection_cursor, record_projection_cursor, ProjectionCursor};
pub(crate) use queries::{
    fetch_event_row_for_workspace, fetch_event_rows_after_cursor,
    fetch_event_rows_after_workspace_cursor, fetch_event_rows_for_workspace,
};
#[cfg(test)]
pub(crate) use row::parse_event_kind;
pub(crate) use row::{event_kind_name, EventRow};
#[cfg(test)]
pub(crate) use status::build_projection_status;
pub(crate) use status::fetch_projection_status;
