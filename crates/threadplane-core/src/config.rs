use std::{
    env,
    path::{Path, PathBuf},
};

use figment::{
    providers::{Env, Format as _, Serialized, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt as _, Snafu};

pub const SERVICE_NAME: &str = "threadplane";
pub const DEFAULT_BIND_ADDR: &str = "127.0.0.1:4000";
pub const DEFAULT_CONFIG_PATH: &str = "etc/config.toml";
pub const DEFAULT_SYSTEM_CONFIG_PATH: &str = "/etc/threadplane/config.toml";
pub const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:4000";
pub const DEFAULT_LEASE_SECONDS: i64 = 300;
pub const DEPENDS_ON_RELATION: &str = "depends_on";
pub const IMPLEMENTS_EPIC_RELATION: &str = "implements_epic";
pub const ENV_CONFIG_PATH: &str = "THREADPLANE_CONFIG";
pub const ENV_PREFIX: &str = "THREADPLANE__";
pub const XANADU_RELATION: &str = "xanadu_link";

#[derive(Debug, Snafu)]
#[snafu(visibility(pub), context(suffix(false)))]
pub enum ThreadplaneError {
    #[snafu(display("configuration load failed: {source}"))]
    ConfigLoad {
        #[snafu(source(from(figment::Error, Box::new)))]
        source: Box<figment::Error>,
        #[snafu(implicit)]
        location: snafu::Location,
    },
}

impl ThreadplaneError {
    #[inline]
    #[must_use]
    pub const fn location(&self) -> &snafu::Location {
        match self {
            Self::ConfigLoad { location, .. } => location,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    pub url: String,
}

impl Default for CliConfig {
    #[inline]
    fn default() -> Self {
        Self {
            url: DEFAULT_SERVER_URL.to_owned(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub bind: String,
    pub database_url: Option<String>,
    pub default_lease_seconds: i64,
    pub neo4j_password: Option<String>,
    pub neo4j_uri: Option<String>,
    pub neo4j_user: Option<String>,
}

impl Default for ServerConfig {
    #[inline]
    fn default() -> Self {
        Self {
            bind: DEFAULT_BIND_ADDR.to_owned(),
            database_url: None,
            default_lease_seconds: DEFAULT_LEASE_SECONDS,
            neo4j_password: None,
            neo4j_uri: None,
            neo4j_user: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize)]
pub struct ConfigDiscovery {
    pub env_override: Option<PathBuf>,
    pub env_prefix: &'static str,
    pub explicit_override: Option<PathBuf>,
    pub search_order: Vec<PathBuf>,
    pub selected_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct LoadedThreadplaneConfig {
    pub config: ThreadplaneConfig,
    pub discovery: ConfigDiscovery,
}

#[inline]
#[must_use]
pub fn default_config_path() -> PathBuf {
    PathBuf::from(DEFAULT_CONFIG_PATH)
}

#[inline]
#[must_use]
pub fn default_system_config_path() -> PathBuf {
    PathBuf::from(DEFAULT_SYSTEM_CONFIG_PATH)
}

#[inline]
/// Loads layered runtime configuration from defaults, optional TOML, and environment overrides.
///
/// # Errors
///
/// Returns an error when the optional config file cannot be parsed or when
/// the gathered values cannot be deserialized into [`ThreadplaneConfig`].
pub fn load_threadplane_config() -> Result<ThreadplaneConfig, ThreadplaneError> {
    load_threadplane_config_with_path(None).map(|loaded| loaded.config)
}

#[inline]
/// Loads layered runtime configuration from defaults, optional TOML, and environment overrides.
///
/// `config_path` is an explicit one-off override, typically provided by a CLI flag.
///
/// # Errors
///
/// Returns an error when the optional config file cannot be parsed or when
/// the gathered values cannot be deserialized into [`ThreadplaneConfig`].
pub fn load_threadplane_config_with_path(
    config_path: Option<&Path>,
) -> Result<LoadedThreadplaneConfig, ThreadplaneError> {
    load_threadplane_config_with_overrides(config_path, &ThreadplaneConfigOverrides::default())
}

#[inline]
/// Loads layered runtime configuration from defaults, optional TOML, environment overrides,
/// and serialized runtime overrides.
///
/// `overrides` should be a sparse serializable structure where unset values are omitted, such as
/// CLI flags represented as `Option<T>` with `skip_serializing_if`.
///
/// # Errors
///
/// Returns an error when the optional config file cannot be parsed or when
/// the gathered values cannot be deserialized into [`ThreadplaneConfig`].
pub fn load_threadplane_config_with_overrides(
    config_path: Option<&Path>,
    overrides: &ThreadplaneConfigOverrides,
) -> Result<LoadedThreadplaneConfig, ThreadplaneError> {
    let discovery = discover_threadplane_config(config_path);
    let figment = threadplane_config_figment(&discovery, overrides);
    let config = figment.extract().context(ConfigLoad)?;

    Ok(LoadedThreadplaneConfig { config, discovery })
}

#[inline]
#[must_use]
pub fn discover_threadplane_config(config_path: Option<&Path>) -> ConfigDiscovery {
    let env_override = config_path_from_env();
    let explicit_override = config_path.map(Path::to_path_buf);
    let (search_order, selected_path) = resolve_config_path(config_path, env_override.clone());

    ConfigDiscovery {
        env_override,
        explicit_override,
        search_order,
        selected_path,
        env_prefix: ENV_PREFIX,
    }
}

#[inline]
#[must_use]
fn threadplane_config_figment(
    discovery: &ConfigDiscovery,
    overrides: &ThreadplaneConfigOverrides,
) -> Figment {
    let base_figment = Figment::from(Serialized::defaults(ThreadplaneConfig::default()));
    let layered_figment = if let Some(config_path) = discovery.selected_path.as_ref() {
        base_figment.merge(Toml::file(config_path))
    } else {
        base_figment
    };
    let env_figment = layered_figment.merge(Env::prefixed(ENV_PREFIX).split("__"));

    env_figment.merge(Serialized::defaults(overrides))
}

#[inline]
#[must_use]
fn resolve_config_path(
    explicit_override: Option<&Path>,
    env_override: Option<PathBuf>,
) -> (Vec<PathBuf>, Option<PathBuf>) {
    if let Some(config_path) = explicit_override {
        let explicit_path = config_path.to_path_buf();
        return (vec![explicit_path.clone()], Some(explicit_path));
    }

    if let Some(config_path) = env_override {
        return (vec![config_path.clone()], Some(config_path));
    }

    let local_path = default_config_path();
    let system_path = default_system_config_path();
    let search_order = vec![local_path.clone(), system_path.clone()];
    let selected_path = local_path
        .exists()
        .then_some(local_path)
        .or_else(|| system_path.exists().then_some(system_path));

    (search_order, selected_path)
}

#[inline]
#[must_use]
fn config_path_from_env() -> Option<PathBuf> {
    env::var(ENV_CONFIG_PATH)
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}
