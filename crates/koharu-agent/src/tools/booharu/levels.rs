//! Page levels, as the agent reaches them: `booharu_fix_page_levels` levels
//! pages' scans and `booharu_restore_original_pages` puts each page's
//! Original scan back, each as one undo step, through `booharu::levels`,
//! the driver the desktop's commands use, and each answering its report.
//!
//! A fix is a Fix page levels job for as long as it runs
//! (`JobKind::Levels`), so a second, the app's or the agent's, is refused,
//! and Stop, the levels dialog's Stop and closing the project stop it, as
//! dropping the call does, changing no page. It's refused as it starts
//! while a run or a Run press goes, or while Koharu's Gallery is still
//! loading pages into the chapter (the host's `RunHost::loading`, asked
//! before the session's lock is taken), with the driver's own messages, and
//! the driver checks again for a run under that lock before it changes
//! anything. It answers once every page is done, so a call takes at most
//! `MOST_PAGES` pages. A restore is refused only while a run goes.
//!
//! The levels' preview and the pages' levels status have no tool of their
//! own: the preview is pictures for the dialog, and `get_page` shows a
//! page's Original scan layer.

use std::collections::BTreeSet;

use booharu::{
    jobs::{self, JobKind},
    levels::{self, LevelsReport},
    runs,
};
use booharu_core::settings::LevelsSettings;
use koharu_scene::PageId;
use rig_core::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use super::{
    BooharuHost,
    context::{blocking, host},
};
use crate::tools::{Tools, failed, project, schema};

/// The most pages one call of `booharu_fix_page_levels` levels: it answers
/// only once every page is done.
const MOST_PAGES: usize = 10;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct FixPageLevelsArgs {
    /// The pages, at least one and at most 10.
    #[schemars(length(min = 1))]
    pages: Vec<PageId>,
    /// Whether each scan's greys are stretched, its darkest ink to black and
    /// its lightest paper to white. Left out, `auto_levels` in
    /// `booharu-levels`.
    levels: Option<bool>,
    /// The share of a scan's pixels, in percent from 0 to 49.9, left out at
    /// each end before its darkest ink and its lightest paper are found, so
    /// a speck of dust or a scanner's edge doesn't set them. Left out,
    /// `clip_percent` in `booharu-levels`.
    #[schemars(range(min = 0.0, max = 49.9))]
    clip_percent: Option<f64>,
    /// Whether paper lighter than `threshold` turns white, after the levels.
    /// Left out, `whiten` in `booharu-levels`.
    whiten: Option<bool>,
    /// The lightness, from 0 to 255, above which `whiten` turns a pixel
    /// white. Left out, `threshold` in `booharu-levels`.
    threshold: Option<u8>,
}

impl FixPageLevelsArgs {
    /// Refuses a call naming no page, or more than `MOST_PAGES`, each page
    /// counted once.
    fn check(&self) -> Result<(), ToolExecutionError> {
        let pages = self.pages.iter().collect::<BTreeSet<_>>().len();
        if pages == 0 {
            return Err(ToolExecutionError::invalid_args(
                "name the pages to fix in `pages`",
            ));
        }
        if pages > MOST_PAGES {
            return Err(ToolExecutionError::invalid_args(format!(
                "booharu_fix_page_levels fixes at most {MOST_PAGES} pages a call, as it answers \
                 once every page is done, and {pages} were named: split them into calls of \
                 {MOST_PAGES} or fewer, one after another"
            )));
        }
        Ok(())
    }

    /// The levels the fix takes: each option the call gives, and the saved
    /// one, `saved`, for each it leaves out.
    fn settings(&self, saved: LevelsSettings) -> LevelsSettings {
        LevelsSettings {
            auto_levels: self.levels.unwrap_or(saved.auto_levels),
            clip_percent: self.clip_percent.unwrap_or(saved.clip_percent),
            whiten: self.whiten.unwrap_or(saved.whiten),
            threshold: self.threshold.unwrap_or(saved.threshold),
        }
    }
}

pub(super) struct FixPageLevels;

