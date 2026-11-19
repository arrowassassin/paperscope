//! What the reader does with a card that has no books on it — the state a new
//! owner is in after first run, and the one the reported fault came from.
use quire_sim::{frame_to_png, Sim};
use quire_ui::Key;
use std::path::PathBuf;

const KEYS: [(&str, Key); 7] = [
    ("Left", Key::Left), ("Back", Key::Back), ("Confirm", Key::Confirm), ("Right", Key::Right),
    ("Up", Key::Up), ("Down", Key::Down), ("Power", Key::Power),
];

fn main() {
    let dir = std::env::temp_dir().join(format!("quire-empty-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Books")).unwrap(); // an empty /Books, as a fresh FAT32 card has

    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "/tmp/emptyshots".into()));
    std::fs::create_dir_all(&out).unwrap();

    for screen in ["10-home", "11-library"] {
        let mut sim = Sim::boot(&dir);
        // Past first run, but with nothing on the card: the state a new owner is
        // actually in once the wizard is done.
        // Walk first run to its end: language, time, reader, add books, Start.
        for k in [Key::Confirm, Key::Right, Key::Right, Key::Right] {
            sim.press(k);
        }
        if screen != "10-home" {
            sim.open(screen);
        }
        println!("\n== {screen} (top = {}) ==", sim.top());
        frame_to_png(sim.frame(), &out.join(format!("{screen}.png"))).unwrap();
        for (label, key) in KEYS {
            let mut s2 = Sim::boot(&dir);
            if screen != "10-home" {
                s2.open(screen);
            }
            let before = (s2.ui.stack_names(), s2.hash());
            s2.press(key);
            let after = (s2.ui.stack_names(), s2.hash());
            let outcome = if before == after { String::from("— nothing") } else { format!("-> {}", s2.top()) };
            println!("  {:<8} {}", label, outcome);
        }
    }
    println!("\npngs in {}", out.display());
}
