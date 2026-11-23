//! Permission-denied handling.
//!
//! The failure these pin: a root with entries the process may not read used to
//! produce one warning per file, on every pass — the observed 1,070 failed reads
//! per five minutes from a single revoked grant. Now a denied subtree is one
//! line for the whole root, the paths are remembered, and they are not
//! re-attempted until the backoff window elapses. Granting the permission again
//! must bring the files back without a reinstall.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn paper(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_paper"))
        .args(args)
        .env("PAPER_HOME", home)
        .env_remove("PAPER_OFFLINE")
        .output()
        .unwrap()
}

fn set_mode(path: &Path, mode: u32) {
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    perms.set_mode(mode);
    std::fs::set_permissions(path, perms).unwrap();
}

/// `index --root` on `docs`, with the modes restored afterwards so the tempdir
/// can always be cleaned up.
fn index(docs: &Path, home: &Path) -> Output {
    paper(home, &["index", "--root", docs.to_str().unwrap()])
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let home = tempfile::tempdir().unwrap();
    let docs = home.path().join("docs");
    std::fs::create_dir(&docs).unwrap();
    let ok = docs.join("ok.txt");
    std::fs::write(&ok, "Warranty\n\nThe fridge warranty covers 24 months.\n").unwrap();
    (home, docs, ok)
}

#[test]
fn an_unreadable_directory_is_one_line_then_silent() {
    let (home, docs, _ok) = fixture();
    let locked = docs.join("locked");
    std::fs::create_dir(&locked).unwrap();
    std::fs::write(locked.join("secret.txt"), "never indexed\n").unwrap();
    set_mode(&locked, 0o000);

    let first = index(&docs, home.path());
    set_mode(&locked, 0o755);
    assert!(first.status.success(), "index failed: {}", stderr(&first));
    let first_err = stderr(&first);
    assert_eq!(
        first_err
            .matches("could not be read (permission denied)")
            .count(),
        1,
        "a denied subtree must produce exactly one line for the root:\n{first_err}"
    );
    assert!(
        first_err.contains("System Settings"),
        "the line must say what to do about it:\n{first_err}"
    );

    // The readable file is still indexed: a denied sibling must not fail the run.
    assert!(
        stdout(&first).contains("indexed 1 file(s)"),
        "the readable file must still be indexed:\n{}",
        stdout(&first)
    );
}

#[test]
fn an_unreadable_file_is_remembered_and_not_retried() {
    let (home, docs, ok) = fixture();
    let secret = docs.join("secret.txt");
    std::fs::write(&secret, "unreadable\n").unwrap();
    set_mode(&secret, 0o000);

    let first = index(&docs, home.path());
    assert!(first.status.success(), "index failed: {}", stderr(&first));
    assert!(
        stdout(&first).contains("1 unreadable (permission denied)"),
        "the pass must count the denied file:\n{}",
        stdout(&first)
    );

    let second = index(&docs, home.path());
    set_mode(&secret, 0o644);
    assert!(second.status.success(), "index failed: {}", stderr(&second));
    assert!(
        stderr(&second).matches("permission denied").count() == 0,
        "the second pass must not warn again:\n{}",
        stderr(&second)
    );
    assert!(
        stdout(&second).contains("1 deferred by the backoff"),
        "the second pass must report the backoff, not the denial:\n{}",
        stdout(&second)
    );

    // Restoring the permission brings the file back once the window elapses;
    // the window is proven here by expiring the row, not by waiting ten minutes.
    expunge_denials(home.path());
    let third = index(&docs, home.path());
    assert!(third.status.success(), "index failed: {}", stderr(&third));
    assert!(
        stdout(&third).contains("indexed 1 file(s)"),
        "a fixed permission must be picked up again:\n{}",
        stdout(&third)
    );
    assert!(ok.exists());
}

/// Clear the backoff bookkeeping, standing in for the clock passing.
fn expunge_denials(home: &Path) {
    let db = home.join("chunks.db");
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute("DELETE FROM unreadable", []).unwrap();
}
