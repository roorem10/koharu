//! Booharu's pipeline hooks: what a stage calls, by one marked line, to apply
//! Booharu's settings and attributes from booharu-core within the stage's
//! own page edit. A run reads Booharu's settings once, as it's queued, into
//! its config's snapshot, and never again mid-stage, so every page of a run
//! uses the same values, whether the Run button, the agent, or MCP queued it.
//!
//! - `queued`, in `Pipeline::run`, takes the snapshot, the run policy of
//!   `[booharu-runs]` included, or refuses the run when a section it reads
//!   can't be read.
//! - `translated`, in the Translation stage's `written`, gives each new
//!   translation the default look and records what the stage wrote, and
//!   `given_back` records a translation it cleared, given back unchanged.
//! - `recognized`, in the Recognition stage, records the wording it read.
//! - `Failure`, from `stages::run`, is a stage task's failure as the run
//!   reports it, with the class the run policy goes by, which the scheduler's
//!   `booharu` module follows: another try, the page given up on, or the run
//!   ended.
//! - `answered`, in a translation's turn, gives a hosted model's request ten
//!   minutes to answer.

use std::{future::Future, time::Duration};

use booharu_core::{
    runs::{RunPolicy, RunStage},
    settings::Snapshot,
    styles,
};
use koharu_llm::rig::{
    catalog::ModelSpec,
    driver::DynModel,
    error::{ErrorReport, ProviderError},
    http_client,
    operation::Completion,
};
use koharu_renderer::RendererConfig;
use koharu_scene::{LayerId, Op, PageId, Project};
use rig_agent::run::PromptError;
use serde::Serialize;

use crate::{PipelineConfig, Stage};

/// `config`, the one a run was queued with, with Booharu's settings as
/// they're saved now: `[booharu-lettering]`, the first of Koharu's Preferred
/// fonts, and `[booharu-runs]`'s run policy, which the run then follows
/// whoever queued it. A section that can't be read refuses the run, as a bad
/// stored value is an error, never replaced by its default.
pub(crate) fn queued(config: PipelineConfig) -> crate::Result<PipelineConfig> {
    let families = koharu_config::get::<RendererConfig>().map(|renderer| renderer.font_families);
    let booharu = snapshot(Snapshot::read(families), RunPolicy::read())?;
    Ok(PipelineConfig { booharu, ..config })
}

