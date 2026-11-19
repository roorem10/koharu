//! Read chapter as the app runs it (F17): [`read_chapter`] is the one
//! function the Context tab's command, the Run step of a Run press, and the
//! agent's tool all call. It refuses while a run or another Run press goes,
//! reads the settings as it starts, picks the model that reads
//! (`booharu::models`), frees the pipeline's models for one of Koharu's own
//! and loads it, reads with `booharu::context::read`, writes only while the
//! project read is still the one open and the read wasn't stopped, and
//! tells the interface how it goes as `booharu_job`, then
//! `booharu_context_changed` once it wrote.
//!
//! A read is a job of `booharu::jobs`, which its caller registers, so a
//! second read is refused there, and Stop and closing the project end it;
//! the agent's and MCP's stop tool reach it through the pipeline's count of
//! Stops, which every model call watches.

use std::{sync::Arc, time::Duration};

use anyhow::{Context as _, bail, ensure};
use booharu::{
    calls::{self, Stops},
    context::read::{self, ReadHost, ReadOutcome, ReadSummary, Reader, Skip, Write},
    jobs::{self, BooharuJob, Job, JobKind, JobState},
    models::{self, ReadingModel},
    runs,
};
use booharu_core::{
    context::{Chapter, SeriesStore},
    settings::ReadingSettings,
};
use koharu_agent::AgentConfig;
use koharu_llm::{
    Model, Size,
    rig::{driver::DynModel, operation::Completion},
};
use koharu_pipeline::PipelineConfig;
use koharu_scene::{PageId, Session};
use parking_lot::Mutex;
use tauri::State;

use super::{context::changed, styles::look};
use crate::{app::App, commands::Result};

/// How long a load of one of Koharu's own models that didn't fit in free
/// memory waits to try once more, as the pipeline's models free theirs.
const REFIT: Duration = Duration::from_secs(2);

/// Reads the open project's whole chapter with the Read chapter model, or
/// `model` in its place, as the Context tab's button and the agent's tool
/// do: its brief, the terms it suggests, and each line's lettering category
/// and note, written once every page is read. Refuses while another read, a
/// run, or a Run press goes; with no text, saying to run OCR first; and with
/// no model that can read the pages, naming why. Answers what it did; a read
/// stopped answers `stopped`, having written nothing.
#[tauri::command]
#[specta::specta]
pub async fn booharu_read_chapter(
    app: State<'_, Arc<App>>,
    model: Option<String>,
) -> Result<ReadSummary> {
    let session = app.session()?;
    Ok(read_by_hand(Arc::clone(&app), session, model).await?)
}

/// The models Read chapter and Explain this line use now, as the Context
/// tab, Settings and the line card show them, with why a model can't read.
#[tauri::command]
#[specta::specta]
pub async fn booharu_get_reading_model() -> Result<ReadingModel> {
    let settings = settings()?;
    let agent = koharu_config::get::<AgentConfig>()?.model;
    let models = koharu_llm::models().await;
    Ok(models::reading_model(
        &models,
        settings.reading.model.as_deref(),
        &settings.translation,
        agent.as_deref(),
    ))
}

/// Reads the whole chapter of the project `session` holds by hand, as the
/// Context tab's command and the agent's tool do: registers the job,
/// refused while another read goes, and calls [`read_chapter`] on every
/// page, with `model` in place of the Read chapter model when given. A read
/// stopped answers a summary that says so. The read goes on a task of its
/// own, which a drop of this future stops as Stop does, as the agent's tool
/// call is dropped with its run, so the read ends as a stopped read ends,
/// telling the interface.
pub(crate) async fn read_by_hand(
    app: Arc<App>,
    session: Arc<Mutex<Session>>,
    model: Option<String>,
) -> anyhow::Result<ReadSummary> {
    let job = jobs::start(JobKind::ReadChapter)?;
    let _stop = job.cancel().clone().drop_guard();
    let request = ReadRequest {
        model,
        ..ReadRequest::default()
    };
    let reading = tokio::spawn(read_chapter(app, session, request, job));
    Ok(match reading.await?? {
        ReadOutcome::Done(summary) => summary,
        // A read by hand refuses where a Run step skips.
        ReadOutcome::Skipped(why) => bail!(why),
        ReadOutcome::Stopped => ReadSummary {
            stopped: true,
            ..ReadSummary::default()
        },
    })
}

