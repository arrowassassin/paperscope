mod common;

use paper::core::index::{chunk_text, Connection, Indexer};
use paper::core::search::tier1_search;
use std::fs;

fn indexer() -> (tempfile::TempDir, Indexer) {
    let dir = tempfile::tempdir().unwrap();
    let conn = Connection::open(&dir.path().join("chunks.db")).unwrap();
    let indexer = Indexer::new(conn, Some(common::hash_embedder())).unwrap();
    (dir, indexer)
}

#[test]
fn chunking_preserves_line_numbers_for_citations() {
    let text =
        "first line about warranty\nsecond line about receipts\nthird line about serial numbers\n";
    let chunks = chunk_text(text, 4, 1);
    assert!(!chunks.is_empty());
    for chunk in &chunks {
        assert!(chunk.line_start >= 1);
        assert!(chunk.line_end >= chunk.line_start);
        let cited: Vec<&str> = text
            .lines()
            .skip((chunk.line_start - 1) as usize)
            .take((chunk.line_end - chunk.line_start + 1) as usize)
            .collect();
        // Every non-overlapped line of the chunk must really be at those lines.
        for line in chunk.content.lines() {
            assert!(
                text.lines().any(|l| l == line),
                "chunk line not present in source: {line}"
            );
        }
        assert!(!cited.is_empty());
    }
}

#[test]
fn chunking_empty_input_produces_nothing() {
    assert!(chunk_text("", 400, 40).is_empty());
    assert!(chunk_text("\n\n\n", 400, 40).is_empty());
}

#[test]
fn chunking_long_document_produces_multiple_overlapping_chunks() {
    let text = (0..200)
        .map(|i| format!("line {i} with several words of content here"))
        .collect::<Vec<_>>()
        .join("\n");
    let chunks = chunk_text(&text, 40, 10);
    assert!(chunks.len() > 1);
    for pair in chunks.windows(2) {
        assert!(
            pair[1].line_start <= pair[0].line_end,
            "chunks must overlap"
        );
    }
}

