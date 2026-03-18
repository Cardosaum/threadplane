use core::net::SocketAddr;

use derive_more::Constructor;
use snafu::ResultExt as _;

use crate::{
    error::ServerResult,
    error::{InvalidBindAddress, InvalidWorkspaceBootstrap},
};
use threadplane_core::{
    validate_workspace_policy, ActorPublicKey, PublicKeyAlgorithm, ThreadplaneConfig,
    WorkspaceAuthPolicy, WorkspaceBootstrapConfig, WorkspaceMembership, WorkspacePolicy,
    WorkspacePriorityPolicy, WorkspaceRole,
};

#[derive(Debug, Clone, Constructor)]
pub(crate) struct BootstrapMembership {
    actor_id: String,
    role: WorkspaceRole,
}

#[derive(Debug, Clone, Constructor)]
pub(crate) struct BootstrapPublicKey {
    actor_id: String,
    algorithm: PublicKeyAlgorithm,
    key_id: String,
    public_key: String,
}

#[derive(Debug, Clone, Constructor)]
pub(crate) struct WorkspaceGovernanceBootstrap {
    auth: WorkspaceAuthPolicy,
    memberships: Vec<BootstrapMembership>,
    priorities: WorkspacePriorityPolicy,
    public_keys: Vec<BootstrapPublicKey>,
}

impl WorkspaceGovernanceBootstrap {
    fn from_config(config: WorkspaceBootstrapConfig) -> ServerResult<Self> {
        validate_workspace_policy(&WorkspacePolicy {
            auth: config.auth.clone(),
            priorities: config.priorities.clone(),
            workspace: "__bootstrap__".to_owned(),
        })
        .map_err(|error| {
            InvalidWorkspaceBootstrap {
                reason: error.to_string(),
            }
            .build()
        })?;

        Ok(Self::new(
            config.auth,
            config
                .memberships
                .into_iter()
                .map(|membership| BootstrapMembership::new(membership.actor_id, membership.role))
                .collect(),
            config.priorities,
            config
                .public_keys
                .into_iter()
                .map(|key| {
                    BootstrapPublicKey::new(key.actor_id, key.algorithm, key.key_id, key.public_key)
                })
                .collect(),
        ))
    }

    pub(crate) fn memberships_for_workspace(&self, workspace: &str) -> Vec<WorkspaceMembership> {
        self.memberships
            .iter()
            .map(|membership| WorkspaceMembership {
                actor_id: membership.actor_id.clone(),
                role: membership.role,
                workspace: workspace.to_owned(),
            })
            .collect()
    }

    pub(crate) fn policy_for_workspace(&self, workspace: &str) -> WorkspacePolicy {
        WorkspacePolicy {
            auth: self.auth.clone(),
            priorities: self.priorities.clone(),
            workspace: workspace.to_owned(),
        }
    }

    pub(crate) fn public_keys(&self) -> Vec<ActorPublicKey> {
        self.public_keys
            .iter()
            .map(|key| ActorPublicKey {
                actor_id: key.actor_id.clone(),
                algorithm: key.algorithm,
                key_id: key.key_id.clone(),
                public_key: key.public_key.clone(),
            })
            .collect()
    }
}

pub(crate) struct AppConfig {
    pub(crate) bind_addr: SocketAddr,
    pub(crate) database_url: String,
    pub(crate) default_lease_seconds: i64,
    pub(crate) neo4j_uri: String,
    pub(crate) neo4j_user: String,
    pub(crate) neo4j_password: String,
    pub(crate) workspace_bootstrap: WorkspaceGovernanceBootstrap,
}

impl AppConfig {
    pub(crate) fn from_runtime_config(config: ThreadplaneConfig) -> ServerResult<Self> {
        let bind_addr = config.server.bind.parse().context(InvalidBindAddress {
            value: config.server.bind.clone(),
        })?;
        let workspace_bootstrap =
            WorkspaceGovernanceBootstrap::from_config(config.server.workspace_bootstrap)?;

        Ok(Self {
            bind_addr,
            database_url: config.server.database_url,
            default_lease_seconds: config.server.default_lease_seconds,
            neo4j_uri: config.server.neo4j_uri,
            neo4j_user: config.server.neo4j_user,
            neo4j_password: config.server.neo4j_password,
            workspace_bootstrap,
        })
    }
}
