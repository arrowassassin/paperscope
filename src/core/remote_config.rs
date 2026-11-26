//! Signed remote config: parse, validate, verify, cache. Pure (no network).
//!
//! `verify_and_parse` is the only constructor, so a `RemoteConfig` value has
//! always passed the signature check and every validation rule.

use crate::core::models::{all_specs, ModelKind};
use base64::Engine;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

pub const SUPPORTED_SCHEMA: u32 = 1;
pub const ALLOWED_ORGS: &[&str] = &["BAAI", "intfloat", "cross-encoder", "sentence-transformers"];
pub const MAX_BYTES: usize = 64 * 1024;

const CONFIG_FILE: &str = "remote-config.toml";
const SIG_FILE: &str = "remote-config.toml.sig";
const EMBEDDER_ARCH: &str = "bert";
const RERANKER_ARCH: &str = "xlm-roberta-seq-cls";

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RemoteConfig {
    pub schema: u32,
    pub version: u64,
    pub expires: NaiveDate,
    #[serde(default)]
    pub defaults: RemoteDefaults,
    #[serde(default)]
    pub search: RemoteSearch,
    #[serde(default)]
    pub models: Vec<RemoteModel>,
    #[serde(default)]
    pub deprecated: Vec<Deprecation>,
    #[serde(default)]
    pub update: Option<RemoteUpdate>,
}

