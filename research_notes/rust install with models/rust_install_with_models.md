# Rust CLI Tool Distribution with Large Model Files via install.sh

## Executive Summary

This research covers patterns for distributing a Rust binary (paperscope) with optional large model files (134 MB - 1.13 GB) via a `curl | sh` installer. Key findings:

1. **No major LLM tool bundles models in install.sh** — ollama, llama.cpp, koboldcpp all download models separately on first run
2. **cargo-dist supports extra-artifacts** for uploading additional files alongside binaries to GitHub Releases
3. **GitHub Releases supports 1000 assets/release, 2 GiB/file** — sufficient for model files
4. **Resumable downloads use `curl --continue-at -`** with SHA256 verification post-download
5. **Recommended pattern**: Install binary via install.sh → Download models on first run with progress + resumable + verification → Cache for offline use

---

## 1. How Major LLM Tools Distribute Models

### ollama (https://ollama.com/install.sh)

**Pattern**: Install binary only; models downloaded on-demand via `ollama pull <model>`

```bash
# install.sh downloads platform-specific binary (~100-200 MB)
# Models stored in ~/.ollama/models/ (downloaded separately)
# Supports: curl --progress-bar for download progress
# No SHA256 verification in install.sh (binary only)
# Models: GGUF format, architecture-independent
```

**Key observations**:
- Install script is a bootstrapper that downloads `ollama-linux-${ARCH}.tar.zst` or `.tgz`
- Uses `zstd` for extraction if available, falls back to `tar -xzf`
- Model management entirely separate (`ollama pull`, `ollama list`, `ollama rm`)
- Offline mode: `OLLAMA_OFFLINE=1` prevents network calls

### llama.cpp (GitHub Releases)

**Pattern**: Many platform-specific binaries (30+ assets/release); **no models bundled**

```bash
# Nightly release b12359 has 30+ assets:
# - llama-b12359-bin-ubuntu-x64.tar.gz (23 MB)
# - llama-b12359-bin-macos-arm64.tar.gz (17 MB)
# - llama-b12359-bin-win-cuda-12.4-x64.zip (319 MB)
# - etc.
# Each asset has SHA256 digest in GitHub API
# Models: Downloaded separately from Hugging Face (GGUF format)
```

**Key observations**:
- Uses GitHub Releases for binary distribution only
- Assets named with platform + variant: `llama-{tag}-bin-{platform}-{variant}.{tar.gz|zip}`
- Cargo-dist or custom CI uploads all variants
- No install.sh — users download manually or use package managers

### koboldcpp (GitHub Releases)

**Pattern**: Single-file executables per platform; models from Hugging Face

```bash
# Release v1.124.2 has 7 assets:
# - koboldcpp-linux-x64 (629 MB, CUDA)
# - koboldcpp-linux-x64-nocuda (107 MB)
# - koboldcpp-mac-arm64 (68 MB)
# - koboldcpp.exe (623 MB, Windows CUDA)
# - koboldcpp-nocuda.exe (90 MB)
# Models: .kcppt templates + .gguf files from Hugging Face
# Download on first run via UI or --model flag
```

---

## 2. cargo-dist Patterns for Extra Assets

### `extra-artifacts` Configuration