impl Tool for FixPageLevels {
    const NAME: &'static str = "booharu_fix_page_levels";
    type Args = FixPageLevelsArgs;
    type Output = LevelsReport;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Fix the levels of pages' scans, as Fix page levels does, as one undo step over every \
         page it changes: stretch each scan from its darkest ink to black and its lightest \
         paper to white (`levels`), leaving out `clip_percent` of its pixels at each end first, \
         from 0 to 49.9, so a speck of dust doesn't set them, then turn paper lighter than \
         `threshold`, from 0 to 255, white (`whiten`), each option left out as `booharu-levels` \
         in Booharu's settings has it. A page's first fix keeps its scan as it was in a hidden, \
         locked image layer named Original scan, under everything, which \
         booharu_restore_original_pages puts back; later fixes keep that first original. \
         view_page then shows the levelled scan, its `original` too, never the Original scan. \
         Levels come from the scan as it is, so fixing a page again with the same options \
         changes nothing, and gentler options need the original put back first. Fix pages \
         before cleaning them: a page is skipped, and reported with its `reason`, when it has \
         no scan (`no_scan`), when a block's clean-up on it has pixels (`cleanup`) or it holds \
         a result of Remove, Generative Fill or booharu_clean_areas (`inpainted`), as those were \
         made from the scan as it is, when levelling changes no pixel (`unchanged`), and when \
         its scan changed meanwhile (`scan_changed`); a page the project doesn't have fails the \
         call, changing nothing. Takes at most 10 pages a call and answers once every page is \
         done: split more into calls of 10 or fewer, one after another. Answers the pages it \
         `changed` and those it `skipped`, in the order named. Refused while a run is going: \
         wait until it ends (wait), or stop it, then call again; while another fix of page \
         levels goes, the user's or yours; and while Koharu's Gallery is still loading pages \
         into the chapter: wait until they're all in, then call again. The user's Stop, or \
         closing the project, stops it with no page changed."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<FixPageLevelsArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: FixPageLevelsArgs,
    ) -> Result<LevelsReport, Self::Error> {
        args.check()?;
        // Refused as the desktop's command refuses a fix, in the same order
        // and words: while a run or a Run press goes, by what the pipeline
        // told as the call came, so a run queued before it counts as going
        // until it ends, then while pages still load, before the session's
        // lock is taken, then while another fix goes.
        let tools = project(context)?;
        levels::startable(&tools.pipeline.told(), loading(&*host(&tools)))
            .map_err(|error| failed(format!("{error:#}")))?;
        let job = jobs::start(JobKind::Levels).map_err(|error| failed(format!("{error:#}")))?;
        // A call dropped, as when the user stops the agent, stops the fix,
        // as Stop does: it changes no page.
        let _stops = job.cancel().clone().drop_guard();
        blocking(context, move |tools, _| {
            let settings = args.settings(saved(tools)?);
            levels::fix(
                &tools.session,
                &args.pages,
                &settings,
                || runs::run_going(&tools.pipeline.told()),
                &job,
                |_| {},
            )
        })
        .await
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct RestoreOriginalPagesArgs {
    /// The pages, at least one.
    #[schemars(length(min = 1))]
    pages: Vec<PageId>,
}

pub(super) struct RestoreOriginalPages;

impl Tool for RestoreOriginalPages {
    const NAME: &'static str = "booharu_restore_original_pages";
    type Args = RestoreOriginalPagesArgs;
    type Output = LevelsReport;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Put back the scan each page had before booharu_fix_page_levels or Fix page levels \
         first changed it, as Restore original scan does, as one undo step over every page it \
         changes: the page's Original scan layer goes and the scan it kept is the page's scan \
         again. A page is skipped, and reported with its `reason`, when it has no Original scan \
         (`no_original`) or no scan (`no_scan`), when a block's clean-up on it has pixels \
         (`cleanup`) or it holds a result of Remove, Generative Fill or booharu_clean_areas \
         (`inpainted`), as those were made from the levelled scan, when its Original scan is \
         inside a locked layer or holds layers of its own (`locked`), and when its scan \
         already is the original (`unchanged`); a page the project doesn't have fails the \
         call, changing nothing. Answers the pages it `changed` and those it `skipped`, in the \
         order named. Refused while a run is going: wait until it ends (wait), or stop it, \
         then call again."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<RestoreOriginalPagesArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: RestoreOriginalPagesArgs,
    ) -> Result<LevelsReport, Self::Error> {
        if args.pages.is_empty() {
            return Err(ToolExecutionError::invalid_args(
                "name the pages to restore in `pages`",
            ));
        }
        // As a fix takes them: a run queued before the call goes until it
        // ends.
        let told = project(context)?.pipeline.told();
        blocking(context, move |tools, _| {
            levels::restore(&tools.session, &args.pages, || {
                runs::run_going(&told) || runs::run_going(&tools.pipeline.told())
            })
        })
        .await
    }
}

