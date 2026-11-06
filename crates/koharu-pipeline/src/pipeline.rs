use std::sync::Arc;

use anyhow::{Context as _, Result};
use arc_swap::ArcSwap;
use koharu_config::Config;
use koharu_scene::Snapshot;

use crate::{
    Committer, PipelineConfig, PipelineError, Report, Request, ResourceSnapshot,
    execution::Execution, resources::ResourceMonitor, stage_runner::StageRunner,
};

#[derive(Clone)]
pub struct Pipeline {
    current: Arc<ArcSwap<StageRunner>>,
    resources: Arc<ResourceMonitor>,
    execution: Arc<tokio::sync::Mutex<()>>,
}

impl Pipeline {
    /// Uses the same stage-owned decisions as execution, without loading a model.
    /// Call again after prerequisites commit: detection and OCR can create work.
    pub fn pending(
        &self,
        snapshot: &Snapshot,
        request: &Request,
    ) -> Result<Vec<(crate::Stage, koharu_scene::EntityId)>> {
        let stages = request.operation.stages()?;
        let scope = crate::scope::NormalizedScope::new(snapshot, &request.scope, &stages)?;
        let resume = request.resume
            || matches!(request.operation, crate::Operation::Full | crate::Operation::Through { .. });
        let runner = self.current.load_full();
        let mut pending = Vec::new();
        for &page in scope.pages() {
            for &stage in &stages {
                let input = crate::stages::StageInput::new(
                    snapshot.clone(),
                    page,
                    scope.entities(),
                    scope.region(page),
                    Arc::new(crate::images::ImageCache::default()),
                    request.inpainting_mask.as_ref()
                        .filter(|mask| stage == crate::Stage::Inpainting && mask.page == page)
                        .cloned(),
                )
                .with_translation_hints(request.translation_hints.clone())
                .with_redetect(request.redetect)
                .with_refresh(request.refresh.contains(&(stage, page)))
                .with_resume(resume && !request.refresh.contains(&(stage, page)));
                if !runner.skip(stage, &input)? {
                    pending.push((stage, page));
                }
            }
        }
        Ok(pending)
    }

    pub fn load(device: koharu_ml::Device) -> Result<Self> {
        Self::from_config(
            PipelineConfig::load()?,
            koharu_translator::ProvidersConfig::load()?,
            device,
        )
    }

    #[tracing::instrument(skip_all)]
    pub fn from_config(
        config: Config<PipelineConfig>,
        providers: Config<koharu_translator::ProvidersConfig>,
        device: koharu_ml::Device,
    ) -> Result<Self> {
        let translator = koharu_translator::Translator::from_config(device.clone(), providers)?;
        let resources = ResourceMonitor::new(&device);
        let runner = {
            let value = config.read()?;
            StageRunner::new(&value, translator.clone(), &device, resources.clone())?
        };
        let current = Arc::new(ArcSwap::from_pointee(runner));
        let watched = current.clone();
        let watched_resources = resources.clone();
        let _watcher = tokio::runtime::Handle::try_current()
            .context("pipeline requires a Tokio runtime")?
            .spawn(async move {
                let mut changes = config.subscribe();
                while changes.changed().await.is_ok() {
                    let runner = config.read().and_then(|value| {
                        StageRunner::new(
                            &value,
                            translator.clone(),
                            &device,
                            watched_resources.clone(),
                        )
                    });
                    match runner {
                        Ok(runner) => watched.store(Arc::new(runner)),
                        Err(error) => tracing::error!(%error, "failed to reload pipeline"),
                    }
                }
            });
        Ok(Self {
            current,
            resources,
            execution: Arc::new(tokio::sync::Mutex::new(())),
        })
    }

    pub fn subscribe_resources(&self) -> tokio::sync::watch::Receiver<ResourceSnapshot> {
        self.resources.start();
        self.resources.subscribe()
    }

    #[tracing::instrument(skip_all)]
    pub async fn unload_models(&self) -> Result<u32> {
        let _execution = self
            .execution
            .try_lock()
            .context("models cannot be unloaded while processing is running")?;
        let count = self.current.load().unload_models();
        self.resources.refresh().await;
        Ok(count)
    }

