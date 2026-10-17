#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use clap::Parser;
use hearth::cli::Cli;
use hearth::config::{Background, Config};
use hearth::corpus::Corpus;
use hearth::history::{DEPTH, History};
use hearth::scene::{Rng, Scene, SceneKind};
use hearth::scenes::hourglass::Lengths;
use hearth::scenes::river::{NOTICE, River};
use hearth::scenes::typing::Speed;
use hearth::theme::{self, Palette};
use rand::SeedableRng;
use ratatui::buffer::Buffer;
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

fn river(history: History) -> River {
    River::new(
        Rng::seed_from_u64(7),
        PALETTE,
        Arc::new(Corpus::default().with_history(history)),
    )
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

/// The cells of the area that draw something in one of `colours`.
fn cells(buffer: &Buffer, area: Rect, colours: &[Color]) -> Vec<(u16, u16)> {
    area.positions()
        .filter(|position| {
            let cell = &buffer[*position];
            cell.symbol() != " " && cell.symbol() != "\u{2800}" && colours.contains(&cell.fg)
        })
        .map(|position| (position.x, position.y))
        .collect()
}

/// The groups of `cells` that touch across a side, each as its cells.
fn groups(mut left: Vec<(u16, u16)>) -> Vec<Vec<(u16, u16)>> {
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
    let mut scene = river(History::read(&repo));

    // When:
    scene.tick(AREA);
    let buffer = render(&scene, AREA, AREA);

    // Then:
    let stones = groups(cells(&buffer, AREA, &[PALETTE.subtle, PALETTE.iris]));
    let merges = groups(cells(&buffer, AREA, &[PALETTE.iris]));
    assert_eq!(stones.len(), 4, "{:#?}", rows(&buffer, AREA));
    assert_eq!(merges.len(), 1, "{:#?}", rows(&buffer, AREA));
    let stone_rows: Vec<u16> = stones.iter().flatten().map(|(_, y)| *y).collect();
    let (top, bottom) = (
        *stone_rows.iter().min().unwrap(),
        *stone_rows.iter().max().unwrap(),
    );
    let water = cells(&buffer, AREA, &[PALETTE.pine, PALETTE.foam]);
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
    let mut scene = river(history.clone());
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
    let mut scene = river(history.clone());
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
/// Then: the corpus holds all three commits in its history.
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
    let corpus = Corpus::cloned(&address, &cache).unwrap();

    // Then:
    let subjects: Vec<&str> = corpus
        .history()
        .commits()
        .iter()
        .map(|commit| commit.subject.as_str())
        .collect();
    assert_eq!(subjects, ["three", "two", "one"]);
    let _ = fs::remove_dir_all(&remote);
    let _ = fs::remove_dir_all(&cache);
}

/// Given: hearth started in a repository with no `--repo` and no `repo`
/// key.
/// When: the corpus is read.
/// Then: its history is the history of that repository.
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

    // Then:
    assert_eq!(corpus.history().commits().len(), 4);
    assert_eq!(corpus.history().commits()[0].subject, "merge feature");
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
    let mut scene = river(History::read(&repo));
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
        Lengths::DEFAULT,
    );
    scene.tick(AREA);

    // Then:
    let mut buffer = Buffer::empty(AREA);
    scene.render(AREA, &mut buffer);
    let rows = rows(&buffer, AREA);
    assert!(rows.iter().any(|row| row.contains(NOTICE)), "{rows:#?}");
}
