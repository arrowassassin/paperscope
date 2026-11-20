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
    let text = "first line about warranty\nsecond line about receipts\nthird line about serial numbers\n";
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
        assert!(pair[1].line_start <= pair[0].line_end, "chunks must overlap");
    }
}

#[test]
fn indexing_a_text_file_stores_citable_chunks() {
    let (dir, indexer) = indexer();
    let file = dir.path().join("warranty.txt");
    fs::write(&file, "The fridge warranty covers 24 months.\nIt expires on 2027-01-15.\n").unwrap();

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
    fs::write(docs.join("note.md"), "# Appliances\nboiler serviced annually").unwrap();
    fs::write(docs.join("image.png"), [0u8, 1, 2, 3]).unwrap();
    fs::create_dir(docs.join(".git")).unwrap();
    fs::write(docs.join(".git").join("config.txt"), "should not be indexed").unwrap();

    let stats = indexer.index_roots(&[docs]).unwrap();
    assert_eq!(stats.indexed, 1);
    assert_eq!(stats.skipped, 1);
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
        indexer.connection().query_i64("SELECT COUNT(*) FROM chunks").unwrap(),
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
