//! Booharu's pipeline hooks: what a stage calls, by one marked line, to apply
//! Booharu's settings and attributes from booharu-core within the stage's
//! own page edit. A run reads Booharu's settings once, as it's queued, into
//! its config's snapshot, and never again mid-stage, so every page of a run
//! uses the same values, whether the Run button, the agent, or MCP queued it.
//!
//! - `queued`, in `Pipeline::run`, takes the snapshot, the run policy of
//!   `[booharu-runs]` included, with the flags the run's caller set on its
//!   config's snapshot (Redo boxes' `redo`, a subtitle run's
//!   `subtitle_run`, a soft Remove stroke's `soft_limit`), or refuses the
//!   run when a section it reads can't be read. For a run that translates
//!   it also appends the chapter's glossary and notes, from its side file
//!   and the series it links, to the Instructions in the run's config,
//!   never in the saved setting, or refuses the run when either file can't
//!   be read; and takes
//!   `[booharu-emphasis]`, with whether the run asks its model which words
//!   to stress, which appends the instruction to ask after those
//!   (`emphasis`).
//! - `emphasis_schema` and `emphasis_answers`, in a translation's turn that
//!   asks, are the answer's shape and the words read back from it, which
//!   the stage gives each block by `picked` for the write.
//! - `translated`, in the Translation stage's `written`, gives each new
//!   translation the default look, its block's lettering category's font
//!   among it while the run's Pick fonts automatically is on, and the marks
//!   of the words its model picked to stress, taking off those a run left on
//!   the one it replaces, and records what the stage wrote; `given_back`
//!   records a translation it cleared, given back unchanged, and
//!   `given_back_patch` clears it, but keeps a placeholder, the style set by
//!   hand on a block before it was translated (D1).
//! - `recognized`, in the Recognition stage, records the wording it read.
//! - `seed`, right after the Translation stage's `keep`, starts a project's
//!   translation conversation that holds nothing yet, as one does the first
//!   time the project translates and again once its model, target, or
//!   instructions changed, with the page before the one it translates as
//!   the user left it, its answer in the shape a run that asks for the
//!   words to stress has its model give.
//! - `Failure`, from `stages::run`, is a stage task's failure as the run
//!   reports it, with the class the run policy goes by, which the scheduler's
//!   `booharu` module follows: another try, the page given up on, or the run
//!   ended.
//! - `answered`, in a translation's turn, gives a hosted model's request ten
//!   minutes to answer.
//! - `Left`, `Writing`, and `stays`, in `filters`, are the run filters each
//!   page stage goes through: what it leaves out of a page, what it checks
//!   again as it writes, what Detect again keeps, and the page's
//!   `Event::Skipped`.
//! - `libtorch_started`, in `gpu.rs`'s `loaded`, notes that a model load got
//!   the runtime's device, and so libtorch, however the load then goes, so
//!   Booharu's unload empties libtorch's cache once it may
//!   (`libtorch_loaded`).
//! - `cleanup`'s `soft` and `limit`, in the Inpainting stage's `inpaint()`,
//!   keep a soft Remove stroke's own coverage as its result's limit (T23),
//!   for a run whose snapshot asks for it (`soft_limit`); its write puts an
//!   area's result under all lettering (`booharu_core::cleanup`, D17), and
//!   Detect again's additions go above the Original scan and the results
//!   under the lettering (`filters`).

pub(crate) mod cleanup;
pub(crate) mod emphasis;
mod filters;

use std::{
    collections::{BTreeSet, HashMap},
    future::Future,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use booharu_core::{
    context::{self, Chapter, Extras, MAX_TERMS, SeriesStore},
    emphasis::{EmphasisSettings, Marking},
    runs::{RunPolicy, RunStage},
    settings::Snapshot,
    status, styles,
};
use koharu_llm::rig::{
    catalog::ModelSpec,
    completion::{Message, Usage},
    driver::DynModel,
    error::{ErrorReport, ProviderError},
    http_client,
    operation::Completion,
};
use koharu_renderer::RendererConfig;
use koharu_scene::{Content, LayerId, Op, PageId, Project, Session};
use parking_lot::Mutex;
use rig_agent::run::PromptError;
use serde::Serialize;

pub(crate) use self::{
    emphasis::{emphasis_answers, emphasis_schema, picked},
    filters::{Left, stays},
};
use crate::{
    PipelineConfig, Stage, TranslatorConfig,
    stages::translate::{Conversation, turn},
};

/// `config`, the one a run of `stages` on the project `session` holds was
/// queued with, with Booharu's settings as they're saved now:
/// `[booharu-lettering]`, the first of Koharu's Preferred fonts, and
/// `[booharu-runs]`'s run policy, which the run then follows whoever queued
/// it; the flags only its caller knows, kept from `config`'s own snapshot
/// ([`snapshot`]); and, when it translates, the chapter's glossary and notes
/// after its Instructions ([`chapter`]), and `[booharu-emphasis]`, with the
/// instruction to pick the words to stress after those when the run asks
/// its model for them ([`emphasis::asked`]). A section that can't be read, as
/// when the file isn't TOML, refuses the run; a stored value that doesn't read
/// reads as its default.
pub(crate) fn queued(
    config: PipelineConfig,
    session: &Mutex<Session>,
    stages: &BTreeSet<Stage>,
) -> crate::Result<PipelineConfig> {
    let families = koharu_config::get::<RendererConfig>().map(|renderer| renderer.font_families);
    let booharu = snapshot(Snapshot::read(families), RunPolicy::read(), &config.booharu)?;
    let (path, project) = {
        let session = session.lock();
        (session.path().to_owned(), session.project())
    };
    let translator = chapter(
        config.translator,
        &path,
        &project,
        stages,
        SeriesStore::open,
    )?;
    let (booharu, translator) =
        emphasis::asked(booharu, translator, stages, EmphasisSettings::read)?;
    Ok(PipelineConfig {
        booharu,
        translator,
        ..config
    })
}

/// `translator` with what a run of `stages` sends with the chapter of the
/// project file `path`, whose project is `project`, after its Instructions:
/// the glossary, the background, and the characters, from the chapter's
/// side file and the series it links, which `store` opens only when it links
/// one. A run that doesn't translate, or a chapter with nothing to send,
/// keeps the Instructions byte for byte, so Koharu's translation
/// conversation goes on as it would. The same inputs give the same text,
/// as the conversation starts again whenever the instructions change. A
/// side file or series file that can't be read refuses the run, naming it.
fn chapter(
    translator: TranslatorConfig,
    path: &Path,
    project: &Project,
    stages: &BTreeSet<Stage>,
    store: impl FnOnce() -> anyhow::Result<SeriesStore>,
) -> crate::Result<TranslatorConfig> {
    if !stages.contains(&Stage::Translate) {
        return Ok(translator);
    }
    let chapter =
        Chapter::read(path, store).map_err(|error| crate::Error::invalid(format!("{error:#}")))?;
    // Text a run detects or reads first isn't known as it's queued.
    let finds_text = stages.contains(&Stage::Detect) || stages.contains(&Stage::Recognize);
    let extras = Extras::of(
        project,
        &chapter.context,
        chapter.series.as_ref(),
        finds_text,
    );
    if extras.left_out > 0 {
        tracing::warn!(
            "{} glossary terms past the first {MAX_TERMS} were left out of the run",
            extras.left_out
        );
    }
    Ok(TranslatorConfig {
        instructions: context::instructions(&translator.instructions, &extras),
        ..translator
    })
}

/// A run's snapshot: `read`, with `policy` in it and the run's own flags
/// from `asked`, the snapshot of the config it was queued with: Redo boxes'
/// `redo`, a subtitle run's `subtitle_run` and a soft Remove stroke's
/// `soft_limit`, which only the caller that queued the run sets, and which
/// no saved setting holds. Or the error of the first that couldn't be read,
/// which refuses the run as an invalid request.
fn snapshot(
    read: anyhow::Result<Snapshot>,
    policy: anyhow::Result<RunPolicy>,
    asked: &Snapshot,
) -> crate::Result<Snapshot> {
    let invalid = |error: anyhow::Error| crate::Error::invalid(format!("{error:#}"));
    let read = read.map_err(invalid)?;
    Ok(Snapshot {
        runs: Some(policy.map_err(invalid)?),
        redo: asked.redo,
        subtitle_run: asked.subtitle_run,
        soft_limit: asked.soft_limit,
        ..read
    })
}

/// A page a run gave up on while it went on with its other pages, as
/// `Outcome::DoneWithProblems` lists it: the stage that failed every try it
/// had, and why.
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
pub struct PageFailure {
    pub page: PageId,
    pub stage: Stage,
    pub reason: String,
}

/// The stage of Koharu's that a Run menu stage is, by the same name, or none
/// for Read chapter, which is Booharu's own and no stage of a run's pages.
pub fn pipeline_stage(stage: RunStage) -> Option<Stage> {
    match stage {
        RunStage::Detect => Some(Stage::Detect),
        RunStage::Recognize => Some(Stage::Recognize),
        RunStage::Inpaint => Some(Stage::Inpaint),
        RunStage::Read => None,
        RunStage::Translate => Some(Stage::Translate),
    }
}

impl From<Stage> for RunStage {
    fn from(stage: Stage) -> Self {
        match stage {
            Stage::Detect => Self::Detect,
            Stage::Recognize => Self::Recognize,
            Stage::Inpaint => Self::Inpaint,
            Stage::Translate => Self::Translate,
        }
    }
}

/// The ops that follow placement's, `placed`, in the Translation stage's
/// edit for `block`'s new translation `text`, both read before the edit, as
/// placement reads the project: the run's default look, when it changes
/// anything, with the block's category font while the run's Pick fonts
/// automatically is on, the translation's word marks, when the words the
/// run's model picked to stress in it, `block`'s in `picked`, are marked in
/// the run's emphasis, or the marks a run left on the one it replaces come
/// off, then the hand-edit record, on every translation the stage writes.
/// A pick the text doesn't hold is left out, logged. None when placement
/// failed, which fails the stage.
pub(crate) fn translated(
    project: &Project,
    block: LayerId,
    text: &str,
    placed: Option<&Op>,
    booharu: &Snapshot,
    picked: &HashMap<LayerId, Vec<String>>,
) -> anyhow::Result<Vec<Op>> {
    let Some(placed) = placed else {
        return Ok(Vec::new());
    };
    // None picked for a block whose run didn't ask, or whose model picked
    // none: its translation only takes off a run's marks.
    let marking = Marking {
        settings: booharu.emphasis.as_ref(),
        words: picked.get(&block).map(Vec::as_slice).unwrap_or_default(),
    };
    styles::translated(
        project,
        block,
        text,
        placed,
        &booharu.lettering,
        booharu.font.as_deref(),
        &marking,
    )
}

/// The op that follows the Translation stage's clearing of `block`'s
/// translation, given back unchanged, in its edit: the hand-edit record of
/// no translation, read before the edit.
pub(crate) fn given_back(project: &Project, block: LayerId) -> anyhow::Result<Op> {
    styles::given_back(project.layer(block)?)
}

/// The patch of the Translation stage's clearing of `block`'s translation,
/// given back unchanged, read before its edit: Koharu's, the translation
/// gone and the clean-up hidden, so the page keeps its own lettering. A
/// placeholder (`status::placeholder`), a style set by hand on the block
/// before it was translated, stays (D1): it shows no words, so only the
/// clean-up hides, and [`given_back`]'s record tells the block was given
/// back (`status::given_back`), its values still the user's.
pub(crate) fn given_back_patch(project: &Project, block: LayerId) -> serde_json::Value {
    let styled = project
        .layer(block)
        .is_ok_and(|layer| match &layer.content {
            Content::Block(found) => status::placeholder(layer, found),
            _ => false,
        });
    match styled {
        true => serde_json::json!({"cleanup": {"visible": false}}),
        false => serde_json::json!({"translation": null, "cleanup": {"visible": false}}),
    }
}

/// The op recording `text`, the wording the Recognition stage read for
/// `block`, in its hand-edit record, which keeps its other parts.
pub(crate) fn recognized(project: &Project, block: LayerId, text: &str) -> anyhow::Result<Op> {
    styles::recognized(project.layer(block)?, text)
}

/// Starts `conversation`, a project's translation conversation, with the
/// page before `page`, the one its turn translates, in the project `session`
/// holds, as the user left it, when the conversation holds nothing yet: the
/// first time the project translates since it opened, and once `keep`
/// started it again because the model, the target, or the instructions
/// changed, the chapter's glossary and notes and the instruction to pick
/// the words to stress among them. The page goes in as one exchange, in the
/// shapes a turn of the run `booharu` is the snapshot of records its own:
/// its texts numbered as a turn sends them, without the scan, and their
/// translations as they are now, hand fixes included, as the answer, each
/// an object of its translation and the words its run's marks still cover
/// while the run asks its model for the words to stress; with no usage, so
/// the context estimates it as it would any turn it fits; and as the
/// conversation's last page, its plain translations, which the next turn's
/// echo check compares its answer with. Nothing for the first page, a page
/// before with no translated text, or a conversation under way, which so
/// never gets a second.
pub(crate) fn seed(
    conversation: &mut Conversation,
    session: &Mutex<Session>,
    page: PageId,
    booharu: &Snapshot,
) {
    if conversation.last.is_some() || !conversation.context.messages().is_empty() {
        return;
    }
    let Some(before) = before(&session.lock().project(), page) else {
        return;
    };
    let keys = turn::keys(before.texts.len());
    let answer = match booharu.asks_emphasis {
        true => emphasis::numbered(&keys, &before.translations, &before.stressed),
        false => turn::numbered(&keys, before.translations.iter().map(String::as_str)),
    };
    let exchange = vec![
        Message::user(turn::numbered(
            &keys,
            before.texts.iter().map(String::as_str),
        )),
        Message::assistant(answer),
    ];
    conversation.context.record([(exchange, Usage::default())]);
    conversation.last = Some((before.texts, before.translations));
}

/// The page before the one a turn translates, as its conversation would
/// have had it.
struct Before {
    /// Its blocks' sources, as a turn sends them.
    texts: Vec<String>,
    /// Their translations, trimmed as an answer is.
    translations: Vec<String>,
    /// The words of each that the marks its run gave it still cover.
    stressed: Vec<Vec<String>>,
}

/// The page before `page` in `project`'s page order, as its translation
/// conversation would have had it: each of its blocks with a source and a
/// translation, in the order a turn lists a page's blocks, the source as a
/// turn sends it and the translation as it is now, trimmed as an answer is,
/// with the words of it its run stressed ([`emphasis::stressed`]); `None`
/// for the first page, or one before it with no such block.
fn before(project: &Project, page: PageId) -> Option<Before> {
    let pages: Vec<PageId> = project.pages().map(|page| page.id).collect();
    let at = pages.iter().position(|&id| id == page)?;
    let before = *pages.get(at.checked_sub(1)?)?;
    let mut lines = Before {
        texts: Vec::new(),
        translations: Vec::new(),
        stressed: Vec::new(),
    };
    for (id, block) in project.blocks(before).ok()? {
        let text = block.source.content.text();
        let Some(translation) = block.translation.as_ref() else {
            continue;
        };
        let trimmed = translation.content.text().trim().to_owned();
        if text.trim().is_empty() || trimmed.is_empty() {
            continue;
        }
        let stressed = project
            .layer(id)
            .map(|layer| emphasis::stressed(layer, &translation.content))
            .unwrap_or_default();
        lines.texts.push(text);
        lines.translations.push(trimmed);
        lines.stressed.push(stressed);
    }
    (!lines.texts.is_empty()).then_some(lines)
}

/// A stage task's failure: the reason its run reports, as Koharu words it,
/// and what kind of failure it was, read from the error before it was worded,
/// which the run policy goes by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Failure {
    pub(crate) reason: String,
    pub(crate) class: Class,
}

