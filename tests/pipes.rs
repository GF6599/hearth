#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod pixels;

use std::collections::BTreeSet;

use hearth::scene::{Rng, Scene};
use hearth::scenes::pipes::{Direction, FILL_PERCENT, Pipes};
use hearth::theme;
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

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

/// Each pixel that shows a colour other than `base`, with that colour, in
/// a render whose area starts at the top left of its buffer.
fn lit(buffer: &Buffer, base: Color) -> Vec<((usize, usize), Color)> {
    let rows = pixels::grid(buffer, buffer.area, |_, cell| {
        pixels::shown(cell).unwrap_or_else(|| panic!("{:?} is not an octant glyph", cell.symbol()))
    });
    rows.into_iter()
        .enumerate()
        .flat_map(|(y, row)| {
            row.into_iter()
                .enumerate()
                .map(move |(x, colour)| ((x, y), colour))
        })
        .filter(|(_, colour)| *colour != base)
        .collect()
}

fn places(lit: &[((usize, usize), Color)]) -> BTreeSet<(usize, usize)> {
    lit.iter().map(|(place, _)| *place).collect()
}

/// Given: a pipe at pixel (4, 9) that arrived moving right.
/// When: it moves right, then down, then down.
/// Then: one line of pixels in one colour runs two pixels right from
/// (4, 9), then turns at (6, 9) and runs down to (6, 16), and no other
/// pixel is lit.
///
/// Why it matters: a step spans one cell, two pixels across or four down,
/// so the pipe grows as fast on screen in cells as it did in runes. A gap
/// or a stray pixel at the turn shows as a pipe that breaks at every
/// corner.
#[test]
fn a_turn_from_right_to_down_draws_a_joined_line_of_pixels() {
    // Given, when:
    let area = Rect::new(0, 0, 10, 6);
    let scene = Pipes::trace(
        Rng::seed_from_u64(7),
        &theme::ROSE_PINE,
        area,
        (4, 9),
        Direction::Right,
        &[Direction::Right, Direction::Down, Direction::Down],
    );

    // Then:
    let lit = lit(&render(&scene, area), theme::ROSE_PINE.base);
    let line: BTreeSet<(usize, usize)> = [(4, 9), (5, 9)]
        .into_iter()
        .chain((9..=16).map(|y| (6, y)))
        .collect();
    assert_eq!(places(&lit), line);
    let colours: BTreeSet<String> = lit.iter().map(|(_, c)| format!("{c:?}")).collect();
    assert_eq!(colours.len(), 1, "the line shows {colours:?}");
}

/// Given: a pipe at pixel (18, 9), in the last column, moving right.
/// When: it moves right twice.
/// Then: the second step lights the first two pixels of the same pixel
/// row, and only the two cells at either edge show the pipe.
#[test]
fn a_pipe_that_leaves_the_right_edge_enters_at_the_left() {
    // Given, when:
    let area = Rect::new(0, 0, 10, 6);
    let scene = Pipes::trace(
        Rng::seed_from_u64(7),
        &theme::ROSE_PINE,
        area,
        (18, 9),
        Direction::Right,
        &[Direction::Right, Direction::Right],
    );

    // Then:
    let buffer = render(&scene, area);
    let lit = lit(&buffer, theme::ROSE_PINE.base);
    let line = BTreeSet::from([(18, 9), (19, 9), (0, 9), (1, 9)]);
    assert_eq!(places(&lit), line);
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
/// Then: no pixel is in the marker colour.
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
            let marked = lit(&buffer, palette.base)
                .into_iter()
                .find(|(_, colour)| *colour == palette.marker());
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

/// Given: a pipe at pixel (18, 9), in the last column, moving right.
/// When: it moves right twice, so it loops to the first column.
/// Then: the cell after the loop is in another colour than the one before.
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
        (18, 9),
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

/// Given: two scenes from the same seed.
/// When: each ticks sixty times on an eighty by twenty-four area.
/// Then: the frames are equal.
///
/// Why it matters: a seed replays a run, so every spawn, turn and colour
/// must come from the seeded source.
#[test]
fn the_same_seed_lays_the_same_pipes() {
    // Given:
    let area = Rect::new(0, 0, 80, 24);
    let mut first = Pipes::new(Rng::seed_from_u64(7), &theme::ROSE_PINE);
    let mut second = Pipes::new(Rng::seed_from_u64(7), &theme::ROSE_PINE);

    // When:
    for _ in 0..60 {
        first.tick(area);
        second.tick(area);
    }

    // Then:
    assert_eq!(render(&first, area), render(&second, area));
}
