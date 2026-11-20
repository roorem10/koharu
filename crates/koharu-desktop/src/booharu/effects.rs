//! The FX window's commands: the text effects each box of a page carries,
//! which its fx tags show, and the skew it sets on boxes, each worked out by
//! `booharu::effects`, which the agent's tools share, the effects under the
//! saved `[booharu-lettering]` and the first of Koharu's Preferred fonts as
//! they are when called.

use std::sync::Arc;

use booharu::effects::{self, SkewCounts};
use booharu_core::effects::TextEffects;
use koharu_scene::{LayerId, PageId};
use parking_lot::Mutex;
use tauri::State;

use super::{apply_if, styles::look};
use crate::{app::App, commands::Result};

/// The text effects of each text layer and translated block of `page` that
/// carries any beyond its lettering default, in reading order: a border, a
/// second border, the outline method, a gradient fill or a skew, with the
/// skew each shows. A box with none is left out. Changes nothing.
#[tauri::command]
#[specta::specta]
pub async fn booharu_text_effects(
    app: State<'_, Arc<App>>,
    page: PageId,
) -> Result<Vec<TextEffects>> {
    let app = Arc::clone(&app);
    Ok(tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let project = app.session()?.lock().project();
        let (settings, font) = look()?;
        effects::page_effects(project.page(page)?, &settings, font.as_deref())
    })
    .await??)
}

/// Skews the texts of `layers`, each a translated block's or a text
/// layer's, `x` degrees along its width and `y` along its height, from -60
/// to 60, in place of the skew it shows, as one undo step labelled
/// `booharu_set_skew`, about its box's centre (a point text's anchor), so
/// its position, rotation and frame stay: an axis left out keeps each box's
/// own skew along it, and 0 along both takes the skew off. A box that is, or
/// is inside, a locked layer is left as it is, a block without a translation
/// too, and any other layer is left out. Answers how many boxes it changed,
/// and how many it left as they are because they're locked or untranslated;
/// with nothing to change, it makes no undo step. An angle out of range
/// refuses the call.
#[tauri::command]
#[specta::specta]
pub async fn booharu_set_skew(
    app: State<'_, Arc<App>>,
    layers: Vec<LayerId>,
    x: Option<f64>,
    y: Option<f64>,
) -> Result<SkewCounts> {
    let answer = Arc::new(Mutex::new(SkewCounts::default()));
    let counted = Arc::clone(&answer);
    apply_if(&app, move |session| {
        let change = effects::set_skew(&session.project(), &layers, x, y)?;
        *counted.lock() = change.counts();
        Ok(change.edit)
    })
    .await?;
    Ok(*answer.lock())
}
