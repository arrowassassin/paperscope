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

#[test]
fn save_is_atomic_leaves_no_temp_files_and_writes_through_a_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real.toml");
    std::fs::write(&real, "rrf_k = 12\n").unwrap();
    let link = dir.path().join("config.toml");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real, &link).unwrap();
    #[cfg(not(unix))]
    let link = real.clone();

    let config = Config {
        rrf_k: 9,
        ..Default::default()
    };
    config.save_to(&link).unwrap();
    assert_eq!(Config::load_from(&real).unwrap().rrf_k, 9);
    #[cfg(unix)]
    assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
    let names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), if cfg!(unix) { 2 } else { 1 }, "{names:?}");
}

/// Concurrent saves never expose a torn file: every read parses. (The MCP
/// server additionally serialises its read-modify-write cycles under the roots
/// write lock, which is not separable from a running server in a test; that
/// lock is what makes `add_index_root`/`remove_index_root` lose no updates.)
#[test]
fn concurrent_saves_never_expose_a_partial_config() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    Config::default().save_to(&path).unwrap();
    let writers: Vec<_> = (0..4)
        .map(|n| {
            let path = path.clone();
            std::thread::spawn(move || {
                for i in 0..50 {
                    let config = Config {
                        roots: vec![format!("/tmp/{n}/{i}").into(); 50],
                        ..Default::default()
                    };
                    config.save_to(&path).unwrap();
                }
            })
        })
        .collect();
    for _ in 0..200 {
        let text = std::fs::read_to_string(&path).unwrap();
        toml::from_str::<Config>(&text).expect("torn config read");
    }
    for writer in writers {
        writer.join().unwrap();
    }
}
