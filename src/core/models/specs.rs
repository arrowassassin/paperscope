use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelKind {
    Embedding,
    Reranker,
}

/// One downloadable artifact, pinned by content hash.
#[derive(Debug, Clone)]
pub struct ModelFile {
    pub name: &'static str,
    pub sha256: &'static str,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct ModelSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: ModelKind,
    /// First-party Hugging Face repository. Only ungated repositories are used:
    /// installing Paper never requires an account or a token.
    pub repo: &'static str,
    /// Pinned commit, so a re-upload upstream cannot change what we run.
    pub revision: &'static str,
    pub files: &'static [ModelFile],
    pub license: &'static str,
    pub languages: &'static str,
    /// Embedding width. `0` for rerankers, which produce a scalar.
    pub dims: usize,
    pub max_tokens: usize,
    /// Retrieval instruction prefixes. BGE wants one on queries only; E5 wants
    /// them on both sides. Getting these wrong measurably hurts recall.
    pub query_prefix: &'static str,
    pub document_prefix: &'static str,
}

impl ModelSpec {
    pub fn size_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }

    pub fn size_mb(&self) -> u64 {
        self.size_bytes() / 1_000_000
    }

    pub fn file(&self, name: &str) -> Option<&ModelFile> {
        self.files.iter().find(|f| f.name == name)
    }
}

/// Default: small, fast, English. 134 MB total.
pub const DEFAULT_EMBEDDING: &str = "bge-small-en-v1.5";
/// Opt-in for multilingual households. 488 MB total.
pub const MULTILINGUAL_EMBEDDING: &str = "multilingual-e5-small";
/// Opt-in cross-encoder for precision. 1.13 GB total, multilingual.
pub const OPTIONAL_RERANKER: &str = "bge-reranker-base";

/// Width of the vectors in `chunks_vec`. Both supported embedders are 384-dim,
/// so one index can be rebuilt from either without a schema change.
pub const EMBEDDING_DIMS: usize = 384;

const BGE_SMALL_FILES: &[ModelFile] = &[
    ModelFile {
        name: "model.safetensors",
        sha256: "3c9f31665447c8911517620762200d2245a2518d6e7208acc78cd9db317e21ad",
        size: 133_466_304,
    },
    ModelFile {
        name: "config.json",
        sha256: "094f8e891b932f2000c92cfc663bac4c62069f5d8af5b5278c4306aef3084750",
        size: 743,
    },
    ModelFile {
        name: "tokenizer.json",
        sha256: "d241a60d5e8f04cc1b2b3e9ef7a4921b27bf526d9f6050ab90f9267a1f9e5c66",
        size: 711_396,
    },
];

const E5_SMALL_FILES: &[ModelFile] = &[
    ModelFile {
        name: "model.safetensors",
        sha256: "1a55775f53449dac10a2bcbc312469fac40b96d53198c407081a831f81c98477",
        size: 470_641_600,
    },
    ModelFile {
        name: "config.json",
        sha256: "69137736cab8b8903a07fe8afaafdda25aac55415a12a55d1bffa9f581abf959",
        size: 655,
    },
    ModelFile {
        name: "tokenizer.json",
        sha256: "0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39",
        size: 17_082_730,
    },
];

const BGE_RERANKER_FILES: &[ModelFile] = &[
    ModelFile {
        name: "model.safetensors",
        sha256: "ced967c45fd1902eb92716c9ceeca7c95a936770ea9db611f5a841b926e33fbd",
        size: 1_112_206_140,
    },
    ModelFile {
        name: "config.json",
        sha256: "289adf7ada1eb6b4afa7589a48a032d45a076cf2e46dcdb3b4cabc33be14f708",
        size: 799,
    },
    ModelFile {
        name: "tokenizer.json",
        sha256: "9eb652ac4e40cc093272bbbe0f55d521cf67570060227109b5cdc20945a4489e",
        size: 17_098_107,
    },
];

pub fn all_specs() -> Vec<ModelSpec> {
    vec![
        ModelSpec {
            id: DEFAULT_EMBEDDING,
            name: "BGE small English v1.5 (default, 384 dims)",
            kind: ModelKind::Embedding,
            repo: "BAAI/bge-small-en-v1.5",
            revision: "5c38ec7c405ec4b44b94cc5a9bb96e735b38267a",
            files: BGE_SMALL_FILES,
            license: "MIT",
            languages: "English",
            dims: 384,
            max_tokens: 512,
            query_prefix: "Represent this sentence for searching relevant passages: ",
            document_prefix: "",
        },
        ModelSpec {
            id: MULTILINGUAL_EMBEDDING,
            name: "Multilingual E5 small (opt-in, 384 dims)",
            kind: ModelKind::Embedding,
            repo: "intfloat/multilingual-e5-small",
            revision: "614241f622f53c4eeff9890bdc4f31cfecc418b3",
            files: E5_SMALL_FILES,
            license: "MIT",
            languages: "100+ languages",
            dims: 384,
            max_tokens: 512,
            query_prefix: "query: ",
            document_prefix: "passage: ",
        },
        ModelSpec {
            id: OPTIONAL_RERANKER,
            name: "BGE reranker base (opt-in precision)",
            kind: ModelKind::Reranker,
            repo: "BAAI/bge-reranker-base",
            revision: "2cfc18c9415c912f9d8155881c133215df768a70",
            files: BGE_RERANKER_FILES,
            license: "MIT",
            languages: "Multilingual",
            dims: 0,
            max_tokens: 512,
            query_prefix: "",
            document_prefix: "",
        },
    ]
}

pub fn spec(model_id: &str) -> Option<ModelSpec> {
    all_specs().into_iter().find(|s| s.id == model_id)
}
