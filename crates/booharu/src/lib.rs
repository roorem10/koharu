//! Booharu's features on top of Koharu: the logic its desktop commands and
//! agent tools share, one module per feature group, such as QC, Read
//! chapter, the run planner, script export and import, Apply's default text
//! look (`styles`), and changing blocks' lettering categories (`lettering`).
//!
//! It may depend on koharu-pipeline, koharu-scene, koharu-llm, and
//! koharu-renderer, as well as booharu-core. Koharu's pipeline calls only
//! booharu-core, so what a pipeline hook needs goes there instead, and this
//! crate stays free of a dependency cycle.

pub mod boxes;
pub mod lettering;
pub mod runs;
pub mod styles;
pub mod translation;
