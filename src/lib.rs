//! # hearth: a terminal screensaver.
//!
//! ```text
//! main ─► loop { draw → handle_events → tick }
//!              │
//!              ├── cli      Command-line arguments
//!              ├── app      The running scene, its seed, and the keys
//!              ├── events   Terminal events → App mutations
//!              ├── scene    The Scene trait and the list of scenes
//!              └── scenes   One module per scene
//! ```

pub mod app;
pub mod cli;
pub mod events;
pub mod scene;
pub mod scenes;
