#![expect(
    clippy::redundant_pub_crate,
    reason = "Transclusion persistence is shared only inside this crate."
)]
#![allow(
    clippy::wildcard_imports,
    reason = "The transclusion submodule intentionally builds on the storage prelude."
)]

mod entities;
mod groups;
mod mutations;

use super::*;

pub(crate) use mutations::{
    prepare_xanadu_group, sync_transclusion_members, update_transclusion_group,
};

pub(crate) struct XanaduGroup {
    pub(crate) canonical_group_id: Uuid,
    pub(crate) merged_group_id: Option<Uuid>,
}