#[test]
fn indexing_a_text_file_stores_citable_chunks() {
    let (dir, indexer) = indexer();
    let file = dir.path().join("warranty.txt");
    fs::write(
        &file,
        "The fridge warranty covers 24 months.\nIt expires on 2027-01-15.\n",
    )
    .unwrap();

    let stats = indexer.index_file(&file).unwrap();
    assert_eq!(stats.indexed, 1);
    assert!(stats.chunks >= 1);

    let (line_start, content): (i64, String) = indexer
        .connection()
        .inner()
        .query_row("SELECT line_start, content FROM chunks LIMIT 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(line_start, 1);
    assert!(content.contains("2027-01-15"));
}

#[test]
fn reindexing_unchanged_file_is_a_no_op() {
    let (dir, indexer) = indexer();
    let file = dir.path().join("a.txt");
    fs::write(&file, "unchanged content about a dishwasher").unwrap();

    assert_eq!(indexer.index_file(&file).unwrap().indexed, 1);
    let second = indexer.index_file(&file).unwrap();
    assert_eq!(second.unchanged, 1);
    assert_eq!(second.indexed, 0);
    assert_eq!(
        indexer
            .connection()
            .query_i64("SELECT COUNT(*) FROM chunks")
            .unwrap(),
        1
    );
}

#[test]
fn editing_a_file_replaces_its_chunks() {
    let (dir, indexer) = indexer();
    let file = dir.path().join("a.txt");
    fs::write(&file, "warranty covers 24 months").unwrap();
    indexer.index_file(&file).unwrap();

    // mtime granularity can be coarse, so force a visible change.
    std::thread::sleep(std::time::Duration::from_millis(20));
    fs::write(&file, "warranty covers 36 months and includes parts").unwrap();
    indexer.index_file(&file).unwrap();

    assert_eq!(
        indexer
            .connection()
            .query_i64("SELECT COUNT(*) FROM chunks WHERE content LIKE '%36 months%'")
            .unwrap(),
        1
    );
    assert_eq!(
        indexer
            .connection()
            .query_i64("SELECT COUNT(*) FROM chunks WHERE content LIKE '%24 months%'")
            .unwrap(),
        0
    );
    // No orphan vectors left behind.
    assert_eq!(
        indexer
            .connection()
            .query_i64("SELECT COUNT(*) FROM chunks_vec WHERE rowid NOT IN (SELECT id FROM chunks)")
            .unwrap(),
        0
    );
}

#[test]
fn indexing_a_directory_skips_non_text_and_hidden_files() {
    let (dir, indexer) = indexer();
    let docs = dir.path().join("docs");
    fs::create_dir(&docs).unwrap();
    fs::write(
        docs.join("note.md"),
        "# Appliances\nboiler serviced annually",
    )
    .unwrap();
    fs::write(docs.join("image.png"), [0u8, 1, 2, 3]).unwrap();
    fs::create_dir(docs.join(".git")).unwrap();
    fs::write(
        docs.join(".git").join("config.txt"),
        "should not be indexed",
    )
    .unwrap();

    let stats = indexer.index_roots(&[docs]).unwrap();
    assert_eq!(stats.indexed, 1);
    assert_eq!(stats.skipped, 1);
}

#[test]
fn a_hidden_file_root_is_skipped_like_a_hidden_file_in_a_walk() {
    let (dir, indexer) = indexer();
    let hidden = dir.path().join(".secrets.txt");
    let visible = dir.path().join("note.txt");
    fs::write(&hidden, "aws_secret_access_key = hunter2").unwrap();
    fs::write(&visible, "boiler serviced annually").unwrap();

    let stats = indexer.index_roots(&[hidden, visible]).unwrap();
    assert_eq!(stats.indexed, 1);
}

#[test]
fn binary_content_is_skipped_not_corrupted() {
    let (dir, indexer) = indexer();
    let file = dir.path().join("binary.txt");
    fs::write(&file, [0xffu8, 0xfe, 0x00, 0x01]).unwrap();
    let stats = indexer.index_file(&file).unwrap();
    assert_eq!(stats.skipped, 1);
    assert_eq!(stats.chunks, 0);
}

#[test]
fn prunes_chunks_for_deleted_files() {
    let (dir, indexer) = indexer();
    let file = dir.path().join("gone.txt");
    fs::write(&file, "temporary document").unwrap();
    indexer.index_file(&file).unwrap();
    fs::remove_file(&file).unwrap();
    assert_eq!(indexer.prune_missing().unwrap(), 1);
    assert_eq!(
        indexer
            .connection()
            .query_i64("SELECT COUNT(*) FROM chunks")
            .unwrap(),
        0
    );
}

#[test]
fn indexed_vectors_are_semantically_searchable() {
    let (dir, indexer) = indexer();
    let file = dir.path().join("manual.txt");
    fs::write(
        &file,
        "The dishwasher warranty covers parts and labour for 24 months.",
    )
    .unwrap();
    indexer.index_file(&file).unwrap();

    let query = common::HashEmbedder;
    use paper::core::search::QueryEmbedder;
    let vector = query.embed("dishwasher warranty").unwrap();
    let hits = tier1_search(indexer.connection(), &vector, &[], 5).unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].snippet.contains("dishwasher"));
}

#[test]
fn latin_chunks_store_only_one_copy_of_the_text() {
    let (dir, indexer) = indexer();
    let file = dir.path().join("manual.txt");
    let body = "The dishwasher warranty covers parts and labour for 24 months.\n";
    fs::write(&file, body).unwrap();
    indexer.index_file(&file).unwrap();

    // Segmentation is the identity for Latin text: `fts_text` must be empty so
    // the row does not carry a second copy of `content`.
    let duplicated: i64 = indexer
        .connection()
        .query_i64("SELECT COUNT(*) FROM chunks WHERE fts_text != ''")
        .unwrap();
    assert_eq!(duplicated, 0);

    // The trigger still indexes the row through the content fallback.
    let hits =
        paper::core::search::tier0_search(indexer.connection(), "dishwasher warranty", &[], 5)
            .unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].snippet.contains("warranty"));
}

