//! Daily remote-config refresh, following its default models, and the
//! resumable re-embed that an embedder switch needs (spec §3, §5, §6).
//! Only the daemon ever touches the network for the config.

use super::{ClearSetupStatus, DaemonState};
use crate::cli::Workspace;
use crate::core::config::Config;
use crate::core::effective::{
    load_live_remote, resolve, Source, BUILTIN_EMBEDDER, BUILTIN_RERANKER,
};
use crate::core::index::{init_db, reembed_pending, Connection, Indexer};
use crate::core::models::ModelManager;
use crate::core::remote_config::{self, RemoteConfig, RemoteConfigError};
use crate::core::search::SharedEmbedder;
use anyhow::{Context, Result};
use chrono::NaiveDate;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const BASE_URL: &str =
    "https://raw.githubusercontent.com/kintsugi-tools/paperscope/remote-config/config/remote-config.toml";

/// Where the signed config and its signature come from.
#[async_trait::async_trait]
pub trait ConfigSource: Send + Sync {
    /// The config bytes and the base64 signature.
    async fn fetch(&self) -> Result<(Vec<u8>, String)>;
}

pub struct GitHubSource;

async fn get_capped(client: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let mut response = client.get(url).send().await?.error_for_status()?;
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        body.extend_from_slice(&chunk);
        if body.len() > remote_config::MAX_BYTES {
            anyhow::bail!(
                "response from {url} is larger than {} bytes",
                remote_config::MAX_BYTES
            );
        }
    }
    Ok(body)
}

#[async_trait::async_trait]
impl ConfigSource for GitHubSource {
    async fn fetch(&self) -> Result<(Vec<u8>, String)> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("paper/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(20))
            .build()?;
        let toml = get_capped(&client, BASE_URL)
            .await
            .context("fetch remote config")?;
        let sig = get_capped(&client, &format!("{BASE_URL}.sig"))
            .await
            .context("fetch remote config signature")?;
        Ok((
            toml,
            String::from_utf8(sig).context("signature is not text")?,
        ))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum RefreshOutcome {
    Updated(u64),
    Unchanged,
    Rejected(String),
    FetchFailed(String),
    Offline,
}

/// `<data_dir>/remote-config.state`, read by doctor and MCP. Swap for
/// `remote_config::write_fetch_state` once that lands.
fn write_state(data_dir: &Path, outcome: &RefreshOutcome) {
    let (name, detail) = match outcome {
        RefreshOutcome::Updated(_) => ("updated", None),
        RefreshOutcome::Unchanged => ("unchanged", None),
        RefreshOutcome::Rejected(d) => ("rejected", Some(d.clone())),
        RefreshOutcome::FetchFailed(d) => ("fetch_failed", Some(d.clone())),
        RefreshOutcome::Offline => ("offline", None),
    };
    let state = crate::core::remote_config::FetchState {
        outcome: name.to_string(),
        detail,
        at: chrono::Utc::now().to_rfc3339(),
    };
    let _ = std::fs::create_dir_all(data_dir);
    let _ = crate::core::remote_config::write_fetch_state(data_dir, &state);
}

pub async fn refresh(
    source: &dyn ConfigSource,
    data_dir: &Path,
    keys: &[[u8; 32]],
    today: NaiveDate,
) -> RefreshOutcome {
    refresh_with(
        source,
        data_dir,
        keys,
        today,
        crate::bootstrap::is_offline(),
    )
    .await
}

async fn refresh_with(
    source: &dyn ConfigSource,
    data_dir: &Path,
    keys: &[[u8; 32]],
    today: NaiveDate,
    offline: bool,
) -> RefreshOutcome {
    let outcome = if offline {
        RefreshOutcome::Offline
    } else {
        match source.fetch().await {
            Err(e) => RefreshOutcome::FetchFailed(format!("{e:#}")),
            Ok((bytes, sig)) => {
                // An expired cached copy still blocks a rollback.
                let previous = remote_config::load_cached(data_dir, keys, today).map(|c| c.version);
                match remote_config::verify_and_parse(&bytes, &sig, keys, previous, today, false) {
                    Ok(cfg) => match remote_config::save_verified(data_dir, &bytes, &sig) {
                        Ok(()) => RefreshOutcome::Updated(cfg.version),
                        Err(e) => RefreshOutcome::FetchFailed(format!("save remote config: {e}")),
                    },
                    Err(RemoteConfigError::NotNewer { got, have }) if got == have => {
                        RefreshOutcome::Unchanged
                    }
                    Err(e) => RefreshOutcome::Rejected(e.to_string()),
                }
            }
        }
    };
    match &outcome {
        RefreshOutcome::Rejected(d) | RefreshOutcome::FetchFailed(d) => {
            tracing::warn!("remote config not updated: {d}");
        }
        _ => {}
    }
    write_state(data_dir, &outcome);
    outcome
}

fn meta(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM index_meta WHERE key = ?", [key], |r| {
        r.get(0)
    })
    .ok()
}

