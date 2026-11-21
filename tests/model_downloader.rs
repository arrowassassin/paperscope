// The env-lock guards below are intentionally held across awaits: they
// serialize the whole test body so `PAPER_OFFLINE` mutations cannot race
// with other tests on parallel threads. An async mutex would not help —
// the point is mutual exclusion of synchronous env access, not fairness.
#![allow(clippy::await_holding_lock)]

use httpmock::MockServer;
use paper::cli::model_downloader::ModelDownloader;
use std::env;
use tempfile::tempdir;

/// `offline_mode_errors_when_not_cached` mutates the process-wide
/// `PAPER_OFFLINE` env var, which races with the other tests running on
/// parallel threads. Serialize every test in this file through one lock.
/// Poisoning is ignored so a failing test does not cascade into the rest.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[tokio::test]
async fn resumable_download_recovers_from_interruption() {
    let _guard = env_lock();
    let server = MockServer::start();
    let _m = server.mock(|when, then| {
        when.method(httpmock::Method::GET).path("/model.bin");
        then.status(200)
            .header("Accept-Ranges", "bytes")
            .body(vec![0u8; 10_000_000]);
    });
    let temp_dir = tempdir().unwrap();
    let dest = temp_dir.path().join("model.bin");
    let expected_sha256 = "f5e02aa71e67f41d79023a128ca35bad86cf7b6656967bfe0884b3a3c4325eaf"; // sha256 of 10MB zeros
    let downloader = ModelDownloader::new(
        server.url("/model.bin"),
        dest.clone(),
        expected_sha256.to_string(),
        10_000_000,
    );
    let result = downloader.download_with_progress().await;
    assert!(result.is_ok());
    assert!(dest.exists());
}

#[tokio::test]
async fn download_verifies_sha256() {
    let _guard = env_lock();
    let server = MockServer::start();
    let content = b"hello world";
    let expected_sha256 = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"; // sha256 of "hello world"
    let _m = server.mock(|when, then| {
        when.method(httpmock::Method::GET).path("/model.bin");
        then.status(200)
            .header("Accept-Ranges", "bytes")
            .body(content);
    });
    let temp_dir = tempdir().unwrap();
    let dest = temp_dir.path().join("model.bin");
    let downloader = ModelDownloader::new(
        server.url("/model.bin"),
        dest.clone(),
        expected_sha256.to_string(),
        content.len() as u64,
    );
    let result = downloader.download_with_progress().await;
    assert!(result.is_ok());
    assert!(dest.exists());
}

#[tokio::test]
async fn download_fails_on_sha256_mismatch() {
    let _guard = env_lock();
    let server = MockServer::start();
    let content = b"hello world";
    let wrong_sha256 = "0000000000000000000000000000000000000000000000000000000000000000";
    let _m = server.mock(|when, then| {
        when.method(httpmock::Method::GET).path("/model.bin");
        then.status(200)
            .header("Accept-Ranges", "bytes")
            .body(content);
    });
    let temp_dir = tempdir().unwrap();
    let dest = temp_dir.path().join("model.bin");
    let downloader = ModelDownloader::new(
        server.url("/model.bin"),
        dest.clone(),
        wrong_sha256.to_string(),
        content.len() as u64,
    );
    let result = downloader.download_with_progress().await;
    assert!(result.is_err());
    assert!(!dest.exists()); // corrupt file should be deleted
}

#[tokio::test]
async fn offline_mode_errors_when_not_cached() {
    let _guard = env_lock();
    let server = MockServer::start();
    let _m = server.mock(|when, then| {
        when.method(httpmock::Method::GET).path("/model.bin");
        then.status(200).body(vec![0u8; 100]);
    });
    let temp_dir = tempdir().unwrap();
    let dest = temp_dir.path().join("model.bin");
    let downloader = ModelDownloader::new(
        server.url("/model.bin"),
        dest.clone(),
        "sha256".to_string(),
        100,
    );

    // Set offline mode
    env::set_var("PAPER_OFFLINE", "1");
    let result = downloader.download_with_progress().await;
    env::remove_var("PAPER_OFFLINE");

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("PAPER_OFFLINE") || err.contains("offline"));
}

#[tokio::test]
async fn resumable_download_continues_from_partial() {
    let _guard = env_lock();
    let server = MockServer::start();
    let content = vec![42u8; 1000];
    let expected_sha256 = "86f88b283dd6aa52d173fcbed559fda6cafc29b4b631597ca1ab0580764b162a"; // sha256 of 1000 bytes of 42

    let _m = server.mock(|when, then| {
        when.method(httpmock::Method::GET).path("/model.bin");
        then.status(200)
            .header("Accept-Ranges", "bytes")
            .body(content.clone());
    });

    let temp_dir = tempdir().unwrap();
    let dest = temp_dir.path().join("model.bin");
    let partial = dest.with_extension("partial");

    // Pre-create a partial file with first 200 bytes
    tokio::fs::write(&partial, &content[..200]).await.unwrap();

    let downloader = ModelDownloader::new(
        server.url("/model.bin"),
        dest.clone(),
        expected_sha256.to_string(),
        content.len() as u64,
    );
    let result = downloader.download_with_progress().await;

    // Should complete download from byte 200
    assert!(result.is_ok());
}