/// What kind of failure a stage's was, as the run policy treats it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Class {
    /// The page's input is invalid, as a translation's placement finds it,
    /// or the project refused the page's edit, naming a layer that's gone or
    /// locked: another try fails the same way.
    Invalid,
    /// The project couldn't be read, written, or saved, or the runtime shut
    /// down while a model loaded: the run ends, whatever its policy, as a try
    /// after either would find the same.
    Write,
    /// A model couldn't load: another try, with nothing freed meanwhile, fails
    /// the same way.
    Load,
    /// A hosted model's rate limit or server error, which koharu-llm already
    /// sent three more times, so another page try would send it four more.
    HostedRetried,
    /// Anything else, which another try may get past: a dropped connection,
    /// the ten minutes a hosted translation has, an answer broken, blank, or
    /// short of a text, or a model's own error.
    Retryable,
}

impl Failure {
    /// A stage task that panicked, worded as Koharu words it, which another
    /// try may get past as it may a model's own error.
    pub(crate) fn panicked(stage: Stage, error: &tokio::task::JoinError) -> Self {
        Self {
            reason: format!("{stage} failed: {error}"),
            class: Class::Retryable,
        }
    }
}

/// The class of `error`, a stage's, read from the first cause in its chain
/// that says: Koharu's own invalid request, scene, load, and shutdown errors,
/// a scene error from the session, then the HTTP status of a hosted model's
/// reply, however rig carries it; anything else is worth another try.
pub(crate) fn class(error: &anyhow::Error) -> Class {
    for cause in error.chain() {
        match cause.downcast_ref::<crate::Error>() {
            Some(crate::Error::Invalid(_)) => return Class::Invalid,
            Some(crate::Error::Scene(error)) => return scene(error),
            Some(crate::Error::Load { .. }) => return Class::Load,
            Some(crate::Error::Shutdown) => return Class::Write,
            Some(crate::Error::Stage { .. }) | None => {}
        }
        if let Some(error) = cause.downcast_ref::<koharu_scene::Error>() {
            return scene(error);
        }
        let status = if let Some(error) = cause.downcast_ref::<PromptError>() {
            error.provider_response_status()
        } else if let Some(error) = cause.downcast_ref::<ProviderError>() {
            error.provider_response_status()
        } else if let Some(report) = cause.downcast_ref::<ErrorReport>() {
            report.provider_response_status()
        } else if let Some(http_client::Error::InvalidStatusCodeWithDetails { status, .. }) =
            cause.downcast_ref::<http_client::Error>()
        {
            Some(*status)
        } else {
            None
        };
        // What koharu-llm's client sends again: a 429 or a 5xx.
        if status.is_some_and(|status| status.as_u16() == 429 || status.is_server_error()) {
            return Class::HostedRetried;
        }
    }
    Class::Retryable
}

/// The class of a scene error: an edit the project refused, one naming a
/// layer that's gone or locked, or wrong in itself, is the page's own, and
/// the run can go on without the page; any other, a file or database error
/// or a project that can't be opened, leaves every page unwritten.
fn scene(error: &koharu_scene::Error) -> Class {
    use koharu_scene::Error::{Invalid, Locked, NotFound, Rejected};
    match error {
        NotFound(_) | Invalid { .. } | Locked { .. } | Rejected(_) => Class::Invalid,
        _ => Class::Write,
    }
}

/// How long a hosted translation's request has to answer, as Booharu 0.0.4
/// gave each: a provider that took the request and never answers then fails
/// the page, which is tried again, rather than holding the run.
const ANSWER_LIMIT: Duration = Duration::from_secs(10 * 60);

/// A hosted translation request that went unanswered for `ANSWER_LIMIT`.
#[derive(Debug, thiserror::Error)]
#[error("The translation model didn't answer within 10 minutes.")]
pub(crate) struct Unanswered;

/// `request`, a translation's call to `model`, which fails as `Unanswered`
/// after ten minutes when `model` is hosted, and waits as long as it takes
/// for Koharu's own: a local model's turn waits only on the GPU, which a
/// slow card makes long. Every other vendor counts as hosted, LM Studio's,
/// Ollama's, and an OpenAI-compatible server's included, as in Booharu 0.0.4.
/// The limit goes on each request, never on koharu-llm's shared client, which
/// also carries the agent's streams.
pub(crate) async fn answered<T, E: Into<anyhow::Error>>(
    model: &DynModel<Completion>,
    request: impl Future<Output = Result<T, E>>,
) -> anyhow::Result<T> {
    within(ANSWER_LIMIT, hosted(model.spec()), request).await
}

/// Whether the model `spec` describes is a hosted one: any vendor's but
/// Koharu's own, as `stages::key` tells them apart by the `vendor/model`
/// reference the spec was connected from.
fn hosted(spec: Option<&ModelSpec>) -> bool {
    spec.is_none_or(|spec| spec.provider.vendor() != koharu_llm::KOHARU)
}

/// `request`, failing as `Unanswered` once `limit` passes when `limited`.
async fn within<T, E: Into<anyhow::Error>>(
    limit: Duration,
    limited: bool,
    request: impl Future<Output = Result<T, E>>,
) -> anyhow::Result<T> {
    if !limited {
        return request.await.map_err(Into::into);
    }
    match tokio::time::timeout(limit, request).await {
        Ok(answer) => answer.map_err(Into::into),
        Err(_) => Err(Unanswered.into()),
    }
}

