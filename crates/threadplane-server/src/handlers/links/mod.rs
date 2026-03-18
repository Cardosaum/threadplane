#![expect(
    clippy::redundant_pub_crate,
    reason = "Link handlers are crate-local endpoints with explicit visibility."
)]

mod shared;
mod standard;
mod xanadu;

pub(crate) use standard::add_link;
pub(crate) use xanadu::add_xanadu_link;