/// What a read reads, and for whom.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ReadRequest {
    /// The model to read with in place of the saved Read chapter model;
    /// none for that one, or the translation model when it can read.
    pub model: Option<String>,
    /// The pages to read, as a Run press leaves out those it gave up on;
    /// none for every page of the project. Those the project no longer has
    /// are left out, and the rest go in page order.
    pub pages: Option<Vec<PageId>>,
    /// The `booharu_run` group whose Run step this is; none for a read by
    /// hand.
    pub group: Option<u64>,
    /// The pipeline's count of Stops a Run step's group reached the read
    /// at, so a Stop since, even one before the read started, ends it; none
    /// for the count as the read starts.
    pub since: Option<u64>,
}

/// Reads the chapter of the project `session` holds as `request` asks, as
/// `job`, a Read chapter job its caller registered with
/// `booharu::jobs::start`, which refuses a second read: the one function
/// every read goes through.
///
/// It refuses while a run goes, or a `booharu_run` group other than
/// `request.group`'s: a Run step telling only its failed end, so the press
/// says why it translates without the read, and a read by hand nothing, as
/// its caller answers. Then it tells the interface `booharu_job` as it
/// starts, after each chunk of pages read, and as it ends, however it ends.
/// It reads the settings: `[booharu-reading]`,
/// the translation model and its target language, and the lettering
/// settings and the default font a line's category is lettered with. A Run
/// step (`request.group`) skips, answering why, where a read by hand fails:
/// on a chapter whose brief has a summary, read meanwhile, which only a Run
/// step checks; with no text on the pages; and with no model that can read
/// them. One of Koharu's own models is loaded once the pipeline unloads its
/// own, with images and the context a chunk needs, trying once more after
/// two seconds when it doesn't fit, and dropped as the read ends. It writes
/// only while the project read is still the one open and the job wasn't
/// stopped, then tells `booharu_context_changed`; otherwise it answers
/// stopped, having written nothing.
pub(crate) async fn read_chapter(
    app: Arc<App>,
    session: Arc<Mutex<Session>>,
    request: ReadRequest,
    job: Job,
) -> anyhow::Result<ReadOutcome> {
    let telling = Arc::new(Telling::new(&app, &job, request.group));
    if let Err(error) = free(&app, request.group) {
        let refused = Err(error);
        if request.group.is_some() {
            telling.tell(|job| ended(job, &refused));
        }
        return refused;
    }
    // From here on, a Stop of any kind ends the read, and for a Run step one
    // since its group reached it.
    let mut stops = Stops::new(job.cancel().clone(), app.pipeline.clone());
    if let Some(since) = request.since {
        stops.since = since;
    }
    telling.tell(|_| {});
    let ending = Ending(Arc::clone(&telling), false);
    let outcome = reading(&app, session, request, stops, &telling).await;
    if let Ok(ReadOutcome::Done(_)) = &outcome {
        changed(&app);
    }
    ending.end(&outcome);
    outcome
}

/// Whether a read may start, as the Run step of `group` or, with none, by
/// hand: not while a run goes, nor a `booharu_run` group other than
/// `group`.
fn free(app: &App, group: Option<u64>) -> anyhow::Result<()> {
    ensure!(
        !runs::run_going(&app.pipeline.told()),
        "Read chapter can't start while a run is going; wait for it to end, or stop it."
    );
    ensure!(
        runs::group_going().is_none_or(|going| Some(going) == group),
        "Read chapter can't start while a Run press is going; wait for it to end, or stop it."
    );
    Ok(())
}

