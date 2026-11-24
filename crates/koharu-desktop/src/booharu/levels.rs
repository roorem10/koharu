//! Fix page levels and Restore original pages as the app runs them, each
//! through `booharu::levels`, which the agent's tools share: the levels
//! status of pages, which the page menu's Restore original scan shows by,
//! the levels dialog's preview, the fix, which tells how far it is as
//! `booharu_levels_progress` and which the dialog's Stop ends
//! (`booharu_stop_levels`), as Koharu's Stop and closing the project do, and
//! the restore. A fix is a `JobKind::Levels` job for as long as it runs, so
//! a second is refused, and it tells no `booharu_job` events, as its command
//! answers. It's refused as it starts while a run or a Run press goes, or
//! while Koharu's Gallery is still loading pages into the chapter
//! ([`free`]); the driver checks again for a run, under the session's lock,
//! before it changes anything, and so does a restore, which loading pages
//! don't hold back.

use std::sync::Arc;

use anyhow::ensure;
use booharu::{
    jobs::{self, JobKind},
    levels::{self, LevelsPreview, LevelsReport, LevelsStatus, PAGES_LOADING, RUN_GOING},
    runs,
};
use booharu_core::settings::LevelsSettings;
use koharu_pipeline::Event;
use koharu_scene::PageId;
use tauri::State;

use super::{events::LEVELS_PROGRESS, read_chapter::pages_loading};
use crate::{app::App, commands::Result};

/// Each of `pages` as the page menu and the levels dialog read it: whether
/// it has an Original scan to restore, and why a fix or a restore would
/// skip it now: no scan, a block's clean-up with pixels, or a raster
/// Inpainting made. A page the project no longer holds is left out.
/// Decodes nothing, and changes nothing.
#[tauri::command]
#[specta::specta]
pub async fn booharu_get_levels_status(
    app: State<'_, Arc<App>>,
    pages: Vec<PageId>,
) -> Result<Vec<LevelsStatus>> {
    let session = app.session()?;
    Ok(tokio::task::spawn_blocking(move || levels::status(&session.lock(), &pages)).await??)
}

/// What a fix as `settings` say would do to `page`'s scan: the black and
/// white points its levels take, measured on the whole scan, and the scan
/// before and after, each a base64 PNG at most 800 pixels on its longer
/// side. Fails on a page with no scan. Changes nothing.
#[tauri::command]
#[specta::specta]
pub async fn booharu_preview_levels(
    app: State<'_, Arc<App>>,
    page: PageId,
    settings: LevelsSettings,
) -> Result<LevelsPreview> {
    let session = app.session()?;
    Ok(tokio::task::spawn_blocking(move || levels::preview(&session, page, &settings)).await??)
}

/// Levels the scans of `pages` as `settings` say, each from its scan as it
/// is, as one undo step labelled `booharu_levels`, telling
/// `booharu_levels_progress` after each page it levelled; a page's first fix
/// keeps the scan it had in a hidden, locked Original scan layer under
/// everything, which Restore original pages puts back. Answers the pages it
/// changed and those it skipped, with why: no scan, a block's clean-up with
/// pixels, a raster Inpainting made, nothing to change, or a scan changed
/// meanwhile. Refused while a run or a Run press goes, while Koharu's
/// Gallery is still loading pages into the chapter, and while another fix
/// goes; stopped, by the dialog's Stop, Koharu's Stop, or the project
/// closing, it changes no page and fails saying so.
#[tauri::command]
#[specta::specta]
pub async fn booharu_fix_page_levels(
    app: State<'_, Arc<App>>,
    pages: Vec<PageId>,
    settings: LevelsSettings,
) -> Result<LevelsReport> {
    free(&app.pipeline.told(), pages_loading(&app))?;
    let session = app.session()?;
    let job = jobs::start(JobKind::Levels)?;
    let app = Arc::clone(&app);
    Ok(tokio::task::spawn_blocking(move || {
        levels::fix(
            &session,
            &pages,
            &settings,
            || runs::run_going(&app.pipeline.told()),
            &job,
            |progress| app.emit(LEVELS_PROGRESS, progress),
        )
    })
    .await??)
}

/// Whether a fix may start, with the events the pipeline `told` and whether
/// pages are still `loading` into the open project ([`pages_loading`]): not
/// while a run goes, nor a Run press, nor while pages are still loading, as
/// it levels only the pages the project has as it starts.
fn free(told: &[Event], loading: bool) -> anyhow::Result<()> {
    ensure!(
        !runs::run_going(told) && runs::group_going().is_none(),
        RUN_GOING
    );
    ensure!(!loading, PAGES_LOADING);
    Ok(())
}

/// Stops the fix of page levels going, as the levels dialog's Stop: it
/// changes no page. Nothing else Booharu runs stops.
#[tauri::command]
#[specta::specta]
pub fn booharu_stop_levels() {
    jobs::cancel(JobKind::Levels);
}

/// Puts back the Original scan of each of `pages` as its scan, as one undo
/// step labelled `booharu_restore_original`, removing the layer that kept
/// it. Answers the pages it changed and those it skipped, with why: no
/// Original scan, a block's clean-up with pixels, a raster Inpainting made,
/// an Original scan inside a locked layer or holding layers of its own, or
/// a scan that already is the original. Refused while a run or a Run press
/// goes.
#[tauri::command]
#[specta::specta]
pub async fn booharu_restore_original_pages(
    app: State<'_, Arc<App>>,
    pages: Vec<PageId>,
) -> Result<LevelsReport> {
    let session = app.session()?;
    let app = Arc::clone(&app);
    Ok(tokio::task::spawn_blocking(move || {
        levels::restore(&session, &pages, || runs::run_going(&app.pipeline.told()))
    })
    .await??)
}

#[cfg(test)]
mod tests {
    use koharu_pipeline::{RunId, Stage};

    use super::*;

    #[test]
    fn a_fix_is_refused_while_a_run_goes_or_pages_still_load() {
        let queued = Event::Queued {
            run: RunId(1),
            steps: 1,
            stages: vec![Stage::Inpaint],
            pages: false,
        };
        let refused =
            |told: &[Event], loading| free(told, loading).expect_err("refused").to_string();
        assert_eq!(refused(std::slice::from_ref(&queued), false), RUN_GOING);
        assert_eq!(refused(&[], true), PAGES_LOADING);
        free(&[], false).expect("with no run and every page in");
        let ended = Event::Ended {
            run: RunId(1),
            outcome: koharu_pipeline::Outcome::Done,
        };
        free(&[queued, ended], false).expect("once the run ended");
    }
}
