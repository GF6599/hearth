//! # hearth: a terminal screensaver.
//!
//! ```text
//! main ─► loop { draw → handle_events → frame }
//!              │
//!              ├── cli       Command-line arguments
//!              ├── config    The preferences kept between runs
//!              ├── dither    The ordered dither the scenes share
//!              ├── splitmix  The hash the scenes share for fixed noise
//!              ├── app       The panes, the picker, and the keys
//!              ├── events    Terminal events → App mutations
//!              ├── mark      The {hearth} lockup above the picker
//!              ├── pane      One scene, its seed, and the ticks it owes
//!              ├── picker    The scene list, the only chrome
//!              ├── scene     The Scene trait and the list of scenes
//!              ├── scenes    One module per scene
//!              ├── screen    Draws each frame as one synchronized update
//!              ├── corpus    The source files the typing scene types
//!              ├── history   The commits the river scene draws
//!              ├── highlight Colours a source file by its grammar
//!              ├── minimap   A whole file drawn small, two lines to a row
//!              ├── octant    Eight pixels to a cell, in octant glyphs
//!              ├── snippet   Cuts a source file into one snippet per function
//!              ├── tree      The rows of a file tree for one open path
//!              └── theme     The palette
//! ```

pub mod app;
pub mod cli;
pub mod clock;
pub mod config;
pub mod corpus;
pub mod dither;
pub mod events;
pub mod highlight;
pub mod history;
pub mod mark;
pub mod minimap;
pub mod octant;
pub mod pane;
pub mod picker;
pub mod scene;
pub mod scenes;
pub mod screen;
pub mod snippet;
pub mod splitmix;
pub mod theme;
pub mod tree;
