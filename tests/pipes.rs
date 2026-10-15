#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use hearth::scene::{Rng, Scene};
use hearth::scenes::pipes::{Direction, FILL_PERCENT, Pipes};
use hearth::theme;
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

fn render(scene: &Pipes, area: Rect) -> Buffer {
    let mut buffer = Buffer::empty(area);
    scene.render(area, &mut buffer);
    buffer
}

fn drawn(buffer: &Buffer) -> usize {
    buffer
        .area
        .positions()
        .filter(|&p| buffer[p].symbol() != " ")
        .count()
}

/// Given: a pipe at (2, 2) that arrived moving right.
/// When: it moves right, then down, then down.
/// Then: (2, 2) is the horizontal rune, (3, 2) the corner that joins left
/// and down, (3, 3) the vertical rune, all in one colour.
///
/// Why it matters: a wrong corner shows as a pipe that breaks at every
/// turn, and the runes have to come from the same set as the frame.
#[test]
fn a_turn_from_right_to_down_draws_the_joining_corner() {
    // Given, when:
    let area = Rect::new(0, 0, 10, 6);
    let scene = Pipes::trace(
        Rng::seed_from_u64(7),
        &theme::ROSE_PINE,
        area,
        (2, 2),
        Direction::Right,
        &[Direction::Right, Direction::Down, Direction::Down],
    );

    // Then:
    let buffer = render(&scene, area);
    let set = theme::BORDER.to_border_set();
    assert_eq!(buffer[(2, 2)].symbol(), set.horizontal_top);
    assert_eq!(buffer[(3, 2)].symbol(), set.top_right);
    assert_eq!(buffer[(3, 3)].symbol(), set.vertical_left);
    assert_eq!(buffer[(2, 2)].fg, buffer[(3, 2)].fg);
    assert_eq!(buffer[(3, 2)].fg, buffer[(3, 3)].fg);
    assert_ne!(buffer[(2, 2)].fg, theme::ROSE_PINE.base);
}

/// Given: a pipe in the last column, moving right.
/// When: it moves right twice.
/// Then: the second rune lands in the first column of the same row.
#[test]
fn a_pipe_that_leaves_the_right_edge_enters_at_the_left() {
    // Given, when:
    let area = Rect::new(0, 0, 10, 6);
    let scene = Pipes::trace(
        Rng::seed_from_u64(7),
        &theme::ROSE_PINE,
        area,
        (9, 2),
        Direction::Right,
        &[Direction::Right, Direction::Right],
    );

    // Then:
    let buffer = render(&scene, area);
    let set = theme::BORDER.to_border_set();
    assert_eq!(buffer[(9, 2)].symbol(), set.horizontal_top);
    assert_eq!(buffer[(0, 2)].symbol(), set.horizontal_top);
    assert_eq!(drawn(&buffer), 2);
}

/// Given: the scene on an eight by four area.
/// When: it ticks three hundred times, with a render after each.
/// Then: the drawn cells never exceed the fill share, and the count drops
/// at least once.
///
/// Why it matters: without the clear, a wrapping walk paints every cell
/// and the scene becomes a static grid of runes.
#[test]
fn the_screen_clears_at_the_fill_share() {
    // Given:
    let area = Rect::new(0, 0, 8, 4);
    let limit = usize::try_from(area.area()).unwrap() * FILL_PERCENT / 100;
    let mut scene = Pipes::new(Rng::seed_from_u64(7), &theme::ROSE_PINE);
    let mut previous = 0;
    let mut dropped = false;

    // When:
    for tick in 0..300 {
        scene.tick(area);
        let count = drawn(&render(&scene, area));

        // Then:
        assert!(count <= limit, "tick {tick}: {count} drawn, limit {limit}");
        dropped |= count < previous;
        previous = count;
    }
    assert!(dropped, "the screen never cleared");
}

/// Given: pipes from many seeds on a screen wide enough for the most
/// pipes at once.
/// When: each runs for a while.
/// Then: no cell is in the marker colour.
///
/// Why it matters: the focused pane shows corner marks in the palette's
/// marker colour. A pipe in that colour that turns at a pane corner draws
/// the same rune in the same colour, and an unfocused pane then reads as
/// focused.
#[test]
fn no_pipe_is_in_the_marker_colour() {
    // Given:
    let area = Rect::new(0, 0, 240, 20);
    for seed in 0..32 {
        let mut scene = Pipes::new(Rng::seed_from_u64(seed), &theme::ROSE_PINE);

        // When:
        for _ in 0..50 {
            scene.tick(area);
        }

        // Then:
        let buffer = render(&scene, area);
        let marked = buffer
            .area
            .positions()
            .find(|&p| buffer[p].fg == theme::ROSE_PINE.marker());
        assert_eq!(marked, None, "seed {seed} drew a pipe in the marker colour");
    }
}