/// A run's snapshot: `read`, with `policy` in it, or the error of the first
/// that couldn't be read, which refuses the run as an invalid request.
fn snapshot(
    read: anyhow::Result<Snapshot>,
    policy: anyhow::Result<RunPolicy>,
) -> crate::Result<Snapshot> {
    let invalid = |error: anyhow::Error| crate::Error::invalid(format!("{error:#}"));
    let read = read.map_err(invalid)?;
    Ok(Snapshot {
        runs: Some(policy.map_err(invalid)?),
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

// A preset's stage is a stage of Koharu's, by the same name.
impl From<RunStage> for Stage {
    fn from(stage: RunStage) -> Self {
        match stage {
            RunStage::Detect => Self::Detect,
            RunStage::Recognize => Self::Recognize,
            RunStage::Inpaint => Self::Inpaint,
            RunStage::Translate => Self::Translate,
        }
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
/// anything, then the hand-edit record, on every translation the stage
/// writes. None when placement failed, which fails the stage.
pub(crate) fn translated(
    project: &Project,
    block: LayerId,
    text: &str,
    placed: Option<&Op>,
    booharu: &Snapshot,
) -> anyhow::Result<Vec<Op>> {
    let Some(placed) = placed else {
        return Ok(Vec::new());
    };
    styles::translated(
        project.layer(block)?,
        text,
        placed,
        &booharu.lettering,
        booharu.font.as_deref(),
    )
}

/// The op that follows the Translation stage's clearing of `block`'s
/// translation, given back unchanged, in its edit: the hand-edit record of
/// no translation, read before the edit.
pub(crate) fn given_back(project: &Project, block: LayerId) -> anyhow::Result<Op> {
    styles::given_back(project.layer(block)?)
}

/// The op recording `text`, the wording the Recognition stage read for
/// `block`, in its hand-edit record, which keeps its other parts.
pub(crate) fn recognized(project: &Project, block: LayerId, text: &str) -> anyhow::Result<Op> {
    styles::recognized(project.layer(block)?, text)
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
    /// The project couldn't be read, written, or saved: the run ends, whatever
    /// its policy.
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
/// that says: Koharu's own invalid request, scene, and load errors, a scene
/// error from the session, then the HTTP status of a hosted model's reply,
/// however rig carries it; anything else is worth another try.
pub(crate) fn class(error: &anyhow::Error) -> Class {
    for cause in error.chain() {
        match cause.downcast_ref::<crate::Error>() {
            Some(crate::Error::Invalid(_)) => return Class::Invalid,
            Some(crate::Error::Scene(error)) => return scene(error),
            Some(crate::Error::Load { .. }) => return Class::Load,
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

/// Stand-ins for the stages in the scheduler's tests, which load no model:
/// what each page stage of a scripted project does, try by try, and a log of
/// the tasks that ran and the models the scheduler unloaded, in order.
#[cfg(test)]
pub(crate) mod stand_in {
    use std::{
        collections::{BTreeMap, HashMap, HashSet, VecDeque},
        time::Duration,
    };

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
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc, time::Duration};

    use anyhow::{Context as _, Result};
    use booharu_core::{
        Edited,
        attrs::Auto,
        edited,
        runs::RunsSettings,
        settings::{DefaultTextStyle, LetteringSettings},
        styles::text_hash,
    };
    use icu_locale_core::langid;
    use koharu_scene::{CharStyle, Content, Created, Edit, Layer, Rgba, Session, Text};
    use parking_lot::Mutex;
    use serde_json::json;

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
        blocks: BTreeMap<String, LayerId>,
    }

    impl Page {
        fn new(blocks: &[(&str, &str)]) -> Result<Self> {
            let dir = tempfile::tempdir()?;
            let mut session = Session::create(dir.path().join("test.khr"))?;
            let mut ops = vec![
                json!({"op": "add_page", "id": "$p", "index": 0, "size": {"width": 400, "height": 400}}),
            ];
            for (index, (label, role)) in blocks.iter().enumerate() {
                let x = 10.0 + 50.0 * index as f64;
                let block = json!({"kind": "block", "role": role, "source": {
                    "frame": {"box": {"bounds": {"x": x, "y": 20.0, "width": 40.0, "height": 120.0}}},
                    "content": SOURCE,
                    "style": {"chars": {"size": 32.0, "fill": BLUE, "stroke": WHITE, "stroke_width": 3.0}},
                }, "attributes": {"c2pa": {
                    "detection": {"action": "c2pa.created"},
                    "ocr": {"action": "c2pa.created"},
                }}});
                ops.push(json!({"op": "add_layer", "id": format!("${label}"), "page": "$p", "index": index, "layer": block}));
            }
            let change = session.apply(Edit {
                label: "test".to_owned(),
                ops: ops
                    .into_iter()
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
            let translated = texts
                .iter()
                .map(|(label, text)| (self.id(label), langid!("ja"), (*text).to_owned()))
                .collect();
            let ops = written(
                &self.session.project(),
                translated,
                &TranslatorConfig::default(),
                booharu,
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
            },
            font: font.map(str::to_owned),
            runs: None,
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
        let queued = super::snapshot(Ok(read.clone()), Ok(policy))?;
        assert_eq!(
            queued,
            Snapshot {
                runs: Some(policy),
                ..read.clone()
            }
        );
        // A bad [booharu-runs] refuses the run, naming the section.
        let bad = anyhow::anyhow!("9 retries aren't between 0 and 5")
            .context("Booharu's run settings can't be read from [booharu-runs]");
        let refused = super::snapshot(Ok(read), Err(bad)).expect_err("a bad policy");
        assert!(
            matches!(&refused, crate::Error::Invalid(message)
                if message.contains("[booharu-runs]") && message.contains("9 retries")),
            "{refused}"
        );
        // A config a caller made, as upstream's tests and examples make one,
        // runs Koharu's way.
        assert_eq!(PipelineConfig::default().booharu.runs, None);
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

    #[test]
    fn a_presets_stages_are_koharus_by_the_same_names_in_the_same_order() {
        use strum::VariantArray as _;
        for &stage in Stage::VARIANTS {
            let preset = RunStage::from(stage);
            assert_eq!(Stage::from(preset), stage);
            assert_eq!(
                serde_json::to_value(preset).ok(),
                serde_json::to_value(stage).ok()
            );
        }
        let presets: Vec<RunStage> = Stage::VARIANTS.iter().map(|&stage| stage.into()).collect();
        assert!(presets.is_sorted());
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
}