/// [`read_chapter`]'s work once it started.
async fn reading(
    app: &Arc<App>,
    session: Arc<Mutex<Session>>,
    request: ReadRequest,
    stops: Stops,
    telling: &Arc<Telling>,
) -> anyhow::Result<ReadOutcome> {
    let run_step = request.group.is_some();
    let skip = |why: Skip| match run_step {
        true => Ok(ReadOutcome::Skipped(why)),
        false => Err(anyhow::anyhow!(why)),
    };
    let settings = settings()?;
    let (lettering, font) = look()?;
    let (path, project) = {
        let session = session.lock();
        (session.path().to_owned(), session.project())
    };
    let chapter =
        tokio::task::spawn_blocking(move || Chapter::read(&path, SeriesStore::open)).await??;
    if run_step && !runs::brief_missing(&chapter.context) {
        return skip(Skip::HasBrief);
    }
    let pages = request
        .pages
        .unwrap_or_else(|| project.pages().map(|page| page.id).collect());
    let prepared = read::prepare(&project, &pages, chapter.series, &settings.target_language)?;
    if !prepared.has_text() {
        return skip(Skip::NoText);
    }
    let Some(listed) = listed(&stops).await else {
        return Ok(ReadOutcome::Stopped);
    };
    let reference = match request.model {
        // Named for a read by hand alone, it's refused as itself, never as
        // the saved setting it stands in for.
        Some(model) => models::named_reader(&listed, &model)?.to_owned(),
        None => {
            let saved = settings.reading.model.as_deref();
            let choice = models::reading_model(&listed, saved, &settings.translation, None);
            match choice.reader() {
                Ok(reference) => reference.to_owned(),
                Err(why) => return skip(why),
            }
        }
    };
    if models::local(&reference) {
        // No run is going: the pipeline's models free their memory for it.
        app.pipeline.unload();
    }
    let size = Size {
        sequences: 1,
        context: prepared.context(settings.reading.pages_per_request),
        images: true,
    };
    let Some(model) = connect(&reference, size, &stops)
        .await
        .with_context(|| format!("Read chapter couldn't load {reference}"))?
    else {
        return Ok(ReadOutcome::Stopped);
    };
    calls::usable(&model, &reference, true)?;
    let host = Arc::new(Host {
        app: Arc::clone(app),
        telling: Arc::clone(telling),
    });
    let reader = Reader {
        reference,
        pages_per_request: settings.reading.pages_per_request,
        lettering,
        font,
        store: SeriesStore::open()?,
    };
    let outcome = read::read(host, session, prepared, &model, reader, stops).await;
    // Gone before a Translation run after it loads its own.
    drop(model);
    outcome
}

/// The settings a read and an explanation go by, as saved when they start.
pub(super) struct Settings {
    pub reading: ReadingSettings,
    /// The translation model, `[pipeline.translator] model`.
    pub translation: String,
    /// The language the translation is written in, as its settings name it.
    pub target_language: String,
}

/// The settings as saved now. A bad stored value fails.
pub(super) fn settings() -> anyhow::Result<Settings> {
    let pipeline = koharu_config::get::<PipelineConfig>()?;
    Ok(Settings {
        reading: koharu_config::get()?,
        translation: pipeline.translator.model,
        target_language: pipeline.translator.target.to_string(),
    })
}

/// The models the user can pick, as koharu-llm lists them, or none once
/// `stops` stops the wait, as the list asks every vendor's server, and one
/// that doesn't answer holds it up.
pub(super) async fn listed(stops: &Stops) -> Option<Vec<Model>> {
    tokio::select! {
        biased;
        () = stops.wait() => None,
        listed = koharu_llm::models() => Some(listed),
    }
}

/// The model `reference` names, connected with `size`, or none once
/// `stops` stops the wait: one of Koharu's own that doesn't fit in free
/// memory gets one more try after [`REFIT`].
pub(super) async fn connect(
    reference: &str,
    size: Size,
    stops: &Stops,
) -> anyhow::Result<Option<DynModel<Completion>>> {
    let connecting = async {
        match koharu_llm::connect(reference, size).await {
            Err(error)
                if models::local(reference) && format!("{error:#}").contains("doesn't fit") =>
            {
                tracing::info!(
                    "{reference} didn't fit in free memory, trying once more: {error:#}"
                );
                tokio::time::sleep(REFIT).await;
                koharu_llm::connect(reference, size).await
            }
            connected => connected,
        }
    };
    tokio::select! {
        biased;
        () = stops.wait() => Ok(None),
        model = connecting => Ok(Some(model?)),
    }
}

/// The app as a read reaches it.
struct Host {
    app: Arc<App>,
    telling: Arc<Telling>,
}

