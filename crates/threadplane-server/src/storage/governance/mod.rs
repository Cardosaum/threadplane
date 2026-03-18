#![expect(
    clippy::redundant_pub_crate,
    reason = "Governance persistence is shared only inside this crate."
)]
#![allow(
    clippy::wildcard_imports,
    reason = "The storage governance module intentionally builds on the storage prelude."
)]

mod bootstrap;
mod keys;
mod memberships;
mod policy;

use super::*;
use threadplane_core::{
    validate_workspace_policy, ActorPublicKey, PublicKeyAlgorithm, TaskPriority,
    WorkspaceAuthPolicy, WorkspaceMembership, WorkspacePolicy, WorkspacePriority,
    WorkspacePriorityPolicy, WorkspaceRole,
};

pub(crate) use bootstrap::ensure_workspace_governance;
pub(crate) use keys::{fetch_actor_public_keys, upsert_actor_public_key};
pub(crate) use memberships::{
    fetch_workspace_memberships, require_workspace_role, upsert_workspace_membership,
};
pub(crate) use policy::{
    fetch_workspace_policy, upsert_workspace_policy, workspace_supports_priority,
};
