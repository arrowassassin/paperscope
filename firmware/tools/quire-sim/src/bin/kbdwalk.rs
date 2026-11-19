//! Does the keyboard reach every key with the two side keys alone?
use quire_sim::{fixture_card, frame_to_png, Sim};
use quire_ui::Key;

fn main() {
    let card = fixture_card("kbd");
    let mut sim = Sim::boot(&card);
    sim.push(quire_ui::keyboard::KeyboardScreen::new("Test", "", "Name").boxed());
    println!("top: {}", sim.top());
    frame_to_png(sim.frame(), std::path::Path::new("/tmp/kbd-start.png")).unwrap();

    // Type by walking with the right side key and pressing Confirm each time.
    let mut typed = String::new();
    for step in 0..12 {
        sim.press(Key::Down);
        sim.press(Key::Confirm);
        let _ = step;
    }
    frame_to_png(sim.frame(), std::path::Path::new("/tmp/kbd-after.png")).unwrap();
    sim.with_ctx(|_| ());
    println!("{typed}");
    println!("shots: /tmp/kbd-start.png /tmp/kbd-after.png");
}
