//! The quality check in the app: each page's findings as its latest check
//! found them, in memory alone, the watcher that checks pages as the
//! project changes, the one-page check that stores them and tells the
//! interface `booharu_qc`, and the commands Mark OK and Dismiss go through,
//! each checking its page again at once.
//!
//! Findings are never saved: a project opened again is checked again, every
//! page in the background (T2). A check reads a snapshot of the project,
//! taken under the session's lock, and stores what it found only while
//! that project is still the one open and the store's generation the one
//! it started in, and never over what a check of a newer snapshot stored.
//! The session's revision has no order, as undo brings an older one back,
//! so snapshots are ordered by when they were taken under the lock.
//!
//! The watcher ([`watch`], from one marked line in `App::open`, and
//! [`stop`], from one in `App::close`) follows the session's changes,
//! holding the app and its receiver of them, never the session, which would
//! keep the project's file locked after it closed. It queues the pages each
//! change changes or adds, and every page as it starts, as it falls behind
//! the changes, and as [`touch_all`] asks, when the glossary, Booharu's
//! settings or Koharu's fonts change. Once the project has gone [`QUIET`],
//! and at most [`CAP`] after the first page queued, it checks them one at a
//! time in page order, with what it read for the pass, and every page when
//! the chapter's majority fonts changed since the pass before. A run isn't
//! waited for: a page is checked as it is, and the run's next write queues
//! it again.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::Context as _;
use booharu::qc::{Finding, Inputs, Majorities};
use booharu_core::{
    context::SeriesStore,
    settings::{LetteringSettings, QcSettings},
};
use koharu_pipeline::PipelineConfig;
use koharu_scene::{Change, LayerId, PageId, Project};
use parking_lot::Mutex;
use tauri::State;
use tokio::sync::{
    broadcast::{Receiver, error::RecvError},
    mpsc::{self, UnboundedReceiver, UnboundedSender},
};
use tokio_util::task::AbortOnDropHandle;

use super::{apply_if, events};
use crate::{app::App, commands::Result};

/// How long the project goes unchanged before the watcher checks the pages
/// its changes queued, so a burst of edits is checked once.
const QUIET: Duration = Duration::from_millis(700);

/// The longest the watcher puts a check off after the first page it
/// queued, however often the project changes meanwhile, as during a run.
const CAP: Duration = Duration::from_secs(5);

/// The store's generation: opening and closing a project start a new one
/// ([`clear`]), and a check that started in another stores nothing.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Each page's findings, as the latest check of it left them.
static FINDINGS: Mutex<BTreeMap<PageId, Checked>> = Mutex::new(BTreeMap::new());

/// How many snapshots checks have taken, which orders them.
static TAKEN: AtomicU64 = AtomicU64::new(0);

/// The watcher on the open project; none while no project is open.
static WATCH: Mutex<Option<Watch>> = Mutex::new(None);

/// A page's findings, as a check of one snapshot of it found them.
struct Checked {
    /// When the snapshot was taken, in the order the session's lock gave
    /// checks: a check never replaces one of a newer snapshot.
    taken: u64,
    findings: Vec<Finding>,
}

/// The watcher on the open project.
struct Watch {
    /// Asks it to check every page.
    touch: UnboundedSender<()>,
    /// Its task, which dropping aborts.
    _task: AbortOnDropHandle<()>,
}

/// Starts the watcher on the project just opening, whose session tells
/// `changes`, in place of any other: it checks every page, then each page
/// as changes queue it, until the project closes.
pub(crate) fn watch(app: &Arc<App>, changes: Receiver<Change>) {
    let (touch, touched) = mpsc::unbounded_channel();
    let task = AbortOnDropHandle::new(tokio::spawn(watching(Arc::clone(app), changes, touched)));
    let replaced = WATCH.lock().replace(Watch { touch, _task: task });
    drop(replaced);
}

/// Stops the watcher, as the project closes, and forgets every page's
/// findings, so a check still going stores nothing.
pub(crate) fn stop() {
    let watch = WATCH.lock().take();
    drop(watch);
    clear();
}

