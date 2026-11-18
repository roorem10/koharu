//! The box tools' commands: the balloons whose text is split over several
//! blocks, joining them, and splitting a block in two, each edit built by
//! `booharu::boxes`, which the agent's tools share, as one undo step.

use std::sync::Arc;

use booharu::boxes::{self, SplitBalloon, SplitBlock};
use koharu_pipeline::Target;
use koharu_scene::{Change, LayerId, PageId};
use tauri::State;

use super::apply;
use crate::{app::App, commands::Result};

/// Every balloon on `page`, or on every page when none is given, whose text
/// is split over two or more text blocks, its blocks in reading order.
#[tauri::command]
#[specta::specta]
pub async fn booharu_find_split_balloons(
    app: State<'_, Arc<App>>,
    page: Option<PageId>,
) -> Result<Vec<SplitBalloon>> {
    let project = app.session()?.lock().project();
    Ok(boxes::find_split_balloons(&project, page)?)
}

/// Joins the text blocks of the balloon holding `block` into the first one
/// read, as one undo step: it keeps its place, its source's frame becomes
/// the box around them all, its wording and its translation every block's
/// that isn't blank, a line each, and the others' masks and clean-ups merge
/// into its own before they go. A text left as scanned, as translating
/// leaves one the model gave back unchanged, gives the translation its
/// wording once the joined clean-up covers it. A locked block or balloon
/// refuses it.
#[tauri::command]
#[specta::specta]
pub async fn booharu_join_balloon(app: State<'_, Arc<App>>, block: LayerId) -> Result<Change> {
    apply(&app, move |session| boxes::join_balloon(session, block)).await
}

/// Joins every balloon of `page` whose text is split over two or more text
/// blocks, each as `booharu_join_balloon` joins one, all as one undo step.
#[tauri::command]
#[specta::specta]
pub async fn booharu_join_page(app: State<'_, Arc<App>>, page: PageId) -> Result<Change> {
    apply(&app, move |session| boxes::join_page(session, page)).await
}

/// Splits `block` in two across its lines as one undo step, the half read
/// first keeping the block's id and a copy after it taking the other, each
/// with its own mask and clean-up and no wording, then reads and translates
/// both halves again. Answers the change, the halves in reading order, and
/// the run reading them.
#[tauri::command]
#[specta::specta]
pub async fn booharu_split_block(app: State<'_, Arc<App>>, block: LayerId) -> Result<SplitBlock> {
    let change = apply(&app, move |session| boxes::split_block(session, block)).await?;
    let halves = boxes::split_halves(block, &change)?;
    let run = app.process(Target::Blocks(halves.clone()), boxes::split_stages(), None)?;
    Ok(SplitBlock {
        change,
        halves,
        run,
    })
}
