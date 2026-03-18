use alloc::collections::BTreeSet;

use super::{normalize_identifier, TaskPriority};
use derive_more::Display;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum WorkspaceRole {
    Admin,
    Editor,
    Viewer,
}

impl WorkspaceRole {
    #[inline]
    #[must_use]
    pub const fn can_administer(self) -> bool {
        matches!(self, Self::Admin)
    }

    #[inline]
    #[must_use]
    pub const fn can_edit(self) -> bool {
        matches!(self, Self::Admin | Self::Editor)
    }

    #[inline]
    #[must_use]
    pub const fn can_view(self) -> bool {
        true
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum PublicKeyAlgorithm {
    Ed25519,
    Secp256k1,
    SshEd25519,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspacePriority {
    pub description: Option<String>,
    pub name: String,
    pub rank: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspacePriorityPolicy {
    pub default_priority: String,
    pub priorities: Vec<WorkspacePriority>,
}

impl WorkspacePriorityPolicy {
    #[inline]
    #[must_use]
    pub fn default_task_priority(&self) -> Option<TaskPriority> {
        TaskPriority::new(self.default_priority.clone())
    }

    #[inline]
    #[must_use]
    pub fn rank_for(&self, priority: &TaskPriority) -> Option<u16> {
        self.priorities
            .iter()
            .find(|candidate| {
                normalize_workspace_priority_name(&candidate.name) == priority.as_str()
            })
            .map(|candidate| candidate.rank)
    }

    #[inline]
    #[must_use]
    pub fn supports(&self, priority: &TaskPriority) -> bool {
        self.rank_for(priority).is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceAuthPolicy {
    pub allowed_algorithms: Vec<PublicKeyAlgorithm>,
    pub challenge_ttl_seconds: u32,
    pub signed_commands_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorPublicKey {
    pub actor_id: String,
    pub algorithm: PublicKeyAlgorithm,
    pub key_id: String,
    pub public_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceMembership {
    pub actor_id: String,
    pub role: WorkspaceRole,
    pub workspace: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspacePolicy {
    pub auth: WorkspaceAuthPolicy,
    pub priorities: WorkspacePriorityPolicy,
    pub workspace: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Display)]
pub enum WorkspacePolicyValidationError {
    #[display("workspace priorities must use unique normalized names; duplicate `{_0}`")]
    DuplicatePriorityName(String),
    #[display("workspace priorities must use unique ranks; duplicate `{_0}`")]
    DuplicatePriorityRank(u16),
    #[display("workspace auth policy must support at least one public-key algorithm")]
    MissingAuthAlgorithms,
    #[display("workspace priorities must include the default priority `{_0}`")]
    MissingDefaultPriority(String),
    #[display("workspace priorities must define at least one supported priority")]
    MissingPriorities,
}

#[inline]
#[must_use]
pub fn normalize_workspace_priority_name(name: &str) -> String {
    normalize_identifier(name)
}

#[inline]
/// Validates the durable policy shape for a workspace.
///
/// # Errors
///
/// Returns an error when either the auth or priority policy is structurally invalid.
pub fn validate_workspace_policy(
    policy: &WorkspacePolicy,
) -> Result<(), WorkspacePolicyValidationError> {
    validate_workspace_auth_policy(&policy.auth)?;
    validate_workspace_priority_policy(&policy.priorities)
}

#[inline]
/// Validates the auth section of a workspace policy.
///
/// # Errors
///
/// Returns an error when no public-key algorithms are configured.
pub fn validate_workspace_auth_policy(
    policy: &WorkspaceAuthPolicy,
) -> Result<(), WorkspacePolicyValidationError> {
    if policy.allowed_algorithms.is_empty() {
        return Err(WorkspacePolicyValidationError::MissingAuthAlgorithms);
    }

    Ok(())
}

#[inline]
/// Validates the priority section of a workspace policy.
///
/// # Errors
///
/// Returns an error when the policy has no priorities, is missing its default priority, or
/// contains duplicate normalized names or ranks.
pub fn validate_workspace_priority_policy(
    policy: &WorkspacePriorityPolicy,
) -> Result<(), WorkspacePolicyValidationError> {
    if policy.priorities.is_empty() {
        return Err(WorkspacePolicyValidationError::MissingPriorities);
    }

    let mut normalized_names = BTreeSet::new();
    let mut ranks = BTreeSet::new();
    let normalized_default = normalize_workspace_priority_name(&policy.default_priority);
    let mut default_present = false;

    for priority in &policy.priorities {
        let normalized_name = normalize_workspace_priority_name(&priority.name);
        if normalized_name == normalized_default {
            default_present = true;
        }
        if !normalized_names.insert(normalized_name.clone()) {
            return Err(WorkspacePolicyValidationError::DuplicatePriorityName(
                normalized_name,
            ));
        }
        if !ranks.insert(priority.rank) {
            return Err(WorkspacePolicyValidationError::DuplicatePriorityRank(
                priority.rank,
            ));
        }
    }

    if !default_present {
        return Err(WorkspacePolicyValidationError::MissingDefaultPriority(
            normalized_default,
        ));
    }

    Ok(())
}
