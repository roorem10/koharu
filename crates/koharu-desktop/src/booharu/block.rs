//! A box drawn with the K tool, Koharu's text block tool: Koharu's canvas
//! adds its block and selects it, and its Booharu effect, `Block`, which
//! `canvas::carry` hands here, has the block read at once and, as the
//! modifiers held or the settings say, translated and cleaned, as the O
//! tool's box was before the O tool folded into the K tool (the owner's
//! decision of 2026-11-25). A block over nothing to read stays, as Koharu
//! keeps it, for typing into, and nothing runs on it.

use std::{collections::BTreeSet, sync::Arc};

use anyhow::{Context as _, Result};
use booharu::boxes;
use booharu_core::settings::{OcrBoxMode, OcrBoxSettings};
use koharu_canvas::booharu::BoxModifiers;
use koharu_pipeline::{Stage, Target};
use koharu_scene::{LayerId, PageId, Session};
use parking_lot::Mutex;

use super::events::{BooharuNotice, NOTICE};
use crate::app::App;

/// Reads `block`, the K tool's box on `page`, with `modifiers` held as its
/// pointer lifted, off the window's thread, since looking for ink in the
/// page's scan takes longer than applying an edit; the scan is decoded
/// without the session's lock, so the window's own edits don't wait on it
/// either. A box with nothing to read, or what went wrong, goes to the
/// interface as Booharu's notice, a failure to the log as well; a block
/// gone by then, as when its edit failed or was undone at once, or a
/// project closed since, is left as it is.
pub(crate) fn carry(app: &Arc<App>, page: PageId, block: LayerId, modifiers: BoxModifiers) {
    let app = Arc::clone(app);
    tauri::async_runtime::spawn_blocking(move || {
        let notice = match read(&app, page, block, modifiers) {
            Ok(None) => return,
            Ok(Some(notice)) => notice,
            Err(error) => {
                tracing::error!("failed to read a text block's box: {error:#}");
                BooharuNotice::Failed {
                    message: format!("{error:#}"),
                }
            }
        };
        tracing::info!(?notice, "the text block tool's box");
        app.emit(NOTICE, notice);
    });
}

/// Starts the run of the stages `block`'s box goes through, as [`plan`]
/// picks them; the notice to show instead, if any.
fn read(
    app: &Arc<App>,
    page: PageId,
    block: LayerId,
    modifiers: BoxModifiers,
) -> Result<Option<BooharuNotice>> {
    let Ok(session) = app.session() else {
        return Ok(None);
    };
    match plan(&session, page, block, modifiers, koharu_config::get)? {
        Plan::Gone => Ok(None),
        Plan::NothingToRead => Ok(Some(BooharuNotice::NothingToRead)),
        Plan::Run(stages) => {
            app.process(Target::Blocks(vec![block]), stages, None)?;
            Ok(None)
        }
    }
}

/// What a K box's block goes through.
#[derive(Debug, PartialEq)]
enum Plan {
    /// Nothing: the block isn't in the project.
    Gone,
    /// Nothing: the box holds no ink to read; the block stays as it is.
    NothingToRead,
    /// These stages, run on the block alone.
    Run(BTreeSet<Stage>),
}

/// What `block`, the K tool's box on `page`, goes through: nothing when
/// it's gone or holds nothing to read ([`boxes::nothing_to_read`]), else the
/// stages of the mode `modifiers` pick ([`mode`]), `settings` being read
/// only when neither modifier was held.
fn plan(
    session: &Mutex<Session>,
    page: PageId,
    block: LayerId,
    modifiers: BoxModifiers,
    settings: impl FnOnce() -> koharu_config::Result<OcrBoxSettings>,
) -> Result<Plan> {
    let here = |session: &Session| session.project().layer(block).is_ok();
    if !here(&session.lock()) {
        return Ok(Plan::Gone);
    }
    let scan = boxes::read_scan(|| session.lock(), page)?;
    {
        let session = session.lock();
        if !here(&session) {
            return Ok(Plan::Gone);
        }
        if boxes::nothing_to_read(&session, block, scan.as_ref())? {
            return Ok(Plan::NothingToRead);
        }
    }
    Ok(Plan::Run(boxes::stages(mode(modifiers, settings)?)))
}

/// What a box drawn with `modifiers` held as the pointer lifted goes
/// through, as Booharu 0.0.4's O tool chose: Alt only reads it, Shift also
/// cleans it, Alt winning when both are held, and with neither, the default
/// mode the text block tool's settings, read by `settings` only then, give.
/// Settings that can't be read, as when the file isn't TOML, leave a box
/// with neither held unread, saying why; a stored mode that doesn't read
/// reads as its default, Read & Translate.
fn mode(
    modifiers: BoxModifiers,
    settings: impl FnOnce() -> koharu_config::Result<OcrBoxSettings>,
) -> Result<OcrBoxMode> {
    Ok(match modifiers {
        BoxModifiers { alt: true, .. } => OcrBoxMode::Read,
        BoxModifiers { shift: true, .. } => OcrBoxMode::ReadTranslateClean,
        BoxModifiers { .. } => {
            settings()
                .context("the text block tool's default action in [booharu-ocr-box] can't be read")?
                .default_mode
        }
    })
}

