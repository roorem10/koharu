//! Booharu's features on top of Koharu: the logic its desktop commands and
//! agent tools share, one module per feature group, such as the quality
//! check (`qc`), Read chapter, the run planner, script export and import,
//! Apply's default text look (`styles`), changing blocks' lettering
//! categories (`lettering`), and the series glossary and chapter notes as
//! the user and the agent change them, Read chapter and Explain this line
//! among them (`context`), with the model calls they make (`calls`), the
//! models they choose (`models`), and the jobs they run as (`jobs`).
//!
//! It may depend on koharu-pipeline, koharu-scene, koharu-llm, and
//! koharu-renderer, as well as booharu-core. Koharu's pipeline calls only
//! booharu-core, so what a pipeline hook needs goes there instead, and this
//! crate stays free of a dependency cycle.

pub mod boxes;
pub mod calls;
pub mod context;
pub mod jobs;
pub mod lettering;
pub mod models;
pub mod qc;
pub mod runs;
pub mod styles;
pub mod translation;
