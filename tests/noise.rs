#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use hearth::scene::{Rng, Scene};
use hearth::scenes::noise::Noise;
use hearth::theme;
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

const AREA: Rect = Rect::new(0, 0, 100, 30);

fn glyphs(scene: &Noise) -> Vec<String> {
    let mut buffer = Buffer::empty(AREA);
    scene.render(AREA, &mut buffer);
    AREA.positions()
        .map(|position| buffer[position].symbol().to_string())
        .collect()
}

/// Noise changes some of its glyphs each tick, not all of them.
/// Given: the scene on a hundred by thirty area, ticked once.
/// When: it ticks fifty times more.
/// Then: from one tick to the next, between a quarter and a little over
/// half of the cells show another glyph, on average.
///
/// Why it matters: a screen that draws every cell again each tick flickers
/// too fast to rest the eye.
#[test]
fn noise_changes_some_of_its_glyphs_each_tick() {
    // Given:
    let mut scene = Noise::new(Rng::seed_from_u64(7), &theme::ROSE_PINE);
    scene.tick(AREA);
    let mut before = glyphs(&scene);
    let mut shares = Vec::new();

    for _ in 0..50 {
        // When:
        scene.tick(AREA);
        let after = glyphs(&scene);
        let changed = before.iter().zip(&after).filter(|(a, b)| a != b).count();
        shares.push(changed as f64 / after.len() as f64);
        before = after;
    }

    // Then:
    let mean = shares.iter().sum::<f64>() / shares.len() as f64;
    assert!(
        (0.25..0.55).contains(&mean),
        "{mean} of the cells changed a tick"
    );
}