fn set_meta(conn: &Connection, key: &str, value: &str) {
    let _ = conn.inner().execute(
        "INSERT INTO index_meta (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    );
}

/// A pinned model is the user's choice, so it is never auto-removed later.
fn default_flag(source: Source) -> &'static str {
    if source == Source::Pinned {
        "0"
    } else {
        "1"
    }
}

/// Delete a model directory by id (the manager only knows ids still in the
/// current config, and a replaced default usually no longer is).
fn remove_model(manager: &ModelManager, id: &str) {
    let plain = !id.is_empty()
        && !id.starts_with('.')
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b));
    if plain {
        let _ = std::fs::remove_dir_all(manager.model_dir(id));
    }
}

/// Embed every chunk that lacks a vector, reporting progress in the setup
/// status line. Blocking; call inside `spawn_blocking`.
fn run_reembed(
    db_path: PathBuf,
    paths: crate::core::paths::Paths,
    embedder: SharedEmbedder,
) -> Result<()> {
    let conn = Connection::open(&db_path)?;
    init_db(&conn)?;
    // Opening with the embedder records the model (and drops stale vectors).
    let indexer = Indexer::new(conn, Some(embedder))?;
    indexer.reembed_missing(64, |done, total| {
        crate::bootstrap::set_setup_status(
            &paths,
            &format!("re-embedding your documents for the new model ({done} of {total})"),
        );
    })?;
    Ok(())
}

type LoadEmbedder = dyn Fn(&ModelManager, &str) -> Option<SharedEmbedder> + Sync;

fn load_real(manager: &ModelManager, id: &str) -> Option<SharedEmbedder> {
    manager
        .embedder_if_installed(id)
        .map(|e| Arc::new(e) as SharedEmbedder)
}

/// Re-read the saved config and bring installed models in line with it.
pub(super) async fn follow_defaults(workspace: &Workspace, state: &DaemonState) -> Result<()> {
    let today = chrono::Local::now().date_naive();
    let remote = load_live_remote(today);
    let manager = ModelManager::new().with_remote(remote.as_ref());
    follow(
        &workspace.config,
        remote.as_ref(),
        &manager,
        today,
        state,
        &load_real,
    )
    .await
}

