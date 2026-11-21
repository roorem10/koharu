//! The O tool's box, which the canvas answers as its Booharu effect's `Box`
//! and `canvas::carry` hands here: a text block made by hand over the box,
//! selected as a layer a canvas edit makes is, then read and, as the
//! modifiers held or the settings say, translated and cleaned.

use std::sync::Arc;

use anyhow::{Context as _, Result};
use booharu::boxes::{self, Refusal};
use booharu_core::settings::{OcrBoxMode, OcrBoxSettings};
use koharu_canvas::{Rect, booharu::BoxModifiers};
use koharu_pipeline::Target;
use koharu_scene::{Bounds, PageId};

use super::events::{BooharuNotice, NOTICE};
use crate::{app::App, events};

/// Carries out the box drawn over `rect` of `page` with `modifiers` held off
/// the window's thread, since looking for ink in the page's scan takes
/// longer than applying an edit, so the page shows the new block once the
/// session tells its change; the scan is decoded without the session's lock,
/// so the window's own edits don't wait on it either. What went wrong goes
/// to the interface as Booharu's notice, a failure to the log as well.
pub(crate) fn carry(app: &Arc<App>, page: PageId, rect: Rect, modifiers: BoxModifiers) {
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
    let mode = mode(modifiers, koharu_config::get)?;
    let rect = Bounds {
        x: rect.x0,
        y: rect.y0,
        width: rect.width(),
        height: rect.height(),
    };
    let session = app.session()?;
    let scan = boxes::read_scan(|| session.lock(), page)?;
    let change = super::applied(app, |session| {
        boxes::add_box(session, page, rect, mode, scan.as_ref())
    })?;
    let block = boxes::added_box(&change)?;
    app.screen.select(Arc::clone(app), vec![block]);
    app.emit(events::SELECTION, vec![block]);
    app.process(Target::Blocks(vec![block]), boxes::stages(mode), None)?;
    Ok(())
}

/// What a box drawn with `modifiers` held as the pointer lifted goes
/// through, as Booharu 0.0.4's O tool chose: Alt only reads it, Shift also
/// cleans it, Alt winning when both are held, and with neither, the default
/// mode the O tool's settings, read by `settings` only then, give. Settings
/// that can't be read refuse the box, as a bad stored value is never taken
/// for its default.
fn mode(
    modifiers: BoxModifiers,
    settings: impl FnOnce() -> koharu_config::Result<OcrBoxSettings>,
) -> Result<OcrBoxMode> {
    Ok(match modifiers {
        BoxModifiers { alt: true, .. } => OcrBoxMode::Read,
        BoxModifiers { shift: true, .. } => OcrBoxMode::ReadTranslateClean,
        BoxModifiers { .. } => {
            settings()
                .context("the O tool's default action in [booharu-ocr-box] can't be read")?
                .default_mode
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(alt: bool, shift: bool) -> BoxModifiers {
        BoxModifiers { alt, shift }
    }

    #[test]
    fn alt_reads_shift_cleans_and_neither_takes_the_settings_mode() -> Result<()> {
        for default_mode in [
            OcrBoxMode::Read,
            OcrBoxMode::ReadTranslate,
            OcrBoxMode::ReadTranslateClean,
        ] {
            let settings = || Ok(OcrBoxSettings { default_mode });
            assert_eq!(mode(held(true, false), settings)?, OcrBoxMode::Read);
            assert_eq!(
                mode(held(false, true), settings)?,
                OcrBoxMode::ReadTranslateClean
            );
            assert_eq!(mode(held(true, true), settings)?, OcrBoxMode::Read);
            assert_eq!(mode(held(false, false), settings)?, default_mode);
        }
        Ok(())
    }

    #[test]
    fn a_bad_stored_mode_reads_as_read_and_translate_and_unreadable_settings_refuse_the_box()
    -> Result<()> {
        // A mode Booharu doesn't have, edited into the file, reads as the
        // default.
        let stored = || {
            koharu_config::booharu_read_section::<OcrBoxSettings>(
                "[booharu-ocr-box]\ndefault_mode = \"clean\"",
            )
        };
        assert_eq!(mode(held(false, false), stored)?, OcrBoxMode::ReadTranslate);
        // A file that isn't TOML, or no home to keep it in, refuses a box
        // that needs the settings.
        let not_toml = || koharu_config::booharu_read_section("[booharu-ocr-box");
        assert!(mode(held(false, false), not_toml).is_err());
        let unreadable = || Err(koharu_config::Error::NoHome);
        let error = mode(held(false, false), unreadable).expect_err("no mode");
        assert!(
            format!("{error:#}").contains("[booharu-ocr-box] can't be read"),
            "{error:#}"
        );
        // A modifier picks the mode without them.
        assert_eq!(mode(held(true, false), unreadable)?, OcrBoxMode::Read);
        assert_eq!(
            mode(held(false, true), unreadable)?,
            OcrBoxMode::ReadTranslateClean
        );
        Ok(())
    }
}
