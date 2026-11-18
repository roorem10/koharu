//! Booharu's settings on the canvas: the `BooharuSettings` built from the
//! saved `[booharu-lettering]` and the first of Koharu's Preferred fonts,
//! which the window holds for its canvas and every one made later. They're
//! pushed at startup, and again whenever either is saved: after Booharu's
//! preferences, from the interface or the agent, and after Koharu's.

use booharu_core::settings::Snapshot;
use koharu_canvas::booharu::{BooharuSettings, TextDefaults};
use koharu_renderer::RendererConfig;
use koharu_scene::Fit;

use crate::screen::Screen;

/// Pushes the settings as the file holds them now to `screen`'s window.
pub(crate) fn push(screen: &Screen) {
    screen.set_booharu(settings());
}

/// The canvas's Booharu settings as the file holds them now. A section that
/// can't be read is logged, and Koharu's behaviour stands in for it, as a
/// pipeline run's snapshot does.
fn settings() -> BooharuSettings {
    let families = koharu_config::get::<RendererConfig>().map(|config| config.font_families);
    of(Snapshot::read(families))
}

/// The canvas's Booharu settings from `snapshot`: its look for new text, and
/// a box drawn with the T tool fitting its text at the largest size it
/// holds, as Booharu 0.0.4's did, whatever the sections held.
fn of(snapshot: Snapshot) -> BooharuSettings {
    let Snapshot { lettering, font } = snapshot;
    BooharuSettings {
        text: TextDefaults {
            lettering,
            family: font,
            fit: Fit::Best,
        },
    }
}

#[cfg(test)]
mod tests {
    use booharu_core::settings::{DefaultTextStyle, LetteringSettings};
    use koharu_scene::Rgba;

    use super::*;

    #[test]
    fn the_saved_look_and_the_default_font_reach_the_canvas() {
        let lettering = LetteringSettings {
            default: DefaultTextStyle {
                fill: Some(Rgba([1.0, 0.0, 0.0, 1.0])),
                stroke: None,
                stroke_width: 4.0,
            },
            detect_colors: false,
        };
        let snapshot = Snapshot {
            lettering: lettering.clone(),
            font: Some("Anime Ace".to_owned()),
        };
        assert_eq!(
            of(snapshot).text,
            TextDefaults {
                lettering,
                family: Some("Anime Ace".to_owned()),
                fit: Fit::Best,
            }
        );
    }

    #[test]
    fn sections_that_cant_be_read_leave_koharus_look_and_best_fit() {
        // A bad section's snapshot is Koharu's behaviour.
        assert_eq!(
            of(Snapshot::default()).text,
            TextDefaults {
                lettering: LetteringSettings::default(),
                family: None,
                fit: Fit::Best,
            }
        );
    }
}
