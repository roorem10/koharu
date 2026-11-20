//! What Booharu's pipeline hooks and features share: its settings sections in
//! the Booharu home's `config.toml` (`settings`), its data on scene layers
//! (`attrs`), the default look of new text and the hand-edit record
//! (`styles`), the lettering categories and the font each letters its text
//! in (`lettering`), how a stressed word looks and which words a run
//! stresses (`emphasis`), words made a step bigger or smaller (`sizes`), the
//! FX window's skew (`skew`) and the text effects a box carries beyond its
//! lettering default (`effects`), the
//! shapes of balloons (`shape`), the run policy and the Run menu's presets
//! (`runs`), what Koharu's stages did to a page's
//! blocks (`status`), which sound effects stay as art and their subtitles
//! (`sfx`), what a run's stages and Detect again leave alone (`protect`),
//! the look Reset to lettering default gives back (`resolver`), the series
//! glossaries and the chapter notes every translation
//! follows (`context`), the quality check's Mark OK record (`qc`), and its
//! version.
//!
//! The crate sits just above the scene and the config, never above
//! koharu-pipeline, koharu-agent, or koharu-desktop, so koharu-pipeline's
//! Booharu hooks can call it without a dependency cycle. Feature logic that
//! needs the pipeline goes in the `booharu` crate.

pub mod attrs;
pub mod context;
pub mod effects;
pub mod emphasis;
pub mod lettering;
pub mod protect;
pub mod qc;
pub mod resolver;
pub mod runs;
pub mod settings;
pub mod sfx;
pub mod shape;
pub mod sizes;
pub mod skew;
pub mod status;
pub mod styles;

pub use styles::{Edited, edited};

/// Booharu's version, such as `0.0.5`, which the repository's `FORK_VERSION`
/// holds as `Booharu 0.0.5`. Koharu's crates carry Koharu's version, so this
/// is the one Booharu reports, as its About dialog does.
pub fn version() -> &'static str {
    release(include_str!("../../../FORK_VERSION"))
}

/// `fork`, a `FORK_VERSION` file's text, without its line end and its leading
/// `Booharu `, as the About dialog drops `/^Booharu\s+/`.
fn release(fork: &str) -> &str {
    let fork = fork.trim();
    fork.strip_prefix("Booharu")
        .filter(|rest| rest.starts_with(char::is_whitespace))
        .map_or(fork, str::trim_start)
}

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
    /// translation the scene starts one from empty point text, so it leaves
    /// a box styled before it's translated: a placeholder
    /// (`status::placeholder`).
    pub(crate) fn type_bar_style(block: LayerId) -> Op {
        Op::UpdateLayer {
            layer: block.into(),
            patch: json!({"translation": {"style": {"chars": {"family": FAMILY, "fill": FILL}}}}),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_is_fork_version_without_the_name() {
        assert_eq!(release("Booharu 0.0.5\n"), "0.0.5");
        assert_eq!(release("Booharu \t1.2.3\r\n"), "1.2.3");
        assert_eq!(release("0.0.5"), "0.0.5");
        // As the About dialog reads it, the name goes only with the space.
        assert_eq!(release("Booharu0.0.5"), "Booharu0.0.5");
        assert!(!version().starts_with("Booharu"), "{}", version());
    }
}