From [cargo-dist config docs](https://axodotdev.github.io/cargo-dist/book/reference/config.html#extra-artifacts):

```toml
# In dist-workspace.toml or Cargo.toml [workspace.metadata.dist]
[[dist.extra-artifacts]]
artifacts = ["models/model-small.bin", "models/model-medium.bin", "models/model-large.bin"]
build = ["scripts/download-models.sh"]  # or copy from local path
```

**Features**:
- Each file in `artifacts` uploaded as **separate GitHub Release asset**
- `build` command runs before packaging (can download/generate files)
- Checksums (SHA256 by default) generated automatically for each asset
- Assets appear alongside platform-specific archives in release

### `include` for Archive Bundling

```toml
[dist]
include = ["models/"]  # Copies entire directory into EVERY platform archive
```

**Trade-off**: Bloats every platform tarball (bad for 1 GB models)

### Installer Generation

cargo-dist generates `myapp-installer.sh` that:
- Detects platform → selects correct archive
- Downloads from `ARTIFACT_DOWNLOAD_URLS` (space-separated fallback URLs)
- Verifies SHA256 checksum from embedded receipt
- Installs to `~/.cargo/bin` or custom `CARGO_DIST_FORCE_INSTALL_DIR`
- Supports `--verbose`/`--quiet` flags

**Limitation**: Current template only downloads **one archive** per platform. No built-in multi-asset download.

---

## 3. Install.sh Patterns from rustup, bun, deno

### Common Patterns

| Tool | Download | Progress | Verification | Resumable | Multi-file |
|------|----------|----------|--------------|-----------|------------|
| rustup | Single binary | `curl --progress-bar` | None in bootstrapper | `curl --retry 3 -C -` | No |
| bun | Single zip | `curl --progress-bar` | None | No | No |
| deno | Single zip | `curl --progress-bar` | None | No | No |
| cargo-dist | Single archive | `curl --progress-bar` | **SHA256 from receipt** | No | No |

### rustup-init.sh Key Techniques

```bash
# Resumable download with retry
check_curl_for_retry_support() {
    if check_help_for "notspecified" "curl" "--retry"; then
        _retry_supported="--retry 3"
        if check_help_for "notspecified" "curl" "--continue-at"; then
            _retry_supported="--retry 3 -C -"  # <-- RESUMABLE
        fi
    fi
}

# Platform detection (comprehensive)
# Falls back from curl to wget
# TLS 1.2 + strong cipher enforcement
```

### cargo-dist installer.sh Key Techniques

```bash
# Download with fallback URLs
for _base_url in $ARTIFACT_DOWNLOAD_URLS; do
    _url="$_base_url/$_artifact_name"
    if ! downloader "$_url" "$_file"; then
        continue  # try next URL
    fi
    # Verify checksum from embedded receipt
    if [ -n "${_checksum_style:-}" ]; then
        verify_checksum "$_file" "$_checksum_style" "$_checksum_value"
    fi
done
```

---

## 4. SHA256 Verification & Resumable Downloads

### Portable SHA256 in Shell

```bash
# Cross-platform (Linux/macOS)
sha256_verify() {
    local file="$1"
    local expected="$2"
    if command -v sha256sum >/dev/null; then
        echo "$expected  $file" | sha256sum -c -
    elif command -v shasum >/dev/null; then
        echo "$expected  $file" | shasum -a 256 -c -
    else
        echo "ERROR: sha256sum or shasum required" >&2
        return 1
    fi
}
```

### Resumable Download with Progress + Verification

```bash
download_with_resume_and_verify() {
    local url="$1"
    local dest="$2"
    local expected_sha256="$3"
    local desc="${4:-file}"

    # Create temp file for atomic write
    local tmp="${dest}.part"
    
    # Resumable download with progress bar
    # -C - : auto-resume from existing file size
    # --retry 3 : retry on failure
    # --progress-bar : visual progress
    curl --fail --location \
         --retry 3 --retry-delay 2 \
         --continue-at - \
         --progress-bar \
         --output "$tmp" \
         "$url" || return 1

    # Verify checksum
    if ! sha256_verify "$tmp" "$expected_sha256"; then
        echo "ERROR: Checksum mismatch for $desc" >&2
        rm -f "$tmp"
        return 1
    fi

    # Atomic move
    mv "$tmp" "$dest"
}
```

### Multi-file Download with Aggregate Progress

```bash
# For multiple large files, show per-file progress + overall
download_models() {
    local models=(
        "model-small.bin|https://.../model-small.bin|sha256_small|134MB"
        "model-medium.bin|https://.../model-medium.bin|sha256_medium|487MB"
        "model-large.bin|https://.../model-large.bin|sha256_large|1130MB"
    )
    
    local total_size=0
    for m in "${models[@]}"; do
        IFS='|' read -r _ _ _ size <<< "$m"
        total_size=$((total_size + ${size%MB} * 1024 * 1024))
    done

    local downloaded=0
    for m in "${models[@]}"; do
        IFS='|' read -r name url sha256 size <<< "$m"
        echo "Downloading $name ($size)..."
        download_with_resume_and_verify "$url" "$MODEL_DIR/$name" "$sha256" "$name"
        downloaded=$((downloaded + ${size%MB} * 1024 * 1024))
        echo "Overall progress: $((downloaded * 100 / total_size))%"
    done
}
```

---

## 5. GitHub Releases for Multi-Asset Distribution

### Limits (from GitHub Docs)

- **1000 assets per release**
- **2 GiB per file** (2,147,483,648 bytes)
- **No total size limit**
- **No bandwidth limit**

### Asset Naming Convention

```
paperscope-v1.0.0-x86_64-unknown-linux-gnu.tar.gz     # Binary archive
paperscope-v1.0.0-aarch64-apple-darwin.tar.gz         # Binary archive
paperscope-v1.0.0-model-small.bin                     # Universal model (134 MB)
paperscope-v1.0.0-model-medium.bin                    # Universal model (487 MB)
paperscope-v1.0.0-model-large.bin                     # Universal model (1.13 GB)
paperscope-v1.0.0-model-small.bin.sha256              # Checksum file
paperscope-v1.0.0-model-medium.bin.sha256
paperscope-v1.0.0-model-large.bin.sha256
```

### cargo-dist Configuration for Models

```toml
# Cargo.toml or dist-workspace.toml
[workspace.metadata.dist]
# ... existing config ...

# Upload model files as extra artifacts (global, not per-platform)
[[extra-artifacts]]
artifacts = [
    "models/model-small.bin",
    "models/model-medium.bin", 
    "models/model-large.bin"
]
build = ["scripts/prepare-models.sh"]  # Downloads/verifies models at build time

# Or if models are pre-downloaded in repo:
# build = ["cp", "models/*.bin", "target/dist/"]
```

### CI Workflow (GitHub Actions)

```yaml
# .github/workflows/release.yml
- uses: axodotdev/cargo-dist@v1
  with:
    # cargo-dist automatically uploads extra-artifacts
    # No extra steps needed
```

---

## 6. Recommended Architecture for paperscope

### Distribution Strategy

**Option A: Hybrid (Recommended)**
1. **install.sh** downloads platform-specific binary via cargo-dist generated installer
2. **First run** downloads selected model(s) with progress + resumable + SHA256
3. **Models cached** in `$PAPERSCOPE_HOME/models/` or `~/.paperscope/models/`
4. **Offline mode**: `PAPER_OFFLINE=1` skips download, uses cached models

**Option B: All-in-Release (Not Recommended)**
- Bundle models in release → bloats every platform download
- 1.13 GB model × 4 platforms = 4.5 GB release size
- Users download models they don't need

### install.sh Stepper UX Design

```
┌─────────────────────────────────────────────────────────┐
│  paperscope installer v1.0.0                            │
├─────────────────────────────────────────────────────────┤
│  [1/4] Detecting platform... linux-x64 ✓               │
│  [2/4] Downloading binary (12 MB)... ████████░░ 85%    │
│  [3/4] Verifying SHA256... ✓                           │
│  [4/4] Installing to ~/.paperscope/bin... ✓            │
├─────────────────────────────────────────────────────────┤
│  Binary installed. Run 'paperscope' to download models. │
└─────────────────────────────────────────────────────────┘
```

**First-run model selection**:
```
$ paperscope
Welcome to paperscope! No models found.

Available models:
  1) small   - 134 MB  - Fast, lower accuracy
  2) medium  - 487 MB  - Balanced (recommended)
  3) large   - 1.13 GB - Highest accuracy

Select model [1-3, or 'all']: 2

Downloading medium model (487 MB)...
████████████████████░░░░░░░░░░░░░░ 62%  2.4 MB/s  ETA: 2m 15s
Verifying SHA256... ✓
Model ready!
```

### Offline Support

```bash
# Environment variable checked at runtime
if [ -n "${PAPER_OFFLINE:-}" ]; then
    # Skip all network calls
    # Use only cached models
    # Error if model not found
fi

# Model directory structure
~/.paperscope/
├── bin/
│   └── paperscope
├── models/
│   ├── model-small.bin
│   ├── model-small.bin.sha256
│   ├── model-medium.bin
│   └── model-medium.bin.sha256
└── receipt.json  # cargo-dist receipt for updates
```

---

## 7. Implementation Checklist

### Cargo.toml / dist-workspace.toml

```toml
[workspace.metadata.dist]
# ... existing config ...

# Extra artifacts for model files
[[extra-artifacts]]
artifacts = [
    "models/model-small.bin",
    "models/model-medium.bin",
    "models/model-large.bin"
]
build = ["scripts/fetch-models.sh"]  # Runs in CI to download & verify

# Ensure checksums for all artifacts
checksum = "sha256"

# Shell installer config
installers = ["shell", "powershell"]
install-path = "~/.paperscope/bin"
install-success-msg = "paperscope installed! Run 'paperscope' to download models."
```

### scripts/fetch-models.sh (runs in CI)

```bash
#!/bin/bash
set -euo pipefail

MODELS=(
    "model-small.bin|https://huggingface.co/.../model-small.bin|sha256_small"
    "model-medium.bin|https://huggingface.co/.../model-medium.bin|sha256_medium"
    "model-large.bin|https://huggingface.co/.../model-large.bin|sha256_large"
)

mkdir -p models

for m in "${MODELS[@]}"; do
    IFS='|' read -r name url sha256 <<< "$m"
    echo "Fetching $name..."
    curl -fL --retry 3 -o "models/$name" "$url"
    echo "$sha256  models/$name" | sha256sum -c -
done
```

### Custom install.sh (extends cargo-dist template)

Add model download step after binary install:

```bash
# In installer.sh, after successful binary install:
if [ "${PAPER_AUTO_INSTALL_MODEL:-}" = "1" ]; then
    download_default_model
fi

# Or let user run `paperscope` which triggers model download
```

### paperscope Binary: Model Management

```rust
// On first run, check for models
fn ensure_model() -> Result<PathBuf> {
    let model_dir = dirs::home_dir().join(".paperscope/models");
    let model_path = model_dir.join("model-medium.bin");
    
    if model_path.exists() {
        verify_checksum(&model_path)?;
        return Ok(model_path);
    }
    
    // Interactive selection
    let selected = prompt_model_selection()?;
    download_model(&selected, &model_dir)?;
    verify_checksum(&model_path)?;
    Ok(model_path)
}
```

---

## 8. Key Takeaways

| Decision | Recommendation |
|----------|----------------|
| **Model bundling** | Don't bundle in release; download on first run |
| **Binary distribution** | Use cargo-dist with shell installer |
| **Model hosting** | GitHub Releases (2 GiB limit OK) or Hugging Face |
| **Checksums** | SHA256 per model file, verified post-download |
| **Resumable downloads** | `curl --continue-at - --retry 3` |
| **Progress** | `curl --progress-bar` per file + aggregate % |
| **Offline mode** | `PAPER_OFFLINE=1` env var, cache in `~/.paperscope/models/` |
| **Platform-specific models** | Not needed (models are architecture-independent) |
| **Installer UX** | Stepper: binary → model selection → download → verify |

---

## References

1. [ollama install.sh](https://github.com/ollama/ollama/blob/main/scripts/install.sh)
2. [llama.cpp releases](https://github.com/ggml-org/llama.cpp/releases)
3. [koboldcpp releases](https://github.com/LostRuins/koboldcpp/releases)
4. [cargo-dist extra-artifacts](https://axodotdev.github.io/cargo-dist/book/reference/config.html#extra-artifacts)
5. [cargo-dist installer template](https://github.com/axodotdev/cargo-dist/blob/main/cargo-dist/templates/installer/installer.sh.j2)
6. [rustup-init.sh](https://github.com/rust-lang/rustup.rs/blob/master/rustup-init.sh)
7. [bun install.sh](https://bun.sh/install.sh)
8. [deno install.sh](https://github.com/denoland/deno_install/blob/master/install.sh)
9. [GitHub Releases limits](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases)
10. [curl resumable downloads](https://everything.curl.dev/usingcurl/downloads/resume.html)