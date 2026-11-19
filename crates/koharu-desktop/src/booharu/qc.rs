//! The quality check in the app: each page's findings as its latest check
//! found them, in memory alone, the one-page check that stores them and
//! tells the interface `booharu_qc`, and the commands Mark OK and Dismiss
//! go through, each checking its page again at once.
//!
//! Findings are never saved: a project opened again is checked again. A
//! check reads a snapshot of the project, taken under the session's lock,
//! and stores what it found only while that project is still the one open
//! and the store's generation the one it started in, and never over what a
//! check of a newer snapshot stored. The session's revision has no order,
//! as undo brings an older one back, so snapshots are ordered by when they
//! were taken under the lock.

use std::{
    collections::BTreeMap,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use anyhow::Context as _;
use booharu::qc::{Finding, Inputs};
use booharu_core::{
    context::SeriesStore,
    settings::{LetteringSettings, QcSettings},
};
use koharu_pipeline::PipelineConfig;
use koharu_scene::{Change, LayerId, PageId, Project, Revision};
use parking_lot::Mutex;
use tauri::State;

use super::{apply_if, events};
use crate::{app::App, commands::Result};

/// The store's generation: opening and closing a project start a new one
/// ([`clear`]), and a check that started in another stores nothing.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Each page's findings, as the latest check of it left them.
static FINDINGS: Mutex<BTreeMap<PageId, Checked>> = Mutex::new(BTreeMap::new());

/// How many snapshots checks have taken, which orders them.
static TAKEN: AtomicU64 = AtomicU64::new(0);

/// A page's findings, as a check of one snapshot of it found them.
struct Checked {
    /// The session's revision the snapshot was of.
    #[expect(
        dead_code,
        reason = "the watcher reads it to leave a page checked at the session's revision"
    )]
    revision: Revision,
    /// When the snapshot was taken, in the order the session's lock gave
    /// checks: a check never replaces one of a newer snapshot.
    taken: u64,
    findings: Vec<Finding>,
}

/// Forgets every page's findings and starts a new generation, so a check
/// that started before stores nothing: for a project closing or opening.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the watcher's stop calls it as a project closes")
)]
pub(crate) fn clear() {
    let mut findings = FINDINGS.lock();
    GENERATION.fetch_add(1, Ordering::SeqCst);
    findings.clear();
}

/// Drops `page`'s findings as a change at `revision` removes the page, and
/// tells the interface an empty `booharu_qc` for it. The page keeps an
/// empty entry, as a newer snapshot's than any that still held it, so a
/// check of one of those finishing late brings nothing back; undo bringing
/// the page back has it checked again.
#[expect(
    dead_code,
    reason = "the watcher calls it for each page a change removes"
)]
pub(crate) fn forget(app: &App, page: PageId, revision: Revision) {
    let generation = GENERATION.load(Ordering::SeqCst);
    let checked = Checked {
        revision,
        taken: TAKEN.fetch_add(1, Ordering::SeqCst),
        findings: Vec::new(),
    };
    keep(page, generation, checked, |findings| {
        app.emit(events::QC, events::BooharuQc { page, findings });
    });
}

/// What a check of `project`, which the project file `file` holds, reads
/// now: `[booharu-qc]`, `[pipeline]`'s target language, and
/// `[booharu-lettering]` as the settings file holds them, a bad stored
/// value failing, and the series the chapter links in the Booharu home's
/// series folder.
pub(crate) fn inputs(file: &Path, project: &Project) -> anyhow::Result<Inputs> {
    let qc = koharu_config::get::<QcSettings>()?;
    let target = koharu_config::get::<PipelineConfig>()?.translator.target;
    let lettering = koharu_config::get::<LetteringSettings>()?;
    Ok(booharu::qc::inputs(
        project,
        file,
        SeriesStore::open,
        &qc,
        target,
        lettering,
    ))
}

/// Checks `page` of the open project: takes a snapshot of the project under
/// the session's lock, lets the lock go, checks the page on a blocking
/// thread with the app's renderer, the canvas's, then stores the page's
/// findings and tells the interface `booharu_qc` with them, unless the
/// project closed or another opened meanwhile, or a check of a newer
/// snapshot stored first. Answers whether it stored them; a page the
/// project no longer has stores nothing.
pub(crate) async fn check_page(app: &Arc<App>, page: PageId) -> anyhow::Result<bool> {
    let generation = GENERATION.load(Ordering::SeqCst);
    let session = app.session()?;
    let (file, project, revision, taken) = {
        let session = session.lock();
        let taken = TAKEN.fetch_add(1, Ordering::SeqCst);
        (
            session.path().to_owned(),
            session.project(),
            session.revision(),
            taken,
        )
    };
    let renderer = app.renderer.clone();
    let found = tokio::task::spawn_blocking(move || -> anyhow::Result<Option<Vec<Finding>>> {
        if project.page(page).is_err() {
            return Ok(None);
        }
        let inputs = inputs(&file, &project)?;
        Ok(Some(booharu::qc::check_page(
            &renderer, &project, page, &inputs,
        )?))
    })
    .await??;
    let Some(findings) = found else {
        return Ok(false);
    };
    let open = app
        .project
        .lock()
        .as_ref()
        .is_some_and(|open| Arc::ptr_eq(&open.session, &session));
    if !open {
        return Ok(false);
    }
    let checked = Checked {
        revision,
        taken,
        findings,
    };
    Ok(keep(page, generation, checked, |findings| {
        app.emit(events::QC, events::BooharuQc { page, findings });
    }))
}