async fn follow(
    config: &Config,
    remote: Option<&RemoteConfig>,
    manager: &ModelManager,
    today: NaiveDate,
    state: &DaemonState,
    load: &LoadEmbedder,
) -> Result<()> {
    let eff = resolve(config, remote, today);
    let conn = Connection::open(&state.db_path)?;
    init_db(&conn)?;
    let stored = conn.stored_embedding_model()?;

    // Embedder. A built-in effective default never moves an index (an expired
    // or absent config must not revert a switch); a pin or remote default does.
    let switch = stored
        .as_deref()
        .is_some_and(|s| s != eff.embedder && eff.embedder_source != Source::BuiltIn);
    let current = state.embedder.read().await.clone();
    let embedder = if switch {
        if !manager.is_installed(&eff.embedder) && !crate::bootstrap::is_offline() {
            if let Err(e) = manager.ensure(&eff.embedder).await {
                tracing::warn!("embedder download failed ({e:#}); keeping the current model");
            }
        }
        // Only a verified replacement moves the index.
        manager
            .is_installed(&eff.embedder)
            .then(|| load(manager, &eff.embedder))
            .flatten()
    } else if reembed_pending(&conn)? {
        current
    } else {
        None
    };
    if let Some(embedder) = embedder {
        let new_id = embedder.model_id().to_string();
        let _running = state.index_lock.lock().await;
        *state.embedder.write().await = Some(embedder.clone());
        let _clear = ClearSetupStatus(&state.paths);
        let (db, paths) = (state.db_path.clone(), state.paths.clone());
        tokio::task::spawn_blocking(move || run_reembed(db, paths, embedder)).await??;
        let flag = meta(&conn, "embedder_in_use_default");
        if let Some(old) = stored.filter(|o| *o != new_id) {
            let old_is_default = flag.map_or(old == BUILTIN_EMBEDDER, |v| v == "1");
            if eff.embedder_source == Source::Remote
                && old_is_default
                && config.embedder.as_deref() != Some(old.as_str())
                && !reembed_pending(&conn)?
            {
                remove_model(manager, &old);
            }
        }
        set_meta(
            &conn,
            "embedder_in_use_default",
            default_flag(eff.embedder_source),
        );
    } else if stored.as_deref() == Some(eff.embedder.as_str()) {
        set_meta(
            &conn,
            "embedder_in_use_default",
            default_flag(eff.embedder_source),
        );
    }

    // Reranker. `ensure_reranker` honours PAPER_OFFLINE and PAPER_NO_RERANKER.
    crate::bootstrap::ensure_reranker(manager, &eff.reranker).await;
    if manager.is_installed(&eff.reranker) {
        let prev = meta(&conn, "reranker_in_use").unwrap_or_else(|| BUILTIN_RERANKER.to_string());
        let prev_default = meta(&conn, "reranker_in_use_default").is_none_or(|v| v == "1");
        if prev != eff.reranker
            && eff.reranker_source == Source::Remote
            && prev_default
            && config.reranker.as_deref() != Some(prev.as_str())
        {
            remove_model(manager, &prev);
        }
        set_meta(&conn, "reranker_in_use", &eff.reranker);
        set_meta(
            &conn,
            "reranker_in_use_default",
            default_flag(eff.reranker_source),
        );
    }
    Ok(())
}