#[cfg(test)]
mod tests {
    use image::{ImageFormat, Rgb, RgbImage};
    use koharu_scene::{Created, Edit, Hash, Op, Rect, Ref};
    use serde_json::json;

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
    fn a_bad_stored_mode_reads_as_read_and_translate_and_unreadable_settings_leave_the_box_unread()
    -> Result<()> {
        // A mode Booharu doesn't have, edited into the file, reads as the
        // default.
        let stored = || {
            koharu_config::booharu_read_section::<OcrBoxSettings>(
                "[booharu-ocr-box]\ndefault_mode = \"clean\"",
            )
        };
        assert_eq!(mode(held(false, false), stored)?, OcrBoxMode::ReadTranslate);
        // A file that isn't TOML, or no home to keep it in, leaves a box
        // that needs the settings unread.
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

    /// A page whose white scan holds black lines over its left half, and two
    /// blocks as the K tool draws them, each over a box with its mask the
    /// whole box: one over the lines, one over blank paper.
    struct Drawn {
        _dir: tempfile::TempDir,
        session: Mutex<Session>,
        page: PageId,
        inked: LayerId,
        blank: LayerId,
    }

    fn drawn() -> Result<Drawn> {
        let dir = tempfile::tempdir()?;
        let mut session = Session::create(dir.path().join("test.khr"))?;
        let scan = RgbImage::from_fn(200, 100, |x, y| {
            Rgb(if x < 100 && y % 6 < 2 {
                [0; 3]
            } else {
                [255; 3]
            })
        });
        let mut png = Vec::new();
        scan.write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png)?;
        let hash = Hash::of(&png);
        let block = |x: u32| {
            let rect = Rect {
                x: x as i32,
                y: 20,
                width: 60,
                height: 60,
            };
            let bounds = json!({"x": x, "y": 20, "width": 60, "height": 60});
            let id = if x < 100 { "$inked" } else { "$blank" };
            let add = json!({"op": "add_layer", "id": id, "page": "$page", "index": 0, "layer": {
                "kind": "block",
                "source": {"frame": {"box": {"bounds": bounds}}, "direction": "horizontal"},
            }});
            let mask = Op::WritePixels {
                layer: Ref::try_from(format!("{id}.mask")).map_err(anyhow::Error::msg)?,
                rect,
                pixels: vec![255; 60 * 60],
            };
            anyhow::Ok([serde_json::from_value(add)?, mask])
        };
        let mut ops = vec![
            Op::AddFile { bytes: png },
            serde_json::from_value(
                json!({"op": "add_page", "id": "$page", "index": 0, "background": hash}),
            )?,
        ];
        ops.extend(block(20)?);
        ops.extend(block(120)?);
        let change = session.apply(Edit {
            label: "test".to_owned(),
            ops,
        })?;
        let made = |label: &str| {
            change
                .created
                .iter()
                .find_map(|created| match created {
                    Created::Layer {
                        id,
                        label: Some(found),
                    } if found == label => Some(*id),
                    _ => None,
                })
                .with_context(|| format!("no ${label}"))
        };
        Ok(Drawn {
            _dir: dir,
            page: *change.pages.added.first().context("the page")?,
            inked: made("inked")?,
            blank: made("blank")?,
            session: Mutex::new(session),
        })
    }

    #[test]
    fn a_box_runs_the_stages_its_modifiers_pick_and_nothing_on_blank_paper() -> Result<()> {
        let page = drawn()?;
        let plan = |block, modifiers, settings: OcrBoxMode| {
            plan(&page.session, page.page, block, modifiers, || {
                Ok(OcrBoxSettings {
                    default_mode: settings,
                })
            })
        };
        let run = |mode| Plan::Run(boxes::stages(mode));
        assert_eq!(
            plan(
                page.inked,
                held(true, false),
                OcrBoxMode::ReadTranslateClean
            )?,
            run(OcrBoxMode::Read)
        );
        assert_eq!(
            plan(page.inked, held(false, true), OcrBoxMode::Read)?,
            run(OcrBoxMode::ReadTranslateClean)
        );
        assert_eq!(
            plan(page.inked, held(false, false), OcrBoxMode::ReadTranslate)?,
            run(OcrBoxMode::ReadTranslate)
        );
        // Over blank paper the block stays and nothing runs, whatever is
        // held, the settings unread.
        for modifiers in [held(false, false), held(true, false), held(false, true)] {
            let plan = super::plan(&page.session, page.page, page.blank, modifiers, || {
                Err(koharu_config::Error::NoHome)
            })?;
            assert_eq!(plan, Plan::NothingToRead);
        }
        // Settings that can't be read leave a plain box over lettering
        // unread, saying why.
        let error = super::plan(
            &page.session,
            page.page,
            page.inked,
            held(false, false),
            || Err(koharu_config::Error::NoHome),
        )
        .expect_err("no mode");
        assert!(format!("{error:#}").contains("can't be read"), "{error:#}");
        Ok(())
    }

    #[test]
    fn a_block_gone_before_its_read_runs_nothing() -> Result<()> {
        let page = drawn()?;
        page.session.lock().apply(Edit {
            label: "test".to_owned(),
            ops: vec![Op::RemoveLayer {
                layer: page.inked.into(),
            }],
        })?;
        let plan = plan(
            &page.session,
            page.page,
            page.inked,
            held(false, false),
            || panic!("the settings aren't read"),
        )?;
        assert_eq!(plan, Plan::Gone);
        Ok(())
    }
}
