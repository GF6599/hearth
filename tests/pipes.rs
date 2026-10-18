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

/// Given: pipes from many seeds, in every theme, on a screen wide enough
/// for the most pipes at once.
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
    for (name, palette) in theme::THEMES {
        for seed in 0..32 {
            let mut scene = Pipes::new(Rng::seed_from_u64(seed), palette);

            // When:
            for _ in 0..50 {
                scene.tick(area);
            }

            // Then:
            let buffer = render(&scene, area);
            let marked = buffer
                .area
                .positions()
                .find(|&p| buffer[p].fg == palette.marker());
            assert_eq!(
                marked, None,
                "{name} seed {seed} drew a pipe in the marker colour"
            );
        }
    }
}

/// Each colour drawn, with the number of separate pipes in it: groups of
/// drawn cells of that colour that touch, diagonals included.
fn pipes_by_colour(buffer: &Buffer) -> Vec<(String, usize)> {
    let mut left: Vec<(u16, u16, String)> = buffer
        .area
        .positions()
        .filter(|&p| buffer[p].symbol() != " ")
        .map(|p| (p.x, p.y, format!("{:?}", buffer[p].fg)))
        .collect();
    let mut groups: Vec<String> = Vec::new();
    while let Some(start) = left.pop() {
        let mut open = vec![start.clone()];
        while let Some((x, y, colour)) = open.pop() {
            let (touching, rest): (Vec<_>, Vec<_>) =
                left.into_iter().partition(|(ox, oy, other)| {
                    *other == colour && x.abs_diff(*ox) <= 1 && y.abs_diff(*oy) <= 1
                });
            open.extend(touching);
            left = rest;
        }
        groups.push(start.2);
    }
    groups.sort();
    let mut counted: Vec<(String, usize)> = Vec::new();
    for colour in groups {
        match counted.last_mut() {
            Some((last, count)) if *last == colour => *count += 1,
            _ => counted.push((colour, 1)),
        }
    }
    counted
}

/// Each live pipe shows a colour that no other live pipe shows.
/// Given: screens wide enough for two to five pipes, in every theme, at
/// many seeds.
/// When: the scene ticks once, so each pipe has drawn its first cell.
/// Then: the cells show as many colours as there are pipes, unless the
/// theme has fewer distinct colours for pipes than that.
///
/// Why it matters: two or three pipes that draw their colours at random
/// often share one, and a screen of pipes in one colour reads as one pipe.
#[test]
fn each_live_pipe_shows_its_own_colour() {
    for (name, palette) in theme::THEMES {
        // Given:
        let mut colours = vec![
            palette.love,
            palette.gold,
            palette.rose,
            palette.pine,
            palette.foam,
            palette.iris,
        ];
        colours.retain(|colour| *colour != palette.marker());
        colours.sort_by_key(|colour| format!("{colour:?}"));
        colours.dedup();
        for width in [80, 120, 160, 200] {
            let area = Rect::new(0, 0, width, 30);
            for seed in 0..32 {
                let mut scene = Pipes::new(Rng::seed_from_u64(seed), palette);

                // When:
                scene.tick(area);

                // Then:
                let buffer = render(&scene, area);
                let pipes = pipes_by_colour(&buffer);
                let count: usize = pipes.iter().map(|(_, groups)| groups).sum();
                if count <= colours.len() {
                    assert!(
                        pipes.iter().all(|(_, groups)| *groups == 1),
                        "{name} width {width} seed {seed}: two pipes share a colour: {pipes:?}"
                    );
                } else {
                    assert_eq!(
                        pipes.len(),
                        colours.len(),
                        "{name} width {width} seed {seed}: a colour is left out: {pipes:?}"
                    );
                }
            }
        }
    }
}

/// Given: a pipe in the last column, moving right.
/// When: it moves right twice, so it loops to the first column.
/// Then: the rune after the loop is in another colour than the one before.
///
/// Why it matters: a pipe that keeps one colour for the whole screen
/// leaves one colour on a narrow pane, which holds a single pipe. In
/// pipes.sh a pipe takes a new colour when it loops at an edge.
#[test]
fn a_pipe_takes_a_new_colour_when_it_loops_at_an_edge() {
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
    assert_ne!(buffer[(9, 2)].fg, buffer[(0, 2)].fg);
}

/// Given: a screen narrow enough for one pipe.
/// When: it ticks twenty times.
/// Then: at least twenty-eight cells are drawn, three for every two ticks
/// less a few the pipe crosses twice.
///
/// Why it matters: at one cell a tick the pipes crawl.
#[test]
fn a_pipe_lays_three_cells_every_two_ticks() {
    // Given:
    let area = Rect::new(0, 0, 60, 30);
    let mut scene = Pipes::new(Rng::seed_from_u64(7), &theme::ROSE_PINE);

    // When:
    for _ in 0..20 {
        scene.tick(area);
    }

    // Then:
    let count = drawn(&render(&scene, area));
    assert!(count >= 28, "{count} cells drawn in twenty ticks");
}