/// Whether pages are still loading into the open project, as the app's runs
/// host tells it, Koharu's Gallery loading a chapter's once it opened the
/// chapter as the project; outside the app, none do.
fn loading(host: &dyn BooharuHost) -> bool {
    host.runs().is_some_and(|runs| runs.loading())
}

/// `[booharu-levels]` as the app holds it now, or outside the app as the
/// file holds it; a section that can't be read is an error.
fn saved(tools: &Tools) -> anyhow::Result<LevelsSettings> {
    Ok(match &tools.booharu {
        Some(host) => host.preferences()?.levels,
        None => koharu_config::get()?,
    })
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeSet,
        io::Cursor,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
        time::Duration,
    };

    use booharu::{
        levels::{LevelsSkip, LevelsSkipped},
        runs::{RunHost, RunPlan},
    };
    use booharu_core::{cleanup::is_original_scan, settings::BooharuPreferences};
    use futures::FutureExt as _;
    use image::{ImageFormat, Rgb, RgbImage};
    use koharu_pipeline::{Event, Pipeline, PipelineConfig, Request, RunId, Stage, Target};
    use koharu_renderer::{Renderer, RendererConfig};
    use koharu_scene::{Edit, Hash, Op, Session};
    use parking_lot::Mutex;
    use rig_core::completion::ToolAnnotations;
    use serde_json::{Map, json};
    use tokio::sync::MutexGuard;

    use super::*;
    use crate::tools::booharu::ONE_AT_A_TIME;

    /// The app as the levels tools reach it: Booharu's settings as saved,
    /// its runs, which tell whether pages are still loading, and a gate a
    /// test can hold a fix at as it reads the settings.
    struct App {
        preferences: BooharuPreferences,
        runs: Arc<Runs>,
        gate: Mutex<Option<mpsc::Receiver<()>>>,
    }

    impl BooharuHost for App {
        fn preferences(&self) -> anyhow::Result<BooharuPreferences> {
            let gate = self.gate.lock().take();
            if let Some(gate) = gate {
                gate.recv()?;
            }
            Ok(self.preferences.clone())
        }

        fn save_section(
            &self,
            _: &str,
            _: Map<String, Value>,
        ) -> anyhow::Result<BooharuPreferences> {
            anyhow::bail!("nothing here saves settings")
        }

        fn font_families(&self) -> anyhow::Result<Vec<String>> {
            Ok(Vec::new())
        }

        fn runs(&self) -> Option<Arc<dyn RunHost>> {
            Some(Arc::clone(&self.runs) as Arc<dyn RunHost>)
        }
    }

    /// The app's runs, as far as a fix asks them: whether pages are still
    /// loading into the project. They queue nothing.
    struct Runs {
        pipeline: Pipeline,
        loading: AtomicBool,
    }

    impl RunHost for Runs {
        fn pipeline(&self) -> &Pipeline {
            &self.pipeline
        }

        fn queue(&self, _: &Arc<Mutex<Session>>, _: Request) -> anyhow::Result<Option<RunId>> {
            anyhow::bail!("nothing here queues runs")
        }

        fn announce(&self, _: &RunPlan, _: usize, _: RunId) {}

        fn loading(&self) -> bool {
            self.loading.load(Ordering::SeqCst)
        }
    }

    /// A faded scan, 16 pixels square: ink at 40, a mid grey at 120 and
    /// paper at 200, as PNG bytes.
    fn faded() -> Vec<u8> {
        let scan = RgbImage::from_fn(16, 16, |x, _| {
            Rgb(match x {
                0..4 => [40; 3],
                4..8 => [120; 3],
                _ => [200; 3],
            })
        });
        let mut bytes = Vec::new();
        scan.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .expect("a scan");
        bytes
    }

    /// `faded()` as a fix with `settings` levels it.
    fn levelled(settings: &LevelsSettings) -> RgbImage {
        let bytes = levels::level(&faded(), settings)
            .expect("a scan that levels")
            .expect("a change");
        image::load_from_memory(&bytes).expect("an image").to_rgb8()
    }

    /// Tools in an [`App`] whose `[booharu-levels]` is `saved`, on a
    /// project of a page showing `faded()` and a page with no scan; the
    /// app, and the two pages.
    fn open(saved: LevelsSettings) -> (tempfile::TempDir, Tools, Arc<App>, [PageId; 2]) {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        let scan = faded();
        let pages = [
            json!({"op": "add_page", "index": 0, "background": Hash::of(&scan)}),
            json!({"op": "add_page", "index": 1, "size": {"width": 16, "height": 16}}),
        ];
        let mut ops = vec![Op::AddFile { bytes: scan }];
        ops.extend(pages.map(|page| serde_json::from_value(page).expect("an op")));
        let change = session
            .apply(Edit {
                label: "test".to_owned(),
                ops,
            })
            .expect("two pages");
        let [scanned, blank] = change.pages.added[..] else {
            panic!("{change:?}");
        };
        let pipeline = Pipeline::new(PipelineConfig::default());
        let app = Arc::new(App {
            preferences: BooharuPreferences {
                levels: saved,
                ..BooharuPreferences::default()
            },
            runs: Arc::new(Runs {
                pipeline: pipeline.clone(),
                loading: AtomicBool::new(false),
            }),
            gate: Mutex::new(None),
        });
        let tools = Tools::new(
            Arc::new(Mutex::new(session)),
            pipeline,
            Renderer::new(RendererConfig::default()),
        )
        .with_booharu(Arc::clone(&app) as Arc<dyn BooharuHost>);
        (dir, tools, app, [scanned, blank])
    }

    /// Booharu's tests that fix pages or start groups of runs, one at a
    /// time, as the process lets one fix and one group go at once: held
    /// once the group of a test that ended went, as its follower lets it go
    /// when that test's runtime ends.
    async fn alone() -> MutexGuard<'static, ()> {
        let held = ONE_AT_A_TIME.lock().await;
        for _ in 0..1000 {
            if runs::group_going().is_none() {
                return held;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("a group of runs still goes");
    }

    async fn fix(tools: &Tools, args: Value) -> Result<LevelsReport, String> {
        let args = serde_json::from_value(args).expect("the arguments");
        FixPageLevels
            .call(&mut tools.tool_context(), args)
            .await
            .map_err(|error| error.to_string())
    }

    async fn restore(tools: &Tools, pages: &[PageId]) -> Result<LevelsReport, String> {
        let args = serde_json::from_value(json!({ "pages": pages })).expect("the arguments");
        RestoreOriginalPages
            .call(&mut tools.tool_context(), args)
            .await
            .map_err(|error| error.to_string())
    }

    /// The report of a call that changed `changed` and skipped `skipped`.
    fn reported(changed: &[PageId], skipped: &[(PageId, LevelsSkip)]) -> LevelsReport {
        LevelsReport {
            changed: changed.to_vec(),
            skipped: skipped
                .iter()
                .map(|&(page, reason)| LevelsSkipped { page, reason })
                .collect(),
        }
    }

    /// `page`'s scan as the project holds it now.
    fn scan(tools: &Tools, page: PageId) -> RgbImage {
        let session = tools.session.lock();
        let project = session.project();
        let at = project.page(page).expect("the page");
        let file = at.background.as_ref().expect("a scan").file;
        let bytes = session.file(&file).expect("the scan's file");
        image::load_from_memory(&bytes).expect("an image").to_rgb8()
    }

    /// How many Original scans `page` holds.
    fn originals(tools: &Tools, page: PageId) -> usize {
        let project = tools.session.lock().project();
        let layers = project.layers_of(page).expect("the page");
        layers
            .into_iter()
            .filter(|layer| is_original_scan(layer))
            .count()
    }

    /// A page id no project holds.
    fn id(number: u32) -> PageId {
        format!("00000000-0000-0000-0000-{number:012}")
            .parse()
            .expect("a page id")
    }

    #[test]
    fn options_left_out_are_the_saved_ones() {
        let args =
            |args: Value| serde_json::from_value::<FixPageLevelsArgs>(args).expect("the arguments");
        let only = || args(json!({"pages": [id(1)]}));
        // As `[booharu-levels]` comes, unsaved.
        assert_eq!(
            only().settings(LevelsSettings::default()),
            LevelsSettings {
                auto_levels: true,
                clip_percent: 0.5,
                whiten: false,
                threshold: 235,
            }
        );
        let saved = LevelsSettings {
            auto_levels: false,
            clip_percent: 2.0,
            whiten: true,
            threshold: 100,
        };
        assert_eq!(only().settings(saved.clone()), saved);
        let given = args(
            json!({"pages": [id(1)], "levels": true, "clip_percent": 1.5,
            "whiten": false, "threshold": 240}),
        );
        assert_eq!(
            given.settings(saved.clone()),
            LevelsSettings {
                auto_levels: true,
                clip_percent: 1.5,
                whiten: false,
                threshold: 240,
            }
        );
        let one = args(json!({"pages": [id(1)], "threshold": 180}));
        assert_eq!(
            one.settings(saved.clone()),
            LevelsSettings {
                threshold: 180,
                ..saved
            }
        );
    }

    #[tokio::test]
    async fn a_fix_takes_booharu_levels_as_saved_for_the_options_left_out() {
        let _alone = alone().await;
        // Saved: no levels, and paper lighter than 150 turned white.
        let saved = LevelsSettings {
            auto_levels: false,
            whiten: true,
            threshold: 150,
            ..LevelsSettings::default()
        };
        let (_dir, tools, _app, [page, _]) = open(saved.clone());
        let report = fix(&tools, json!({"pages": [page]})).await;
        assert_eq!(report, Ok(reported(&[page], &[])));
        let fixed = scan(&tools, page);
        assert_eq!(fixed, levelled(&saved));
        // Ink and the mid grey as they were, and the paper white.
        let row = [0, 5, 12].map(|x| fixed.get_pixel(x, 0).0);
        assert_eq!(row, [[40; 3], [120; 3], [255; 3]]);
        // Those given take the saved ones' place: from the original again,
        // levels, and no whitening.
        assert_eq!(restore(&tools, &[page]).await, Ok(reported(&[page], &[])));
        let args = json!({"pages": [page], "levels": true, "clip_percent": 0, "whiten": false});
        assert_eq!(fix(&tools, args).await, Ok(reported(&[page], &[])));
        let given = LevelsSettings {
            auto_levels: true,
            clip_percent: 0.0,
            whiten: false,
            threshold: 150,
        };
        let fixed = scan(&tools, page);
        assert_eq!(fixed, levelled(&given));
        assert_eq!(fixed.get_pixel(0, 0).0, [0; 3], "ink is black");
        assert_eq!(fixed.get_pixel(12, 0).0, [255; 3], "paper is white");
    }

    #[tokio::test]
    async fn a_call_fixes_at_most_ten_pages() {
        let args = |pages: &[PageId]| FixPageLevelsArgs {
            pages: pages.to_vec(),
            levels: None,
            clip_percent: None,
            whiten: None,
            threshold: None,
        };
        let pages: Vec<PageId> = (1..=11).map(id).collect();
        assert!(args(&pages[..10]).check().is_ok());
        // A page named twice counts once.
        let twice = [&pages[..10], &pages[..1]].concat();
        assert!(args(&twice).check().is_ok());
        let error = args(&pages).check().expect_err("11 pages").to_string();
        assert_eq!(
            error,
            "booharu_fix_page_levels fixes at most 10 pages a call, as it answers once every \
             page is done, and 11 were named: split them into calls of 10 or fewer, one after \
             another"
        );
        assert!(args(&[]).check().is_err(), "no page");
        // Refused as the call comes, before it starts anything.
        let _alone = alone().await;
        let (_dir, tools, _app, [page, _]) = open(LevelsSettings::default());
        let eleven = [&[page][..], &pages[..10]].concat();
        let error = fix(&tools, json!({ "pages": eleven })).await;
        assert_eq!(error, Err(error_of(&args(&eleven))));
        assert_eq!(originals(&tools, page), 0, "no page changed");
        assert!(!jobs::busy(JobKind::Levels), "no fix started");
    }

    /// What `check` refuses `args` with.
    fn error_of(args: &FixPageLevelsArgs) -> String {
        args.check().expect_err("refused").to_string()
    }

    #[tokio::test]
    async fn a_fix_and_a_restore_wait_for_a_run_to_end() {
        let _alone = alone().await;
        let (_dir, tools, _app, [page, blank]) = open(LevelsSettings::default());
        // Detection on the page with no scan, which loads no model, queued
        // as it's asked, so it goes as each call comes.
        let detect = || {
            let request = Request {
                target: Target::Pages(vec![blank]),
                stages: BTreeSet::from([Stage::Detect]),
                config: None,
            };
            tools
                .pipeline
                .run(Arc::clone(&tools.session), request)
                .expect("a run")
        };
        let first = detect();
        let error = fix(&tools, json!({"pages": [page]})).await;
        assert_eq!(error, Err(levels::RUN_GOING.to_owned()));
        let second = detect();
        let error = restore(&tools, &[page]).await;
        assert_eq!(error, Err(levels::RUN_GOING.to_owned()));
        assert_eq!(originals(&tools, page), 0, "no page changed");
        assert!(!jobs::busy(JobKind::Levels), "the fix refused ended");
        // Once they ended, a fix goes.
        for run in [first, second] {
            let told = tools
                .pipeline
                .history(run, Duration::from_secs(30))
                .await
                .expect("the run's events");
            assert!(matches!(told.last(), Some(Event::Ended { .. })), "{told:?}");
        }
        let report = fix(&tools, json!({"pages": [page]})).await;
        assert_eq!(report, Ok(reported(&[page], &[])));
        assert_eq!(originals(&tools, page), 1);
    }

    #[tokio::test]
    async fn a_second_fix_is_refused_while_one_goes() {
        let _alone = alone().await;
        let (_dir, tools, _app, [page, _]) = open(LevelsSettings::default());
        // The user's, from the levels dialog.
        let going = jobs::start(JobKind::Levels).expect("a fix going");
        let error = fix(&tools, json!({"pages": [page]})).await;
        assert_eq!(
            error,
            Err("Fix page levels is already running; stop it or wait for it to end.".to_owned())
        );
        assert_eq!(originals(&tools, page), 0, "no page changed");
        drop(going);
        let report = fix(&tools, json!({"pages": [page]})).await;
        assert_eq!(report, Ok(reported(&[page], &[])));
        assert!(!jobs::busy(JobKind::Levels), "the fix's job ended with it");
    }

    #[tokio::test]
    async fn a_fix_waits_for_the_pages_still_loading() {
        let _alone = alone().await;
        let (_dir, tools, app, [page, _]) = open(LevelsSettings::default());
        app.runs.loading.store(true, Ordering::SeqCst);
        let error = fix(&tools, json!({"pages": [page]})).await;
        assert_eq!(error, Err(levels::PAGES_LOADING.to_owned()));
        assert_eq!(originals(&tools, page), 0, "no page changed");
        // A restore isn't held back.
        let report = restore(&tools, &[page]).await;
        assert_eq!(report, Ok(reported(&[], &[(page, LevelsSkip::NoOriginal)])));
        // Once they're in, a fix goes.
        app.runs.loading.store(false, Ordering::SeqCst);
        let report = fix(&tools, json!({"pages": [page]})).await;
        assert_eq!(report, Ok(reported(&[page], &[])));
    }

    #[tokio::test]
    async fn the_fix_and_the_restore_answer_what_they_did() {
        let _alone = alone().await;
        let (_dir, tools, _app, [page, blank]) = open(LevelsSettings::default());
        let before = scan(&tools, page);
        // Each page once, in the order named.
        let report = fix(&tools, json!({"pages": [blank, page, blank]})).await;
        assert_eq!(
            report,
            Ok(reported(&[page], &[(blank, LevelsSkip::NoScan)]))
        );
        assert_eq!(scan(&tools, page), levelled(&LevelsSettings::default()));
        assert_eq!(originals(&tools, page), 1);
        // Fixed again as before, it changes nothing.
        let report = fix(&tools, json!({"pages": [page]})).await;
        assert_eq!(report, Ok(reported(&[], &[(page, LevelsSkip::Unchanged)])));
        let report = restore(&tools, &[blank, page]).await;
        assert_eq!(
            report,
            Ok(reported(&[page], &[(blank, LevelsSkip::NoOriginal)]))
        );
        assert_eq!(scan(&tools, page), before);
        assert_eq!(originals(&tools, page), 0);
        // One undo step each.
        let steps = tools.session.lock().history().steps;
        assert_eq!(
            steps[steps.len() - 2..],
            ["booharu_levels", "booharu_restore_original"]
        );
        // A page the project doesn't have fails the call, changing nothing.
        assert!(fix(&tools, json!({"pages": [page, id(9)]})).await.is_err());
        assert!(restore(&tools, &[page, id(9)]).await.is_err());
        assert_eq!(scan(&tools, page), before);
        assert_eq!(tools.session.lock().history().steps, steps);
    }

    #[tokio::test]
    async fn a_dropped_call_stops_its_fix_with_no_page_changed() {
        let _alone = alone().await;
        let (_dir, tools, app, [page, _]) = open(LevelsSettings::default());
        let before = scan(&tools, page);
        let (open_gate, gate) = mpsc::channel();
        *app.gate.lock() = Some(gate);
        // Polled once, the call starts the fix, which waits at the gate as
        // it reads the settings; then the call goes, as when the user
        // stops the agent.
        let args = serde_json::from_value(json!({"pages": [page]})).expect("the arguments");
        let mut context = tools.tool_context();
        let call = FixPageLevels.call(&mut context, args).now_or_never();
        assert!(call.is_none(), "it waits at the gate");
        assert!(jobs::busy(JobKind::Levels), "the fix goes on, stopped");
        open_gate.send(()).expect("the fix at the gate");
        for _ in 0..1000 {
            if !jobs::busy(JobKind::Levels) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(!jobs::busy(JobKind::Levels), "the fix ended");
        assert_eq!(scan(&tools, page), before, "no page changed");
        assert_eq!(originals(&tools, page), 0);
    }

    #[test]
    fn they_change_pages_so_they_arent_read_only() {
        for annotations in [
            FixPageLevels.annotations(),
            RestoreOriginalPages.annotations(),
        ] {
            assert_ne!(annotations.read_only_hint, Some(true));
            assert_eq!(
                annotations,
                ToolAnnotations::default(),
                "a writer's defaults"
            );
        }
        let definitions = Tools::toolset().tool_definitions();
        for name in [FixPageLevels::NAME, RestoreOriginalPages::NAME] {
            let offered = definitions
                .iter()
                .find(|tool| tool.name.as_str() == name)
                .expect("the agent and MCP offer it");
            assert_ne!(offered.annotations.read_only_hint, Some(true), "{name}");
        }
        let page = id(1);
        let schema = FixPageLevels.parameters();
        let every = json!({"pages": [page], "levels": true, "clip_percent": 49.9,
            "whiten": true, "threshold": 255});
        for fits in [every, json!({"pages": [page]})] {
            assert_eq!(crate::tools::refusal(&schema, &fits), None, "{fits}");
        }
        for refused in [
            json!({}),
            json!({"pages": []}),
            json!({"pages": [page], "clip_percent": 50}),
            json!({"pages": [page], "clip_percent": -1}),
            json!({"pages": [page], "threshold": 256}),
            json!({"pages": [page], "auto_levels": true}),
        ] {
            assert!(
                crate::tools::refusal(&schema, &refused).is_some(),
                "{refused}"
            );
        }
        let schema = RestoreOriginalPages.parameters();
        assert_eq!(
            crate::tools::refusal(&schema, &json!({"pages": [page]})),
            None
        );
        for refused in [
            json!({"pages": []}),
            json!({"pages": [page], "levels": true}),
        ] {
            assert!(
                crate::tools::refusal(&schema, &refused).is_some(),
                "{refused}"
            );
        }
    }
}