/// Whether the pipeline got the runtime's device, which loads libtorch, for
/// this process, as each model load does first, whether the load then
/// succeeds or fails: before, libtorch isn't loaded and can't be called.
static LIBTORCH: AtomicBool = AtomicBool::new(false);

/// Notes that libtorch is loaded, as a model load got the runtime's device.
pub(crate) fn libtorch_started() {
    LIBTORCH.store(true, Ordering::Release);
}

/// Whether libtorch is loaded, so its cache may be emptied, as Booharu's
/// unload (`scheduler/booharu.rs`) empties it, though no model loaded, as
/// when the first load failed after it allocated.
pub(crate) fn libtorch_loaded() -> bool {
    LIBTORCH.load(Ordering::Acquire)
}

/// Stand-ins for the stages in the scheduler's tests, which load no model:
/// what each page stage of a scripted project does, try by try, and a log of
/// the tasks that ran and the models the scheduler unloaded, in order; and
/// for a hosted model in the Translation stage's own tests, a scripted one.
#[cfg(test)]
pub(crate) mod stand_in {
    use std::{
        collections::{BTreeMap, HashMap, HashSet, VecDeque},
        time::Duration,
    };

    use koharu_llm::rig::{driver::DynModel, operation::Completion};
    use koharu_scene::ProjectId;
    use parking_lot::Mutex;
    use tokio::time::Instant;

    use crate::{
        Stage,
        gpu::Key,
        stages::{Member, Stopped, Task},
    };

    /// What a page stage's try does.
    pub(crate) enum Try {
        /// It takes this long, then passes.
        Slow(Duration),
        /// It fails with the error this makes.
        Fail(fn() -> anyhow::Error),
    }

    /// A stand-in task that ran, or an unloading the scheduler queued.
    #[derive(Clone, Debug, PartialEq)]
    pub(crate) enum Entry {
        /// A task of `stage` on the pages at these places in the run, from
        /// `at`.
        Ran {
            stage: Stage,
            pages: Vec<usize>,
            at: Instant,
        },
        /// Every model but these stages' was unloaded.
        Unloaded(Vec<Stage>),
    }

    #[derive(Default)]
    struct Script {
        /// Each page stage's tries to come; a try with none passes.
        tries: HashMap<(usize, Stage), VecDeque<Try>>,
        log: Vec<Entry>,
    }

    static SCRIPTS: Mutex<BTreeMap<ProjectId, Script>> = Mutex::new(BTreeMap::new());

    /// The stand-ins of a project's stages while it's held.
    pub(crate) struct Scripted(ProjectId);

    impl Scripted {
        pub(crate) fn new(project: ProjectId) -> Self {
            SCRIPTS.lock().insert(project, Script::default());
            Self(project)
        }

        /// Gives the stage of the page at `index` these tries, before any
        /// it has left.
        pub(crate) fn tries(&self, index: usize, stage: Stage, tries: Vec<Try>) {
            let mut scripts = SCRIPTS.lock();
            let script = scripts.get_mut(&self.0).expect("held");
            script
                .tries
                .entry((index, stage))
                .or_default()
                .extend(tries);
        }

        /// What ran, and what was unloaded, so far.
        pub(crate) fn log(&self) -> Vec<Entry> {
            SCRIPTS.lock()[&self.0].log.clone()
        }
    }

    impl Drop for Scripted {
        fn drop(&mut self) {
            SCRIPTS.lock().remove(&self.0);
        }
    }

    fn project(task: &Task) -> ProjectId {
        task.members[0].session.lock().id()
    }

    /// Whether `task`'s project has stand-ins.
    pub(crate) fn scripted(task: &Task) -> bool {
        SCRIPTS.lock().contains_key(&project(task))
    }

    /// `task` as its stand-ins go: started as a stage starts, unless its run
    /// stopped, then each of its pages' next try, in order, the first failure
    /// failing it whole, as a recognition task fails with any of its pages.
    pub(crate) async fn run(task: &Task) -> anyhow::Result<()> {
        let members: Vec<&Member> = task.members.iter().collect();
        // As the GPU drops a stopped run's work unrun.
        if members.iter().all(|member| member.cancel.is_cancelled()) {
            return Err(Stopped.into());
        }
        task.started(&members);
        let tries: Vec<Try> = {
            let mut scripts = SCRIPTS.lock();
            let script = scripts.get_mut(&project(task)).expect("scripted");
            script.log.push(Entry::Ran {
                stage: task.stage,
                pages: members.iter().map(|member| member.index).collect(),
                at: Instant::now(),
            });
            members
                .iter()
                .filter_map(|member| {
                    script
                        .tries
                        .get_mut(&(member.index, task.stage))?
                        .pop_front()
                })
                .collect()
        };
        for tried in tries {
            match tried {
                Try::Slow(time) => tokio::time::sleep(time).await,
                Try::Fail(error) => return Err(error()),
            }
        }
        Ok(())
    }

    /// Logs that the scheduler unloaded every model but `wanted`, for a run
    /// of `project`.
    pub(crate) fn unloaded(project: ProjectId, wanted: &HashSet<Key>) {
        if let Some(script) = SCRIPTS.lock().get_mut(&project) {
            let mut kept: Vec<Stage> = wanted.iter().map(|key| key.stage).collect();
            kept.sort();
            script.log.push(Entry::Unloaded(kept));
        }
    }

    /// The scripted models standing in for hosted ones, by the `vendor/model`
    /// reference a test's runs name, each test its own.
    static HOSTED: Mutex<BTreeMap<String, DynModel<Completion>>> = Mutex::new(BTreeMap::new());

    /// A scripted model a translation connects for its reference while it's
    /// held, as it would a hosted one.
    pub(crate) struct Hosted(String);

    impl Hosted {
        pub(crate) fn new(reference: &str, model: DynModel<Completion>) -> Self {
            HOSTED.lock().insert(reference.to_owned(), model);
            Self(reference.to_owned())
        }
    }

    impl Drop for Hosted {
        fn drop(&mut self) {
            HOSTED.lock().remove(&self.0);
        }
    }

    /// The scripted model a test holds for `reference`, if any.
    pub(crate) fn hosted(reference: &str) -> Option<DynModel<Completion>> {
        HOSTED.lock().get(reference).cloned()
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc, time::Duration};

    use anyhow::{Context as _, Result};
    use booharu_core::{
        Edited,
        attrs::{Auto, AutoMark},
        context::{ChapterContext, GlossaryEntry},
        edited,
        lettering::{CategoryFont, LetteringCategory},
        resolver::koharu_chars,
        runs::RunsSettings,
        settings::{DefaultTextStyle, LetteringSettings},
        styles::text_hash,
    };
    use icu_locale_core::langid;
    use koharu_scene::{CharStyle, Content, Created, Edit, Layer, Rgba, Session, Span, Text};
    use parking_lot::Mutex;
    use serde_json::{Value, json};

    use super::*;
    use crate::{Event, Pipeline, Request, Target, TranslatorConfig, stages::translate::written};

    const BLUE: Rgba = Rgba([0.0, 0.0, 1.0, 1.0]);
    const WHITE: Rgba = Rgba([1.0; 4]);
    const BLACK: Rgba = Rgba([0.0, 0.0, 0.0, 1.0]);
    const RED: Rgba = Rgba([1.0, 0.0, 0.0, 1.0]);
    const YELLOW: Rgba = Rgba([1.0, 1.0, 0.0, 1.0]);

    /// The source wording of every block.
    const SOURCE: &str = "どうして";

    /// A project of one 400-pixel page holding a block for each `(label,
    /// role)`, found and read by Koharu's stages: lettered in blue outlined
    /// 3 pixels wide in white at size 32, reading [`SOURCE`], and not yet
    /// translated.
    struct Page {
        _dir: tempfile::TempDir,
        session: Session,
        /// The layers labelled, by label.
        blocks: BTreeMap<String, LayerId>,
    }

    /// A block of `role` as [`Page::new`] makes each, its source's box 40 by
    /// 120 at `x`, `y`.
    fn found(role: &str, (x, y): (f64, f64)) -> Value {
        json!({"kind": "block", "role": role, "source": {
            "frame": {"box": {"bounds": {"x": x, "y": y, "width": 40.0, "height": 120.0}}},
            "content": SOURCE,
            "style": {"chars": {"size": 32.0, "fill": BLUE, "stroke": WHITE, "stroke_width": 3.0}},
        }, "attributes": {"c2pa": {
            "detection": {"action": "c2pa.created"},
            "ocr": {"action": "c2pa.created"},
        }}})
    }

    impl Page {
        fn new(blocks: &[(&str, &str)]) -> Result<Self> {
            let layers = blocks.iter().enumerate().map(|(index, (label, role))| {
                let x = 10.0 + 50.0 * index as f64;
                json!({"op": "add_layer", "id": format!("${label}"), "page": "$p", "index": index, "layer": found(role, (x, 20.0))})
            });
            Self::of(layers.collect())
        }

        /// The page holding `layers`, add_layer ops' JSON on the page `$p`.
        fn of(layers: Vec<Value>) -> Result<Self> {
            let dir = tempfile::tempdir()?;
            let mut session = Session::create(dir.path().join("test.khr"))?;
            let page = json!({"op": "add_page", "id": "$p", "index": 0, "size": {"width": 400, "height": 400}});
            let change = session.apply(Edit {
                label: "test".to_owned(),
                ops: std::iter::once(page)
                    .chain(layers)
                    .map(serde_json::from_value)
                    .collect::<Result<_, _>>()?,
            })?;
            let blocks = change
                .created
                .into_iter()
                .filter_map(|created| match created {
                    Created::Layer {
                        id,
                        label: Some(label),
                    } => Some((label, id)),
                    _ => None,
                })
                .collect();
            Ok(Self {
                _dir: dir,
                session,
                blocks,
            })
        }

        fn id(&self, label: &str) -> LayerId {
            self.blocks[label]
        }

        fn layer(&self, label: &str) -> Result<Layer> {
            Ok(self.session.project().layer(self.id(label))?.clone())
        }

        fn translation(&self, label: &str) -> Result<Option<Text>> {
            match self.layer(label)?.content {
                Content::Block(block) => Ok(block.translation),
                _ => anyhow::bail!("{label} isn't a block"),
            }
        }

        fn chars(&self, label: &str) -> Result<CharStyle> {
            let translation = self.translation(label)?.context("a translation")?;
            Ok(translation.style.chars)
        }

        fn edit(&mut self, ops: Vec<Op>) -> Result<()> {
            self.session.apply(Edit {
                label: "test".to_owned(),
                ops,
            })?;
            Ok(())
        }

