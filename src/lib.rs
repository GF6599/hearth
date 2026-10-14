//! # hearth: a terminal screensaver.
//!
//! ```text
//! main ─► loop { draw → handle_events → tick }
//!              │
//!              ├── cli      Command-line arguments
//!              ├── app      The running scene, its seed, and the keys
//!              ├── events   Terminal events → App mutations
//!              ├── picker   The scene list, the only chrome
//!              ├── scene    The Scene trait and the list of scenes
//!              ├── scenes   One module per scene
//!              ├── corpus   The source files the typing scene types
//!              └── theme    The palette
//! ```

pub mod app;
pub mod cli;
pub mod corpus;
pub mod events;
pub mod picker;
pub mod scene;
pub mod scenes;
pub mod theme;