/// The release the daemon may install (`[update]`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RemoteUpdate {
    pub version: String,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct RemoteDefaults {
    pub embedder: Option<String>,
    pub reranker: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct RemoteSearch {
    pub rerank_top: Option<usize>,
    pub semantic_min_score: Option<f32>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RemoteModel {
    pub id: String,
    /// "embedder" | "reranker"
    pub kind: String,
    pub arch: String,
    pub repo: String,
    pub revision: String,
    /// "en" | "en-zh" | "multilingual"
    pub languages: String,
    pub dims: Option<usize>,
    /// Embedders only; when absent the prefixes follow the Hugging Face org.
    #[serde(default)]
    pub query_prefix: Option<String>,
    #[serde(default)]
    pub document_prefix: Option<String>,
    pub files: Vec<RemoteFile>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RemoteFile {
    pub name: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Deprecation {
    pub id: String,
    pub message: String,
    pub replacement: Option<String>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum RemoteConfigError {
    #[error("remote config is larger than {MAX_BYTES} bytes")]
    TooLarge,
    #[error("remote config signature does not verify")]
    BadSignature,
    #[error("remote config is not valid TOML: {0}")]
    Parse(String),
    #[error("unsupported remote config schema {0}")]
    UnsupportedSchema(u32),
    #[error("remote config version {got} is not newer than {have}")]
    NotNewer { got: u64, have: u64 },
    #[error("remote config has expired")]
    Expired,
    #[error("invalid remote config: {0}")]
    Invalid(String),
}

impl RemoteConfig {
    pub fn is_expired(&self, today: NaiveDate) -> bool {
        today > self.expires
    }
}

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn invalid<T>(msg: impl Into<String>) -> Result<T, RemoteConfigError> {
    Err(RemoteConfigError::Invalid(msg.into()))
}

/// Verify the signature, parse, and validate (spec §2). `allow_expired` is true
/// only when re-reading the saved copy; fetch passes false.
pub fn verify_and_parse(
    toml_bytes: &[u8],
    sig_b64: &str,
    keys: &[[u8; 32]],
    previous_version: Option<u64>,
    today: NaiveDate,
    allow_expired: bool,
) -> Result<RemoteConfig, RemoteConfigError> {
    use RemoteConfigError::*;
    if toml_bytes.len() > MAX_BYTES {
        return Err(TooLarge);
    }
    let sig = base64::engine::general_purpose::STANDARD
        .decode(sig_b64.trim())
        .map_err(|_| BadSignature)?;
    let signed = keys.iter().any(|k| {
        ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, k)
            .verify(toml_bytes, &sig)
            .is_ok()
    });
    if !signed {
        return Err(BadSignature);
    }
    let text = std::str::from_utf8(toml_bytes).map_err(|e| Parse(e.to_string()))?;
    let cfg: RemoteConfig = toml::from_str(text).map_err(|e| Parse(e.to_string()))?;
    if cfg.schema != SUPPORTED_SCHEMA {
        return Err(UnsupportedSchema(cfg.schema));
    }
    if let Some(have) = previous_version {
        if cfg.version <= have {
            return Err(NotNewer {
                got: cfg.version,
                have,
            });
        }
    }
    if !allow_expired && cfg.is_expired(today) {
        return Err(Expired);
    }
    validate(&cfg)?;
    Ok(cfg)
}

fn validate(cfg: &RemoteConfig) -> Result<(), RemoteConfigError> {
    if let Some(u) = &cfg.update {
        let parts: Vec<&str> = u.version.split('.').collect();
        let digits = |p: &&str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
        if parts.len() != 3 || !parts.iter().all(digits) {
            return invalid(format!(
                "update version '{}' is not MAJOR.MINOR.PATCH",
                u.version
            ));
        }
        if u.notes.as_ref().is_some_and(|n| n.chars().count() > 200) {
            return invalid("update notes are longer than 200 chars");
        }
    }
    let builtin = all_specs();
    let mut known: Vec<(String, ModelKind)> =
        builtin.iter().map(|s| (s.id.to_string(), s.kind)).collect();
    for m in &cfg.models {
        if builtin.iter().any(|s| s.id == m.id) || known.iter().any(|(id, _)| *id == m.id) {
            return invalid(format!(
                "model id '{}' collides with an existing model",
                m.id
            ));
        }
        let kind = match m.kind.as_str() {
            "embedder" => ModelKind::Embedding,
            "reranker" => ModelKind::Reranker,
            k => return invalid(format!("model '{}': unknown kind '{k}'", m.id)),
        };
        match kind {
            ModelKind::Embedding if m.arch != EMBEDDER_ARCH || m.dims != Some(384) => {
                return invalid(format!(
                    "model '{}': embedders need arch \"{EMBEDDER_ARCH}\" and dims = 384",
                    m.id
                ));
            }
            ModelKind::Reranker if m.arch != RERANKER_ARCH => {
                return invalid(format!(
                    "model '{}': rerankers need arch \"{RERANKER_ARCH}\"",
                    m.id
                ));
            }
            _ => {}
        }
        let org_ok = m.repo.split_once('/').is_some_and(|(org, name)| {
            ALLOWED_ORGS.contains(&org) && !name.is_empty() && !name.contains('/')
        });
        if !org_ok {
            return invalid(format!(
                "model '{}': repo '{}' is not on the allowlist",
                m.id, m.repo
            ));
        }
        for (name, p) in [
            ("query_prefix", &m.query_prefix),
            ("document_prefix", &m.document_prefix),
        ] {
            if p.as_ref().is_some_and(|p| p.chars().count() > 64) {
                return invalid(format!("model '{}': {name} is longer than 64 chars", m.id));
            }
        }
        if !is_hex(&m.revision, 40) {
            return invalid(format!(
                "model '{}': revision must be a 40-hex commit",
                m.id
            ));
        }
        if !["en", "en-zh", "multilingual"].contains(&m.languages.as_str()) {
            return invalid(format!(
                "model '{}': unknown languages '{}'",
                m.id, m.languages
            ));
        }
        if m.files.is_empty() {
            return invalid(format!("model '{}': no files", m.id));
        }
        for f in &m.files {
            let bad_name =
                f.name.is_empty() || f.name.contains(['/', '\\']) || f.name.contains("..");
            if bad_name || !is_hex(&f.sha256, 64) || f.size == 0 {
                return invalid(format!("model '{}': bad file entry '{}'", m.id, f.name));
            }
        }
        known.push((m.id.clone(), kind));
    }
    let kind_of = |id: &str| known.iter().find(|(k, _)| k == id).map(|(_, kind)| *kind);
    for (what, id, want) in [
        ("embedder", &cfg.defaults.embedder, ModelKind::Embedding),
        ("reranker", &cfg.defaults.reranker, ModelKind::Reranker),
    ] {
        if let Some(id) = id {
            if kind_of(id) != Some(want) {
                return invalid(format!("default {what} '{id}' is not a known {what}"));
            }
        }
    }
    for d in &cfg.deprecated {
        if let Some(r) = &d.replacement {
            let Some(rk) = kind_of(r) else {
                return invalid(format!("replacement '{r}' is not a known model"));
            };
            if kind_of(&d.id).is_some_and(|k| k != rk) {
                return invalid(format!(
                    "replacement '{r}' is the wrong kind for '{}'",
                    d.id
                ));
            }
        }
    }
    if cfg
        .search
        .rerank_top
        .is_some_and(|n| !(1..=100).contains(&n))
    {
        return invalid("rerank_top must be within 1..=100");
    }
    if cfg
        .search
        .semantic_min_score
        .is_some_and(|s| !(0.0..=1.0).contains(&s))
    {
        return invalid("semantic_min_score must be within 0.0..=1.0");
    }
    Ok(())
}

fn parse_key(s: &str) -> Option<[u8; 32]> {
    hex::decode(s.trim()).ok()?.try_into().ok()
}

/// Compiled-in keys (blank file = none) and, in debug builds only,
/// `PAPER_REMOTE_CONFIG_TEST_PUBKEY`. Rotating a key takes a release: nothing on
/// disk can add a trusted key.
pub fn compiled_keys() -> Vec<[u8; 32]> {
    let mut keys: Vec<[u8; 32]> = [
        include_str!("../../config/keys/primary.pub"),
        include_str!("../../config/keys/backup.pub"),
    ]
    .iter()
    .filter_map(|s| parse_key(s))
    .collect();
    if cfg!(debug_assertions) {
        keys.extend(
            std::env::var("PAPER_REMOTE_CONFIG_TEST_PUBKEY")
                .ok()
                .and_then(|s| parse_key(&s)),
        );
    }
    keys
}

pub(crate) fn write_atomic(dir: &Path, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    let mut temp = tempfile::Builder::new()
        .prefix(".remote-config-")
        .suffix(".tmp")
        .tempfile_in(dir)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(dir.join(name)).map_err(|e| e.error)?;
    Ok(())
}

/// Save an already-verified pair. The reader re-verifies, so a torn pair is
/// harmless: it just fails to load.
pub fn save_verified(dir: &Path, toml_bytes: &[u8], sig_b64: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    write_atomic(dir, CONFIG_FILE, toml_bytes)?;
    write_atomic(dir, SIG_FILE, sig_b64.as_bytes())
}

/// Re-verify the saved copy. An expired copy is returned; callers check `is_expired`.
pub fn load_cached(dir: &Path, keys: &[[u8; 32]], today: NaiveDate) -> Option<RemoteConfig> {
    let toml_bytes = std::fs::read(dir.join(CONFIG_FILE)).ok()?;
    let sig = std::fs::read_to_string(dir.join(SIG_FILE)).ok()?;
    verify_and_parse(&toml_bytes, &sig, keys, None, today, true).ok()
}

const STATE_FILE: &str = "remote-config.state";

/// Last fetch outcome, written by the daemon and read by doctor and MCP.
/// `outcome` is "updated" | "unchanged" | "rejected" | "fetch_failed" | "offline".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FetchState {
    pub outcome: String,
    #[serde(default)]
    pub detail: Option<String>,
    /// RFC 3339 timestamp.
    pub at: String,
}

/// Best effort: missing or garbled means `None`.
pub fn read_fetch_state(dir: &Path) -> Option<FetchState> {
    toml::from_str(&std::fs::read_to_string(dir.join(STATE_FILE)).ok()?).ok()
}

pub fn write_fetch_state(dir: &Path, state: &FetchState) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let body = toml::to_string(state).map_err(std::io::Error::other)?;
    write_atomic(dir, STATE_FILE, body.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    const REV: &str = "0123456789abcdef0123456789abcdef01234567";
    const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

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

    fn builtin(kind: ModelKind) -> String {
        all_specs()
            .iter()
            .find(|s| s.kind == kind)
            .unwrap()
            .id
            .to_string()
    }

    fn base(extra: &str) -> String {
        format!("schema = 1\nversion = 7\nexpires = \"2027-06-01\"\n{extra}\n")
    }

    fn model(
        id: &str,
        kind: &str,
        arch: &str,
        repo: &str,
        rev: &str,
        sha: &str,
        dims: &str,
    ) -> String {
        format!(
            "[[models]]\nid = \"{id}\"\nkind = \"{kind}\"\narch = \"{arch}\"\nrepo = \"{repo}\"\nrevision = \"{rev}\"\nlanguages = \"multilingual\"\n{dims}\nfiles = [ {{ name = \"model.safetensors\", sha256 = \"{sha}\", size = 10 }} ]\n"
        )
    }

    fn good_reranker() -> String {
        model(
            "new-rr",
            "reranker",
            RERANKER_ARCH,
            "BAAI/new-rr",
            REV,
            SHA,
            "",
        )
    }

    /// Signs `body` with a fresh key and runs fetch-style verification.
    fn check(body: &str) -> Result<RemoteConfig, RemoteConfigError> {
        let (kp, public) = keypair();
        verify_and_parse(
            body.as_bytes(),
            &sign(&kp, body),
            &[public],
            None,
            today(),
            false,
        )
    }

    fn assert_invalid(body: &str) {
        assert!(
            matches!(check(body), Err(RemoteConfigError::Invalid(_))),
            "{body}"
        );
    }

    #[test]
    fn accepts_a_valid_signed_config() {
        let body = base(&format!(
            "[defaults]\nembedder = \"{}\"\nreranker = \"new-rr\"\n[search]\nrerank_top = 20\nsemantic_min_score = 0.25\n{}",
            builtin(ModelKind::Embedding),
            good_reranker()
        ));
        let cfg = check(&body).unwrap();
        assert_eq!(cfg.version, 7);
        assert_eq!(cfg.models[0].id, "new-rr");
        assert_eq!(cfg.search.rerank_top, Some(20));
        assert!(!cfg.is_expired(today()));
    }

    #[test]
    fn rejects_a_config_modified_after_signing() {
        let (kp, public) = keypair();
        let body = base("[search]\nrerank_top = 20\n");
        let sig = sign(&kp, &body);
        let tampered = body.replace("20", "30");
        let r = verify_and_parse(tampered.as_bytes(), &sig, &[public], None, today(), false);
        assert_eq!(r, Err(RemoteConfigError::BadSignature));
    }

    #[test]
    fn rejects_a_signature_from_an_unknown_key() {
        let (kp, _) = keypair();
        let (_, other) = keypair();
        let body = base("");
        let r = verify_and_parse(
            body.as_bytes(),
            &sign(&kp, &body),
            &[other],
            None,
            today(),
            false,
        );
        assert_eq!(r, Err(RemoteConfigError::BadSignature));
    }

    #[test]
    fn ignores_a_validly_signed_but_older_version() {
        let (kp, public) = keypair();
        let body = base("");
        let r = verify_and_parse(
            body.as_bytes(),
            &sign(&kp, &body),
            &[public],
            Some(9),
            today(),
            false,
        );
        assert_eq!(r, Err(RemoteConfigError::NotNewer { got: 7, have: 9 }));
        let same = verify_and_parse(
            body.as_bytes(),
            &sign(&kp, &body),
            &[public],
            Some(7),
            today(),
            false,
        );
        assert!(matches!(same, Err(RemoteConfigError::NotNewer { .. })));
    }

    #[test]
    fn rejects_expired_on_fetch_but_load_cached_returns_it_marked_expired() {
        let (kp, public) = keypair();
        let body = base("");
        let sig = sign(&kp, &body);
        let late = NaiveDate::from_ymd_opt(2027, 6, 2).unwrap();
        let r = verify_and_parse(body.as_bytes(), &sig, &[public], None, late, false);
        assert_eq!(r, Err(RemoteConfigError::Expired));
        let dir = tempfile::tempdir().unwrap();
        save_verified(dir.path(), body.as_bytes(), &sig).unwrap();
        let cached = load_cached(dir.path(), &[public], late).unwrap();
        assert!(cached.is_expired(late));
    }

    #[test]
    fn rejects_repo_outside_allowed_orgs() {
        assert_invalid(&base(&model(
            "new-rr",
            "reranker",
            RERANKER_ARCH,
            "evil/new-rr",
            REV,
            SHA,
            "",
        )));
        assert_invalid(&base(&model(
            "new-rr",
            "reranker",
            RERANKER_ARCH,
            "BAAI",
            REV,
            SHA,
            "",
        )));
    }

    #[test]
    fn rejects_unsupported_arch_and_non_384_embedder() {
        assert_invalid(&base(&model(
            "new-rr", "reranker", "llama", "BAAI/x", REV, SHA, "",
        )));
        assert_invalid(&base(&model(
            "new-e",
            "embedder",
            "bert",
            "BAAI/x",
            REV,
            SHA,
            "dims = 768",
        )));
        assert_invalid(&base(&model(
            "new-e", "embedder", "bert", "BAAI/x", REV, SHA, "",
        )));
        assert!(check(&base(&model(
            "new-e",
            "embedder",
            "bert",
            "BAAI/x",
            REV,
            SHA,
            "dims = 384"
        )))
        .is_ok());
    }

    #[test]
    fn rejects_id_colliding_with_builtin() {
        let id = builtin(ModelKind::Reranker);
        assert_invalid(&base(&model(
            &id,
            "reranker",
            RERANKER_ARCH,
            "BAAI/x",
            REV,
            SHA,
            "",
        )));
    }

    #[test]
    fn rejects_non_hex_revision_or_sha() {
        let short = &REV[..39];
        assert_invalid(&base(&model(
            "n",
            "reranker",
            RERANKER_ARCH,
            "BAAI/x",
            short,
            SHA,
            "",
        )));
        assert_invalid(&base(&model(
            "n",
            "reranker",
            RERANKER_ARCH,
            "BAAI/x",
            &REV.replace('0', "z"),
            SHA,
            "",
        )));
        assert_invalid(&base(&model(
            "n",
            "reranker",
            RERANKER_ARCH,
            "BAAI/x",
            REV,
            &SHA[..63],
            "",
        )));
        assert_invalid(&base(&model(
            "n",
            "reranker",
            RERANKER_ARCH,
            "BAAI/x",
            REV,
            &SHA.replace('0', "g"),
            "",
        )));
    }

    #[test]
    fn rejects_default_naming_unknown_model_or_wrong_kind() {
        assert_invalid(&base("[defaults]\nreranker = \"nope\"\n"));
        let embedder = builtin(ModelKind::Embedding);
        assert_invalid(&base(&format!("[defaults]\nreranker = \"{embedder}\"\n")));
        assert_invalid(&base(
            "[[deprecated]]\nid = \"x\"\nmessage = \"m\"\nreplacement = \"nope\"\n",
        ));
    }

    #[test]
    fn rejects_search_values_out_of_bounds() {
        for s in [
            "rerank_top = 0",
            "rerank_top = 101",
            "semantic_min_score = 1.5",
            "semantic_min_score = -0.1",
        ] {
            assert_invalid(&base(&format!("[search]\n{s}\n")));
        }
    }

    #[test]
    fn rejects_oversized_file() {
        let body = base(&format!("# {}\n", "x".repeat(MAX_BYTES)));
        assert_eq!(check(&body), Err(RemoteConfigError::TooLarge));
    }

    #[test]
    fn save_then_load_cached_round_trips() {
        let (kp, public) = keypair();
        let body = base(&good_reranker());
        let sig = sign(&kp, &body);
        let dir = tempfile::tempdir().unwrap();
        save_verified(dir.path(), body.as_bytes(), &sig).unwrap();
        let cached = load_cached(dir.path(), &[public], today()).unwrap();
        let direct =
            verify_and_parse(body.as_bytes(), &sig, &[public], None, today(), false).unwrap();
        assert_eq!(cached, direct);
        let (_, other) = keypair();
        assert!(load_cached(dir.path(), &[other], today()).is_none());
    }

    #[test]
    fn malformed_test_pubkey_env_is_ignored_and_blank_files_mean_no_key() {
        assert_eq!(parse_key("  \n"), None);
        assert_eq!(parse_key("abc"), None);
        assert_eq!(parse_key(&"zz".repeat(32)), None);
        assert!(parse_key(&"ab".repeat(32)).is_some());
        let before = compiled_keys();
        std::env::set_var("PAPER_REMOTE_CONFIG_TEST_PUBKEY", "not-hex");
        let after = compiled_keys();
        std::env::remove_var("PAPER_REMOTE_CONFIG_TEST_PUBKEY");
        assert_eq!(before, after);
    }

    #[test]
    fn update_section_parses_and_validates() {
        let ok = check(&base("[update]\nversion = \"0.4.2\"\nnotes = \"hi\"")).unwrap();
        assert_eq!(
            ok.update,
            Some(RemoteUpdate {
                version: "0.4.2".into(),
                notes: Some("hi".into())
            })
        );
        assert!(check(&base("")).unwrap().update.is_none());
        for v in ["latest", "0.4", "v0.4.2", "0.4.2-beta", "0.4.", "1.2.3.4"] {
            assert_invalid(&base(&format!("[update]\nversion = \"{v}\"")));
        }
        let long = "x".repeat(201);
        assert_invalid(&base(&format!(
            "[update]\nversion = \"0.4.2\"\nnotes = \"{long}\""
        )));
    }

    #[test]
    fn fetch_state_round_trips_and_garbage_reads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_fetch_state(dir.path()), None);
        let st = FetchState {
            outcome: "rejected".into(),
            detail: Some("bad \"signature\"".into()),
            at: "2026-11-25T10:00:00+00:00".into(),
        };
        write_fetch_state(dir.path(), &st).unwrap();
        assert_eq!(read_fetch_state(dir.path()), Some(st));
        std::fs::write(dir.path().join(STATE_FILE), "not [ toml").unwrap();
        assert_eq!(read_fetch_state(dir.path()), None);
    }

    #[test]
    fn model_prefixes_are_optional_and_capped_at_64_chars() {
        let with = |extra: &str| {
            base(&format!(
                "[[models]]\nid=\"p-emb\"\nkind=\"embedder\"\narch=\"bert\"\ndims=384\nrepo=\"intfloat/p\"\nrevision=\"{REV}\"\nlanguages=\"multilingual\"\n{extra}\nfiles=[{{name=\"model.safetensors\",sha256=\"{SHA}\",size=10}}]\n"
            ))
        };
        let ok = check(&with("query_prefix=\"q: \"\ndocument_prefix=\"d: \"")).unwrap();
        assert_eq!(ok.models[0].query_prefix.as_deref(), Some("q: "));
        assert_eq!(ok.models[0].document_prefix.as_deref(), Some("d: "));
        assert!(check(&with("")).unwrap().models[0].query_prefix.is_none());
        let long = "x".repeat(65);
        assert_invalid(&with(&format!("query_prefix=\"{long}\"")));
        assert_invalid(&with(&format!("document_prefix=\"{long}\"")));
    }
}