        /// Translates each `(label, text)` as the Translation stage writes
        /// a page's translations, under `booharu`, a run's snapshot: the ops
        /// of `written`, applied as its one edit.
        fn translate(&mut self, texts: &[(&str, &str)], booharu: &Snapshot) -> Result<()> {
            let texts: Vec<(&str, &str, &[&str])> = texts
                .iter()
                .map(|&(label, text)| (label, text, &[][..]))
                .collect();
            self.stress(&texts, booharu)
        }

        /// [`Page::translate`], the run's model picking the words of each
        /// `(label, text, words)` to stress.
        fn stress(&mut self, texts: &[(&str, &str, &[&str])], booharu: &Snapshot) -> Result<()> {
            let translated: Vec<_> = texts
                .iter()
                .map(|(label, text, _)| (self.id(label), langid!("ja"), (*text).to_owned()))
                .collect();
            let picks = texts
                .iter()
                .map(|(_, _, words)| words.iter().map(|word| (*word).to_owned()).collect())
                .collect();
            let picked = super::picked(&translated, picks);
            let ops = written(
                &self.session.project(),
                translated,
                &TranslatorConfig::default(),
                booharu,
                &picked,
            )?;
            self.edit(ops)
        }

        /// Reads each `(label, text)` as the Recognition stage writes the
        /// wording it read, with its hook.
        fn recognize(&mut self, texts: &[(&str, &str)]) -> Result<()> {
            let project = self.session.project();
            let mut ops = Vec::new();
            for (label, text) in texts {
                let block = self.id(label);
                ops.push(Op::UpdateLayer {
                    layer: block.into(),
                    patch: json!({"source": {"content": text, "language": "ja"}}),
                });
                ops.push(Op::SetAttribute {
                    layer: block.into(),
                    namespace: "c2pa".to_owned(),
                    key: "ocr".to_owned(),
                    value: Some(json!({"action": "c2pa.created"})),
                });
                ops.push(recognized(&project, block, text)?);
            }
            self.edit(ops)
        }
    }

    fn snapshot(
        detect_colors: bool,
        fill: Option<Rgba>,
        stroke: Option<Rgba>,
        font: Option<&str>,
    ) -> Snapshot {
        Snapshot {
            lettering: LetteringSettings {
                default: DefaultTextStyle {
                    fill,
                    stroke,
                    stroke_width: 2.5,
                },
                detect_colors,
                ..LetteringSettings::default()
            },
            font: font.map(str::to_owned),
            ..Snapshot::default()
        }
    }

    #[test]
    fn a_queued_run_takes_the_run_policy_or_is_refused() -> Result<()> {
        let policy = RunsSettings {
            retries: 4,
            ..RunsSettings::default()
        }
        .policy();
        let read = snapshot(true, None, None, Some("CCWildWords"));
        let queued = super::snapshot(Ok(read.clone()), Ok(policy), &Snapshot::default())?;
        assert_eq!(
            queued,
            Snapshot {
                runs: Some(policy),
                ..read.clone()
            }
        );
        // A [booharu-runs] that can't be read, in a settings file that isn't
        // TOML, refuses the run, naming the section.
        let unreadable = anyhow::anyhow!("config.toml is invalid: TOML parse error")
            .context("Booharu's run settings can't be read from [booharu-runs]");
        let refused = super::snapshot(Ok(read), Err(unreadable), &Snapshot::default())
            .expect_err("an unreadable policy");
        assert!(
            matches!(&refused, crate::Error::Invalid(message)
                if message.contains("[booharu-runs]") && message.contains("TOML parse error")),
            "{refused}"
        );
        // A config a caller made, as upstream's tests and examples make one,
        // runs Koharu's way.
        assert_eq!(PipelineConfig::default().booharu.runs, None);
        Ok(())
    }

    #[test]
    fn a_queued_run_keeps_its_callers_flags_and_reads_every_setting_again() -> Result<()> {
        let read = snapshot(true, None, None, Some("CCWildWords"));
        let policy = RunsSettings::default().policy();
        // Redo boxes taking hand-fixed boxes, a subtitle run, and a soft
        // Remove stroke, as their callers ask on the config they queue; a
        // setting the caller's config holds is read again, as it's saved now.
        let asked = Snapshot {
            redo: true,
            subtitle_run: true,
            soft_limit: true,
            font: Some("Comic Neue".to_owned()),
            ..Snapshot::default()
        };
        assert_eq!(
            super::snapshot(Ok(read.clone()), Ok(policy), &asked)?,
            Snapshot {
                runs: Some(policy),
                redo: true,
                subtitle_run: true,
                soft_limit: true,
                ..read.clone()
            }
        );
        // Each alone, as no flag carries another.
        let soft = Snapshot {
            soft_limit: true,
            ..Snapshot::default()
        };
        let queued = super::snapshot(Ok(read), Ok(policy), &soft)?;
        assert!(queued.soft_limit && !queued.redo && !queued.subtitle_run);
        Ok(())
    }

    #[test]
    fn the_callers_flags_go_through_queued() -> Result<()> {
        let Page {
            _dir: _held,
            session,
            blocks: _,
        } = Page::new(&[("a", "text")])?;
        let session = Mutex::new(session);
        for (redo, subtitle_run, soft_limit) in [
            (true, false, false),
            (false, true, false),
            (false, false, true),
            (false, false, false),
        ] {
            let config = PipelineConfig {
                booharu: Snapshot {
                    redo,
                    subtitle_run,
                    soft_limit,
                    ..Snapshot::default()
                },
                ..PipelineConfig::default()
            };
            for stages in [&[Stage::Recognize][..], &[Stage::Translate, Stage::Inpaint]] {
                let run = queued(config.clone(), &session, &stages.iter().copied().collect())?;
                assert_eq!(
                    (
                        run.booharu.redo,
                        run.booharu.subtitle_run,
                        run.booharu.soft_limit
                    ),
                    (redo, subtitle_run, soft_limit),
                    "{stages:?}"
                );
                assert!(run.booharu.runs.is_some(), "the run policy is read");
            }
        }
        Ok(())
    }

    #[test]
    fn a_queued_translation_takes_the_emphasis_and_a_translation_model_keeps_its_request()
    -> Result<()> {
        let Page {
            _dir: _held,
            session,
            blocks: _,
        } = Page::new(&[("a", "text")])?;
        let session = Mutex::new(session);
        // Koharu's default model, Index-Translate, which is never asked.
        let config = PipelineConfig {
            translator: TranslatorConfig {
                instructions: "Keep honorifics.".to_owned(),
                ..TranslatorConfig::default()
            },
            ..PipelineConfig::default()
        };
        for stages in [
            &[Stage::Translate][..],
            &[Stage::Recognize, Stage::Translate],
        ] {
            let run = queued(config.clone(), &session, &stages.iter().copied().collect())?;
            assert!(run.booharu.emphasis.is_some(), "{stages:?}");
            assert!(!run.booharu.asks_emphasis, "{stages:?}");
            assert_eq!(run.translator, config.translator, "{stages:?}");
        }
        // A run that doesn't translate takes none.
        let run = queued(config.clone(), &session, &[Stage::Recognize].into())?;
        assert_eq!(run.booharu.emphasis, None);
        Ok(())
    }

