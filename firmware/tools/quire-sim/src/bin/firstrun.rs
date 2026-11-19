//! Walk first run and check the key under each rail label does what it says.
use quire_sim::{frame_to_png, Sim};
use quire_ui::Key;

fn main() {
    let dir = std::env::temp_dir().join(format!("quire-fr-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Books")).unwrap();
    let out = std::path::PathBuf::from("/tmp/frshots");
    std::fs::create_dir_all(&out).unwrap();

    // page 1 -> 2 -> 3 -> 4, then Back and Left off each one.
    for (page, forward) in [(2, vec![Key::Confirm]), (3, vec![Key::Confirm, Key::Right]), (4, vec![Key::Confirm, Key::Right, Key::Right])] {
        for (name, back) in [("Back", Key::Back), ("Left", Key::Left)] {
            let mut sim = Sim::boot(&dir);
            for k in &forward {
                sim.press(*k);
            }
            let at = sim.top();
            let before = sim.hash();
            sim.press(back);
            let moved = sim.hash() != before;
            println!("page {page} ({at:<22}) {name:<5} -> {}", if moved { format!("moved, now {}", sim.top()) } else { String::from("NOTHING") });
        }
    }
    // snapshot each page so the rails can be read
    for (n, keys) in [(1, vec![]), (2, vec![Key::Confirm]), (3, vec![Key::Confirm, Key::Right]), (4, vec![Key::Confirm, Key::Right, Key::Right])] {
        let mut sim = Sim::boot(&dir);
        for k in &keys {
            sim.press(*k);
        }
        frame_to_png(sim.frame(), &out.join(format!("firstrun-{n}.png"))).unwrap();
    }
    println!("\nshots in {}", out.display());
}
