//! What Booharu's pipeline hooks and features share: its settings sections in
//! the Booharu home's `config.toml` (`settings`) and its data on scene layers
//! (`attrs`).
//!
//! The crate sits just above the scene and the config, never above
//! koharu-pipeline, koharu-agent, or koharu-desktop, so koharu-pipeline's
//! Booharu hooks can call it without a dependency cycle. Feature logic that
//! needs the pipeline goes in the `booharu` crate.

pub mod settings;