    #[tokio::test]
    async fn blocks_a_stage_left_alone_are_told_in_the_runs_history_as_skipped() -> Result<()> {
        let events = crate::event::Events::new();
        let mut listener = events.subscribe();
        let page: PageId = "00000000-0000-0000-0000-000000000001".parse()?;
        let skipped = Event::Skipped {
            run: crate::RunId(3),
            page,
            stage: Stage::Translate,
            sound_effects: 2,
            hand_fixed: 1,
            locked: 0,
            blocks: true,
        };
        events.send(skipped.clone());
        assert_eq!(events.told(), std::slice::from_ref(&skipped));
        assert_eq!(listener.recv().await?, skipped);
        assert_eq!(
            serde_json::to_value(&skipped)?,
            json!({"kind": "skipped", "run": 3, "page": page, "stage": "translate",
                "sound_effects": 2, "hand_fixed": 1, "locked": 0, "blocks": true})
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_run_tells_its_stages_and_whether_it_works_on_pages() -> Result<()> {
        // A page with no scan, so no stage here loads a model.
        let Page {
            _dir: _held,
            session,
            blocks,
        } = Page::new(&[("a", "text")])?;
        let session = Arc::new(Mutex::new(session));
        let pipeline = Pipeline::new(PipelineConfig::default());
        let queued = async |target: Target, stages: &[Stage]| -> Result<(Vec<Stage>, bool)> {
            let request = Request {
                target,
                stages: stages.iter().copied().collect(),
                config: None,
            };
            let run = pipeline.run(Arc::clone(&session), request)?;
            let told = pipeline.history(run, Duration::from_secs(5)).await?;
            match told.first() {
                Some(Event::Queued { stages, pages, .. }) => Ok((stages.clone(), *pages)),
                other => anyhow::bail!("the run's first event is {other:?}"),
            }
        };
        assert_eq!(
            queued(Target::Project, &[Stage::Inpaint, Stage::Detect]).await?,
            (vec![Stage::Detect, Stage::Inpaint], true)
        );
        assert_eq!(
            queued(Target::Blocks(vec![blocks["a"]]), &[Stage::Recognize]).await?,
            (vec![Stage::Recognize], false)
        );
        Ok(())
    }

    /// The Instructions a run of `stages` on `page` is sent, the user's own
    /// being `own`, with the series in the folder `series` beside it.
    fn sent(page: &Page, own: &str, stages: &[Stage]) -> crate::Result<TranslatorConfig> {
        let path = page.session.path();
        let translator = TranslatorConfig {
            instructions: own.to_owned(),
            ..TranslatorConfig::default()
        };
        chapter(
            translator,
            path,
            &page.session.project(),
            &stages.iter().copied().collect(),
            || Ok(SeriesStore::new(path.with_file_name("series"))),
        )
    }

    #[test]
    fn a_translating_run_sends_the_linked_series_terms_after_the_instructions() -> Result<()> {
        let page = Page::new(&[("a", "text")])?;
        let path = page.session.path();
        let own = "Keep honorifics.\n";
        // A chapter linking no series sends nothing: the Instructions stay
        // byte for byte, and so does the rest.
        let unlinked = sent(&page, own, &[Stage::Translate])?;
        assert_eq!(
            unlinked,
            TranslatorConfig {
                instructions: own.to_owned(),
                ..TranslatorConfig::default()
            }
        );

        let store = SeriesStore::new(path.with_file_name("series"));
        let mut series = store.create("Blue Archive")?;
        series.glossary = vec![
            GlossaryEntry::new("リムル", "Rimuru"),
            GlossaryEntry {
                note: "a question".to_owned(),
                ..GlossaryEntry::new(SOURCE, "Why")
            },
        ];
        store.save(&series)?;
        ChapterContext::update(path, |chapter| chapter.series = Some(series.id.clone()))?;
        let linked = sent(&page, own, &[Stage::Inpaint, Stage::Translate])?;
        assert_eq!(
            linked.instructions,
            "Keep honorifics.\n\nGlossary (mandatory: use these translations and keep their \
             spelling):\n- どうして → Why (a question)"
        );
        assert_eq!(
            TranslatorConfig {
                instructions: own.to_owned(),
                ..linked
            },
            unlinked,
            "only the Instructions change"
        );
        // A run that doesn't translate is sent nothing.
        assert_eq!(sent(&page, own, &[Stage::Inpaint])?, unlinked);
        // One that reads the text first, which isn't known yet, every term.
        let fresh = sent(&page, own, &[Stage::Recognize, Stage::Translate])?;
        assert!(
            fresh.instructions.contains("- リムル → Rimuru"),
            "{}",
            fresh.instructions
        );
        Ok(())
    }

    #[test]
    fn a_side_file_or_series_that_cant_be_read_refuses_a_translating_run() -> Result<()> {
        let page = Page::new(&[("a", "text")])?;
        let path = page.session.path();
        fs_err::write(context::side_file(path), "{ not json")?;
        let refused = sent(&page, "", &[Stage::Translate]).expect_err("a broken side file");
        assert!(
            matches!(&refused, crate::Error::Invalid(message) if message.contains("test.booharu.json")),
            "{refused}"
        );
        // A run that doesn't translate never reads it.
        sent(
            &page,
            "",
            &[Stage::Detect, Stage::Recognize, Stage::Inpaint],
        )?;

        let id = "blue-archive-000000000000";
        fs_err::write(context::side_file(path), json!({"series": id}).to_string())?;
        fs_err::create_dir_all(path.with_file_name("series"))?;
        fs_err::write(
            path.with_file_name("series").join(format!("{id}.json")),
            "[",
        )?;
        let refused = sent(&page, "", &[Stage::Translate]).expect_err("a broken series");
        assert!(
            matches!(&refused, crate::Error::Invalid(message)
                if message.contains("test.booharu.json") && message.contains(&format!("{id}.json"))),
            "{refused}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn pipeline_run_refuses_a_translation_its_chapter_cant_be_read_for() -> Result<()> {
        let Page {
            _dir: _held,
            session,
            blocks: _,
        } = Page::new(&[("a", "text")])?;
        fs_err::write(context::side_file(session.path()), "{ not json")?;
        let session = Arc::new(Mutex::new(session));
        let pipeline = Pipeline::new(PipelineConfig::default());
        let request = Request {
            target: Target::Project,
            stages: [Stage::Translate].into(),
            config: None,
        };
        let refused = pipeline
            .run(session, request)
            .expect_err("a broken side file");
        assert!(
            matches!(&refused, crate::Error::Invalid(message) if message.contains("test.booharu.json")),
            "{refused}"
        );
        Ok(())
    }

    /// A chapter of pages without a scan, each holding a block, found and
    /// read, for each of its `(source, translation)`, the translation the
    /// Translation stage's; and its pages and each page's blocks, in order.
    struct Chapter {
        _dir: tempfile::TempDir,
        session: Mutex<Session>,
        pages: Vec<PageId>,
        blocks: Vec<Vec<LayerId>>,
    }

    impl Chapter {
        fn new(pages: &[&[(&str, Option<&str>)]]) -> Result<Self> {
            let dir = tempfile::tempdir()?;
            let mut session = Session::create(dir.path().join("test.khr"))?;
            let mut ops = Vec::new();
            for (index, blocks) in pages.iter().enumerate() {
                let page = format!("$p{index}");
                ops.push(json!({"op": "add_page", "id": page, "index": index,
                    "size": {"width": 400, "height": 400}}));
                for (at, (source, translation)) in blocks.iter().enumerate() {
                    let id = format!("$p{index}b{at}");
                    let mut layer = found("text", (10.0 + 50.0 * at as f64, 20.0));
                    layer["source"]["content"] = json!(source);
                    ops.push(
                        json!({"op": "add_layer", "id": id, "page": page, "index": at,
                        "layer": layer}),
                    );
                    if let Some(text) = translation {
                        ops.push(json!({"op": "set_text", "layer": id, "text": text}));
                        ops.push(
                            json!({"op": "set_attribute", "layer": id, "namespace": "c2pa",
                            "key": "translation", "value": {"action": "c2pa.translated"}}),
                        );
                    }
                }
            }
            let change = session.apply(Edit {
                label: "test".to_owned(),
                ops: ops
                    .into_iter()
                    .map(serde_json::from_value)
                    .collect::<Result<_, _>>()?,
            })?;
            let (mut made, mut layers) = (BTreeMap::new(), BTreeMap::new());
            for created in change.created {
                match created {
                    Created::Page {
                        id,
                        label: Some(label),
                    } => {
                        made.insert(label, id);
                    }
                    Created::Layer {
                        id,
                        label: Some(label),
                    } => {
                        layers.insert(label, id);
                    }
                    _ => {}
                }
            }
            let blocks = pages.iter().enumerate().map(|(index, blocks)| {
                let at = 0..blocks.len();
                at.map(|at| layers[&format!("p{index}b{at}")]).collect()
            });
            Ok(Self {
                _dir: dir,
                session: Mutex::new(session),
                pages: (0..pages.len())
                    .map(|index| made[&format!("p{index}")])
                    .collect(),
                blocks: blocks.collect(),
            })
        }

        /// The project's translation conversation, new, as the first
        /// translation since the project opened finds it, and as `keep`
        /// leaves one it started again.
        fn conversation(&self) -> Result<tokio::sync::OwnedMutexGuard<Conversation>> {
            let id = self.session.lock().id();
            crate::stages::translate::Conversations::default()
                .lock(id)
                .context("a conversation no one holds")
        }
    }

    /// `conversation`'s history, as the next turn sends it and the context
    /// fits it.
    fn history(conversation: &Conversation) -> Vec<Message> {
        let messages = conversation.context.messages().iter();
        messages.map(|message| Message::clone(message)).collect()
    }

    /// Records a page's turn in `conversation` as a turn records it once its
    /// answer passed.
    fn turn(conversation: &mut Conversation, text: &str, translation: &str) {
        let exchange = vec![
            Message::user(format!("{{\n  \"1\": {}\n}}", json!(text))),
            Message::assistant(format!("{{\"1\": {}}}", json!(translation))),
        ];
        conversation.context.record([(exchange, Usage::default())]);
        conversation.last = Some((vec![text.to_owned()], vec![translation.to_owned()]));
    }

    #[test]
    fn a_fresh_conversation_starts_with_the_page_before_as_the_user_left_it() -> Result<()> {
        let chapter = Chapter::new(&[
            &[
                ("ヒナです。", Some("It's Hina.")),
                // Nothing read, nothing translated: neither goes in.
                ("", None),
                ("どうして", None),
                ("先生!", Some("Teacher!")),
            ],
            &[("ここに", None)],
        ])?;
        // The user fixed the last line by hand.
        chapter.session.lock().apply(Edit {
            label: "test".to_owned(),
            ops: vec![Op::SetText {
                layer: chapter.blocks[0][3].into(),
                text: " Sensei! ".to_owned(),
            }],
        })?;
        let mut conversation = chapter.conversation()?;
        seed(
            &mut conversation,
            &chapter.session,
            chapter.pages[1],
            &Snapshot::default(),
        );
        // One exchange in the shapes a turn records: the page's texts
        // numbered as a turn sends them, then their translations as they are
        // now, the hand fix among them.
        assert_eq!(
            history(&conversation),
            [
                Message::user("{\n  \"1\": \"ヒナです。\",\n  \"2\": \"先生!\"\n}"),
                Message::assistant("{\n  \"1\": \"It's Hina.\",\n  \"2\": \"Sensei!\"\n}"),
            ]
        );
        // And the last page, which the next answer mustn't echo.
        assert_eq!(
            conversation.last,
            Some((
                vec!["ヒナです。".to_owned(), "先生!".to_owned()],
                vec!["It's Hina.".to_owned(), "Sensei!".to_owned()],
            ))
        );
        Ok(())
    }

    #[test]
    fn a_conversation_under_way_gets_no_second_seed() -> Result<()> {
        let chapter = Chapter::new(&[
            &[("ヒナです。", Some("It's Hina."))],
            &[("ここに", Some("Here."))],
            &[("まだ", None)],
        ])?;
        let mut conversation = chapter.conversation()?;
        seed(
            &mut conversation,
            &chapter.session,
            chapter.pages[1],
            &Snapshot::default(),
        );
        let seeded = (history(&conversation), conversation.last.clone());
        assert_eq!(seeded.0.len(), 2);
        // The next page's turn keeps the conversation going: no seed.
        seed(
            &mut conversation,
            &chapter.session,
            chapter.pages[2],
            &Snapshot::default(),
        );
        assert_eq!((history(&conversation), conversation.last.clone()), seeded);

        // Nor does one a turn went into.
        let mut conversation = chapter.conversation()?;
        turn(&mut conversation, "ヒナです。", "It's Hina.");
        let kept = (history(&conversation), conversation.last.clone());
        seed(
            &mut conversation,
            &chapter.session,
            chapter.pages[1],
            &Snapshot::default(),
        );
        assert_eq!((history(&conversation), conversation.last.clone()), kept);
        Ok(())
    }

    #[test]
    fn the_first_page_and_one_after_an_untranslated_page_get_no_seed() -> Result<()> {
        let chapter = Chapter::new(&[
            &[("ヒナです。", Some("It's Hina."))],
            &[("ここに", None), ("", None)],
            &[("まだ", None)],
        ])?;
        for page in [chapter.pages[0], chapter.pages[2]] {
            let mut conversation = chapter.conversation()?;
            seed(
                &mut conversation,
                &chapter.session,
                page,
                &Snapshot::default(),
            );
            assert!(history(&conversation).is_empty());
            assert_eq!(conversation.last, None);
        }
        Ok(())
    }

    #[test]
    fn the_seed_is_history_the_context_fits_as_any_turn() -> Result<()> {
        let chapter = Chapter::new(&[&[("ヒナです。", Some("It's Hina."))], &[("ここに", None)]])?;
        let mut conversation = chapter.conversation()?;
        seed(
            &mut conversation,
            &chapter.session,
            chapter.pages[1],
            &Snapshot::default(),
        );
        // The page's own turn goes in after it, so the seed is the oldest
        // exchange of the history the context measures before each turn and
        // cuts from when it's full, as it would a page's turn.
        turn(&mut conversation, "ここに", "Here.");
        assert_eq!(
            history(&conversation),
            [
                Message::user("{\n  \"1\": \"ヒナです。\"\n}"),
                Message::assistant("{\n  \"1\": \"It's Hina.\"\n}"),
                Message::user("{\n  \"1\": \"ここに\"\n}"),
                Message::assistant("{\"1\": \"Here.\"}"),
            ]
        );
        Ok(())
    }

    #[test]
    fn a_run_that_asks_for_emphasis_is_seeded_with_the_words_its_run_stressed() -> Result<()> {
        let chapter = Chapter::new(&[
            &[("ヒナです。", None), ("先生!", None), ("どうして", None)],
            &[("ここに", None)],
        ])?;
        // The page before, translated by a run whose model picked words to
        // stress in two of its lines.
        let blocks = &chapter.blocks[0];
        let translated: Vec<_> = [(0, "It's Hina."), (1, "Sensei!"), (2, "Why NO?")]
            .into_iter()
            .map(|(at, text)| (blocks[at], langid!("ja"), text.to_owned()))
            .collect();
        let picks = vec![vec!["Hina".to_owned()], Vec::new(), vec!["NO".to_owned()]];
        let picked = super::picked(&translated, picks);
        let ops = written(
            &chapter.session.lock().project(),
            translated,
            &TranslatorConfig::default(),
            &stressing(true),
            &picked,
        )?;
        chapter.session.lock().apply(Edit {
            label: "test".to_owned(),
            ops,
        })?;
        // The last line was retyped by hand since: its run's marks no longer
        // describe it. The first was pushed down a line, which leaves its
        // run's mark on its word.
        chapter.session.lock().apply(Edit {
            label: "test".to_owned(),
            ops: vec![
                Op::SetText {
                    layer: blocks[2].into(),
                    text: "Why NOT?".to_owned(),
                },
                Op::SetText {
                    layer: blocks[0].into(),
                    text: "\nIt's Hina.".to_owned(),
                },
            ],
        })?;
        let asks = Snapshot {
            asks_emphasis: true,
            ..stressing(true)
        };
        let mut conversation = chapter.conversation()?;
        seed(&mut conversation, &chapter.session, chapter.pages[1], &asks);
        assert_eq!(
            history(&conversation),
            [
                Message::user(
                    "{\n  \"1\": \"ヒナです。\",\n  \"2\": \"先生!\",\n  \"3\": \"どうして\"\n}"
                ),
                Message::assistant(
                    "{\n  \"1\": {\"text\": \"It's Hina.\", \"emphasis\": [\"Hina\"]},\n  \
                     \"2\": {\"text\": \"Sensei!\", \"emphasis\": []},\n  \
                     \"3\": {\"text\": \"Why NOT?\", \"emphasis\": []}\n}"
                ),
            ]
        );
        // The last page holds the plain translations, as a turn keeps them.
        let plain = ["It's Hina.", "Sensei!", "Why NOT?"]
            .map(str::to_owned)
            .to_vec();
        assert_eq!(
            conversation
                .last
                .as_ref()
                .map(|(_, translations)| translations),
            Some(&plain)
        );

        // A run that doesn't ask is seeded as Koharu's is.
        let mut conversation = chapter.conversation()?;
        seed(
            &mut conversation,
            &chapter.session,
            chapter.pages[1],
            &stressing(true),
        );
        assert_eq!(
            history(&conversation)[1],
            Message::assistant(
                "{\n  \"1\": \"It's Hina.\",\n  \"2\": \"Sensei!\",\n  \"3\": \"Why NOT?\"\n}"
            )
        );
        Ok(())
    }

    #[test]
    fn a_presets_stages_are_koharus_by_the_same_names_in_the_same_order() {
        use strum::VariantArray as _;
        for &stage in Stage::VARIANTS {
            let preset = RunStage::from(stage);
            assert_eq!(pipeline_stage(preset), Some(stage));
            assert_eq!(
                serde_json::to_value(preset).ok(),
                serde_json::to_value(stage).ok()
            );
        }
        let presets: Vec<RunStage> = Stage::VARIANTS.iter().map(|&stage| stage.into()).collect();
        assert!(presets.is_sorted());
        // Read chapter is no stage of Koharu's, and comes just before
        // Translation.
        assert_eq!(pipeline_stage(RunStage::Read), None);
        assert!(RunStage::Inpaint < RunStage::Read && RunStage::Read < RunStage::Translate);
    }

    fn colours(chars: &CharStyle) -> (Option<Rgba>, Option<Rgba>, Option<f64>) {
        (chars.fill, chars.stroke, chars.stroke_width)
    }

    #[test]
    fn detection_off_writes_the_default_colours_and_on_writes_none() -> Result<()> {
        let mut page = Page::new(&[("a", "text"), ("b", "text")])?;
        // On: placement's measured colours stay, whatever the default look.
        page.translate(
            &[("a", "Why?")],
            &snapshot(true, Some(RED), Some(BLACK), None),
        )?;
        assert_eq!(
            colours(&page.chars("a")?),
            (Some(BLUE), Some(WHITE), Some(3.0))
        );
        // Off: the default colours replace them, null where it sets none.
        page.translate(&[("b", "Why?")], &snapshot(false, Some(RED), None, None))?;
        let b = page.chars("b")?;
        assert_eq!(colours(&b), (Some(RED), None, None));
        assert_eq!(b.size, Some(32.0), "placement's size stays");
        // Translating again keeps an existing translation's colours.
        page.translate(
            &[("a", "Why!?")],
            &snapshot(false, Some(RED), Some(BLACK), None),
        )?;
        assert_eq!(
            colours(&page.chars("a")?),
            (Some(BLUE), Some(WHITE), Some(3.0))
        );
        Ok(())
    }

    #[test]
    fn a_sound_effect_keeps_a_contrasting_outline_without_a_default_one() -> Result<()> {
        let mut page = Page::new(&[("talk", "text"), ("sfx", "sfx"), ("boom", "sfx")])?;
        // Around the light default fill, a black outline a sixteenth of the
        // page's lettering size; the dialogue gets none.
        let light = snapshot(false, Some(YELLOW), None, None);
        page.translate(&[("talk", "Why?"), ("sfx", "Bam")], &light)?;
        assert_eq!(
            colours(&page.chars("sfx")?),
            (Some(YELLOW), Some(BLACK), Some(2.0))
        );
        assert_eq!(colours(&page.chars("talk")?), (Some(YELLOW), None, None));
        // A default outline is the effect's outline too.
        let outlined = snapshot(false, Some(YELLOW), Some(RED), None);
        page.translate(&[("boom", "Boom")], &outlined)?;
        assert_eq!(
            colours(&page.chars("boom")?),
            (Some(YELLOW), Some(RED), Some(2.5))
        );
        Ok(())
    }

    #[test]
    fn the_default_font_goes_only_into_a_translation_without_a_family() -> Result<()> {
        let mut page = Page::new(&[("a", "text"), ("b", "text"), ("c", "text")])?;
        // b's translation was typed with its own family.
        let b = page.id("b");
        page.edit(vec![
            Op::SetText {
                layer: b.into(),
                text: "Hm".to_owned(),
            },
            Op::UpdateLayer {
                layer: b.into(),
                patch: json!({"translation": {"style": {"chars": {"family": "Comic Neue"}}}}),
            },
        ])?;
        let font = snapshot(true, None, None, Some("CCWildWords"));
        page.translate(&[("a", "Why?"), ("b", "Why?")], &font)?;
        assert_eq!(page.chars("a")?.family.as_deref(), Some("CCWildWords"));
        assert_eq!(page.chars("b")?.family.as_deref(), Some("Comic Neue"));
        // Its family stays the user's choice after the run.
        assert!(edited(&page.layer("b")?).style);
        assert!(!edited(&page.layer("a")?).style);
        // With no Preferred font, none is written: Koharu's chain applies.
        page.translate(&[("c", "Why?")], &snapshot(true, None, None, None))?;
        assert_eq!(page.chars("c")?.family, None);
        Ok(())
    }

    #[test]
    fn every_translated_block_gets_a_record_that_reads_as_automatic() -> Result<()> {
        let mut page = Page::new(&[("a", "text"), ("b", "sfx"), ("c", "text")])?;
        // Styling nothing, as Koharu's own run would: still recorded. c is
        // given back unchanged, which clears its translation, so its record
        // holds no translation.
        let koharu = Snapshot::default();
        page.translate(&[("a", "Why?"), ("b", "Bam"), ("c", SOURCE)], &koharu)?;
        for (label, text) in [("a", "Why?"), ("b", "Bam"), ("c", "")] {
            let layer = page.layer(label)?;
            assert_eq!(Auto::of(&layer).text, Some(text_hash(text)), "{label}");
            assert_eq!(edited(&layer), Edited::default(), "{label}");
        }
        assert_eq!(page.translation("c")?, None);
        // So the same wording typed there afterwards is the user's.
        let c = page.id("c");
        page.edit(vec![Op::SetText {
            layer: c.into(),
            text: SOURCE.to_owned(),
        }])?;
        assert!(edited(&page.layer("c")?).text);

        // Styled: the record holds what the look left.
        let mut page = Page::new(&[("a", "text")])?;
        let styled = snapshot(false, Some(RED), None, Some("CCWildWords"));
        page.translate(&[("a", "Why?")], &styled)?;
        let layer = page.layer("a")?;
        let style = Auto::of(&layer).style.context("a style")?;
        assert_eq!(
            (style.family.as_deref(), style.fill, style.stroke),
            (Some("CCWildWords"), Some(RED), None)
        );
        assert!(!edited(&layer).hand_edited());

        // Retyped by hand, it's the user's.
        let a = page.id("a");
        page.edit(vec![Op::SetText {
            layer: a.into(),
            text: "Why...?".to_owned(),
        }])?;
        let retyped = edited(&page.layer("a")?);
        assert_eq!((retyped.text, retyped.style), (true, false));
        Ok(())
    }

    /// `snapshot`'s with detection on and CCWildWords the default font, and
    /// Pick fonts automatically `auto`: Shout in a bold Badaboom BB,
    /// Floating italic in the default font, SFX in Wild Words, and Whisper
    /// in a bold italic Comic Neue.
    fn lettered(auto: bool) -> Snapshot {
        let font = |family: Option<&str>, bold, italic| CategoryFont {
            family: family.map(str::to_owned),
            bold,
            italic,
        };
        let mut booharu = snapshot(true, None, None, Some("CCWildWords"));
        booharu.lettering.auto = auto;
        booharu.lettering.categories = BTreeMap::from([
            (
                LetteringCategory::Shout,
                font(Some("Badaboom BB"), true, false),
            ),
            (LetteringCategory::Floating, font(None, false, true)),
            (
                LetteringCategory::Sfx,
                font(Some("Wild Words"), false, false),
            ),
            (
                LetteringCategory::Whisper,
                font(Some("Comic Neue"), true, true),
            ),
        ]);
        booharu
    }

    /// A page of four blocks as [`Page::new`] makes them: `shout`, in a
    /// spiky balloon, as detection leaves a block in its balloon; and
    /// outside it `floating`, `boom`, Koharu's sound effect, and `labelled`,
    /// which the user labelled Whisper.
    fn categorized() -> Result<Page> {
        let mut star = String::new();
        for i in 0..24 {
            let angle = f64::from(i) * std::f64::consts::TAU / 24.0;
            let radius = if i % 2 == 0 { 110.0 } else { 66.0 };
            let (x, y) = (200.0 + radius * angle.cos(), 280.0 + radius * angle.sin());
            star.push_str(&format!("{}{x} {y} ", if i == 0 { "M" } else { "L" }));
        }
        let mut labelled = found("text", (110.0, 20.0));
        labelled["attributes"]["booharu"] =
            json!({"lettering": {"category": "whisper", "source": "user"}});
        Page::of(vec![
            json!({"op": "add_layer", "id": "$spiky", "page": "$p", "index": 0,
                "layer": {"kind": "balloon", "outline": star + "Z"}}),
            json!({"op": "add_layer", "id": "$shout", "page": "$p", "parent": "$spiky", "index": 0,
                "layer": found("text", (180.0, 220.0))}),
            json!({"op": "add_layer", "id": "$floating", "page": "$p", "index": 1,
                "layer": found("text", (10.0, 20.0))}),
            json!({"op": "add_layer", "id": "$boom", "page": "$p", "index": 2,
                "layer": found("sfx", (60.0, 20.0))}),
            json!({"op": "add_layer", "id": "$labelled", "page": "$p", "index": 3, "layer": labelled}),
        ])
    }

    /// The translations [`categorized`]'s blocks are given.
    const SAID: [(&str, &str); 4] = [
        ("shout", "No!"),
        ("floating", "Hey"),
        ("boom", "Bam"),
        ("labelled", "Psst"),
    ];

    #[test]
    fn with_the_switch_on_each_new_translation_gets_its_categorys_font_as_automatic() -> Result<()>
    {
        let mut page = categorized()?;
        page.translate(&SAID, &lettered(true))?;
        let font = |family: &str, weight: Option<u16>, italic: Option<bool>| {
            (Some(family.to_owned()), weight, italic)
        };
        // The spiky balloon's block is a shout, the floating block takes
        // Floating's italic in the default font, the sound effect SFX's, and
        // the labelled block its label's, whatever its place.
        for (label, wanted) in [
            ("shout", font("Badaboom BB", Some(700), None)),
            ("floating", font("CCWildWords", None, Some(true))),
            ("boom", font("Wild Words", None, None)),
            ("labelled", font("Comic Neue", Some(700), Some(true))),
        ] {
            let chars = page.chars(label)?;
            assert_eq!(
                (chars.family, chars.weight, chars.italic),
                wanted,
                "{label}"
            );
            // Booharu's values, recorded as automatic ones, so Apply and a
            // change of category may letter the block again.
            let layer = page.layer(label)?;
            let style = Auto::of(&layer).style.context("a style")?;
            assert_eq!(
                (style.family, style.weight, style.italic),
                wanted,
                "{label}"
            );
            assert_eq!(edited(&layer), Edited::default(), "{label}");
        }
        Ok(())
    }

    #[test]
    fn with_the_switch_off_a_new_translation_gets_stage_3s_look_whatever_its_category() -> Result<()>
    {
        let mut off = categorized()?;
        off.translate(&SAID, &lettered(false))?;
        let mut plain = categorized()?;
        plain.translate(&SAID, &snapshot(true, None, None, Some("CCWildWords")))?;
        for (label, _) in SAID {
            let chars = off.chars(label)?;
            assert_eq!(chars, plain.chars(label)?, "{label}");
            assert_eq!(
                (chars.family.as_deref(), chars.weight, chars.italic),
                (Some("CCWildWords"), None, None),
                "{label}"
            );
            assert_eq!(
                Auto::of(&off.layer(label)?),
                Auto::of(&plain.layer(label)?),
                "{label}"
            );
        }
        Ok(())
    }

    #[test]
    fn the_lettering_default_gives_the_chars_koharus_placement_writes() -> Result<()> {
        let block = |role: &str, x: f64, chars: Value| {
            json!({"kind": "block", "role": role, "source": {
                "frame": {"box": {"bounds": {"x": x, "y": 20.0, "width": 40.0, "height": 120.0}}},
                "content": SOURCE,
                "style": {"chars": chars},
            }})
        };
        let add = |label: &str, parent: Option<&str>, index: usize, layer: Value| {
            let mut op = json!({"op": "add_layer", "id": format!("${label}"), "page": "$p",
                "index": index, "layer": layer});
            if let Some(parent) = parent {
                op["parent"] = json!(format!("${parent}"));
            }
            op
        };
        let panel = json!({"kind": "panel", "outline": "M0 200 H400 V400 H0 Z"});
        // Untranslated: a text block outlined in white at size 32 and one
        // filled black at 40; light and dark sound effects without outlines
        // at 100, and one without a colour; and two plain text blocks at 20
        // in panels two deep, which put the page's lettering size at 20,
        // counting the text blocks inside other layers and no sound effect.
        let page = Page::of(vec![
            add(
                "stroked",
                None,
                0,
                block(
                    "text",
                    10.0,
                    json!({"size": 32.0, "fill": BLUE, "stroke": WHITE, "stroke_width": 3.0}),
                ),
            ),
            add(
                "big",
                None,
                1,
                block("text", 60.0, json!({"size": 40.0, "fill": BLACK})),
            ),
            add(
                "light",
                None,
                2,
                block("sfx", 110.0, json!({"size": 100.0, "fill": YELLOW})),
            ),
            add(
                "dark",
                None,
                3,
                block("sfx", 160.0, json!({"size": 100.0, "fill": RED})),
            ),
            add("bare", None, 4, block("sfx", 210.0, json!({}))),
            add("outer", None, 5, panel.clone()),
            add("inner", Some("outer"), 0, panel),
            add(
                "deep",
                Some("inner"),
                0,
                block("text", 260.0, json!({"size": 20.0})),
            ),
            add(
                "deeper",
                Some("inner"),
                1,
                block("text", 310.0, json!({"size": 20.0})),
            ),
        ])?;
        let project = page.session.project();
        let mut placed = BTreeMap::new();
        for label in ["stroked", "big", "light", "dark", "bare", "deep", "deeper"] {
            let id = page.id(label);
            let Op::UpdateLayer { patch, .. } = crate::placement(&project, id, &langid!("en"))?
            else {
                anyhow::bail!("placement updates {label}");
            };
            let chars: CharStyle =
                serde_json::from_value(patch["translation"]["style"]["chars"].clone())?;
            let (on, _) = project.parent(id)?;
            let Content::Block(block) = &project.layer(id)?.content else {
                anyhow::bail!("{label} isn't a block");
            };
            assert_eq!(koharu_chars(on, block, None), chars, "{label}");
            placed.insert(label, chars);
        }
        // The fixtures reach each rule: the size, the outline scaled to it,
        // and a sound effect's outline, black around a light fill and white
        // around any other.
        let outline = |label: &str| (placed[label].stroke, placed[label].stroke_width);
        assert!(placed.values().all(|chars| chars.size == Some(20.0)));
        assert_eq!(outline("stroked"), (Some(WHITE), Some(3.0 * 20.0 / 32.0)));
        assert_eq!(outline("big"), (None, None));
        assert_eq!(outline("light"), (Some(BLACK), Some(20.0 / 16.0)));
        assert_eq!(outline("dark"), (Some(WHITE), Some(20.0 / 16.0)));
        assert_eq!(outline("bare"), (Some(WHITE), Some(20.0 / 16.0)));
        Ok(())
    }

    #[test]
    fn a_corrected_source_reads_as_hand_edited_after_recognition_recorded_it() -> Result<()> {
        let mut page = Page::new(&[("a", "text")])?;
        page.recognize(&[("a", SOURCE)])?;
        assert!(!edited(&page.layer("a")?).source);

        let a = page.id("a");
        let corrected = |text: &str| Op::UpdateLayer {
            layer: a.into(),
            patch: json!({"source": {"content": text}}),
        };
        page.edit(vec![corrected("どうしてよ")])?;
        assert!(edited(&page.layer("a")?).source);

        // Read again, after a translation: the new wording is recorded, and
        // the record keeps what the Translation stage wrote.
        page.translate(&[("a", "Why?")], &Snapshot::default())?;
        page.recognize(&[("a", "どうしてよ")])?;
        let layer = page.layer("a")?;
        assert_eq!(edited(&layer), Edited::default());
        assert_eq!(Auto::of(&layer).text, Some(text_hash("Why?")));
        Ok(())
    }

    #[test]
    fn a_stage_failure_is_classed_by_what_another_try_could_do() {
        use koharu_llm::rig::{
            error::ErrorKind,
            http_client::{HeaderMap, StatusCode},
        };

        let report = |status: u16| {
            PromptError::Report(
                ErrorReport::new(ErrorKind::ProviderResponse, "the server is busy")
                    .with_http_status(status),
            )
        };
        let replied = |status: StatusCode| http_client::Error::InvalidStatusCodeWithDetails {
            status,
            body: "slow down".to_owned(),
            headers: HeaderMap::new(),
        };
        let load = crate::Error::Load {
            model: "lama-manga".to_owned(),
            device: "CUDA".to_owned(),
            free: bytesize::ByteSize(0),
            reserved: bytesize::ByteSize(0),
            active: bytesize::ByteSize(0),
            source: anyhow::anyhow!("out of memory"),
        };
        let full = || koharu_scene::Error::Io(std::io::Error::other("the disk is full"));
        let locked = koharu_scene::Error::Locked {
            op: 2,
            layer: "01a1ee94-9e1f-7088-bee4-f54661aa2bd2"
                .parse()
                .expect("an id"),
        };
        let cases: Vec<(anyhow::Error, Class)> = vec![
            (
                crate::Error::invalid("a balloon's outline isn't a path").into(),
                Class::Invalid,
            ),
            // The page's edit, refused.
            (locked.into(), Class::Invalid),
            (
                koharu_scene::Error::NotFound("layer 01a1".to_owned()).into(),
                Class::Invalid,
            ),
            // The project, unwritten.
            (full().into(), Class::Write),
            (
                anyhow::Error::from(crate::Error::Scene(full()))
                    .context("the page couldn't be written"),
                Class::Write,
            ),
            (load.into(), Class::Load),
            // The app quitting while a model loaded, never tried again.
            (
                anyhow::Error::from(crate::Error::Shutdown).context("the model couldn't load"),
                Class::Write,
            ),
            // A rate limit or a server error koharu-llm's client sent again,
            // however rig carries it.
            (report(503).into(), Class::HostedRetried),
            (report(429).into(), Class::HostedRetried),
            (
                anyhow::Error::from(ProviderError::from(replied(StatusCode::BAD_GATEWAY)))
                    .context("couldn't summarize the conversation"),
                Class::HostedRetried,
            ),
            (
                replied(StatusCode::TOO_MANY_REQUESTS).into(),
                Class::HostedRetried,
            ),
            // What the client doesn't send again is worth a page try.
            (report(408).into(), Class::Retryable),
            (report(400).into(), Class::Retryable),
            (
                PromptError::Report(ErrorReport::new(ErrorKind::Http, "connection reset")).into(),
                Class::Retryable,
            ),
            (
                anyhow::anyhow!("the answer leaves text 2 blank"),
                Class::Retryable,
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(class(&error), expected, "{error:#}");
        }
    }

    #[test]
    fn only_a_hosted_model_has_a_time_limit() {
        use koharu_llm::rig::providers::registry::ProviderId;

        let spec =
            |vendor: &str| ModelSpec::new(ProviderId::declare(vendor).expect("a vendor"), "model");
        assert!(!hosted(Some(&spec(koharu_llm::KOHARU))));
        for vendor in ["openai", "lmstudio", "ollama"] {
            assert!(hosted(Some(&spec(vendor))), "{vendor}");
        }
        assert!(hosted(None));
    }

    #[tokio::test]
    async fn a_hosted_request_left_unanswered_fails_once_its_time_is_up() {
        let limit = Duration::from_millis(20);
        let stalled = std::future::pending::<anyhow::Result<()>>();
        let error = within(limit, true, stalled).await.expect_err("no answer");
        assert_eq!(
            error.to_string(),
            "The translation model didn't answer within 10 minutes."
        );
        assert_eq!(class(&error), Class::Retryable);
        // Koharu's own model takes as long as it takes.
        let slow = async {
            tokio::time::sleep(limit * 3).await;
            anyhow::Ok("answered")
        };
        assert_eq!(within(limit, false, slow).await.ok(), Some("answered"));
    }

    // S10-e: the words a run's model picks to stress, as the stage writes
    // them.

    /// `snapshot`'s with detection on and `[booharu-emphasis]` as it starts,
    /// bold at 115%, its switch `ai`.
    fn stressing(ai: bool) -> Snapshot {
        Snapshot {
            emphasis: Some(EmphasisSettings {
                ai,
                ..EmphasisSettings::default()
            }),
            ..snapshot(true, None, None, None)
        }
    }

    fn span(insert: &str, attributes: CharStyle) -> Span {
        Span {
            insert: insert.to_owned(),
            attributes,
        }
    }

    impl Page {
        /// The marks the default emphasis gives a word of `label`'s
        /// translation, over its own values.
        fn stressed(&self, label: &str) -> Result<CharStyle> {
            let chars = self.chars(label)?;
            let style = EmphasisSettings::default().style_for(None);
            Ok(booharu_core::emphasis::patch(&style, &chars))
        }

        /// `label`'s translation's spans.
        fn spans(&self, label: &str) -> Result<Vec<Span>> {
            Ok(self.translation(label)?.context("a translation")?.content.0)
        }

        fn marks(&self, label: &str) -> Result<Vec<AutoMark>> {
            Ok(Auto::of(&self.layer(label)?).marks)
        }

        /// Gives `label`'s translation `spans`, by hand.
        fn restyle(&mut self, label: &str, spans: Vec<Span>) -> Result<()> {
            let layer = self.id(label).into();
            self.edit(vec![Op::SetRichText { layer, spans }])
        }
    }

    #[test]
    fn a_runs_picks_are_marked_once_however_often_it_translates_again() -> Result<()> {
        let mut page = Page::new(&[("a", "text")])?;
        let on = stressing(true);
        page.stress(&[("a", "No way!", &["No"])], &on)?;
        let bold = page.stressed("a")?;
        assert_eq!((bold.weight, bold.size.is_some()), (Some(700), true));
        let plain = CharStyle::default();
        let first = vec![span("No", bold.clone()), span(" way!", plain.clone())];
        assert_eq!(page.spans("a")?, first);
        let mark = AutoMark {
            start: 0,
            end: 2,
            style: bold.clone(),
        };
        assert_eq!(page.marks("a")?, std::slice::from_ref(&mark));
        // The same answer again: the run's own mark comes off before the
        // word is marked, so it's never stressed twice.
        page.stress(&[("a", "No way!", &["No"])], &on)?;
        assert_eq!(page.spans("a")?, first);
        assert_eq!(page.marks("a")?, [mark]);
        // Another pick moves it.
        page.stress(&[("a", "No way!", &["way"])], &on)?;
        assert_eq!(
            page.spans("a")?,
            [
                span("No ", plain.clone()),
                span("way", bold.clone()),
                span("!", plain)
            ]
        );
        assert_eq!(page.marks("a")?.len(), 1);
        assert_eq!(edited(&page.layer("a")?), Edited::default());
        Ok(())
    }

    #[test]
    fn a_word_marked_by_hand_keeps_its_mark_when_the_run_translates_again() -> Result<()> {
        let mut page = Page::new(&[("a", "text")])?;
        let on = stressing(true);
        let text = "No way, Sensei!";
        page.stress(&[("a", text, &["No"])], &on)?;
        let bold = page.stressed("a")?;
        let plain = CharStyle::default();
        let italic = CharStyle {
            italic: Some(true),
            ..CharStyle::default()
        };
        // The user sets Sensei in italic.
        page.restyle(
            "a",
            vec![
                span("No", bold.clone()),
                span(" way, ", plain.clone()),
                span("Sensei", italic.clone()),
                span("!", plain.clone()),
            ],
        )?;
        assert!(edited(&page.layer("a")?).style);
        // Translated again, the model picking Sensei and way: Sensei keeps
        // the user's mark, way is stressed, and No's run mark comes off.
        page.stress(&[("a", text, &["Sensei", "way"])], &on)?;
        assert_eq!(
            page.spans("a")?,
            [
                span("No ", plain.clone()),
                span("way", bold.clone()),
                span(", ", plain.clone()),
                span("Sensei", italic),
                span("!", plain),
            ]
        );
        assert_eq!(
            page.marks("a")?,
            [AutoMark {
                start: 3,
                end: 6,
                style: bold,
            }]
        );
        assert!(edited(&page.layer("a")?).style, "still the user's");
        Ok(())
    }

    #[test]
    fn picks_the_text_doesnt_hold_are_left_out_and_logged_and_the_page_is_written() -> Result<()> {
        let mut page = Page::new(&[("a", "text"), ("b", "text")])?;
        let on = stressing(true);
        let (written, log) = emphasis::logged(|| {
            page.stress(
                &[
                    ("a", "No way!", &["Hello", "way"]),
                    ("b", "Why?", &["Hello"]),
                ],
                &on,
            )
        });
        written?;
        assert!(
            log.contains("emphasis picks matching no word were left out"),
            "{log}"
        );
        let plain = CharStyle::default();
        assert_eq!(
            page.spans("a")?,
            [
                span("No ", plain.clone()),
                span("way", page.stressed("a")?),
                span("!", plain)
            ]
        );
        assert_eq!(page.spans("b")?, [span("Why?", CharStyle::default())]);
        assert!(page.marks("b")?.is_empty());
        Ok(())
    }

    #[test]
    fn with_the_switch_off_or_a_translation_model_a_run_takes_its_old_marks_off_and_adds_none()
    -> Result<()> {
        let mut page = Page::new(&[("a", "text")])?;
        let plain = [span("No way!", CharStyle::default())];
        // Off: the model's picks, had it given any, mark nothing.
        page.stress(&[("a", "No way!", &["No"])], &stressing(true))?;
        page.stress(&[("a", "No way!", &["way"])], &stressing(false))?;
        assert_eq!(page.spans("a")?, plain);
        assert!(page.marks("a")?.is_empty());
        assert_eq!(edited(&page.layer("a")?), Edited::default());

        // A run on Index-Translate isn't asked, so it has no picks.
        page.stress(&[("a", "No way!", &["No"])], &stressing(true))?;
        let (translation_only, translator) = emphasis::asked(
            Snapshot::default(),
            TranslatorConfig::default(),
            &[Stage::Translate].into(),
            || Ok(EmphasisSettings::default()),
        )?;
        assert_eq!(
            (translation_only.asks_emphasis, translator),
            (false, TranslatorConfig::default())
        );
        page.translate(&[("a", "No way!")], &translation_only)?;
        assert_eq!(page.spans("a")?, plain);
        assert!(page.marks("a")?.is_empty());
        assert_eq!(edited(&page.layer("a")?), Edited::default());
        Ok(())
    }

    #[test]
    fn a_runs_marks_are_no_hand_edit_for_apply_a_category_or_detect_again_but_a_change_is()
    -> Result<()> {
        let mut page = Page::new(&[("a", "text")])?;
        page.stress(&[("a", "No way!", &["No"])], &stressing(true))?;
        assert_eq!(page.marks("a")?.len(), 1);
        let a = page.layer("a")?;
        // No hand edit: Apply restyles the block (crates/booharu's styles
        // skips one `hand_edited`), a model's category letters it (its
        // lettering keeps the font of one whose `style` was edited), and
        // Detect again doesn't keep it for its marks.
        assert_eq!(edited(&a), Edited::default());
        let project = page.session.project();
        let on = project.parent(a.id)?.0;
        assert!(!stays(on, &a));

        // The stressed word made heavier by hand is a style edit, which
        // each of them keeps.
        let heavier = CharStyle {
            weight: Some(900),
            ..page.stressed("a")?
        };
        page.restyle(
            "a",
            vec![span("No", heavier), span(" way!", CharStyle::default())],
        )?;
        let a = page.layer("a")?;
        assert!(edited(&a).style);
        let project = page.session.project();
        assert!(stays(project.parent(a.id)?.0, &a));
        Ok(())
    }
}
