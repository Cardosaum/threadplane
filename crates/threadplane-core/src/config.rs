use std::{env, path::PathBuf};

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
    let figment = config_path_from_env()
        .or_else(local_config_path_if_present)
        .or_else(system_config_path_if_present)
        .map_or_else(
            || {
                Figment::from(Serialized::defaults(ThreadplaneConfig::default()))
                    .merge(Env::prefixed("THREADPLANE__").split("__"))
            },
            |config_path| {
                Figment::from(Serialized::defaults(ThreadplaneConfig::default()))
                    .merge(Toml::file(config_path))
                    .merge(Env::prefixed("THREADPLANE__").split("__"))
            },
        );

    figment.extract().context(ConfigLoad)
}

#[inline]
#[must_use]
fn config_path_from_env() -> Option<PathBuf> {
    env::var("THREADPLANE_CONFIG")
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[inline]
#[must_use]
fn local_config_path_if_present() -> Option<PathBuf> {
    let config_path = default_config_path();
    config_path.exists().then_some(config_path)
}

#[inline]
#[must_use]
fn system_config_path_if_present() -> Option<PathBuf> {
    let config_path = default_system_config_path();
    config_path.exists().then_some(config_path)
}
