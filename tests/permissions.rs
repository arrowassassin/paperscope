//! Permission-denied handling.
//!
//! The failure these pin: a root with entries the process may not read used to
//! produce one failed read and one warning per file, on every pass — the
//! observed 1,070 failed reads per five minutes from a single revoked grant. Now
//! a denied subtree is one line for the whole root, the paths are remembered,
//! they are not re-attempted until the backoff elapses, and content already
//! indexed behind a revoked grant is not deleted merely because it can no longer
//! be *seen*.

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

fn index(docs: &Path, home: &Path) -> Output {
    paper(home, &["index", "--root", docs.to_str().unwrap()])
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let home = tempfile::tempdir().unwrap();
    let docs = home.path().join("docs");
    std::fs::create_dir(&docs).unwrap();
    let ok = docs.join("ok.txt");
    std::fs::write(&ok, "Warranty\n\nThe fridge warranty covers 24 months.\n").unwrap();
    (home, docs, ok)
}

/// A denied directory is reported once, then pruned — and the *second* pass is
/// silent, which is the half the first version of this test never ran.
#[test]
fn a_denied_directory_is_one_line_then_silent() {
    let (home, docs, _ok) = fixture();
    let locked = docs.join("locked");
    std::fs::create_dir(&locked).unwrap();
    for n in 0..25 {
        std::fs::write(locked.join(format!("secret-{n}.txt")), "never indexed\n").unwrap();
    }
    set_mode(&locked, 0o000);

    let first = index(&docs, home.path());
    assert!(first.status.success(), "index failed: {}", stderr(&first));
    let first_err = stderr(&first);
    assert_eq!(
        count(&first_err, "could not be read (permission denied)"),
        1,
        "a denied subtree must produce exactly one line for the root, not one per \
         descendant:\n{first_err}"
    );
    assert!(
        first_err.contains("Full Disk Access"),
        "the line must name the remedy:\n{first_err}"
    );
    // The readable sibling is still indexed: a denied neighbour never fails a run.
    assert!(
        stdout(&first).contains("indexed 1 file(s)"),
        "the readable file must still be indexed:\n{}",
        stdout(&first)
    );

    let second = index(&docs, home.path());
    set_mode(&locked, 0o755);
    assert!(second.status.success(), "index failed: {}", stderr(&second));
    assert_eq!(
        count(&stderr(&second), "permission denied"),
        0,
        "the second pass must be silent, the denied subtree being pruned:\n{}",
        stderr(&second)
    );
    assert!(
        stdout(&second).contains("1 deferred by the backoff"),
        "the pruned subtree must be counted as deferred, not vanish:\n{}",
        stdout(&second)
    );
}

/// A denied *file* is counted, then deferred rather than retried, and it comes
/// back once the backoff window has passed — without anyone clearing a cache by
/// hand.
#[test]
fn a_denied_file_is_remembered_then_retried_after_the_backoff() {
    let (home, docs, _ok) = fixture();
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
    assert!(second.status.success(), "index failed: {}", stderr(&second));
    assert_eq!(
        count(&stderr(&second), "permission denied"),
        0,
        "the second pass must not warn again:\n{}",
        stderr(&second)
    );
    assert!(
        stdout(&second).contains("1 deferred by the backoff"),
        "the second pass must report the backoff, not the denial:\n{}",
        stdout(&second)
    );

    // The permission comes back *and* the window passes. Both halves matter: the
    // retry must come from the backoff expiring, which is the code path a manual
    // cache clear would skip entirely.
    set_mode(&secret, 0o644);
    age_the_backoff(home.path(), 601);
    let third = index(&docs, home.path());
    assert!(third.status.success(), "index failed: {}", stderr(&third));
    assert!(
        stdout(&third).contains("indexed 1 file(s)"),
        "a restored permission must be picked up once the window passes:\n{}",
        stdout(&third)
    );
    assert_eq!(
        unreadable_rows(home.path()),
        0,
        "the expired row must be deleted, not left to match forever"
    );
}

/// A root that does not exist is not a permission problem, and telling the user
/// to grant Full Disk Access for it is advice they cannot act on.
#[test]
fn a_missing_root_is_not_reported_as_a_permission_problem() {
    let home = tempfile::tempdir().unwrap();
    let missing = home.path().join("does-not-exist");
    let out = index(&missing, home.path());
    let err = stderr(&out);
    assert!(
        !err.contains("permission denied"),
        "NotFound must not be dressed up as a permission problem:\n{err}"
    );
    assert!(
        err.contains("walk error"),
        "it should still be reported, as the error it is:\n{err}"
    );
}

/// Revoking access to an already-indexed directory must not delete its content.
/// `exists()` answers false behind a denied directory, and pruning on that alone
/// turned a revoked grant into "1 removed" and a search that found nothing.
#[test]
fn a_revoked_grant_does_not_prune_content_it_can_no_longer_see() {
    let (home, docs, _ok) = fixture();
    let keep = docs.join("keep");
    std::fs::create_dir(&keep).unwrap();
    std::fs::write(
        keep.join("contract.txt"),
        "Retention\n\nThe contract renews on 2027-03-01.\n",
    )
    .unwrap();

    let indexed = index(&docs, home.path());
    assert!(
        indexed.status.success(),
        "index failed: {}",
        stderr(&indexed)
    );
    let found = paper(home.path(), &["search", "contract renews"]);
    assert!(
        found.status.success(),
        "precondition: the file is searchable before the grant is revoked:\n{}",
        stderr(&found)
    );

    set_mode(&keep, 0o000);
    let again = index(&docs, home.path());
    set_mode(&keep, 0o755);
    assert!(again.status.success(), "index failed: {}", stderr(&again));
    assert!(
        stdout(&again).contains("0 removed"),
        "unreadable content must not be pruned:\n{}",
        stdout(&again)
    );

    let still = paper(home.path(), &["search", "contract renews"]);
    assert!(
        still.status.success(),
        "content behind a revoked grant must stay searchable:\n{}",
        stderr(&still)
    );
}

/// Move remembered denials into the past, so the next pass finds them expired.
fn age_the_backoff(home: &Path, seconds: u64) {
    let conn = rusqlite::Connection::open(home.join("chunks.db")).unwrap();
    conn.execute(
        "UPDATE unreadable SET last_attempt = last_attempt - ?1",
        [seconds as i64],
    )
    .unwrap();
}

fn unreadable_rows(home: &Path) -> i64 {
    let conn = rusqlite::Connection::open(home.join("chunks.db")).unwrap();
    conn.query_row("SELECT COUNT(*) FROM unreadable", [], |row| row.get(0))
        .unwrap()
}