/// Has the watcher check every page soon, as something they're checked
/// against changed: the glossary or the series the chapter links, a Booharu
/// settings section, or Koharu's fonts and target language. Nothing while
/// no project is open.
pub(crate) fn touch_all() {
    if let Some(watch) = WATCH.lock().as_ref() {
        // A watcher that ended as its project closed takes nothing.
        let _ = watch.touch.send(());
    }
}

/// The watcher's work: follows `changes`, the open project's, and
/// `touched`, [`touch_all`]'s asks, and checks the pages they queue as
/// they're due, until the session closes its changes as it goes.
async fn watching(
    app: Arc<App>,
    mut changes: Receiver<Change>,
    mut touched: UnboundedReceiver<()>,
) {
    let mut queue = Queue::default();
    // The project is opening: its tags come back as its pages are checked.
    queue.add_all(Instant::now());
    loop {
        tokio::select! {
            change = changes.recv() => match change {
                Ok(change) => {
                    let pages = change.pages;
                    queue.remove(&pages.removed);
                    queue.add(pages.changed.into_iter().chain(pages.added), Instant::now());
                    for page in pages.removed {
                        forget(&app, page);
                    }
                }
                // It missed changes, which may have changed any page.
                Err(RecvError::Lagged(_)) => queue.add_all(Instant::now()),
                Err(RecvError::Closed) => return,
            },
            Some(()) = touched.recv() => queue.add_all(Instant::now()),
            () = until(queue.due()) => pass(&app, &mut queue).await,
        }
    }
}

/// Waits until `due`, or for ever with nothing due.
async fn until(due: Option<Instant>) {
    match due {
        Some(due) => tokio::time::sleep_until(due.into()).await,
        None => std::future::pending().await,
    }
}

/// Checks the pages `queue` holds, now they're due: reads the settings, the
/// glossary and the chapter's majority fonts once for the pass, then checks
/// each page in page order. Before the project has opened the pass is put
/// off, and while the settings can't be read the pages wait for the next
/// change, keeping the findings they have.
async fn pass(app: &Arc<App>, queue: &mut Queue) {
    // The project opens right after its watcher starts.
    let Ok(session) = app.session() else {
        queue.postpone(Instant::now());
        return;
    };
    let (file, project) = {
        let session = session.lock();
        (session.path().to_owned(), session.project())
    };
    drop(session);
    let read = tokio::task::spawn_blocking(move || {
        let inputs = inputs(&file, &project)?;
        anyhow::Ok((project, inputs))
    })
    .await
    .map_err(anyhow::Error::from)
    .and_then(|read| read);
    let (project, inputs) = match read {
        Ok(read) => read,
        Err(error) => {
            tracing::warn!("the quality check can't check the pages: {error:#}");
            queue.hold();
            return;
        }
    };
    let pages = queue.take(project.pages().map(|page| page.id), &inputs.majorities);
    let inputs = Arc::new(inputs);
    for page in pages {
        if let Err(error) = check(app, page, Some(Arc::clone(&inputs))).await {
            tracing::warn!("the quality check couldn't check page {page}: {error:#}");
        }
    }
}

/// The pages the watcher checks next, and when: [`QUIET`] after the latest
/// change that queued any, but at most [`CAP`] after the first queued since
/// they were last taken. Pure, and driven by the instants it's given, so a
/// test drives its clock.
#[derive(Debug, Default)]
struct Queue {
    /// The pages changes queued.
    pages: BTreeSet<PageId>,
    /// Whether every page is queued, whichever the project has when they're
    /// taken.
    all: bool,
    /// When the pages are due; none while none are queued, or they wait for
    /// the next change.
    due: Option<Instant>,
    /// The latest they may be due.
    cap: Option<Instant>,
    /// The chapter's majority fonts as the pages were last taken.
    majorities: Option<Majorities>,
}

impl Queue {
    /// Queues `pages`, which a change at `now` changed or added; a change
    /// of none queues nothing.
    fn add(&mut self, pages: impl IntoIterator<Item = PageId>, now: Instant) {
        let mut pages = pages.into_iter().peekable();
        if pages.peek().is_some() {
            self.pages.extend(pages);
            self.wait(now);
        }
    }