/// Startup, then every 24 hours.
pub(super) async fn run_loop(workspace: Arc<Workspace>, state: Arc<DaemonState>) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(24 * 60 * 60));
    loop {
        tick.tick().await;
        let today = chrono::Local::now().date_naive();
        refresh(
            &GitHubSource,
            &workspace.paths.base_dir(),
            &remote_config::compiled_keys(),
            today,
        )
        .await;
        if let Err(e) = follow_defaults(&workspace, &state).await {
            tracing::warn!("following remote defaults failed: {e:#}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::index::EMBEDDING_DIMS;
    use crate::core::paths::Paths;
    use crate::core::search::QueryEmbedder;
    use base64::Engine;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use sha2::{Digest, Sha256};
    use std::sync::Mutex as StdMutex;
    use tokio::sync::{Mutex, Notify, RwLock};

    const REV: &str = "0123456789abcdef0123456789abcdef01234567";

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 25).unwrap()
    }

    fn keypair() -> (Ed25519KeyPair, [u8; 32]) {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
        let kp = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let public = kp.public_key().as_ref().try_into().unwrap();
        (kp, public)
    }

    fn sign(kp: &Ed25519KeyPair, body: &str) -> String {
        base64::engine::general_purpose::STANDARD.encode(kp.sign(body.as_bytes()).as_ref())
    }

    /// An embedder whose model id is chosen by the test.
    struct Named(String);
    impl QueryEmbedder for Named {
        fn embed(&self, text: &str) -> Result<Vec<f32>> {
            let mut v = vec![0.0f32; EMBEDDING_DIMS];
            v[text.len() % EMBEDDING_DIMS] = 1.0;
            Ok(v)
        }
        fn model_id(&self) -> &str {
            &self.0
        }
    }

    fn named(id: &str) -> SharedEmbedder {
        Arc::new(Named(id.to_string()))
    }

    fn fake_loader(_: &ModelManager, id: &str) -> Option<SharedEmbedder> {
        Some(named(id))
    }

    fn model_toml(id: &str, kind: &str) -> String {
        let (arch, repo, dims) = if kind == "embedder" {
            ("bert", format!("intfloat/{id}"), "dims = 384")
        } else {
            ("xlm-roberta-seq-cls", format!("BAAI/{id}"), "")
        };
        let sha = hex::encode(Sha256::digest(id.as_bytes()));
        format!(
            "[[models]]\nid = \"{id}\"\nkind = \"{kind}\"\narch = \"{arch}\"\nrepo = \"{repo}\"\nrevision = \"{REV}\"\nlanguages = \"multilingual\"\n{dims}\nfiles = [ {{ name = \"model.safetensors\", sha256 = \"{sha}\", size = {} }} ]\n",
            id.len()
        )
    }

    fn body(version: u64, expires: &str, defaults: &str, models: &[(&str, &str)]) -> String {
        let models: String = models.iter().map(|(i, k)| model_toml(i, k)).collect();
        format!(
            "schema = 1\nversion = {version}\nexpires = \"{expires}\"\n[defaults]\n{defaults}\n{models}"
        )
    }

    fn parse(text: &str, expired_ok: bool) -> RemoteConfig {
        let (kp, pk) = keypair();
        remote_config::verify_and_parse(
            text.as_bytes(),
            &sign(&kp, text),
            &[pk],
            None,
            today(),
            expired_ok,
        )
        .unwrap()
    }

    /// Write the model's file with the content the config pins.
    fn install(manager: &ModelManager, id: &str) {
        let dir = manager.model_dir(id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("model.safetensors"), id).unwrap();
        assert!(manager.is_installed(id));
    }

    struct Env {
        _dir: tempfile::TempDir,
        manager: ModelManager,
        state: DaemonState,
    }

    fn env(remote: &RemoteConfig, embedder: Option<SharedEmbedder>) -> Env {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::with_base(dir.path());
        let manager =
            ModelManager::with_cache_dir(dir.path().join("models")).with_remote(Some(remote));
        let state = DaemonState {
            roots: RwLock::new(vec![]),
            paused: RwLock::new(false),
            shutdown: RwLock::new(false),
            shutdown_notify: Arc::new(Notify::new()),
            db_path: paths.db_path(),
            embedder: RwLock::new(embedder),
            index_lock: Mutex::new(()),
            paths,
        };
        Env {
            _dir: dir,
            manager,
            state,
        }
    }

    fn conn(env: &Env) -> Connection {
        let c = Connection::open(&env.state.db_path).unwrap();
        init_db(&c).unwrap();
        c
    }

    fn count(c: &Connection, sql: &str) -> i64 {
        c.query_i64(sql).unwrap()
    }

    struct Fake(StdMutex<Option<(Vec<u8>, String)>>);
    #[async_trait::async_trait]
    impl ConfigSource for Fake {
        async fn fetch(&self) -> Result<(Vec<u8>, String)> {
            self.0.lock().unwrap().clone().context("no response")
        }
    }

    struct Panics;
    #[async_trait::async_trait]
    impl ConfigSource for Panics {
        async fn fetch(&self) -> Result<(Vec<u8>, String)> {
            panic!("an offline daemon must not fetch");
        }
    }

    #[tokio::test]
    async fn offline_daemon_never_fetches_remote_config() {
        let dir = tempfile::tempdir().unwrap();
        let out = refresh_with(&Panics, dir.path(), &[], today(), true).await;
        assert_eq!(out, RefreshOutcome::Offline);
        let state = std::fs::read_to_string(dir.path().join("remote-config.state")).unwrap();
        assert!(state.contains("outcome = \"offline\""), "{state}");
        assert!(state.contains("at = "), "{state}");
    }

    #[tokio::test]
    async fn refresh_rejects_bad_signature_and_keeps_previous() {
        let dir = tempfile::tempdir().unwrap();
        let (kp, pk) = keypair();
        let good = body(2, "2027-06-01", "", &[]);
        let source = Fake(StdMutex::new(Some((
            good.clone().into_bytes(),
            sign(&kp, &good),
        ))));
        let out = refresh_with(&source, dir.path(), &[pk], today(), false).await;
        assert_eq!(out, RefreshOutcome::Updated(2));
        assert_eq!(
            refresh_with(&source, dir.path(), &[pk], today(), false).await,
            RefreshOutcome::Unchanged
        );
        // Edited after signing: rejected, previous copy untouched.
        let tampered = body(3, "2027-06-01", "", &[]);
        *source.0.lock().unwrap() = Some((tampered.into_bytes(), sign(&kp, &good)));
        let out = refresh_with(&source, dir.path(), &[pk], today(), false).await;
        assert!(matches!(out, RefreshOutcome::Rejected(_)), "{out:?}");
        let cached = remote_config::load_cached(dir.path(), &[pk], today()).unwrap();
        assert_eq!(cached.version, 2);
        let state = std::fs::read_to_string(dir.path().join("remote-config.state")).unwrap();
        assert!(state.contains("outcome = \"rejected\""), "{state}");
        // A failed fetch is reported, not fatal.
        *source.0.lock().unwrap() = None;
        let out = refresh_with(&source, dir.path(), &[pk], today(), false).await;
        assert!(matches!(out, RefreshOutcome::FetchFailed(_)));
    }

    #[tokio::test]
    async fn pinned_reranker_is_never_moved_or_deleted() {
        let remote = parse(
            &body(
                1,
                "2027-06-01",
                "reranker = \"new-rr\"",
                &[("old-rr", "reranker"), ("new-rr", "reranker")],
            ),
            false,
        );
        let env = env(&remote, None);
        install(&env.manager, "old-rr");
        install(&env.manager, "new-rr");
        set_meta(&conn(&env), "reranker_in_use", "old-rr");
        set_meta(&conn(&env), "reranker_in_use_default", "0");
        let config = Config {
            reranker: Some("old-rr".into()),
            ..Config::default()
        };
        follow(
            &config,
            Some(&remote),
            &env.manager,
            today(),
            &env.state,
            &fake_loader,
        )
        .await
        .unwrap();
        assert!(env.manager.is_installed("old-rr"));
        assert!(env.manager.is_installed("new-rr"));
        assert_eq!(
            meta(&conn(&env), "reranker_in_use").as_deref(),
            Some("old-rr")
        );
    }

    #[tokio::test]
    async fn default_reranker_change_installs_new_then_removes_old() {
        let remote = parse(
            &body(
                1,
                "2027-06-01",
                "reranker = \"new-rr\"",
                &[("old-rr", "reranker"), ("new-rr", "reranker")],
            ),
            false,
        );
        let env = env(&remote, None);
        install(&env.manager, "old-rr");
        install(&env.manager, "new-rr");
        set_meta(&conn(&env), "reranker_in_use", "old-rr");
        set_meta(&conn(&env), "reranker_in_use_default", "1");
        follow(
            &Config::default(),
            Some(&remote),
            &env.manager,
            today(),
            &env.state,
            &fake_loader,
        )
        .await
        .unwrap();
        assert!(!env.manager.model_dir("old-rr").exists());
        assert!(env.manager.is_installed("new-rr"));
        assert_eq!(
            meta(&conn(&env), "reranker_in_use").as_deref(),
            Some("new-rr")
        );
    }

    fn indexed(env: &Env, old: &str) -> i64 {
        let root = env._dir.path().join("docs");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.txt"), "alpha zebra giraffe").unwrap();
        std::fs::write(root.join("b.txt"), "beta zebra okapi").unwrap();
        let indexer = Indexer::new(conn(env), Some(named(old))).unwrap();
        indexer.index_roots(&[root]).unwrap();
        count(&conn(env), "SELECT COUNT(*) FROM chunks")
    }

    fn pin_rr() -> Config {
        // A pinned, installed reranker keeps these tests off the network.
        Config {
            reranker: Some("pin-rr".into()),
            ..Config::default()
        }
    }

    #[tokio::test]
    async fn embedder_switch_reembeds_and_keeps_keyword_search() {
        let remote = parse(
            &body(
                1,
                "2027-06-01",
                "embedder = \"new-emb\"",
                &[("new-emb", "embedder"), ("pin-rr", "reranker")],
            ),
            false,
        );
        let env = env(&remote, Some(named(BUILTIN_EMBEDDER)));
        install(&env.manager, "new-emb");
        install(&env.manager, "pin-rr");
        let old_dir = env.manager.model_dir(BUILTIN_EMBEDDER);
        std::fs::create_dir_all(&old_dir).unwrap();
        let chunks = indexed(&env, BUILTIN_EMBEDDER);
        assert!(chunks >= 2);

        follow(
            &pin_rr(),
            Some(&remote),
            &env.manager,
            today(),
            &env.state,
            &fake_loader,
        )
        .await
        .unwrap();

        let c = conn(&env);
        assert_eq!(
            c.stored_embedding_model().unwrap().as_deref(),
            Some("new-emb")
        );
        assert_eq!(count(&c, "SELECT COUNT(*) FROM chunks"), chunks);
        assert_eq!(count(&c, "SELECT COUNT(*) FROM chunks_vec"), chunks);
        assert_eq!(
            count(
                &c,
                "SELECT COUNT(*) FROM chunks_fts WHERE chunks_fts MATCH 'zebra'"
            ),
            chunks
        );
        assert!(!reembed_pending(&c).unwrap());
        assert_eq!(
            env.state.embedder.read().await.as_ref().unwrap().model_id(),
            "new-emb"
        );
        assert!(crate::bootstrap::setup_status(&env.state.paths).is_none());
        assert!(!old_dir.exists(), "old default embedder is removed");
    }

    #[tokio::test]
    async fn pinned_embedder_is_followed_but_its_predecessor_is_not_removed() {
        // Guard for the removal rule: only a remote-default switch deletes.
        let remote = parse(
            &body(
                1,
                "2027-06-01",
                "",
                &[("new-emb", "embedder"), ("pin-rr", "reranker")],
            ),
            false,
        );
        let env = env(&remote, Some(named(BUILTIN_EMBEDDER)));
        install(&env.manager, "new-emb");
        install(&env.manager, "pin-rr");
        let old_dir = env.manager.model_dir(BUILTIN_EMBEDDER);
        std::fs::create_dir_all(&old_dir).unwrap();
        indexed(&env, BUILTIN_EMBEDDER);
        let config = Config {
            embedder: Some("new-emb".into()),
            ..pin_rr()
        };
        follow(
            &config,
            Some(&remote),
            &env.manager,
            today(),
            &env.state,
            &fake_loader,
        )
        .await
        .unwrap();
        assert_eq!(
            conn(&env).stored_embedding_model().unwrap().as_deref(),
            Some("new-emb")
        );
        assert!(old_dir.exists());
    }

    #[tokio::test]
    async fn expired_config_never_switches_embedder() {
        let remote = parse(
            &body(
                1,
                "2026-01-01",
                "embedder = \"new-emb\"",
                &[("new-emb", "embedder"), ("pin-rr", "reranker")],
            ),
            true,
        );
        assert!(remote.is_expired(today()));
        let env = env(&remote, Some(named("old-emb")));
        install(&env.manager, "new-emb");
        install(&env.manager, "pin-rr");
        let chunks = indexed(&env, "old-emb");
        follow(
            &pin_rr(),
            Some(&remote),
            &env.manager,
            today(),
            &env.state,
            &fake_loader,
        )
        .await
        .unwrap();
        let c = conn(&env);
        assert_eq!(
            c.stored_embedding_model().unwrap().as_deref(),
            Some("old-emb")
        );
        assert_eq!(count(&c, "SELECT COUNT(*) FROM chunks_vec"), chunks);
    }

    #[tokio::test]
    async fn an_interrupted_reembed_resumes_on_the_next_follow() {
        let remote = parse(&body(1, "2027-06-01", "", &[("pin-rr", "reranker")]), false);
        let env = env(&remote, Some(named("new-emb")));
        install(&env.manager, "pin-rr");
        let chunks = indexed(&env, "old-emb");
        // Opening with another model leaves chunks without vectors and the flag set.
        drop(Indexer::new(conn(&env), Some(named("new-emb"))).unwrap());
        assert!(reembed_pending(&conn(&env)).unwrap());
        follow(
            &pin_rr(),
            Some(&remote),
            &env.manager,
            today(),
            &env.state,
            &fake_loader,
        )
        .await
        .unwrap();
        let c = conn(&env);
        assert!(!reembed_pending(&c).unwrap());
        assert_eq!(count(&c, "SELECT COUNT(*) FROM chunks_vec"), chunks);
    }
}
