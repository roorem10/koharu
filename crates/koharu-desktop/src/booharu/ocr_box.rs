//! The O tool's box, which the canvas answers as its Booharu effect: a text
//! block made by hand over the box, selected as a layer a canvas edit makes
//! is, then read and, as the modifiers held or the settings say, translated
//! and cleaned.

use std::sync::Arc;

use anyhow::Result;
use booharu::boxes::{self, Refusal};
use booharu_core::settings::{OcrBoxMode, OcrBoxSettings};
use koharu_canvas::{
    Rect,
    booharu::{BoxModifiers, Effect},
};
use koharu_pipeline::Target;
use koharu_scene::{Bounds, PageId};

use super::events::{BooharuNotice, NOTICE};
use crate::{app::App, events};

/// Carries out `effect` off the window's thread, since looking for ink in the
/// page's scan takes longer than applying an edit, so the page shows the new
/// block once the session tells its change. What went wrong goes to the
/// interface as Booharu's notice, a failure to the log as well.
pub(crate) fn carry(app: &Arc<App>, effect: Effect) {
    let Effect::Box {
        page,
        rect,
        modifiers,
    } = effect;
    let app = Arc::clone(app);
    tauri::async_runtime::spawn_blocking(move || {
        let Err(error) = add(&app, page, rect, modifiers) else {
            return;
        };
        let notice = match error.downcast_ref::<Refusal>() {
            Some(Refusal::NothingToRead) => BooharuNotice::NothingToRead,
            Some(Refusal::Locked { .. }) => BooharuNotice::Locked,
            None => {
                tracing::error!("failed to carry out a canvas gesture: {error:#}");
                BooharuNotice::Failed {
                    message: format!("{error:#}"),
                }
            }
        };
        tracing::info!(?notice, "the O tool's box");
        app.emit(NOTICE, notice);
    });
}

/// Adds the box over `rect` of `page` as one undo step, selects its block,
/// and starts the run of the stages the mode `modifiers` pick.
fn add(app: &Arc<App>, page: PageId, rect: Rect, modifiers: BoxModifiers) -> Result<()> {
    let mode = mode(modifiers, settled(koharu_config::get()));
    let rect = Bounds {
        x: rect.x0,
        y: rect.y0,
        width: rect.width(),
        height: rect.height(),
    };
    let change = super::applied(app, |session| boxes::add_box(session, page, rect, mode))?;
    let block = boxes::added_box(&change)?;
    app.screen.select(Arc::clone(app), vec![block]);
    app.emit(events::SELECTION, vec![block]);
    app.process(Target::Blocks(vec![block]), boxes::stages(mode), None)?;
    Ok(())
}

/// What a box drawn with `modifiers` held as the pointer lifted goes
/// through, as Booharu 0.0.4's O tool chose: Alt only reads it, Shift also
/// cleans it, Alt winning when both are held, and with neither, `default`.
fn mode(modifiers: BoxModifiers, default: OcrBoxMode) -> OcrBoxMode {
    match modifiers {
        BoxModifiers { alt: true, .. } => OcrBoxMode::Read,
        BoxModifiers { shift: true, .. } => OcrBoxMode::ReadTranslateClean,
        BoxModifiers { .. } => default,
    }
}

/// The mode `read`'s settings give a box drawn with neither modifier, or
/// the default one when they couldn't be read, which the log says.
fn settled(read: koharu_config::Result<OcrBoxSettings>) -> OcrBoxMode {
    match read {
        Ok(settings) => settings.default_mode,
        Err(error) => {
            tracing::warn!(
                "couldn't read the O tool's settings, so it takes its default mode: {error:#}"
            );
            OcrBoxMode::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alt_reads_shift_cleans_and_neither_takes_the_settings_mode() {
        let held = |alt, shift| BoxModifiers { alt, shift };
        for default in [
            OcrBoxMode::Read,
            OcrBoxMode::ReadTranslate,
            OcrBoxMode::ReadTranslateClean,
        ] {
            assert_eq!(mode(held(true, false), default), OcrBoxMode::Read);
            assert_eq!(
                mode(held(false, true), default),
                OcrBoxMode::ReadTranslateClean
            );
            assert_eq!(mode(held(true, true), default), OcrBoxMode::Read);
            assert_eq!(mode(held(false, false), default), default);
        }
    }

    #[test]
    fn unreadable_settings_give_the_default_mode() {
        let read = OcrBoxSettings {
            default_mode: OcrBoxMode::Read,
        };
        assert_eq!(settled(Ok(read)), OcrBoxMode::Read);
        assert_eq!(
            settled(Err(koharu_config::Error::NoHome)),
            OcrBoxMode::ReadTranslate
        );
    }
}