impl ReadHost for Host {
    fn while_open(
        &self,
        session: &Arc<Mutex<Session>>,
        write: Write<'_>,
    ) -> anyhow::Result<Option<ReadSummary>> {
        // Held while the read writes, as a run is queued, so a close waits
        // for the write, or the read writes nothing.
        let open = self.app.project.lock();
        if !open
            .as_ref()
            .is_some_and(|open| Arc::ptr_eq(&open.session, session))
        {
            return Ok(None);
        }
        write()
    }

    fn progress(&self, step: u32, steps: u32) {
        self.telling.tell(|job| {
            job.step = step;
            job.steps = steps;
        });
    }
}

/// A read's job as the interface follows it.
struct Telling {
    app: Arc<App>,
    job: Mutex<BooharuJob>,
}

impl Telling {
    fn new(app: &Arc<App>, job: &Job, group: Option<u64>) -> Self {
        Self {
            app: Arc::clone(app),
            job: Mutex::new(BooharuJob {
                id: job.id(),
                kind: job.kind(),
                state: JobState::Running,
                message: None,
                skip: None,
                step: 0,
                steps: 0,
                group,
                summary: None,
            }),
        }
    }

    /// Changes the job with `change`, and tells it.
    fn tell(&self, change: impl FnOnce(&mut BooharuJob)) {
        let job = {
            let mut job = self.job.lock();
            change(&mut job);
            job.clone()
        };
        super::jobs::tell(&self.app, job);
    }
}

/// Tells a read's end once: as it ended, or as stopped when its caller went
/// away before it could.
struct Ending(Arc<Telling>, bool);

impl Ending {
    fn end(mut self, outcome: &anyhow::Result<ReadOutcome>) {
        self.1 = true;
        self.0.tell(|job| ended(job, outcome));
    }
}

/// `job` as a read that ended with `outcome` leaves it: in a final state,
/// with why it failed or was skipped, or, done, every step and what it did.
fn ended(job: &mut BooharuJob, outcome: &anyhow::Result<ReadOutcome>) {
    match outcome {
        Ok(ReadOutcome::Done(summary)) => {
            job.state = JobState::Done;
            job.step = job.steps;
            job.summary = Some(summary.clone());
        }
        Ok(ReadOutcome::Skipped(why)) => {
            job.state = JobState::Skipped;
            job.message = Some(why.to_string());
            job.skip = Some(why.clone());
        }
        Ok(ReadOutcome::Stopped) => job.state = JobState::Stopped,
        Err(error) => {
            job.state = JobState::Failed;
            job.message = Some(format!("{error:#}"));
        }
    }
}

impl Drop for Ending {
    fn drop(&mut self) {
        if !self.1 {
            self.0.tell(|job| job.state = JobState::Stopped);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running() -> BooharuJob {
        BooharuJob {
            id: 1,
            kind: JobKind::ReadChapter,
            state: JobState::Running,
            message: None,
            skip: None,
            step: 1,
            steps: 3,
            group: Some(4),
            summary: None,
        }
    }

    #[test]
    fn a_read_ends_in_a_final_state_that_says_why() {
        let summary = ReadSummary {
            chunks: 3,
            ..ReadSummary::default()
        };
        let mut done = running();
        ended(&mut done, &Ok(ReadOutcome::Done(summary.clone())));
        assert_eq!(
            (done.state, done.step, done.summary),
            (JobState::Done, 3, Some(summary))
        );
        let mut skipped = running();
        ended(&mut skipped, &Ok(ReadOutcome::Skipped(Skip::NoText)));
        assert_eq!(
            (skipped.state, skipped.skip, skipped.message.as_deref()),
            (
                JobState::Skipped,
                Some(Skip::NoText),
                Some("There's no text to read; run OCR first.")
            )
        );
        let mut failed = running();
        let error = anyhow::anyhow!("no answer").context("Read chapter couldn't read page 1");
        ended(&mut failed, &Err(error));
        assert_eq!(
            (failed.state, failed.message.as_deref()),
            (
                JobState::Failed,
                Some("Read chapter couldn't read page 1: no answer")
            )
        );
        let mut stopped = running();
        ended(&mut stopped, &Ok(ReadOutcome::Stopped));
        assert_eq!((stopped.state, stopped.group), (JobState::Stopped, Some(4)));
    }
}
