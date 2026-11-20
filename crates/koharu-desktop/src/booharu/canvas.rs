//! Booharu's part of the canvas on the desktop: what the canvas's Booharu
//! effect does, which `carry` dispatches, and Booharu's settings on the
//! canvas: the `BooharuSettings` built from the saved `[booharu-lettering]`
//! and the first of Koharu's Preferred fonts, which the window holds for its
//! canvas and every one made later. They're pushed at startup, and again
//! whenever either is saved: after Booharu's preferences, from the interface
//! or the agent, and after Koharu's.

use std::sync::{Arc, Mutex, PoisonError};

use booharu_core::settings::Snapshot;
use koharu_canvas::{
    Effect as CanvasEffect,
    booharu::{BooharuSettings, Effect, TextDefaults},
};
use koharu_renderer::RendererConfig;
use koharu_scene::Fit;

use super::events::{BOX_MENU, BooharuBoxMenu};
use crate::{app::App, screen::Screen};

/// Carries out what the canvas's Booharu `effect` asks: a box drawn with the
/// O tool, which Booharu adds and reads off the window's thread
/// (`ocr_box`), or a right-click on a box, whose menu the interface opens
/// where the pointer pressed, told as `booharu_box_menu`.
pub(crate) fn carry(app: &Arc<App>, effect: Effect) {
    match effect {
        Effect::Box {
            page,
            rect,
            modifiers,
        } => super::ocr_box::carry(app, page, rect, modifiers),
        Effect::Menu { page, layer, at } => app.emit(
            BOX_MENU,
            BooharuBoxMenu {
                page,
                layer,
                x: at.x,
                y: at.y,
            },
        ),
    }
}

/// Whether `effects`, what the canvas answered a pointer event with, open a
/// box's menu, so the window gives the interface the keys for it.
pub(crate) fn opens_menu(effects: &[CanvasEffect]) -> bool {
    effects
        .iter()
        .any(|effect| matches!(effect, CanvasEffect::Booharu(Effect::Menu { .. })))
}

/// Pushes the settings as the file holds them now to `screen`'s window.
/// Pushes from different saves run one at a time, each read and posted to
/// the window in turn, so the last one posted holds the last one read. When
/// a section can't be read, the canvas keeps the settings it has, and the
/// error is logged; the Settings page shows it too, as it reads the section.
pub(crate) fn push(screen: &Screen) {
    static PUSHING: Mutex<()> = Mutex::new(());
    let _turn = PUSHING.lock().unwrap_or_else(PoisonError::into_inner);
    match settings() {
        Ok(settings) => screen.set_booharu(settings),
        Err(error) => tracing::error!("the canvas keeps its Booharu settings: {error:#}"),
    }
}

/// The canvas's Booharu settings as the file holds them now.
fn settings() -> anyhow::Result<BooharuSettings> {
    let families = koharu_config::get::<RendererConfig>().map(|config| config.font_families);
    Ok(of(Snapshot::read(families)?))
}

/// The canvas's Booharu settings from `snapshot`: its look for new text, and
/// a box drawn with the T tool fitting its text at the largest size it
/// holds, as Booharu 0.0.4's did, whatever the sections held.
fn of(snapshot: Snapshot) -> BooharuSettings {
    let Snapshot {
        lettering, font, ..
    } = snapshot;
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
    use koharu_canvas::{Point, Rect, booharu::BoxModifiers};
    use koharu_scene::{LayerId, PageId, Rgba, Uuid};

    use super::*;

    #[test]
    fn only_a_boxs_menu_takes_the_keys_for_the_interface() {
        let (page, layer) = (PageId(Uuid::nil()), LayerId(Uuid::from_u128(1)));
        let menu = CanvasEffect::Booharu(Effect::Menu {
            page,
            layer,
            at: Point::new(160.0, 101.0),
        });
        let drawn = CanvasEffect::Booharu(Effect::Box {
            page,
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            modifiers: BoxModifiers::default(),
        });
        let selected = CanvasEffect::Selection(vec![layer]);
        assert!(opens_menu(&[selected.clone(), menu]));
        assert!(!opens_menu(&[selected, drawn]));
        assert!(!opens_menu(&[]));
    }

    #[test]
    fn the_saved_look_and_the_default_font_reach_the_canvas() {
        let lettering = LetteringSettings {
            default: DefaultTextStyle {
                fill: Some(Rgba([1.0, 0.0, 0.0, 1.0])),
                stroke: None,
                stroke_width: 4.0,
            },
            detect_colors: false,
            ..LetteringSettings::default()
        };
        let snapshot = Snapshot {
            lettering: lettering.clone(),
            font: Some("Anime Ace".to_owned()),
            ..Snapshot::default()
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
    fn koharus_look_still_fits_a_drawn_box_at_best() {
        // The snapshot of Koharu's own behaviour.
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
