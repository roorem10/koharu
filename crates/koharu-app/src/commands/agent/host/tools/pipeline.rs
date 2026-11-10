//! Running the processing pipeline.

use super::*;

pub(super) fn tools() -> Vec<Tool> {
    vec![
            definition::<RunPipeline>(
                "run_pipeline",
                "Run Booharu's configured processing pipeline for the whole project, selected pages, or selected text elements.",
            ),
    ]
}

pub(super) async fn invoke(
    host: &KoharuHost,
    call: &ToolCall,
    control: &Control,
) -> Result<Option<Invocation>> {
    let result: Result<Invocation> = match call.name.as_str() {
        "run_pipeline" => host.run_pipeline(arguments(call)?, control).await,
        _ => return Ok(None),
    };
    result.map(Some)
}

impl KoharuHost {
    async fn run_pipeline(&self, arguments: RunPipeline, control: &Control) -> Result<Invocation> {
        let scope = arguments.scope()?;
        let operation = arguments.operation.into();
        let translation_hints = {
            let projects = self.handle.state::<CurrentProject>();
            let project = projects.project.lock().await;
            let project = project.as_ref().context("no project is open")?;
            let snapshot = project.snapshot();
            let hints =
                crate::commands::context::translation_hints(&project.load_context()?, &snapshot)?;
            std::sync::Arc::new(hints)
        };
        let job = JobId::new();
        let stop = StopToken::default();
        {
            let processing = self.handle.state::<Processing>();
            let mut stops = processing.stops.lock();
            anyhow::ensure!(
                !self
                    .handle
                    .state::<crate::commands::runs::BatchState>()
                    .active(),
                "a chapter batch is running"
            );
            if !stops.is_empty() {
                bail!("another pipeline process is already running");
            }
            stops.insert(job, stop.clone());
        }

        let watcher = tauri::async_runtime::spawn({
            let control = control.clone();
            let stop = stop.clone();
            async move {
                control.cancelled().await;
                stop.stop();
            }
        });
        let mut committer = AgentCommitter { host: self.clone() };
        let request = koharu_pipeline::Request {
            operation,
            scope,
            stop,
            progress: None,
            inpainting_mask: None,
            translation_hints,
            redetect: false,
            policy: crate::commands::runs::RunSettings::load()?.policy(),
            ..Default::default()
        };
        let mut request = request;
        let result = async {
            if crate::commands::context::operation_translates(&request.operation) {
                crate::commands::context::auto_read(&self.handle, &request.scope, &request.stop, job)
                    .await?;
            }
            let snapshot = {
                let projects = self.handle.state::<CurrentProject>();
                let projects = projects.project.lock().await;
                let project = projects.as_ref().context("no project is open")?;
                let snapshot = project.snapshot();
                request.translation_hints = std::sync::Arc::new(
                    crate::commands::context::translation_hints(&project.load_context()?, &snapshot)?,
                );
                snapshot
            };
            self.handle
                .state::<koharu_pipeline::Pipeline>()
                .execute(snapshot, request, &mut committer)
                .await
                .map_err(anyhow::Error::from)
        }
        .await;
        watcher.abort();
        self.handle.state::<Processing>().stops.lock().remove(&job);
        let report = result.map_err(|error| anyhow!(error))?;
        if report.status == RunStatus::Stopped {
            bail!("pipeline processing was cancelled");
        }
        Invocation::changed(json!({
            "base_revision": report.base,
            "final_revision": report.final_revision,
            "completed": report.completed,
            "total": report.total,
            "elapsed_ms": report.elapsed.as_millis(),
            "failures": report.failures,
        }))
    }
}

struct AgentCommitter {
    host: KoharuHost,
}

#[async_trait]
impl Committer for AgentCommitter {
    async fn commit(&mut self, output: StageOutput) -> Result<Snapshot> {
        let (commit, page) = {
            let current = self.host.handle.state::<CurrentProject>();
            let mut current = current.project.lock().await;
            let project = current.as_mut().context("no project is open")?;
            let Some(commit) = project.commit_rebased(output.patch).await? else {
                return Ok(project.snapshot());
            };
            project.record_commit(&commit);
            if output.stage == Stage::Translation {
                crate::commands::context::refresh_flags(project)?;
            }
            (commit, project.active_page())
        };
        let snapshot = commit.snapshot.clone();
        self.host.synchronize(commit, page).await?;
        Ok(snapshot)
    }
}

#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum AgentPipelineOperation {
    Full,
    Detection,
    Ocr,
    Translation,
    Inpainting,
}

impl From<AgentPipelineOperation> for Operation {
    fn from(value: AgentPipelineOperation) -> Self {
        match value {
            AgentPipelineOperation::Full => Self::Full,
            AgentPipelineOperation::Detection => Self::Only {
                stage: Stage::Detection,
            },
            AgentPipelineOperation::Ocr => Self::Only { stage: Stage::Ocr },
            AgentPipelineOperation::Translation => Self::Only {
                stage: Stage::Translation,
            },
            AgentPipelineOperation::Inpainting => Self::Only {
                stage: Stage::Inpainting,
            },
        }
    }
}

#[derive(Deserialize, JsonSchema)]
struct RunPipeline {
    operation: AgentPipelineOperation,
    #[serde(default)]
    pages: Vec<String>,
    #[serde(default)]
    elements: Vec<String>,
}

impl RunPipeline {
    fn scope(&self) -> Result<Scope> {
        match (self.pages.is_empty(), self.elements.is_empty()) {
            (true, true) => Ok(Scope::Project),
            (false, true) => Ok(Scope::Pages(entities(&self.pages)?)),
            (true, false) => Ok(Scope::Entities(entities(&self.elements)?)),
            (false, false) => bail!("pipeline scope cannot contain both pages and elements"),
        }
    }
}
