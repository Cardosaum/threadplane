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
use xdg::BaseDirectories;

pub const CONFIG_FILE_NAME: &str = "config.toml";
pub const DEPENDS_ON_RELATION: &str = "depends_on";
pub const IMPLEMENTS_EPIC_RELATION: &str = "implements_epic";
pub const ENV_CONFIG_PATH: &str = "THREADPLANE_CONFIG";
pub const ENV_PREFIX: &str = "THREADPLANE__";
pub const SERVICE_NAME: &str = "threadplane";
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

    #[snafu(display("configuration discovery failed: {reason}"))]
    ConfigPathUnavailable {
        reason: String,
        #[snafu(implicit)]
        location: snafu::Location,
    },
}

impl ThreadplaneError {
    #[inline]
    #[must_use]
    pub const fn location(&self) -> &snafu::Location {
        match self {
            Self::ConfigPathUnavailable { location, .. } | Self::ConfigLoad { location, .. } => {
                location
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub bind: String,
    pub database_url: String,
    pub default_lease_seconds: i64,
    pub neo4j_password: String,
    pub neo4j_uri: String,
    pub neo4j_user: String,
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
/// Returns the primary XDG config path for `threadplane`.
///
/// # Errors
///
/// Returns an error when the platform XDG base directories cannot be resolved.
pub fn default_config_path() -> Result<PathBuf, ThreadplaneError> {
    let directories = base_directories();
    config_home(&directories)
}

#[inline]
/// Returns the fallback XDG config search paths for `threadplane`.
///
/// # Errors
///
/// Returns an error when the platform XDG base directories cannot be resolved.
pub fn default_system_config_paths() -> Result<Vec<PathBuf>, ThreadplaneError> {
    let directories = base_directories();
    Ok(directories
        .get_config_dirs()
        .into_iter()
        .map(|path| path.join(CONFIG_FILE_NAME))
        .collect())
}

#[inline]
/// Loads layered runtime configuration from an optional TOML file, environment overrides,
/// and serialized runtime overrides.
///
/// # Errors
///
/// Returns an error when configuration discovery fails, when the optional config file cannot be
/// parsed, or when the gathered values cannot be deserialized into [`ThreadplaneConfig`].
pub fn load_threadplane_config() -> Result<ThreadplaneConfig, ThreadplaneError> {
    load_threadplane_config_with_path(None).map(|loaded| loaded.config)
}

#[inline]
/// Loads layered runtime configuration from an optional TOML file, environment overrides,
/// and serialized runtime overrides.
///
/// `config_path` is an explicit one-off override, typically provided by a CLI flag.
///
/// # Errors
///
/// Returns an error when configuration discovery fails, when the optional config file cannot be
/// parsed, or when the gathered values cannot be deserialized into [`ThreadplaneConfig`].
pub fn load_threadplane_config_with_path(
    config_path: Option<&Path>,
) -> Result<LoadedThreadplaneConfig, ThreadplaneError> {
    load_threadplane_config_with_overrides(config_path, &ThreadplaneConfigOverrides::default())
}

#[inline]
/// Loads layered runtime configuration from an optional TOML file, environment overrides,
/// and serialized runtime overrides.
///
/// `overrides` should be a sparse serializable structure where unset values are omitted, such as
/// CLI flags represented as `Option<T>` with `skip_serializing_if`.
///
/// # Errors
///
/// Returns an error when configuration discovery fails, when the optional config file cannot be
/// parsed, or when the gathered values cannot be deserialized into [`ThreadplaneConfig`].
pub fn load_threadplane_config_with_overrides(
    config_path: Option<&Path>,
    overrides: &ThreadplaneConfigOverrides,
) -> Result<LoadedThreadplaneConfig, ThreadplaneError> {
    let discovery = discover_threadplane_config(config_path)?;
    let config = threadplane_config_figment(&discovery, overrides)
        .extract()
        .context(ConfigLoad)?;

    Ok(LoadedThreadplaneConfig { config, discovery })
}

#[inline]
/// Resolves the config discovery order using explicit override, `THREADPLANE_CONFIG`,
/// and XDG config locations.
///
/// # Errors
///
/// Returns an error when the platform XDG base directories cannot be resolved.
pub fn discover_threadplane_config(
    config_path: Option<&Path>,
) -> Result<ConfigDiscovery, ThreadplaneError> {
    let env_override = config_path_from_env();
    let explicit_override = config_path.map(Path::to_path_buf);
    let (search_order, selected_path) =
        resolve_config_path(config_path, env_override.clone())?;

    Ok(ConfigDiscovery {
        env_override,
        explicit_override,
        search_order,
        selected_path,
        env_prefix: ENV_PREFIX,
    })
}

#[inline]
fn threadplane_config_figment(
    discovery: &ConfigDiscovery,
    overrides: &ThreadplaneConfigOverrides,
) -> Figment {
    let file_layer = discovery.selected_path.as_ref().map_or_else(Figment::new, |config_path| {
        Figment::new().merge(Toml::file(config_path))
    });

    file_layer
        .merge(Env::prefixed(ENV_PREFIX).split("__"))
        .merge(Serialized::defaults(overrides))
}

#[inline]
fn resolve_config_path(
    explicit_override: Option<&Path>,
    env_override: Option<PathBuf>,
) -> Result<(Vec<PathBuf>, Option<PathBuf>), ThreadplaneError> {
    if let Some(config_path) = explicit_override {
        let explicit_path = config_path.to_path_buf();
        return Ok((vec![explicit_path.clone()], Some(explicit_path)));
    }

    if let Some(config_path) = env_override {
        return Ok((vec![config_path.clone()], Some(config_path)));
    }

    let directories = base_directories();
    let search_order = xdg_search_paths(&directories)?;
    let selected_path = directories.find_config_file(CONFIG_FILE_NAME);

    Ok((search_order, selected_path))
}

#[inline]
fn xdg_search_paths(directories: &BaseDirectories) -> Result<Vec<PathBuf>, ThreadplaneError> {
    let mut search_order = vec![config_home(directories)?];
    search_order.extend(
        directories
            .get_config_dirs()
            .into_iter()
            .map(|path| path.join(CONFIG_FILE_NAME)),
    );
    Ok(search_order)
}

#[inline]
fn base_directories() -> BaseDirectories {
    BaseDirectories::with_prefix(SERVICE_NAME)
}

fn config_home(directories: &BaseDirectories) -> Result<PathBuf, ThreadplaneError> {
    directories
        .get_config_home()
        .map(|path| path.join(CONFIG_FILE_NAME))
        .ok_or_else(|| {
            ConfigPathUnavailable {
                reason: "xdg config home is not available".to_owned(),
            }
            .build()
        })
}

#[inline]
#[must_use]
fn config_path_from_env() -> Option<PathBuf> {
    env::var(ENV_CONFIG_PATH)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}
