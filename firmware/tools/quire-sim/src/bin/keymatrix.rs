//! Presses every key on every screen and reports the ones that do nothing.
//!
//! A key "does something" if it changes the screen stack or the pixels. A key
//! that does neither is either deliberately inert or a dead control; the rail
//! label tells them apart, so the report prints the rail alongside.
use quire_sim::{fixture_card, tour_targets, Sim};
use quire_ui::Key;

const KEYS: [(&str, Key); 7] = [
    ("Left", Key::Left),
    ("Back", Key::Back),
    ("Confirm", Key::Confirm),
    ("Right", Key::Right),
    ("Up", Key::Up),
    ("Down", Key::Down),
    ("Power", Key::Power),
];

fn main() {
    let card = fixture_card("keymatrix");
    let mut rows: Vec<(String, Vec<&'static str>)> = Vec::new();

    let mut targets: Vec<(&'static str, Vec<Key>)> = vec![("10-home", vec![]), ("20-reading", vec![])];
    targets.extend(tour_targets());

    for (name, setup) in targets {
        for (label, key) in KEYS {
            let mut sim = Sim::boot(&card);
            if name != "10-home" {
                sim.open(name);
            }
            for k in &setup {
                sim.press(*k);
            }
            let before_stack = sim.ui.stack_names();
            let before_hash = sim.hash();
            let before_top = sim.top();
            sim.press(key);
            let changed = sim.ui.stack_names() != before_stack || sim.hash() != before_hash;
            if !changed {
                rows.entry_push(before_top.to_string(), label);
            }
        }
    }

    println!("# Dead keys by screen\n");
    println!("| screen | keys that do nothing |");
    println!("|---|---|");
    for (screen, keys) in &rows {
        println!("| {} | {} |", screen, keys.join(", "));
    }
    if rows.is_empty() {
        println!("| (none) | |");
    }
}

trait EntryPush {
    fn entry_push(&mut self, k: String, v: &'static str);
}
impl EntryPush for Vec<(String, Vec<&'static str>)> {
    fn entry_push(&mut self, k: String, v: &'static str) {
        if let Some(e) = self.iter_mut().find(|(n, _)| *n == k) {
            e.1.push(v);
        } else {
            self.push((k, vec![v]));
        }
    }
}