    /// Queues every page at `now`.
    fn add_all(&mut self, now: Instant) {
        self.all = true;
        self.wait(now);
    }

    /// Drops `pages`, which a change removed.
    fn remove(&mut self, pages: &[PageId]) {
        for page in pages {
            self.pages.remove(page);
        }
        if self.pages.is_empty() && !self.all {
            self.hold();
        }
    }

    /// Puts the pages off by [`QUIET`] from `now`, as when the project
    /// they're on hasn't opened yet.
    fn postpone(&mut self, now: Instant) {
        self.cap = None;
        self.wait(now);
    }

    /// Keeps the pages queued until a change queues more, as when what a
    /// check reads can't be read.
    fn hold(&mut self) {
        self.due = None;
        self.cap = None;
    }

    /// When the pages are due.
    fn due(&self) -> Option<Instant> {
        self.due
    }

    /// Takes the pages to check now, in `order`, the project's page order:
    /// every page when every page was queued or `majorities`, the chapter's
    /// majority fonts now, aren't those the pages were last taken with, as
    /// they change what the odd font check finds on any page; else the
    /// queued pages `order` still has.
    fn take(
        &mut self,
        order: impl IntoIterator<Item = PageId>,
        majorities: &Majorities,
    ) -> Vec<PageId> {
        let mut all = std::mem::take(&mut self.all);
        if self.majorities.as_ref() != Some(majorities) {
            self.majorities = Some(majorities.clone());
            all = true;
        }
        let queued = std::mem::take(&mut self.pages);
        self.hold();
        order
            .into_iter()
            .filter(|page| all || queued.contains(page))
            .collect()
    }

    /// Makes the pages due [`QUIET`] after `now`, but no later than [`CAP`]
    /// after the first queued.
    fn wait(&mut self, now: Instant) {
        let cap = *self.cap.get_or_insert(now + CAP);
        self.due = Some((now + QUIET).min(cap));
    }
}

/// Forgets every page's findings and starts a new generation, so a check
/// that started before stores nothing: for a project closing or opening.
pub(crate) fn clear() {
    let mut findings = FINDINGS.lock();
    GENERATION.fetch_add(1, Ordering::SeqCst);
    findings.clear();
}

