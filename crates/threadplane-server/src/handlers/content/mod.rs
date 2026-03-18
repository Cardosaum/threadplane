#![expect(
    clippy::redundant_pub_crate,
    reason = "Content handlers are crate-local endpoints with explicit visibility."
)]
#![allow(
    clippy::wildcard_imports,
    reason = "The content handler submodule intentionally builds on the handler prelude."
)]

use super::*;

mod epic;
mod memory;
mod note;

pub(crate) use epic::create_epic;
pub(crate) use memory::create_memory;
pub(crate) use note::{create_note, update_note};
