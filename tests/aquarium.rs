#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use hearth::scene::{Rng, Scene};
use hearth::scenes::aquarium::{Aquarium, BUBBLE, Heading};
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};

const SMALL_FISH: &[&str] = &["><>"];
const TALL_FISH: &[&str] = &[" \\  /\\", "  >(o )>", " /  \\/"];

fn rng() -> Rng {
    Rng::seed_from_u64(7)
}

fn render(scene: &Aquarium, area: Rect) -> Buffer {
    let mut buffer = Buffer::empty(area);
    scene.render(area, &mut buffer);
    buffer
}

fn cells_with(buffer: &Buffer, glyphs: &[&str]) -> Vec<(Position, String)> {
    buffer
        .area
        .positions()
        .map(|p| (p, buffer[p].symbol().to_string()))
        .filter(|(_, symbol)| glyphs.contains(&symbol.as_str()))
        .collect()
}

fn ticked(mut scene: Aquarium, area: Rect, ticks: usize) -> Aquarium {
    for _ in 0..ticks {
        scene.tick(area);
    }
    scene
}

/// Given: a small fish heading right, its tail in column 17 of a twenty
/// column tank, moving one column per tick.
/// When: two ticks pass, then a third.
/// Then: after two only its tail shows, in the last column, and after
/// three only its head shows, in the first column.
///
/// Why it matters: a fish that stops at the edge, or that jumps back in
/// whole, breaks the illusion that the tank continues past the glass.
#[test]
fn a_fish_that_leaves_the_right_edge_enters_at_the_left() {
    // Given, when:
    let area = Rect::new(0, 0, 20, 8);
    let tank = |ticks| {
        ticked(
            Aquarium::with_fish(rng(), area, SMALL_FISH, Heading::Right, 17, 4),
            area,
            ticks,
        )
    };

    // Then:
    let after_two = cells_with(&render(&tank(2), area), &["<", ">"]);
    assert_eq!(after_two.len(), 1, "{after_two:?}");
    assert_eq!(after_two[0].0.x, 19);
    assert_eq!(after_two[0].1, ">");

    let after_three = cells_with(&render(&tank(3), area), &["<", ">"]);
    assert_eq!(after_three.len(), 1, "{after_three:?}");
    assert_eq!(after_three[0].0.x, 0);
    assert_eq!(after_three[0].1, ">");
}

/// Given: a bubble in column 5, row 6, of a tank with the surface on row 0.
/// When: it ticks once to five times, then a sixth.
/// Then: the one bubble sits one row higher per tick, and after six it is
/// gone.
#[test]
fn a_bubble_rises_one_row_per_tick_and_pops_at_the_surface() {
    let area = Rect::new(0, 0, 20, 10);
    for ticks in 1..=5u16 {
        // Given, when:
        let tank = ticked(
            Aquarium::with_bubble(rng(), area, 5, 6),
            area,
            usize::from(ticks),
        );

        // Then:
        let bubbles = cells_with(&render(&tank, area), &[BUBBLE]);
        assert_eq!(bubbles.len(), 1, "after {ticks} ticks: {bubbles:?}");
        assert_eq!(
            bubbles[0].0,
            Position::new(5, 6 - ticks),
            "after {ticks} ticks"
        );
    }

    // And:
    let tank = ticked(Aquarium::with_bubble(rng(), area, 5, 6), area, 6);
    assert!(cells_with(&render(&tank, area), &[BUBBLE]).is_empty());
}

/// Given: a three-row fish offered to a tank with two rows of water under
/// the surface.
/// When: ten ticks pass.
/// Then: nothing is drawn below the surface row.
///
/// Why it matters: a fish taller than the water is clipped to its top
/// rows, which reads as a broken sprite rather than a fish.
#[test]
fn a_fish_taller_than_the_water_is_not_spawned() {
    // Given, when:
    let area = Rect::new(0, 0, 20, 3);
    let tank = ticked(
        Aquarium::with_fish(rng(), area, TALL_FISH, Heading::Right, 5, 1),
        area,
        10,
    );

    // Then:
    let buffer = render(&tank, area);
    let drawn: Vec<Position> = area
        .positions()
        .filter(|&p| p.y > 0 && buffer[p].symbol() != " ")
        .collect();
    assert!(drawn.is_empty(), "{drawn:?}");
}
