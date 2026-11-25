//! Which models are in effect, and why. Precedence: local pin > remote default
//! > built-in (spec §4).

use super::config::Config;
use super::models::{MMARCO_RERANKER, MULTILINGUAL_EMBEDDING};
use super::remote_config::{Deprecation, RemoteConfig};
use chrono::NaiveDate;

/// The one place the built-in defaults live.
pub const BUILTIN_EMBEDDER: &str = MULTILINGUAL_EMBEDDING;
pub const BUILTIN_RERANKER: &str = MMARCO_RERANKER;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Pinned,
    Remote,
    BuiltIn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effective {
    pub embedder: String,
    pub reranker: String,
    pub embedder_source: Source,
    pub reranker_source: Source,
}

fn pick(pin: &Option<String>, remote: Option<&String>, builtin: &str) -> (String, Source) {
    match (pin, remote) {
        (Some(id), _) => (id.clone(), Source::Pinned),
        (None, Some(id)) => (id.clone(), Source::Remote),
        (None, None) => (builtin.to_string(), Source::BuiltIn),
    }
}

/// An expired remote config contributes nothing.
pub fn resolve(config: &Config, remote: Option<&RemoteConfig>, today: NaiveDate) -> Effective {
    let remote = remote.filter(|r| !r.is_expired(today));
    let (embedder, embedder_source) = pick(
        &config.embedder,
        remote.and_then(|r| r.defaults.embedder.as_ref()),
        BUILTIN_EMBEDDER,
    );
    let (reranker, reranker_source) = pick(
        &config.reranker,
        remote.and_then(|r| r.defaults.reranker.as_ref()),
        BUILTIN_RERANKER,
    );
    Effective {
        embedder,
        reranker,
        embedder_source,
        reranker_source,
    }
}

/// The saved, signature-verified remote config if it is still valid today.
/// Reads only local files; never touches the network.
pub fn load_live_remote(today: NaiveDate) -> Option<RemoteConfig> {
    let dir = super::paths::Paths::new().base_dir();
    super::remote_config::load_cached(&dir, &super::remote_config::compiled_keys(), today)
        .filter(|r| !r.is_expired(today))
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Pinned => "pinned",
            Source::Remote => "remote",
            Source::BuiltIn => "built-in",
        }
    }
}

/// Deprecations naming the embedder or reranker actually in use.
pub fn deprecations_in_use<'a>(
    remote: Option<&'a RemoteConfig>,
    effective: &Effective,
) -> Vec<&'a Deprecation> {
    remote
        .into_iter()
        .flat_map(|r| &r.deprecated)
        .filter(|d| d.id == effective.embedder || d.id == effective.reranker)
        .collect()
}

/// The CLI notice: one stderr line per deprecated in-use model.
pub fn note_deprecations(remote: Option<&RemoteConfig>, effective: &Effective) {
    for d in deprecations_in_use(remote, effective) {
        eprintln!("note: {} is deprecated: {}", d.id, d.message);
    }
}
