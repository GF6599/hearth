#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod pixels;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use hearth::cli::Cli;
use hearth::config::{Background, Config};
use hearth::corpus::Corpus;
use hearth::history::{DEPTH, History};
use hearth::octant::{self, CELL_HEIGHT, CELL_WIDTH};
use hearth::scene::{ALL, Rng, Scene, SceneKind, Settings};
use hearth::scenes::river::{NOTICE, READING, River};
use hearth::scenes::typing::Speed;
use hearth::theme::{self, Palette};
use rand::SeedableRng;
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::style::Color;

const AREA: Rect = Rect::new(0, 0, 100, 40);
const PALETTE: &Palette = &theme::ROSE_PINE;

/// A fresh directory under the system temp dir, emptied first.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hearth-river-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Runs git in `repo` with a fixed author and no hooks or signing from the
/// machine's own config, and answers what it printed.
fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args([
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?} failed in {repo:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn commit(repo: &Path, file: &str, subject: &str) {
    fs::write(repo.join(file), subject).unwrap();
    git(repo, &["add", file]);
    git(repo, &["commit", "-q", "-m", subject]);
}

/// A repository whose history splits and joins: `start` on main, `feature
/// work` on a branch from it, `main work` on main, and a merge of the
/// branch into main.
fn branched(name: &str) -> PathBuf {
    let repo = scratch(name);
    git(&repo, &["init", "-q", "-b", "main"]);
    commit(&repo, "a.txt", "start");
    git(&repo, &["checkout", "-q", "-b", "feature"]);
    commit(&repo, "b.txt", "feature work");
    git(&repo, &["checkout", "-q", "main"]);
    commit(&repo, "c.txt", "main work");
    git(
        &repo,
        &["merge", "-q", "--no-ff", "-m", "merge feature", "feature"],
    );
    repo
}

/// A linear history of `count` commits, the first named `commit 0`.
fn linear(name: &str, count: usize) -> PathBuf {
    let repo = scratch(name);
    git(&repo, &["init", "-q", "-b", "main"]);
    for index in 0..count {
        commit(&repo, "a.txt", &format!("commit {index}"));
    }
    repo
}

/// The river over `corpus`, ticked until its history has landed, with a
/// bound, so the read off the UI thread has finished.
fn landed(corpus: Corpus) -> River {
    landed_in(corpus, PALETTE)
}

/// The river over `corpus` in `palette`, ticked until its history has
/// landed.
fn landed_in(corpus: Corpus, palette: &'static Palette) -> River {
    let mut scene = River::new(Rng::seed_from_u64(7), palette, Arc::new(corpus));
    for _ in 0..1000 {
        scene.tick(AREA);
        if !rows(&render(&scene, AREA, AREA), AREA)
            .iter()
            .any(|row| row.contains(READING))
        {
            return scene;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("the history never landed");
}

fn river(root: &Path) -> River {
    landed(Corpus::default().with_history_root(root.to_path_buf()))
}

fn render(scene: &River, area: Rect, canvas: Rect) -> Buffer {
    let mut buffer = Buffer::empty(canvas);
    scene.render(area, &mut buffer);
    buffer
}

fn rows(buffer: &Buffer, area: Rect) -> Vec<String> {
    (area.y..area.y + area.height)
        .map(|y| {
            (area.x..area.x + area.width)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect()
        })
        .collect()
}

type Cells = Vec<(u16, u16)>;

/// What a pixel of the river shows. The scene shades each part with mixes
/// of theme colours, so a test reads a part by its hue and its brightness
/// and not by one exact colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    /// The ground beyond the banks, in the base.
    Ground,
    /// The bed of the river: dark, and not the base.
    Bed,
    /// Sand: warm, red over green over blue, as gold is.
    Sand,
    /// Water, as pine is: more green than red.
    Water,
    /// Foam: water brighter than the mix half way from pine to foam.
    Foam,
    /// A stone: bright, blue over red over green, as subtle is.
    Stone,
    /// The stone of a merge: a stone with clearly more red than green, as
    /// iris is.
    Merge,
    /// A colour of no part of the river.
    Other,
}

/// The farthest each part lies from its role. The greys of Rose Pine share
/// one hue, so the bed and the stones are read by their distance to their
/// own roles and not by hue alone.
const SAND_REACH: f32 = 64.0;
const BED_REACH: f32 = 40.0;
const STONE_REACH: f32 = 48.0;

/// The part each colour shows, read once for each colour, as a test reads
/// millions of pixels.
fn part(colour: Color) -> Part {
    thread_local! {
        static READ: RefCell<HashMap<Color, Part>> = RefCell::new(HashMap::new());
    }
    READ.with(|read| {
        *read
            .borrow_mut()
            .entry(colour)
            .or_insert_with(|| read_part(colour))
    })
}

fn read_part(colour: Color) -> Part {
    let Color::Rgb(r, g, b) = colour else {
        panic!("{colour:?} is not a colour of the theme");
    };
    let nearest = |roles: &[Color]| {
        roles
            .iter()
            .copied()
            .min_by(|one, other| {
                pixels::distance(colour, *one)
                    .floor()
                    .total_cmp(&pixels::distance(colour, *other).floor())
            })
            .unwrap()
    };
    let greys = [
        PALETTE.base,
        PALETTE.surface,
        PALETTE.overlay,
        PALETTE.muted,
        PALETTE.subtle,
    ];
    let stone = nearest(&[PALETTE.subtle, PALETTE.iris]);
    if colour == PALETTE.base {
        Part::Ground
    } else if g > r {
        let half = theme::luma(octant::mix(PALETTE.pine, PALETTE.foam, 0.5));
        if theme::luma(colour) > half {
            Part::Foam
        } else {
            Part::Water
        }
    } else if r > g && g >= b {
        if pixels::distance(colour, PALETTE.gold).floor() <= SAND_REACH {
            Part::Sand
        } else {
            Part::Other
        }
    } else if nearest(&greys) == PALETTE.overlay
        && pixels::distance(colour, PALETTE.overlay).floor() <= BED_REACH
    {
        Part::Bed
    } else if pixels::distance(colour, stone).floor() <= STONE_REACH {
        if r >= g + 16 {
            Part::Merge
        } else {
            Part::Stone
        }
    } else {
        Part::Other
    }
}

/// Given: the Rose Pine roles of each part of the river, and the roles the
/// river does not draw as a part.
/// When: each is read as a part.
/// Then: each role of a part reads as its own part, and each other role
/// reads as no part.
///
/// Why it matters: the other tests read the river by its parts, and a part
/// read wrong would pass a broken picture.
#[test]
fn each_role_reads_as_its_part_of_the_river() {
    // Given:
    for (role, expected) in [
        (PALETTE.base, Part::Ground),
        (PALETTE.overlay, Part::Bed),
        (PALETTE.gold, Part::Sand),
        (PALETTE.pine, Part::Water),
        (PALETTE.foam, Part::Foam),
        (PALETTE.subtle, Part::Stone),
        (PALETTE.iris, Part::Merge),
        (PALETTE.surface, Part::Other),
        (PALETTE.muted, Part::Other),
        (PALETTE.text, Part::Other),
        (PALETTE.love, Part::Other),
        (PALETTE.rose, Part::Other),
    ] {
        // When, then:
        assert_eq!(part(role), expected, "{role:?}");
    }
}

/// How many pixels of a cell show one of `parts`, and none for a cell
/// that holds no octant glyph, such as a letter of a subject.
fn count(cell: &Cell, parts: &[Part]) -> usize {
    pixels::shown(cell).map_or(0, |shown| {
        shown
            .iter()
            .filter(|colour| parts.contains(&part(**colour)))
            .count()
    })
}

/// The cells of the area that draw something in one of `parts`.
fn cells(buffer: &Buffer, area: Rect, parts: &[Part]) -> Cells {
    area.positions()
        .filter(|position| count(&buffer[*position], parts) > 0)
        .map(|position| (position.x, position.y))
        .collect()
}

/// The cells of the lanes: water or foam in five pixels or more. A ripple
/// on the open water is water too, but it lies on one row of pixels, two
/// at the most in a cell.
fn lanes(buffer: &Buffer, area: Rect) -> Cells {
    area.positions()
        .filter(|position| count(&buffer[*position], &[Part::Water, Part::Foam]) >= 5)
        .map(|position| (position.x, position.y))
        .collect()
}

/// The groups of `cells` that touch across a side, each as its cells.
fn groups(mut left: Cells) -> Vec<Cells> {
    let mut found = Vec::new();
    while let Some(start) = left.pop() {
        let mut group = vec![start];
        let mut open = vec![start];
        while let Some((x, y)) = open.pop() {
            let (touching, rest): (Vec<_>, Vec<_>) = left
                .into_iter()
                .partition(|(other_x, other_y)| x.abs_diff(*other_x) + y.abs_diff(*other_y) == 1);
            open.extend(&touching);
            group.extend(touching);
            left = rest;
        }
        found.push(group);
    }
    found
}

/// The runs of touching water cells in one row.
fn runs(water: &[(u16, u16)], row: u16) -> usize {
    let mut columns: Vec<u16> = water
        .iter()
        .filter(|(_, y)| *y == row)
        .map(|(x, _)| *x)
        .collect();
    columns.sort_unstable();
    columns
        .windows(2)
        .filter(|pair| pair[1] > pair[0] + 1)
        .count()
        + usize::from(!columns.is_empty())
}

/// Given: a repository whose history splits onto a branch and merges back.
/// When: its history is read.
/// Then: the four commits are read, the newest first, each with its
/// subject, and the merge has the main and the branch commit as parents.
///
/// Why it matters: a river drawn from the wrong parents joins lanes that
/// never met.
#[test]
fn the_history_holds_the_commits_and_their_parents() {
    // Given:
    let repo = branched("read");

    // When:
    let history = History::read(&repo);

    // Then:
    let subjects: Vec<&str> = history
        .commits()
        .iter()
        .map(|commit| commit.subject.as_str())
        .collect();
    assert_eq!(subjects.len(), 4, "{subjects:?}");
    assert_eq!(subjects[0], "merge feature");
    assert_eq!(subjects[3], "start");
    let merge = &history.commits()[0];
    assert_eq!(merge.hash, git(&repo, &["rev-parse", "main"]));
    assert_eq!(
        merge.parents,
        [
            git(&repo, &["rev-parse", "main^1"]),
            git(&repo, &["rev-parse", "feature"])
        ]
    );
    let _ = fs::remove_dir_all(&repo);
}

/// Given: a repository with more commits than the river reads.
/// When: its history is read.
/// Then: it holds the newest `DEPTH` commits, the newest first.
///
/// Why it matters: a repository with years of history would make the
/// first frame wait on the whole of it.
#[test]
fn the_read_stops_at_the_bound() {
    // Given:
    let repo = scratch("bound");
    git(&repo, &["init", "-q", "-b", "main"]);
    let total = DEPTH + 10;
    let mut stream = String::new();
    for index in 0..total {
        let message = format!("commit {index}");
        stream.push_str(&format!(
            "commit refs/heads/main\nmark :{}\ncommitter test <test@example.com> {} +0000\ndata {}\n{message}\n",
            index + 1,
            1_700_000_000 + index,
            message.len(),
        ));
        if index > 0 {
            stream.push_str(&format!("from :{index}\n"));
        }
        stream.push('\n');
    }
    let mut import = Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["fast-import", "--quiet"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    import
        .stdin
        .take()
        .unwrap()
        .write_all(stream.as_bytes())
        .unwrap();
    assert!(import.wait().unwrap().success());

    // When:
    let history = History::read(&repo);

    // Then:
    assert_eq!(history.commits().len(), DEPTH);
    assert_eq!(
        history.commits()[0].subject,
        format!("commit {}", total - 1)
    );
    let _ = fs::remove_dir_all(&repo);
}

/// The river draws two lanes that split and meet again.
/// Given: the river of a repository whose history splits onto a branch
/// and merges back, in a pane tall enough for all of it.
/// When: it renders.
/// Then: it draws a stone for each of the four commits, the merge's stone
/// in iris, and between the newest and the oldest stone some row holds
/// two lanes of water apart. No water flows above the newest stone or
/// below the oldest, a root.
///
/// Why it matters: a branch that never shows as its own lane hides the
/// shape of the history, and water past the root reads as commits that
/// were never read.
#[test]
fn a_branch_that_splits_and_merges_draws_two_lanes_that_meet() {
    // Given:
    let repo = branched("lanes");
    let mut scene = river(&repo);

    // When:
    scene.tick(AREA);
    let buffer = render(&scene, AREA, AREA);

    // Then:
    let stones = groups(cells(&buffer, AREA, &[Part::Stone, Part::Merge]));
    let merges = groups(cells(&buffer, AREA, &[Part::Merge]));
    assert_eq!(stones.len(), 4, "{:#?}", rows(&buffer, AREA));
    assert_eq!(merges.len(), 1, "{:#?}", rows(&buffer, AREA));
    let stone_rows: Vec<u16> = stones.iter().flatten().map(|(_, y)| *y).collect();
    let (top, bottom) = (
        *stone_rows.iter().min().unwrap(),
        *stone_rows.iter().max().unwrap(),
    );
    let water = lanes(&buffer, AREA);
    assert!(
        water.iter().all(|(_, y)| (top..=bottom).contains(y)),
        "water outside rows {top} to {bottom}: {:#?}",
        rows(&buffer, AREA)
    );
    assert!(
        (top + 1..bottom).any(|row| runs(&water, row) == 2),
        "no row holds two lanes: {:#?}",
        rows(&buffer, AREA)
    );
    let _ = fs::remove_dir_all(&repo);
}

/// Given: a repository whose only commit's subject holds an escape
/// sequence, a bell and a C1 control.
/// When: its history is read, and the river renders it with that commit
/// at the middle row.
/// Then: the subject is read without the control characters, and the
/// pane shows it so, with no control character in any cell.
///
/// Why it matters: a subject is text anyone with commit access wrote, and
/// an escape sequence in it would reach the terminal as a command.
#[test]
fn a_subject_with_control_characters_shows_without_them() {
    // Given:
    let repo = scratch("escape");
    git(&repo, &["init", "-q", "-b", "main"]);
    commit(
        &repo,
        "a.txt",
        "fix \u{1b}[31mred\u{1b}[0m\u{7} bell \u{9b}c1",
    );

    // When:
    let history = History::read(&repo);
    let mut scene = river(&repo);
    scene.tick(AREA);
    let buffer = render(&scene, AREA, AREA);

    // Then:
    let clean = "fix [31mred[0m bell c1";
    assert_eq!(history.commits()[0].subject, clean);
    let rows = rows(&buffer, AREA);
    assert!(rows.iter().any(|row| row.contains(clean)), "{rows:#?}");
    assert!(
        rows.iter()
            .flat_map(|row| row.chars())
            .all(|c| !c.is_control()),
        "{rows:#?}"
    );
    let _ = fs::remove_dir_all(&repo);
}

/// Given: a repository whose only commit's subject is in Chinese, two
/// cells wide a glyph.
/// When: its history is read, and the river renders it with that commit
/// at the middle row.
/// Then: the subject keeps its wide glyphs, and the pane shows them.
///
/// Why it matters: a history written in a language with wide glyphs would
/// show only its spaces and Latin words.
#[test]
fn a_subject_in_wide_glyphs_keeps_them() {
    // Given:
    let repo = scratch("wide");
    git(&repo, &["init", "-q", "-b", "main"]);
    commit(&repo, "a.txt", "修复登录 bug");

    // When:
    let history = History::read(&repo);
    let mut scene = river(&repo);
    scene.tick(AREA);
    let buffer = render(&scene, AREA, AREA);

    // Then:
    assert_eq!(history.commits()[0].subject, "修复登录 bug");
    let rows = rows(&buffer, AREA);
    assert!(
        rows.iter()
            .any(|row| row.contains('修') && row.contains('录')),
        "{rows:#?}"
    );
    let _ = fs::remove_dir_all(&repo);
}

/// Given: a directory that is not a git repository.
/// When: its history is read, and the river renders it.
/// Then: the history is empty, and the pane shows the notice.
///
/// Why it matters: hearth started outside a repository must still show
/// the river scene as something, not as a blank pane or a crash.
#[test]
fn a_directory_outside_a_repository_shows_the_notice() {
    // Given:
    let dir = scratch("plain");

    // When:
    let history = History::read(&dir);
    let mut scene = river(&dir);
    scene.tick(AREA);
    let buffer = render(&scene, AREA, AREA);

    // Then:
    assert!(history.commits().is_empty());
    let rows = rows(&buffer, AREA);
    assert!(rows.iter().any(|row| row.contains(NOTICE)), "{rows:#?}");
    let _ = fs::remove_dir_all(&dir);
}

/// Given: a repository with three commits, standing in for a remote, and
/// an empty cache.
/// When: the corpus is cloned from its `file://` address.
/// Then: the river over it draws a stone for each of the three commits.
///
/// Why it matters: a clone of one commit draws a river of one stone.
#[test]
fn a_clone_holds_enough_history_for_the_river() {
    // Given:
    let remote = scratch("remote");
    git(&remote, &["init", "-q", "-b", "main"]);
    for subject in ["one", "two", "three"] {
        commit(&remote, "a.txt", subject);
    }
    let cache = scratch("cache");
    let address = format!("file://{}", remote.display());

    // When:
    let mut scene = landed(Corpus::cloned(&address, &cache).unwrap());
    scene.tick(AREA);

    // Then:
    let buffer = render(&scene, AREA, AREA);
    let stones = groups(cells(&buffer, AREA, &[Part::Stone, Part::Merge]));
    assert_eq!(stones.len(), 3, "{:#?}", rows(&buffer, AREA));
    let _ = fs::remove_dir_all(&remote);
    let _ = fs::remove_dir_all(&cache);
}

/// Given: hearth started in a repository with no `--repo` and no `repo`
/// key.
/// When: the corpus is read, and the river is drawn over it.
/// Then: the river draws that repository's four commits, with its oldest
/// subject at the middle row.
///
/// Why it matters: with nothing chosen, the river draws the project the
/// user is in, which is the history they know.
#[test]
fn with_no_repo_the_history_is_the_current_directory() {
    // Given:
    let repo = branched("current");
    std::env::set_current_dir(&repo).unwrap();

    // When:
    let corpus = Cli::parse_from(["hearth"])
        .corpus(&Config::default())
        .unwrap();
    let mut scene = landed(corpus);
    scene.tick(AREA);

    // Then:
    let buffer = render(&scene, AREA, AREA);
    let stones = groups(cells(&buffer, AREA, &[Part::Stone, Part::Merge]));
    assert_eq!(stones.len(), 4, "{:#?}", rows(&buffer, AREA));
    assert_eq!(middle_subject(&buffer, AREA).as_deref(), Some("start"));
    std::env::set_current_dir(std::env::temp_dir()).unwrap();
    let _ = fs::remove_dir_all(&repo);
}

/// Given: a river that ticked on a hundred by forty area.
/// When: it renders on a twenty by six area inside a larger canvas, then
/// ticks and renders on that area, on one cell, on a row of no height, and
/// on none.
/// Then: it draws without a panic, and no cell outside the area is touched.
#[test]
fn a_smaller_area_draws_only_inside_it() {
    // Given:
    let repo = branched("small");
    let canvas = Rect::new(0, 0, 40, 12);
    let mut scene = river(&repo);
    scene.tick(AREA);
    let inside_only = |buffer: &Buffer, area: Rect| {
        for position in canvas.positions() {
            if !area.contains(position) {
                assert_eq!(buffer[position].symbol(), " ", "glyph outside {area:?}");
                assert_eq!(buffer[position].fg, Color::Reset, "colour outside {area:?}");
            }
        }
    };

    // When, then:
    let first = Rect::new(3, 2, 20, 6);
    inside_only(&render(&scene, first, canvas), first);
    for area in [
        first,
        Rect::new(3, 2, 1, 1),
        Rect::new(3, 2, 5, 0),
        Rect::new(3, 2, 0, 0),
    ] {
        scene.tick(area);
        inside_only(&render(&scene, area, canvas), area);
    }
    let _ = fs::remove_dir_all(&repo);
}

/// Given: the scene name `river`.
/// When: it is looked up and built with a corpus that has no history, then
/// ticked and rendered.
/// Then: the name finds a scene, and it shows the notice.
///
/// Why it matters: a scene that is built but not listed cannot be chosen
/// from the command line, the picker or Tab.
#[test]
fn river_is_a_scene_by_name() {
    // When:
    let kind = SceneKind::from_name("river").expect("river is listed");
    let mut scene = kind.build(
        7,
        PALETTE,
        Background::Theme,
        &Arc::new(Corpus::embedded()),
        Speed::DEFAULT,
        Settings::DEFAULT,
    );
    scene.tick(AREA);

    // Then:
    let mut buffer = Buffer::empty(AREA);
    scene.render(AREA, &mut buffer);
    let rows = rows(&buffer, AREA);
    assert!(rows.iter().any(|row| row.contains(NOTICE)), "{rows:#?}");
}

/// The subject at the pane's middle row, if one shows there: the piece of
/// the row between octant glyphs that holds a letter or a digit, because
/// water and sand lie on both sides of it. A space is an octant glyph too,
/// and the subject holds spaces, so a space does not split it.
fn middle_subject(buffer: &Buffer, area: Rect) -> Option<String> {
    let row = &rows(buffer, area)[usize::from(area.height / 2)];
    row.split(|c: char| c != ' ' && pixels::shown(Cell::default().set_char(c)).is_some())
        .map(str::trim)
        .find(|piece| piece.chars().any(|c| c.is_ascii_alphanumeric()))
        .map(str::to_string)
}

/// Each subject that shows at the middle row as the river ticks, once for
/// each time it comes there.
fn subjects_in_turn(scene: &mut River, ticks: u32) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    let mut last = None;
    for _ in 0..ticks {
        scene.tick(AREA);
        let subject = middle_subject(&render(scene, AREA, AREA), AREA);
        if subject.is_some() && subject != last {
            seen.push(subject.clone().unwrap());
        }
        last = subject;
    }
    seen
}

/// The river runs from the oldest commit to the newest and starts again.
/// Given: the river of a repository whose history splits and merges.
/// When: it ticks for a while.
/// Then: the subjects at the middle row come in the order of the commits
/// from the oldest to the newest, and then the oldest comes again.
///
/// Why it matters: a river that stops at the newest commit leaves a
/// screensaver that no longer moves.
#[test]
fn the_middle_subject_runs_from_the_oldest_to_the_newest_and_again() {
    // Given:
    let repo = branched("turn");
    let history = History::read(&repo);
    let mut scene = river(&repo);

    // When:
    let seen = subjects_in_turn(&mut scene, 2000);

    // Then:
    let mut expected: Vec<String> = history
        .commits()
        .iter()
        .rev()
        .map(|commit| commit.subject.clone())
        .collect();
    expected.push("start".to_string());
    assert!(seen.len() >= expected.len(), "{seen:?}");
    assert_eq!(seen[..expected.len()], expected[..], "{seen:?}");
    let _ = fs::remove_dir_all(&repo);
}

/// The stones flow down the pane.
/// Given: the river of a repository whose history splits and merges.
/// When: it ticks for a while, and the top row of its stones is read in
/// each frame.
/// Then: that row moves down in many frames, and it moves up no more often
/// than the river starts again from the oldest commit.
///
/// Why it matters: stones that drift up or jitter read as a river flowing
/// back, and the history as running backward.
#[test]
fn the_stones_flow_down_the_pane() {
    // Given:
    let repo = branched("flow");
    let history = History::read(&repo);
    let oldest = history.commits().last().unwrap().subject.clone();
    let mut scene = river(&repo);
    let (mut down, mut up, mut again) = (0, 0, 0);
    let mut before: Option<(u16, Option<String>)> = None;

    for _ in 0..2000 {
        // When:
        scene.tick(AREA);
        let buffer = render(&scene, AREA, AREA);
        let stones = cells(&buffer, AREA, &[Part::Stone, Part::Merge]);
        let Some(top) = stones.iter().map(|(_, y)| *y).min() else {
            continue;
        };
        let subject = middle_subject(&buffer, AREA);

        // Then:
        if let Some((last_top, last_subject)) = &before {
            if top > *last_top {
                down += 1;
            } else if top < *last_top {
                up += 1;
            }
            if subject.as_ref() == Some(&oldest) && last_subject.as_ref() != Some(&oldest) {
                again += 1;
            }
        }
        before = Some((top, subject));
    }
    assert!(again >= 1, "the river never started again");
    assert!(down >= 10, "the stones moved down in {down} frames");
    assert!(
        up <= again,
        "the stones moved up {up} times in {again} loops"
    );
    let _ = fs::remove_dir_all(&repo);
}

/// The water flows along the lanes faster than the stones.
/// Given: the river of a repository whose history splits and merges.
/// When: it ticks, and the foam cells are read in frames where no stone has
/// moved since the frame before.
/// Then: foam shows, and in some of those frames it has moved.
///
/// Why it matters: water that only moves with the stones reads as a picture
/// sliding down, not as a river.
#[test]
fn the_water_flows_along_the_lanes() {
    // Given:
    let repo = branched("water");
    let mut scene = river(&repo);
    let (mut foam_frames, mut moved) = (0, 0);
    let mut before: Option<(Cells, Cells)> = None;

    for _ in 0..400 {
        // When:
        scene.tick(AREA);
        let buffer = render(&scene, AREA, AREA);
        let stones = cells(&buffer, AREA, &[Part::Stone, Part::Merge]);
        let foam = cells(&buffer, AREA, &[Part::Foam]);

        // Then:
        if !foam.is_empty() {
            foam_frames += 1;
        }
        if let Some((last_stones, last_foam)) = &before
            && *last_stones == stones
            && *last_foam != foam
        {
            moved += 1;
        }
        before = Some((stones, foam));
    }
    assert!(foam_frames >= 200, "foam showed in {foam_frames} frames");
    assert!(moved >= 20, "the foam moved alone in {moved} frames");
    let _ = fs::remove_dir_all(&repo);
}

/// Given: a river just built over a repository with four commits.
/// When: it renders before its first tick, then ticks until the history
/// lands, with a bound.
/// Then: the first frame shows that the history is being read, and once
/// it lands the river draws the four stones.
///
/// Why it matters: a read that holds the first frame freezes the app on a
/// large repository, and a read that never lands leaves the notice for
/// good.
#[test]
fn the_river_reads_the_history_while_it_shows_a_notice() {
    // Given:
    let repo = branched("reading");
    let corpus = || Corpus::default().with_history_root(repo.clone());
    let scene = River::new(Rng::seed_from_u64(7), PALETTE, Arc::new(corpus()));

    // When:
    let first = rows(&render(&scene, AREA, AREA), AREA);
    let mut scene = landed(corpus());
    scene.tick(AREA);

    // Then:
    assert!(first.iter().any(|row| row.contains(READING)), "{first:#?}");
    let buffer = render(&scene, AREA, AREA);
    let stones = groups(cells(&buffer, AREA, &[Part::Stone, Part::Merge]));
    assert_eq!(stones.len(), 4, "{:#?}", rows(&buffer, AREA));
    let _ = fs::remove_dir_all(&repo);
}

/// Given: a repository as the current directory, no config file, and a
/// `git` first on the path that only leaves a mark.
/// When: hearth starts each scene but the river with no terminal, so it
/// builds the scene and stops.
/// Then: no start left the mark.
///
/// Why it matters: a history read at every start walks a large repository
/// before the terminal opens, for a scene the user did not choose.
#[test]
fn no_scene_but_the_river_runs_git() {
    // Given:
    let repo = branched("quiet");
    let tools = scratch("tools");
    let mark = tools.join("mark");
    let fake = tools.join("git");
    fs::write(
        &fake,
        format!("#!/bin/sh\necho \"$@\" >> {}\nexit 1\n", mark.display()),
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    fs::set_permissions(&fake, permissions).unwrap();
    let config = scratch("no-config");

    // When:
    for kind in ALL.iter().filter(|kind| **kind != SceneKind::River) {
        Command::new(env!("CARGO_BIN_EXE_hearth"))
            .args([kind.name(), "--seed", "7"])
            .current_dir(&repo)
            .env("PATH", &tools)
            .env("XDG_CONFIG_HOME", &config)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
    }

    // Then:
    assert!(
        !mark.exists(),
        "git ran: {}",
        fs::read_to_string(&mark).unwrap_or_default()
    );
    let _ = fs::remove_dir_all(&repo);
    let _ = fs::remove_dir_all(&tools);
    let _ = fs::remove_dir_all(&config);
}

/// The river fills the middle of the pane between two banks.
/// Given: the river of a linear history and of one that splits and merges.
/// When: it renders.
/// Then: in every row but the subject's, the river, its bed and all that
/// lies in it, is one run of cells at least two fifths of the pane wide,
/// with its middle within a tenth of the pane of the pane's middle and a
/// bank cell beside it on each side, and every lane and stone lies in it.
///
/// Why it matters: lanes in a corner of an empty pane read as a diagram,
/// and a history of one lane must still read as a river.
#[test]
fn the_river_fills_the_middle_of_the_pane_between_two_banks() {
    for repo in [linear("straight", 5), branched("banks")] {
        // Given:
        let mut scene = river(&repo);

        // When:
        scene.tick(AREA);
        let buffer = render(&scene, AREA, AREA);

        // Then:
        let river = [Part::Bed, Part::Water, Part::Foam, Part::Stone, Part::Merge];
        let mut rivers = Vec::new();
        let rows_text = rows(&buffer, AREA);
        for y in AREA.top()..AREA.bottom() {
            if rows_text[usize::from(y)]
                .chars()
                .any(|c| c.is_ascii_alphanumeric())
            {
                continue;
            }
            let water: Vec<u16> = (AREA.left()..AREA.right())
                .filter(|x| count(&buffer[(*x, y)], &river) > 0)
                .collect();
            let (first, last) = (water[0], water[water.len() - 1]);
            assert_eq!(
                usize::from(last - first + 1),
                water.len(),
                "row {y}: water in more than one run"
            );
            assert!(
                water.len() * 5 >= usize::from(AREA.width) * 2,
                "row {y}: {} cells of water",
                water.len()
            );
            let middle = (first + last) as f32 / 2.0;
            assert!(
                (middle - f32::from(AREA.width) / 2.0).abs() <= f32::from(AREA.width) / 10.0,
                "row {y}: water from {first} to {last}"
            );
            for bank in [first.checked_sub(1), Some(last + 1)] {
                let bank = bank.expect("a bank inside the pane");
                assert!(
                    count(&buffer[(bank, y)], &[Part::Sand]) > 0,
                    "row {y}: no bank at {bank}"
                );
            }
            rivers.push((y, first, last));
        }
        for (x, y) in cells(
            &buffer,
            AREA,
            &[Part::Water, Part::Foam, Part::Stone, Part::Merge],
        ) {
            assert!(
                rivers
                    .iter()
                    .all(|(row, first, last)| *row != y || (first..=last).contains(&&x)),
                "{:?} out of the water",
                (x, y)
            );
        }
        let _ = fs::remove_dir_all(&repo);
    }
}

/// A stone sits in the water of its lane.
/// Given: the river of a repository whose history splits and merges.
/// When: it ticks and renders for a while.
/// Then: in some frames a cell shows a stone and the water of its lane
/// together.
///
/// Why it matters: a cell that drops the water where a stone is cuts the
/// lane into a square round the stone, and the stone reads as a block in
/// a gap and not as a stone in the water.
#[test]
fn a_stone_sits_in_the_water_of_its_lane() {
    // Given:
    let repo = branched("stone");
    let mut scene = river(&repo);
    let mut shared = 0;

    for _ in 0..200 {
        // When:
        scene.tick(AREA);
        let buffer = render(&scene, AREA, AREA);

        // Then:
        shared += AREA
            .positions()
            .filter(|position| {
                let cell = &buffer[*position];
                count(cell, &[Part::Stone, Part::Merge]) > 0
                    && count(cell, &[Part::Water, Part::Foam]) > 0
            })
            .count();
    }
    assert!(shared > 0, "no cell shows a stone and water together");
    let _ = fs::remove_dir_all(&repo);
}

/// Each pixel of a ripple on the open water, as (column, row) in pixels
/// from the top left of the area. A ripple lies on one row of pixels, so a
/// cell with more water than that is a lane.
fn ripples(buffer: &Buffer, area: Rect) -> HashSet<(usize, usize)> {
    let cell = |(x, y): (usize, usize)| {
        &buffer[(
            area.x + (x / CELL_WIDTH) as u16,
            area.y + (y / CELL_HEIGHT) as u16,
        )]
    };
    pixels::showing(buffer, area, |colour| part(colour) == Part::Water)
        .into_iter()
        .map(|(place, _)| place)
        .filter(|place| count(cell(*place), &[Part::Water, Part::Foam]) <= CELL_WIDTH)
        .collect()
}

/// The pixel rows down, up to a cell, by which the pixels of `to` best
/// match the pixels of `from`.
fn shift(from: &HashSet<(usize, usize)>, to: &HashSet<(usize, usize)>) -> usize {
    (0..=CELL_HEIGHT)
        .max_by_key(|rows| {
            from.iter()
                .filter(|(x, y)| to.contains(&(*x, y + rows)))
                .count()
        })
        .unwrap()
}

/// The ripples glide down the river between two ticks.
/// Given: the river of a linear history of thirty commits, at the first
/// tick after which its ripples move down two pixel rows.
/// When: it renders a quarter, a half and three quarters of the way to the
/// next tick, and again at no progress.
/// Then: at no progress the frame is the frame of a scene that was never
/// given progress. The ripples move down in the order of the progress,
/// and half way to the next tick they have moved down by one pixel row.
///
/// Why it matters: at a slow rate a scene ticks once in eight or sixteen
/// frames, and water that waits for the tick moves in jumps of two pixels.
#[test]
fn the_ripples_glide_between_two_ticks() {
    // Given:
    let repo = linear("glide", 30);
    let mut scene = river(&repo);
    let mut next = river(&repo);
    next.tick(AREA);
    let (mut from, mut to) = (
        ripples(&render(&scene, AREA, AREA), AREA),
        ripples(&render(&next, AREA, AREA), AREA),
    );
    for _ in 0..8 {
        if shift(&from, &to) == 2 {
            break;
        }
        scene.tick(AREA);
        next.tick(AREA);
        (from, to) = (to, ripples(&render(&next, AREA, AREA), AREA));
    }
    assert_eq!(shift(&from, &to), 2, "the ripples never move two rows");
    let still = render(&scene, AREA, AREA);

    // When:
    let mut path = vec![0];
    for progress in [0.25, 0.5, 0.75] {
        scene.set_progress(progress);
        path.push(shift(&from, &ripples(&render(&scene, AREA, AREA), AREA)));
    }
    path.push(2);
    scene.set_progress(0.0);

    // Then:
    assert_eq!(
        render(&scene, AREA, AREA),
        still,
        "the frame at no progress"
    );
    assert!(
        path.windows(2).all(|pair| pair[0] <= pair[1]),
        "the ripples move {path:?} from one tick to the next"
    );
    assert_eq!(
        path[2], 1,
        "half way the ripples move {path:?} from one tick to the next"
    );
    let _ = fs::remove_dir_all(&repo);
}

/// The brightest pixel of a cell that shows one of `parts`, by luma, or
/// none.
fn brightest(cell: &Cell, parts: &[Part]) -> Option<u32> {
    pixels::shown(cell)?
        .into_iter()
        .filter(|colour| parts.contains(&part(*colour)))
        .map(theme::luma)
        .max()
}

/// The water is lighter in the middle of its lane than at its sides.
/// Given: the river of a linear history of five commits, one lane four
/// cells wide.
/// When: it renders.
/// Then: in most rows where the lane shows as water in four cells side by
/// side, its two middle cells are brighter than its two outer cells.
///
/// Why it matters: a lane of one flat colour reads as a pipe drawn on the
/// bed and not as water that runs deeper in its middle.
#[test]
fn the_water_is_lighter_in_the_middle_of_its_lane() {
    // Given:
    let repo = linear("shaded", 5);
    let mut scene = river(&repo);

    // When:
    scene.tick(AREA);
    let buffer = render(&scene, AREA, AREA);

    // Then:
    let (mut rows, mut shaded) = (0, 0);
    for y in AREA.top()..AREA.bottom() {
        let lane: Vec<u32> = (AREA.left()..AREA.right())
            .filter(|x| count(&buffer[(*x, y)], &[Part::Water]) >= 5)
            .filter(|x| count(&buffer[(*x, y)], &[Part::Foam, Part::Stone, Part::Merge]) == 0)
            .filter_map(|x| brightest(&buffer[(x, y)], &[Part::Water]))
            .collect();
        if lane.len() != 4 {
            continue;
        }
        rows += 1;
        shaded += usize::from(lane[1].min(lane[2]) > lane[0].max(lane[3]));
    }
    assert!(
        rows > 0,
        "no row shows the lane: {:#?}",
        self::rows(&buffer, AREA)
    );
    assert!(
        shaded * 2 > rows,
        "{shaded} of {rows} rows are lighter in the middle"
    );
    let _ = fs::remove_dir_all(&repo);
}

/// The foam fades in and out as it runs down its lane.
/// Given: the river of a linear history of five commits.
/// When: it ticks and renders for a few seconds.
/// Then: its foam shows in three shades or more.
///
/// Why it matters: foam of one flat colour that jumps from cell to cell
/// reads as a block that blinks and not as froth carried by the water.
#[test]
fn the_foam_fades_in_and_out_along_its_lane() {
    // Given:
    let repo = linear("froth", 5);
    let mut scene = river(&repo);
    let mut shades: Vec<Color> = Vec::new();

    for _ in 0..100 {
        // When:
        scene.tick(AREA);
        let buffer = render(&scene, AREA, AREA);
        for (_, colour) in pixels::showing(&buffer, AREA, |colour| part(colour) == Part::Foam) {
            if !shades.contains(&colour) {
                shades.push(colour);
            }
        }
    }

    // Then:
    assert!(shades.len() >= 3, "the foam in {shades:?}");
    let _ = fs::remove_dir_all(&repo);
}

/// Each stone is lit from its left and shaded on its right.
/// Given: the river of a repository whose history splits and merges.
/// When: it renders.
/// Then: in each stone the brightest stone pixel is brighter than the
/// darkest, and lies in a column left of it.
///
/// Why it matters: a stone of one flat colour reads as a flat tile on the
/// water and not as a stone that stands up out of it.
#[test]
fn each_stone_is_lit_from_one_side() {
    // Given:
    let repo = branched("lit");
    let mut scene = river(&repo);

    // When:
    scene.tick(AREA);
    let buffer = render(&scene, AREA, AREA);

    // Then:
    let stones = groups(cells(&buffer, AREA, &[Part::Stone, Part::Merge]));
    assert_eq!(stones.len(), 4, "{:#?}", rows(&buffer, AREA));
    for stone in stones {
        let lit: Vec<(u16, u32)> = stone
            .iter()
            .filter_map(|(x, y)| {
                brightest(&buffer[(*x, *y)], &[Part::Stone, Part::Merge]).map(|luma| (*x, luma))
            })
            .collect();
        let brightest = lit.iter().max_by_key(|(_, luma)| *luma).unwrap();
        let darkest = lit.iter().min_by_key(|(_, luma)| *luma).unwrap();
        assert!(
            brightest.1 > darkest.1 && brightest.0 < darkest.0,
            "the stone in cells {stone:?} is brightest at {brightest:?} and darkest at {darkest:?}"
        );
    }
    let _ = fs::remove_dir_all(&repo);
}

/// The banks are bands of shaded sand, and the bed turns sandy beside them.
/// Given: the river of a linear history of five commits.
/// When: it renders.
/// Then: the banks show two shades of sand or more, and in most rows the
/// bed in the cell beside each bank differs from the bed in the middle
/// of the river.
///
/// Why it matters: a bank of one hard line beside a flat bed reads as the
/// edge of a drawing and not as a shore that slopes into the water.
#[test]
fn the_banks_slope_softly_into_the_bed() {
    // Given:
    let repo = linear("shore", 5);
    let mut scene = river(&repo);

    // When:
    scene.tick(AREA);
    let buffer = render(&scene, AREA, AREA);

    // Then:
    let mut sand: Vec<Color> = Vec::new();
    for (_, colour) in pixels::showing(&buffer, AREA, |colour| part(colour) == Part::Sand) {
        if !sand.contains(&colour) {
            sand.push(colour);
        }
    }
    assert!(sand.len() >= 2, "the banks in {sand:?}");
    let bed = |x: u16, y: u16| {
        pixels::shown(&buffer[(x, y)])
            .and_then(|shown| shown.into_iter().find(|colour| part(*colour) == Part::Bed))
    };
    let (mut rows, mut sandy) = (0, 0);
    for y in AREA.top()..AREA.bottom() {
        let river: Vec<u16> = (AREA.left()..AREA.right())
            .filter(|x| bed(*x, y).is_some())
            .collect();
        let (Some(first), Some(last)) = (river.first(), river.last()) else {
            continue;
        };
        let middle = river
            .iter()
            .map(|x| (x.abs_diff((first + last) / 2), *x))
            .min()
            .map(|(_, x)| x)
            .unwrap();
        rows += 1;
        sandy += usize::from(bed(*first, y) != bed(middle, y) && bed(*last, y) != bed(middle, y));
    }
    assert!(
        sandy * 2 > rows,
        "{sandy} of {rows} rows have a sandy bed beside the banks"
    );
    let _ = fs::remove_dir_all(&repo);
}

/// Each colour within a channel step of `colour`.
fn around(colour: Color) -> Vec<(u8, u8, u8)> {
    let Color::Rgb(r, g, b) = colour else {
        return Vec::new();
    };
    (0..27)
        .map(|at| {
            let step = |channel: u8, by: u8| channel.saturating_add(by).saturating_sub(1);
            (step(r, at % 3), step(g, at / 3 % 3), step(b, at / 9))
        })
        .collect()
}

/// Every colour within a channel step of a mix strictly between `base` and
/// one of `colours`, sampled finely along each mix, less the colours within
/// a step of the base or of one of `colours` themselves.
fn base_mixes(base: Color, colours: &[Color]) -> HashSet<(u8, u8, u8)> {
    let mut near: HashSet<(u8, u8, u8)> = colours
        .iter()
        .flat_map(|colour| {
            (1..200).flat_map(move |step| around(octant::mix(base, *colour, step as f32 / 200.0)))
        })
        .collect();
    for end in colours.iter().chain([&base]) {
        for each in around(*end) {
            near.remove(&each);
        }
    }
    near
}

/// Over the terminal's background the river mixes no colour with the base.
/// Given: the river of a repository whose history splits and merges, in
/// flat Rose Pine, whose base is the terminal's own background.
/// When: it ticks and renders over a few seconds.
/// Then: no pixel lies on a mix from the base toward gold, pine or foam,
/// the colours the river draws. Its stones are greys, and the greys of Rose
/// Pine lie almost on one line from the base, so a shaded stone would read
/// as a mix with it.
///
/// Why it matters: a mix with a see-through base is an opaque colour that
/// fades toward the theme's base, which the user does not see, so a bank or
/// a lane shows a dark rim over their own background.
#[test]
fn over_the_terminal_background_nothing_mixes_with_the_base() {
    // Given:
    let repo = branched("flat");
    let palette = &theme::FLAT_THEMES[0];
    let drawn = [palette.gold, palette.pine, palette.foam];
    let mixes = base_mixes(palette.base, &drawn);
    let mut scene = landed_in(Corpus::default().with_history_root(repo.clone()), palette);
    for tick in 0..100 {
        // When:
        scene.tick(AREA);
        let buffer = render(&scene, AREA, AREA);

        // Then:
        for (place, colour) in pixels::showing(&buffer, AREA, |_| true) {
            if let Color::Rgb(r, g, b) = colour {
                assert!(
                    !mixes.contains(&(r, g, b)),
                    "tick {tick}: {colour:?} at {place:?} mixes with the base"
                );
            }
        }
    }
    let _ = fs::remove_dir_all(&repo);
}
