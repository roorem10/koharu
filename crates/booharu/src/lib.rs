//! Booharu's features on top of Koharu: the logic its desktop commands and
//! agent tools share, one module per feature group, such as QC, Read
//! chapter, the run planner, and script export and import.
//!
//! It may depend on koharu-pipeline, koharu-scene, koharu-llm, and
//! koharu-renderer, as well as booharu-core. Koharu's pipeline calls only
//! booharu-core, so what a pipeline hook needs goes there instead, and this
//! crate stays free of a dependency cycle.

pub mod translation;