/// Stores `checked` as `page`'s findings, from a check that started in the
/// store's generation `generation`, and tells them with `tell`, under the
/// store's lock, so the interface hears pages' findings in the order they
/// were stored; nothing when the generation moved on since or the page
/// holds a newer snapshot's findings. Answers whether it stored them.
fn keep(page: PageId, generation: u64, checked: Checked, tell: impl FnOnce(Vec<Finding>)) -> bool {
    let mut stored = FINDINGS.lock();
    if GENERATION.load(Ordering::SeqCst) != generation
        || stored
            .get(&page)
            .is_some_and(|held| held.taken > checked.taken)
    {
        return false;
    }
    let findings = checked.findings.clone();
    stored.insert(page, checked);
    tell(findings);
    true
}

/// Checks again the page `layer` is on in the open project, logging what
/// fails: the edit before it stands either way.
async fn check_again(app: &Arc<App>, layer: LayerId) {
    let page = app.session().and_then(|session| {
        let project = session.lock().project();
        Ok(project.parent(layer)?.0.id)
    });
    let checked = match page {
        Ok(page) => check_page(app, page).await,
        Err(error) => Err(error),
    };
    if let Err(error) = checked.context("the quality check couldn't check the page again") {
        tracing::warn!("{error:#}");
    }
}

/// Every finding of the open project the quality check holds, in page
/// order, each page's in reading order and then in the checks' order,
/// hidden ones included: what the checks of its pages stored, each
/// replaced as `booharu_qc` tells. None while no project is open.
#[tauri::command]
#[specta::specta]
pub fn booharu_get_qc(app: State<'_, Arc<App>>) -> Result<Vec<Finding>> {
    let Ok(session) = app.session() else {
        return Ok(Vec::new());
    };
    let project = session.lock().project();
    let stored = FINDINGS.lock();
    Ok(project
        .pages()
        .filter_map(|page| stored.get(&page.id))
        .flat_map(|checked| checked.findings.iter().cloned())
        .collect())
}

/// Marks the finding of `check` on `layer`, a block or a text layer, OK
/// while the layer stays as it is, when `ok`, or takes back its marks of
/// `check`, as one undo step labelled `booharu_mark_qc_ok` or
/// `booharu_clear_qc_ok`, then checks the layer's page again. `check` is a
/// finding's: `overflow`, `too_small`, `small_word`, `missing_translation`,
/// `empty_source`, `leftover_source`, `odd_font`, or
/// `glossary:<term source>`; an agent's flag is dismissed instead. A locked
/// layer, or one inside a locked layer, refuses it. Null when the marks
/// stay as they are, which makes no undo step.
#[tauri::command]
#[specta::specta]
pub async fn booharu_mark_qc_ok(
    app: State<'_, Arc<App>>,
    layer: LayerId,
    check: String,
    ok: bool,
) -> Result<Option<Change>> {
    let change = apply_if(&app, move |session| {
        booharu::qc::mark_qc_ok(&session.project(), layer, &check, ok)
    })
    .await?;
    check_again(&app, layer).await;
    Ok(change)
}

/// Dismisses the agent's flag on `layer`, a block or a text layer, with the
/// message `message`, or every flag there when null, as one undo step
/// labelled `booharu_dismiss_flag`, then checks the layer's page again. A
/// locked layer, or one inside a locked layer, refuses it. Null when no
/// flag matches, which makes no undo step.
#[tauri::command]
#[specta::specta]
pub async fn booharu_dismiss_flag(
    app: State<'_, Arc<App>>,
    layer: LayerId,
    message: Option<String>,
) -> Result<Option<Change>> {
    let change = apply_if(&app, move |session| {
        let (edit, _) = booharu::qc::dismiss_flag(&session.project(), layer, message.as_deref())?;
        Ok(edit)
    })
    .await?;
    check_again(&app, layer).await;
    Ok(change)
}

#[cfg(test)]
mod tests {
    use booharu::qc::{Problem, Source};
    use koharu_scene::Uuid;

    use super::*;

    #[test]
    fn a_page_keeps_the_newest_snapshots_findings_in_its_generation() {
        let page = PageId(Uuid::now_v7());
        let revision: Revision = serde_json::from_str("\"r\"").expect("a revision");
        let checked = |taken, text: &str| Checked {
            revision: revision.clone(),
            taken,
            findings: vec![Finding {
                page,
                layer: LayerId(Uuid::now_v7()),
                check: "overflow".to_owned(),
                problem: Problem::Overflow,
                source: Source::Auto,
                text: text.to_owned(),
                hidden: false,
            }],
        };
        let held = || {
            FINDINGS
                .lock()
                .get(&page)
                .map(|held| held.findings[0].text.clone())
        };
        let mut told = Vec::new();
        let generation = GENERATION.load(Ordering::SeqCst);
        assert!(keep(page, generation, checked(5, "newer"), |found| {
            told.push(found[0].text.clone());
        }));
        assert!(
            !keep(page, generation, checked(4, "older"), |_| told
                .push("older".to_owned())),
            "an older snapshot's check finishing late"
        );
        assert!(keep(page, generation, checked(5, "again"), |found| {
            told.push(found[0].text.clone());
        }));
        assert_eq!(held().as_deref(), Some("again"));
        clear();
        assert_eq!(held(), None, "a closed project's findings go");
        assert!(
            !keep(page, generation, checked(6, "stale"), |_| told
                .push("stale".to_owned())),
            "a check that started before the project closed"
        );
        assert_eq!(held(), None);
        assert_eq!(told, ["newer", "again"]);
    }
}
