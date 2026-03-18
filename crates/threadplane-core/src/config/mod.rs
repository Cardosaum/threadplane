mod discovery;
mod schema;

pub use self::discovery::{
    default_config_path, default_system_config_paths, discover_threadplane_config,
    load_threadplane_config, load_threadplane_config_with_overrides,
    load_threadplane_config_with_path, ConfigDiscovery, LoadedThreadplaneConfig, ThreadplaneError,
};
pub use self::schema::{
    CliConfig, CliConfigOverrides, ServerConfig, ServerConfigOverrides, ThreadplaneConfig,
    ThreadplaneConfigOverrides, WorkspaceBootstrapConfig, WorkspaceBootstrapMembershipConfig,
    WorkspaceBootstrapPublicKeyConfig,
};

pub const CONFIG_FILE_NAME: &str = "config.toml";
pub const DEPENDS_ON_RELATION: &str = "depends_on";
pub const IMPLEMENTS_EPIC_RELATION: &str = "implements_epic";
pub const ENV_CONFIG_PATH: &str = "THREADPLANE_CONFIG";
pub const ENV_PREFIX: &str = "THREADPLANE__";
pub const SERVICE_NAME: &str = "threadplane";
pub const XANADU_RELATION: &str = "xanadu_link";
