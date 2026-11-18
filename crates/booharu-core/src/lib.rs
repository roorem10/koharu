//! What Booharu's pipeline hooks and features share: its settings sections in
//! the Booharu home's `config.toml` (`settings`), its data on scene layers
//! (`attrs`), the default look of new text and the hand-edit record
//! (`styles`), the lettering categories and the font each letters its text
//! in (`lettering`), the shapes of balloons (`shape`), the run policy and
//! the Run menu's presets (`runs`), and its version.
//!
//! The crate sits just above the scene and the config, never above
//! koharu-pipeline, koharu-agent, or koharu-desktop, so koharu-pipeline's
//! Booharu hooks can call it without a dependency cycle. Feature logic that
//! needs the pipeline goes in the `booharu` crate.

pub mod attrs;
pub mod lettering;
pub mod runs;
pub mod settings;
pub mod shape;
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
