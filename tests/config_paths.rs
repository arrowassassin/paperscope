use paper::core::config::{Config, SearchConfig};
use paper::core::paths::Paths;

#[test]
fn default_config_matches_spec() {
    let config = Config::default();
    assert_eq!(config.keyword_sufficiency_threshold, 10);
    assert_eq!(config.rerank_top, 20);
    assert_eq!(config.rrf_k, 60);
    assert_eq!(config.max_chunk_tokens, 400);
    assert_eq!(config.chunk_overlap, 40);
    assert!(config.roots.is_empty());

    let search = SearchConfig::default();
    assert_eq!(search.keyword_limit, 20);
    assert_eq!(search.semantic_limit, 20);
    assert_eq!(search.fused_limit, 50);
}

#[test]
fn paths_resolve_expected_locations() {
    let paths = Paths::with_base("/tmp/paper-test");
    assert!(paths.model_cache_dir().ends_with("paper-test/models"));
    assert!(paths.db_path().ends_with("chunks.db"));
    assert!(paths.config_path().ends_with("config.toml"));
}

#[test]
fn default_paths_live_under_paper() {
    let paths = Paths::new();
    assert!(paths.model_cache_dir().ends_with("paper/models"));
    assert!(paths.db_path().ends_with("chunks.db"));
}

#[test]
fn config_round_trips_through_toml() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let root = dir.path().join("docs");
    let mut config = paper::Config {
        rrf_k: 42,
        ..Default::default()
    };
    config.roots.push(root.clone());
    std::fs::write(&path, toml::to_string_pretty(&config).unwrap()).unwrap();

    let loaded = Config::load_from(&path).unwrap();
    assert_eq!(loaded.rrf_k, 42);
    assert_eq!(loaded.roots, vec![root]);
    // Unspecified fields fall back to defaults rather than failing to parse.
    assert_eq!(loaded.search().keyword_limit, 20);
}

#[test]
fn partial_config_uses_defaults_for_missing_keys() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "rrf_k = 12\n").unwrap();
    let loaded = Config::load_from(&path).unwrap();
    assert_eq!(loaded.rrf_k, 12);
    assert_eq!(loaded.rerank_top, 20);
    assert_eq!(loaded.search().keyword_limit, 20);
}

#[test]
fn missing_config_file_is_defaults_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let loaded = Config::load_from(&dir.path().join("nope.toml")).unwrap();
    assert_eq!(loaded.rrf_k, 60);
}

#[cfg(unix)]
fn mode(path: &std::path::Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[cfg(unix)]
#[test]
fn ensure_dirs_creates_an_owner_only_data_dir() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::with_base(dir.path().join("paper"));
    paths.ensure_dirs().unwrap();
    assert_eq!(mode(&paths.base_dir()), 0o700);
}

/// The config can hold the OpenRouter API key, so it must not be world-readable.
#[cfg(unix)]
#[test]
fn a_new_config_file_and_its_directory_are_owner_only() {
    let dir = tempfile::tempdir().unwrap();
    let config_dir = dir.path().join("paper");
    let path = config_dir.join("config.toml");
    Config::default().save_to(&path).unwrap();
    assert_eq!(mode(&path), 0o600);
    assert_eq!(mode(&config_dir), 0o700);
    assert_eq!(Config::load_from(&path).unwrap().rrf_k, 60);
}

#[cfg(unix)]
#[test]
fn saving_tightens_an_existing_world_readable_config() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "rrf_k = 12\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let before = mode(dir.path());

    let config = Config {
        rrf_k: 7,
        ..Default::default()
    };
    config.save_to(&path).unwrap();
    assert_eq!(mode(&path), 0o600);
    assert_eq!(Config::load_from(&path).unwrap().rrf_k, 7);
    // An existing directory is never chmodded.
    assert_eq!(mode(dir.path()), before);
}
