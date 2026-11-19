//! What does a scan of a card with three books actually find?
use quire_library::{ingest_book, scan, Library};
use quire_fs::host::HostFs;
use quire_sim::fixtures;
use std::path::Path;

const NOW: u32 = 1_760_000_000;

fn main() {
    let dir = std::env::temp_dir().join(format!("quire-libdiag-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Books/classics")).unwrap();
    let fx = fixtures();
    for (src, dst) in [
        ("moby-dick.epub", "Books/classics/moby-dick.epub"),
        ("tracemonkey.pdf", "Books/tracemonkey.pdf"),
        ("middlemarch.txt", "Books/middlemarch.txt"),
    ] {
        let s = fx.join(src);
        println!("fixture {:<20} exists={} size={:?}", src, s.exists(), std::fs::metadata(&s).map(|m| m.len()).ok());
        std::fs::copy(&s, dir.join(dst)).unwrap();
    }

    let fs = HostFs::new(&dir);
    let mut lib = Library::load(&fs);
    match scan(&fs, &mut lib, NOW - 86_400 * 40) {
        Ok(n) => println!("\nscan ok: {n:?}"),
        Err(e) => println!("\nscan FAILED: {e:?}"),
    }
    println!("pending after scan: {:?}", lib.pending());
    for id in lib.pending() {
        match ingest_book(&fs, &mut lib, id, &mut |_, _| {}) {
            Ok(_) => println!("  ingest {id}: ok"),
            Err(e) => println!("  ingest {id}: FAILED {e:?}"),
        }
    }
    println!("\nbooks the library now holds:");
    let mut any = false;
    for b in &lib.books {
        any = true;
        println!("  id={} path={:?}", b.id, b.path);
    }
    if !any {
        println!("  (none)");
    }
    println!("\nby_path(/Books/classics/moby-dick.epub) = {:?}", lib.by_path("/Books/classics/moby-dick.epub").map(|b| b.id));
    let _ = Path::new("");
}
