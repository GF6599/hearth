#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::process::Command;

use hearth::scene::{ALL, SceneKind};

/// hearth's own scenes lead the list, and the placeholder ends it.
/// Given: the list the picker shows and Tab walks.
/// When: each scene is placed by whether it follows someone else's work.
/// Then: every scene of hearth's own comes before every scene that credits
/// another work, and `noise` comes last.
///
/// Why it matters: a new scene of hearth's own added at the end of the
/// list would hide among the others, and a placeholder first would be the
/// first thing a new user sees.
#[test]
fn hearths_own_scenes_come_first_and_noise_last() {
    // When:
    let own = |kind: &SceneKind| kind.credit().is_none() && *kind != SceneKind::Noise;
    let last_own = ALL
        .iter()
        .rposition(own)
        .expect("hearth has scenes of its own");
    let first_credited = ALL
        .iter()
        .position(|kind| kind.credit().is_some())
        .expect("some scenes credit another work");

    // Then:
    assert!(
        last_own < first_credited,
        "{:?} comes after {:?}",
        ALL[last_own],
        ALL[first_credited]
    );
    assert_eq!(ALL.last(), Some(&SceneKind::Noise));
}

/// The help lists the scenes in the order of the picker.
/// Given: the built binary.
/// When: it prints its help.
/// Then: the scene names it offers are those of the list, in its order.
///
/// Why it matters: a help that lists the scenes in another order than the
/// picker reads as two different apps.
#[test]
fn the_help_lists_the_scenes_in_the_picker_order() {
    // When:
    let output = Command::new(env!("CARGO_BIN_EXE_hearth"))
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&output.stdout);

    // Then:
    let offered = help
        .split("possible values: ")
        .nth(1)
        .and_then(|rest| rest.split(']').next())
        .unwrap_or_else(|| panic!("no scene list in the help:\n{help}"));
    let names: Vec<&str> = offered.split(", ").collect();
    let expected: Vec<&str> = ALL.iter().map(|kind| kind.name()).collect();
    assert_eq!(names, expected);
}
