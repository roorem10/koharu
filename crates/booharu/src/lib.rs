//! Booharu's features on top of Koharu: the logic its desktop commands and
//! agent tools share, one module per feature group, such as the quality
//! check (`qc`), Read chapter, the run planner, script export and import,
//! Apply's default text look and Reset to lettering default (`styles`),
//! changing blocks' lettering categories (`lettering`), words made bold,
//! italic or stressed by hand (`words`), the skew the FX window sets and
//! the text effects the fx tag shows (`effects`), sound effects' subtitles
//! (`sfx`), Redo boxes (`redo`), page levels and the Original scan they
//! keep (`levels`), cleaning exact shapes of a page at once (`cleanup`),
//! and the series glossary and chapter notes as the user and the agent
//! change them, Read chapter and Explain this line among them (`context`),
//! with the model calls they make (`calls`), the models they choose
//! (`models`), and the jobs they run as (`jobs`).
//!
//! It may depend on koharu-pipeline, koharu-scene, koharu-llm, and
//! koharu-renderer, as well as booharu-core. Koharu's pipeline calls only
//! booharu-core, so what a pipeline hook needs goes there instead, and this
//! crate stays free of a dependency cycle.

pub mod boxes;
pub mod calls;
pub mod cleanup;
pub mod context;
pub mod effects;
pub mod jobs;
pub mod lettering;
pub mod levels;
pub mod models;
pub mod qc;
pub mod redo;
pub mod runs;
pub mod sfx;
pub mod styles;
pub mod translation;
pub mod words;

/// What the crate's tests share.
#[cfg(test)]
pub(crate) mod testing {
    use koharu_scene::{LayerId, Op, Rgba};
    use serde_json::json;

    /// The font family [`type_bar_style`] sets.
    pub(crate) const FAMILY: &str = "Comic Neue";
    /// The fill [`type_bar_style`] sets.
    pub(crate) const FILL: Rgba = Rgba([0.0, 0.5, 0.0, 1.0]);

    /// The edit Koharu's type bar sends as a font and a fill are picked for
    /// `block`, merged into its translation's style. On a block with no
    /// translation the scene starts one as its source's blank (Koharu's
    /// #1494), so it leaves a box styled before it's translated: a
    /// placeholder (`booharu_core::status::placeholder`).
    pub(crate) fn type_bar_style(block: LayerId) -> Op {
        Op::UpdateLayer {
            layer: block.into(),
            patch: json!({"translation": {"style": {"chars": {"family": FAMILY, "fill": FILL}}}}),
        }
    }
}
