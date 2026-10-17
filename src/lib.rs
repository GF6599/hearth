//! # hearth: a terminal screensaver.
//!
//! ```text
//! main ─► loop { draw → handle_events → tick }
//!              │
//!              ├── cli       Command-line arguments
//!              ├── config    The preferences kept between runs
//!              ├── dither    The ordered dither the scenes share
//!              ├── app       The panes, the picker, and the keys
//!              ├── braille   Eight dots to a cell, in braille glyphs
//!              ├── events    Terminal events → App mutations
//!              ├── mark      The {hearth} lockup above the picker
//!              ├── pane      One scene, its seed, and the ticks it owes
//!              ├── picker    The scene list, the only chrome
//!              ├── scene     The Scene trait and the list of scenes
//!              ├── scenes    One module per scene
//!              ├── corpus    The source files the typing scene types
//!              ├── highlight Colours a source file by its grammar
//!              ├── minimap   A whole file drawn small, two lines to a row
//!              ├── quadrant  Four pixels to a cell, in quadrant glyphs
//!              ├── snippet   Cuts a source file into one snippet per function
//!              ├── tree      The rows of a file tree for one open path
//!              └── theme     The palette
//! ```

pub mod app;
pub mod braille;
pub mod cli;
pub mod clock;
pub mod config;
pub mod corpus;
pub mod dither;
pub mod events;
pub mod highlight;
pub mod mark;
pub mod minimap;
pub mod pane;
pub mod picker;
pub mod quadrant;
pub mod scene;
pub mod scenes;
pub mod snippet;
pub mod theme;
pub mod tree;