/// Drops `page`'s findings as a change removes the page, and tells the
/// interface an empty `booharu_qc` for it. The page keeps an empty entry,
/// as a newer snapshot's than any that still held it, so a check of one of
/// those finishing late brings nothing back; undo bringing the page back
/// has it checked again.
pub(crate) fn forget(app: &App, page: PageId) {
    let generation = GENERATION.load(Ordering::SeqCst);
    let checked = Checked {
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
    check(app, page, None).await
}

/// Checks `page` as [`check_page`] does, with `read`, what the watcher read
/// for its pass, in place of what the check would read now.
async fn check(app: &Arc<App>, page: PageId, read: Option<Arc<Inputs>>) -> anyhow::Result<bool> {
    let generation = GENERATION.load(Ordering::SeqCst);
    let session = app.session()?;
    let (file, project, taken) = {
        let session = session.lock();
        let taken = TAKEN.fetch_add(1, Ordering::SeqCst);
        (session.path().to_owned(), session.project(), taken)
    };
    let renderer = app.renderer.clone();
    let found = tokio::task::spawn_blocking(move || -> anyhow::Result<Option<Vec<Finding>>> {
        if project.page(page).is_err() {
            return Ok(None);
        }
        let inputs = match read {
            Some(inputs) => inputs,
            None => Arc::new(inputs(&file, &project)?),
        };
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
    let checked = Checked { taken, findings };
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
/// replaced as `booharu_qc` tells. Empty while no project is open.
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
    use booharu_core::lettering::LetteringCategory;
    use koharu_scene::Uuid;

    use super::*;

    fn pages<const N: usize>() -> [PageId; N] {
        std::array::from_fn(|_| PageId(Uuid::now_v7()))
    }

    /// Majority fonts in which Normal Dialogue's is `family`.
    fn majorities(family: &str) -> Majorities {
        Majorities::from([(LetteringCategory::NormalDialogue, Some(family.to_owned()))])
    }

    /// A queue whose pages were taken once, with `majorities("A")`.
    fn checked_once(order: &[PageId], now: Instant) -> Queue {
        let mut queue = Queue::default();
        queue.add_all(now);
        assert_eq!(queue.take(order.iter().copied(), &majorities("A")), order);
        queue
    }

    #[test]
    fn a_change_is_checked_once_the_project_goes_quiet_and_at_most_five_seconds_after() {
        let start = Instant::now();
        let [a, b, c] = pages();
        let mut queue = checked_once(&[a, b, c], start);
        assert_eq!(queue.due(), None, "nothing queued");
        queue.add([], start);
        assert_eq!(queue.due(), None, "a change of no page queues none");

        queue.add([b], start);
        assert_eq!(queue.due(), Some(start + QUIET));
        // Each change puts the check off again, as a run goes on writing...
        let step = Duration::from_millis(600);
        let mut at = start;
        for _ in 0..8 {
            at += step;
            queue.add([a], at);
        }
        // ...but never later than five seconds after the first.
        assert_eq!(queue.due(), Some(start + CAP));
        assert_eq!(queue.take([a, b, c], &majorities("A")), [a, b]);
        assert_eq!(queue.due(), None, "taken");

        // The cap starts again from the next change.
        queue.add([c], at);
        assert_eq!(queue.due(), Some(at + QUIET));
    }

    #[test]
    fn pages_go_in_page_order_without_those_removed() {
        let start = Instant::now();
        let [a, b, c, d] = pages();
        let mut queue = checked_once(&[a, b, c, d], start);
        queue.add([d, a, c], start);
        queue.remove(&[c]);
        assert_eq!(
            queue.take([d, c, b, a], &majorities("A")),
            [d, a],
            "in the order the project has them now"
        );
        queue.add([b], start);
        assert_eq!(
            queue.take([a, c], &majorities("A")),
            [],
            "a page the project no longer has"
        );

        queue.add([b], start);
        queue.remove(&[b]);
        assert_eq!(queue.due(), None, "nothing left to check");
    }

    #[test]
    fn every_page_is_checked_as_the_watcher_falls_behind_or_the_majority_fonts_change() {
        let start = Instant::now();
        let [a, b, c] = pages();
        let mut queue = Queue::default();
        queue.add_all(start);
        assert_eq!(queue.due(), Some(start + QUIET), "every page as it starts");
        assert_eq!(queue.take([a, b, c], &majorities("A")), [a, b, c]);

        // Missed changes, or a touch, queue every page.
        queue.add_all(start);
        queue.add([b], start);
        assert_eq!(queue.take([a, b, c], &majorities("A")), [a, b, c]);

        // A change of one page checks that page alone while the majority
        // fonts stay as they were...
        queue.add([b], start);
        assert_eq!(queue.take([a, b, c], &majorities("A")), [b]);
        // ...and every page once they change.
        queue.add([b], start);
        assert_eq!(queue.take([a, b, c], &majorities("B")), [a, b, c]);
        queue.add([c], start);
        assert_eq!(queue.take([a, b, c], &majorities("B")), [c]);
    }

    #[test]
    fn a_check_waits_for_the_project_to_open_and_for_settings_that_read() {
        let start = Instant::now();
        let [a, b] = pages();
        let mut queue = checked_once(&[a, b], start);
        queue.add([a], start);

        // Before the project has opened, the pass is put off, keeping what's
        // queued, from the moment it found none open.
        let later = start + CAP;
        queue.postpone(later);
        assert_eq!(queue.due(), Some(later + QUIET));

        // While the settings don't read, the pages wait for the next change.
        queue.hold();
        assert_eq!(queue.due(), None);
        queue.add([b], later);
        assert_eq!(queue.due(), Some(later + QUIET));
        assert_eq!(queue.take([a, b], &majorities("A")), [a, b]);
    }

    #[test]
    fn a_page_keeps_the_newest_snapshots_findings_in_its_generation() {
        let page = PageId(Uuid::now_v7());
        let checked = |taken, text: &str| Checked {
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
