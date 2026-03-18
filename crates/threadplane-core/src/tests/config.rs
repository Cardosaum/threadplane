use super::*;

#[test]
fn xdg_config_paths_use_threadplane_standard_locations() -> Result<(), Box<dyn Error>> {
    let user_path = default_config_path()?;
    let system_paths = default_system_config_paths()?;

    if !user_path
        .to_string_lossy()
        .ends_with("threadplane/config.toml")
    {
        return Err(Box::new(io::Error::other(
            "unexpected XDG user config path",
        )));
    }
    if system_paths.is_empty() {
        return Err(Box::new(io::Error::other(
            "missing XDG system config paths",
        )));
    }
    if !system_paths
        .iter()
        .all(|path| path.to_string_lossy().ends_with("threadplane/config.toml"))
    {
        return Err(Box::new(io::Error::other(
            "unexpected XDG system config path",
        )));
    }

    Ok(())
}

#[test]
fn discover_threadplane_config_prefers_explicit_path() -> Result<(), Box<dyn Error>> {
    let explicit_path = PathBuf::from("/tmp/threadplane-explicit.toml");
    let discovery = discover_threadplane_config(Some(explicit_path.as_path()))?;

    if discovery.explicit_override != Some(explicit_path.clone()) {
        return Err(Box::new(io::Error::other("unexpected explicit_override")));
    }
    if discovery.selected_path != Some(explicit_path.clone()) {
        return Err(Box::new(io::Error::other("unexpected selected_path")));
    }
    if discovery.search_order != vec![explicit_path] {
        return Err(Box::new(io::Error::other("unexpected search_order")));
    }
    if discovery.env_prefix != ENV_PREFIX {
        return Err(Box::new(io::Error::other("unexpected env_prefix")));
    }

    Ok(())
}

#[test]
#[expect(
    clippy::result_large_err,
    reason = "figment::Jail fixes the closure error type to figment::Result."
)]
fn load_threadplane_config_with_path_reads_explicit_config() {
    Jail::expect_with(|jail| {
        jail.create_file("config.toml", full_config_body())?;
        let config_path = jail.directory().join("config.toml");
        let loaded = load_threadplane_config_with_path(Some(config_path.as_path()))
            .map_err(|error| error.to_string())?;

        assert_eq!(loaded.config.cli.url, "http://127.0.0.1:4123");
        assert_eq!(loaded.config.server.bind, "127.0.0.1:4321");
        assert_eq!(
            loaded.config.server.database_url,
            "postgres://threadplane:secret@127.0.0.1:5432/threadplane"
        );
        assert_eq!(loaded.config.server.default_lease_seconds, 42);
        assert_eq!(loaded.discovery.selected_path, Some(config_path));

        Ok(())
    });
}

#[test]
#[expect(
    clippy::result_large_err,
    reason = "figment::Jail fixes the closure error type to figment::Result."
)]
fn load_threadplane_config_with_overrides_applies_sparse_runtime_layer() {
    Jail::expect_with(|jail| {
        jail.create_file("config.toml", full_config_body())?;
        let config_path = jail.directory().join("config.toml");
        let overrides = ThreadplaneConfigOverrides {
            cli: Some(CliConfigOverrides {
                url: Some("http://127.0.0.1:4999".to_owned()),
            }),
            ..ThreadplaneConfigOverrides::default()
        };
        let loaded =
            load_threadplane_config_with_overrides(Some(config_path.as_path()), &overrides)
                .map_err(|error| error.to_string())?;

        assert_eq!(loaded.config.cli.url, "http://127.0.0.1:4999");
        assert_eq!(loaded.config.server.bind, "127.0.0.1:4321");

        Ok(())
    });
}

#[test]
#[expect(
    clippy::result_large_err,
    reason = "figment::Jail fixes the closure error type to figment::Result."
)]
fn load_threadplane_config_requires_all_fields_to_be_explicit() {
    let config_body = r#"
[cli]
url = "http://127.0.0.1:4123"

[server]
bind = "127.0.0.1:4321"
"#;
    Jail::expect_with(|jail| {
        jail.create_file("config.toml", config_body)?;
        let config_path = jail.directory().join("config.toml");

        let load_result = load_threadplane_config_with_path(Some(config_path.as_path()));
        assert!(
            load_result.is_err(),
            "incomplete config unexpectedly loaded"
        );
        let rendered = load_result
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        assert!(rendered.contains("configuration load failed"));

        Ok(())
    });
}
