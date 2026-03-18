use serde::{Deserialize, Serialize};

use crate::types::{
    PublicKeyAlgorithm, WorkspaceAuthPolicy, WorkspacePriorityPolicy, WorkspaceRole,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceBootstrapMembershipConfig {
    pub actor_id: String,
    pub role: WorkspaceRole,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceBootstrapPublicKeyConfig {
    pub actor_id: String,
    pub algorithm: PublicKeyAlgorithm,
    pub key_id: String,
    pub public_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceBootstrapConfig {
    pub auth: WorkspaceAuthPolicy,
    pub memberships: Vec<WorkspaceBootstrapMembershipConfig>,
    pub priorities: WorkspacePriorityPolicy,
    pub public_keys: Vec<WorkspaceBootstrapPublicKeyConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub bind: String,
    pub database_url: String,
    pub default_lease_seconds: i64,
    pub neo4j_password: String,
    pub neo4j_uri: String,
    pub neo4j_user: String,
    pub workspace_bootstrap: WorkspaceBootstrapConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadplaneConfig {
    pub cli: CliConfig,
    pub server: ServerConfig,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CliConfigOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ServerConfigOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub database_url: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_lease_seconds: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub neo4j_password: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub neo4j_uri: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub neo4j_user: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ThreadplaneConfigOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli: Option<CliConfigOverrides>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<ServerConfigOverrides>,
}