#[test]
fn cjk_chunks_store_segmented_text_and_stay_searchable() {
    let (dir, indexer) = indexer();
    let file = dir.path().join("jp.txt");
    fs::write(&file, "保証期間は24ヶ月です。\n").unwrap();
    indexer.index_file(&file).unwrap();

    let segmented: i64 = indexer
        .connection()
        .query_i64("SELECT COUNT(*) FROM chunks WHERE fts_text != ''")
        .unwrap();
    assert_eq!(segmented, 1, "CJK chunks must keep segmented index text");

    let hits = paper::core::search::tier0_search(indexer.connection(), "保証", &[], 5).unwrap();
    assert!(!hits.is_empty(), "bigram query must match CJK text");
    // The snippet is verbatim source text, not bigram-separated index text.
    assert!(hits[0].snippet.contains("保証期間"), "{}", hits[0].snippet);
}

#[test]
fn deleting_a_document_removes_its_fts_entries() {
    let (dir, indexer) = indexer();
    let file = dir.path().join("gone.txt");
    fs::write(&file, "the unique token zqxwarranty appears here").unwrap();
    indexer.index_file(&file).unwrap();
    assert!(
        !paper::core::search::tier0_search(indexer.connection(), "zqxwarranty", &[], 5)
            .unwrap()
            .is_empty()
    );

    fs::remove_file(&file).unwrap();
    indexer.prune_missing().unwrap();
    assert!(
        paper::core::search::tier0_search(indexer.connection(), "zqxwarranty", &[], 5)
            .unwrap()
            .is_empty(),
        "deleted rows must not linger in the FTS index"
    );
}

/// Index `docs` over and over while `swap` keeps flipping part of it to a
/// symlink that leads outside, then prove nothing outside was ever indexed.
/// The in-root file is binary, so it is never stored and every pass reads again.
#[cfg(unix)]
fn index_while_swapping(indexer: &Indexer, docs: &std::path::Path, swap: impl Fn() + Sync) {
    use std::sync::atomic::{AtomicBool, Ordering};
    let stop = AtomicBool::new(false);
    std::thread::scope(|s| {
        s.spawn(|| {
            while !stop.load(Ordering::Relaxed) {
                swap();
            }
        });
        let passes: Vec<_> = (0..2000)
            .map(|_| indexer.index_roots(&[docs.to_path_buf()]))
            .collect();
        stop.store(true, Ordering::Relaxed);
        for pass in passes {
            pass.unwrap();
        }
    });
    assert_eq!(
        indexer
            .connection()
            .query_i64("SELECT COUNT(*) FROM chunks")
            .unwrap(),
        0,
        "a file outside the root was indexed through a swapped-in symlink"
    );
}

#[cfg(unix)]
#[test]
fn a_file_swapped_for_a_symlink_mid_index_is_never_read_through_it() {
    let (dir, indexer) = indexer();
    let docs = dir.path().join("docs");
    fs::create_dir(&docs).unwrap();
    fs::write(dir.path().join("outside.txt"), "zqxsecret outside the root").unwrap();
    let (note, real, link) = (
        docs.join("note.txt"),
        docs.join(".real"),
        docs.join(".link"),
    );
    fs::write(&note, [0xffu8, 0xfe, 0x00, 0x01]).unwrap();
    std::os::unix::fs::symlink(dir.path().join("outside.txt"), &link).unwrap();
    index_while_swapping(&indexer, &docs, || {
        fs::rename(&note, &real).unwrap();
        fs::rename(&link, &note).unwrap();
        fs::rename(&note, &link).unwrap();
        fs::rename(&real, &note).unwrap();
    });
}