    pub async fn run_task(
        &self,
        selection: &koharu_translator::ModelSelection,
        generation: koharu_translator::GenerationConfig,
        request: koharu_translator::TaskRequest,
    ) -> Result<serde_json::Value> {
        let _execution = self.execution.lock().await;
        self.current
            .load_full()
            .run_task(selection, generation, request)
            .await
    }

    #[tracing::instrument(skip_all)]
    pub async fn execute(
        &self,
        snapshot: Snapshot,
        request: Request,
        committer: &mut dyn Committer,
    ) -> std::result::Result<Report, PipelineError> {
        let _execution = self.execution.lock().await;
        Execution::new(
            self.current.load_full(),
            self.resources.clone(),
            snapshot,
            request,
            committer,
        )?
        .run()
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn split_recipe_passes_resume_and_refresh_only_generated_outputs() {
        use koharu_scene::{
            At, Authored, Generation, Origin, PageDraft, ProducerId, SourceText, TextLayout,
            TextLayoutKind, Translation,
        };
        let pipeline = Pipeline::from_config(
            Config::memory(PipelineConfig::default()),
            Config::memory(koharu_translator::ProvidersConfig::default()),
            koharu_ml::Device::cpu(),
        )
        .unwrap();
        let mut session = koharu_scene::Session::memory().await.unwrap();
        let generated = Generation::new(ProducerId::new("dev.koharu.test").unwrap());
        let mut edit = session.snapshot().edit_as(generated.clone());
        let page = edit
            .add_page(PageDraft::new("one", 100.0, 100.0), At::End)
            .unwrap();
        let content = edit.add_text_content(page, At::End).unwrap();
        edit.set(
            content,
            &SourceText {
                text: Authored::generated("source".into(), generated.clone()),
                language: None,
            },
        )
        .unwrap();
        edit.set(
            content,
            &Translation {
                text: Authored::generated("done".into(), generated.clone()),
                language: None,
            },
        )
        .unwrap();
        edit.add_text_layer(
            page,
            At::End,
            content,
            &TextLayout {
                origin: Origin::Generated(generated),
                kind: TextLayoutKind::Paragraph,
                angle_degrees: None,
                warp: None,
            },
        )
        .unwrap();
        session.commit(edit.finish().unwrap()).await.unwrap();
        let mut request = Request {
            operation: crate::Operation::Stages {
                stages: vec![crate::Stage::Ocr, crate::Stage::Translation, crate::Stage::Inpainting],
            },
            resume: true,
            ..Default::default()
        };
        assert!(
            pipeline.pending(&session.snapshot(), &request).unwrap().is_empty()
        );
        request.refresh.insert((crate::Stage::Translation, page));
        assert_eq!(
            pipeline.pending(&session.snapshot(), &request).unwrap(),
            vec![(crate::Stage::Translation, page)]
        );
        let patch = session
            .snapshot()
            .patch(|edit| {
                edit.set(content, &Translation {
                    text: Authored::user("".into()),
                    language: None,
                })
            })
            .unwrap();
        session.commit(patch).await.unwrap();
        assert!(
            pipeline.pending(&session.snapshot(), &request).unwrap().is_empty()
        );
        let patch = session
            .snapshot()
            .patch(|edit| {
                edit.remove::<SourceText>(content)?;
                edit.remove::<Translation>(content)
            })
            .unwrap();
        session.commit(patch).await.unwrap();
        request.operation = crate::Operation::Only { stage: crate::Stage::Translation };
        assert!(
            pipeline.pending(&session.snapshot(), &request).unwrap().is_empty()
        );
    }

    #[tokio::test]
    async fn unloading_refuses_an_active_execution_and_publishes_a_fresh_sample() {
        let pipeline = Pipeline::from_config(
            Config::memory(PipelineConfig::default()),
            Config::memory(koharu_translator::ProvidersConfig::default()),
            koharu_ml::Device::cpu(),
        )
        .unwrap();
        let execution = pipeline.execution.lock().await;
        assert!(
            pipeline
                .unload_models()
                .await
                .unwrap_err()
                .to_string()
                .contains("processing is running")
        );
        drop(execution);
        let mut resources = pipeline.subscribe_resources();
        resources.borrow_and_update();
        assert_eq!(pipeline.unload_models().await.unwrap(), 0);
        assert!(resources.has_changed().unwrap());
        assert_eq!(pipeline.unload_models().await.unwrap(), 0);
    }
}