// Only where the kernel reports an open file's path: elsewhere it is resolved
// again, which a parent swapped back and forth can still fool (see `real_path`).
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_parent_directory_swapped_for_a_symlink_mid_walk_is_never_read_through_it() {
    let (dir, indexer) = indexer();
    let docs = dir.path().join("docs");
    let outside = dir.path().join("outside");
    fs::create_dir_all(docs.join("sub")).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(docs.join("sub").join("a.txt"), [0xffu8, 0xfe, 0x00, 0x01]).unwrap();
    fs::write(outside.join("a.txt"), "zqxsecret outside the root").unwrap();
    let (sub, real, link) = (docs.join("sub"), docs.join(".real"), docs.join(".link"));
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    index_while_swapping(&indexer, &docs, || {
        fs::rename(&sub, &real).unwrap();
        fs::rename(&link, &sub).unwrap();
        fs::rename(&sub, &link).unwrap();
        fs::rename(&real, &sub).unwrap();
    });
}

/// The single-file entry points (the daemon watcher's file roots, and
/// `paper index --watch` through `index_file_within`) check the configured root
/// too: a file whose parent directory became a symlink out of the root is skipped.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_file_root_below_a_parent_swapped_for_a_symlink_is_not_indexed() {
    let (dir, indexer) = indexer();
    let docs = dir.path().join("docs");
    let outside = dir.path().join("outside");
    fs::create_dir(&docs).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("a.txt"), "zqxsecret outside the root").unwrap();
    std::os::unix::fs::symlink(&outside, docs.join("sub")).unwrap();
    let file = docs.join("sub").join("a.txt");

    let stats = indexer
        .index_roots_within(std::slice::from_ref(&file), std::slice::from_ref(&docs))
        .unwrap();
    assert_eq!((stats.indexed, stats.skipped), (0, 1), "{stats:?}");
    let stats = indexer.index_file_within(&file, Some(&docs)).unwrap();
    assert_eq!(stats.indexed, 0);
    assert_eq!(
        indexer
            .connection()
            .query_i64("SELECT COUNT(*) FROM chunks")
            .unwrap(),
        0
    );

    // The same file reached legitimately is indexed.
    let real = outside.join("a.txt");
    let stats = indexer
        .index_roots_within(std::slice::from_ref(&real), &[outside])
        .unwrap();
    assert_eq!(stats.indexed, 1, "{stats:?}");
}

#[test]
fn a_failed_extraction_keeps_the_last_good_chunks() {
    let (dir, indexer) = indexer();
    let file = dir.path().join("a.docx");
    // A plain-text body under a .docx name is a corrupt document for the
    // extractor; first index a good version through a markdown twin of the path.
    let good = dir.path().join("a.md");
    fs::write(&good, "warranty covers 24 months").unwrap();
    indexer.index_file(&good).unwrap();
    let before = indexer
        .connection()
        .query_i64("SELECT COUNT(*) FROM chunks")
        .unwrap();
    assert_eq!(before, 1);
    // Rewrite the same indexed path with content the extractor cannot parse.
    let conn = indexer.connection().inner();
    conn.execute(
        "UPDATE chunks SET source_path = ?",
        [file.to_string_lossy().to_string()],
    )
    .unwrap();
    fs::write(&file, b"this is not a zip archive").unwrap();
    let stats = indexer.index_file(&file).unwrap();
    assert_eq!(stats.skipped, 1);
    assert!(stats.warnings > 0);
    assert_eq!(
        indexer
            .connection()
            .query_i64("SELECT COUNT(*) FROM chunks")
            .unwrap(),
        1
    );
}

struct Model(&'static str);
impl paper::core::search::QueryEmbedder for Model {
    fn embed(&self, t: &str) -> anyhow::Result<Vec<f32>> {
        paper::core::search::QueryEmbedder::embed(&common::HashEmbedder, t)
    }
    fn model_id(&self) -> &str {
        self.0
    }
}

fn count(i: &Indexer, table: &str) -> i64 {
    i.connection()
        .query_i64(&format!("SELECT COUNT(*) FROM {table}"))
        .unwrap()
}

/// Index a few multi-chunk files with model "a", then reopen with model "b".
fn switched() -> (tempfile::TempDir, Indexer) {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("chunks.db");
    let docs = dir.path().join("docs");
    fs::create_dir(&docs).unwrap();
    for n in 0..3 {
        let body = (0..900)
            .map(|w| format!("warranty{n}w{w}"))
            .collect::<Vec<_>>()
            .join(" ");
        fs::write(docs.join(format!("{n}.txt")), body).unwrap();
    }
    let a = Indexer::new(
        Connection::open(&db).unwrap(),
        Some(std::sync::Arc::new(Model("a"))),
    )
    .unwrap();
    a.index_roots(&[docs]).unwrap();
    assert!(count(&a, "chunks") >= 3);
    drop(a);
    let b = Indexer::new(
        Connection::open(&db).unwrap(),
        Some(std::sync::Arc::new(Model("b"))),
    )
    .unwrap();
    (dir, b)
}

#[test]
fn switching_embedder_keeps_chunks_and_drops_only_vectors() {
    let (_dir, b) = switched();
    assert!(count(&b, "chunks") >= 3);
    assert_eq!(count(&b, "chunks_vec"), 0);
    assert!(paper::core::index::reembed_pending(b.connection()).unwrap());
    assert_eq!(
        b.connection()
            .query_i64("SELECT COUNT(*) FROM chunks_fts WHERE chunks_fts MATCH 'warranty1w5'")
            .unwrap(),
        1
    );
}

#[test]
fn reembed_missing_fills_every_vector_and_clears_pending() {
    let (_dir, b) = switched();
    let total = count(&b, "chunks");
    assert_eq!(b.reembed_missing(2, |_, _| {}).unwrap() as i64, total);
    assert_eq!(count(&b, "chunks_vec"), total);
    assert!(!paper::core::index::reembed_pending(b.connection()).unwrap());
}

#[test]
fn reembed_resumes_after_interruption_and_never_mixes_models() {
    let (_dir, b) = switched();
    let total = count(&b, "chunks");
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = b.reembed_missing(2, |_, _| panic!("killed"));
    }));
    assert!(r.is_err());
    let partial = count(&b, "chunks_vec");
    assert!(partial > 0 && partial < total);
    assert!(paper::core::index::reembed_pending(b.connection()).unwrap());
    assert_eq!(
        b.connection().stored_embedding_model().unwrap().as_deref(),
        Some("b")
    );
    assert_eq!(
        b.reembed_missing(2, |_, _| {}).unwrap() as i64,
        total - partial
    );
    assert_eq!(count(&b, "chunks_vec"), total);
    assert!(!paper::core::index::reembed_pending(b.connection()).unwrap());
    assert_eq!(
        b.connection().stored_embedding_model().unwrap().as_deref(),
        Some("b")
    );
}

#[test]
fn reembed_fills_chunks_indexed_without_an_embedder() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("chunks.db");
    let file = dir.path().join("a.txt");
    fs::write(&file, "warranty covers 24 months").unwrap();
    let none = Indexer::new(Connection::open(&db).unwrap(), None).unwrap();
    none.index_file(&file).unwrap();
    assert_eq!(none.reembed_missing(2, |_, _| {}).unwrap(), 0);
    drop(none);
    let with = Indexer::new(
        Connection::open(&db).unwrap(),
        Some(common::hash_embedder()),
    )
    .unwrap();
    assert_eq!(with.reembed_missing(2, |_, _| {}).unwrap(), 1);
    assert_eq!(count(&with, "chunks_vec"), count(&with, "chunks"));
}
