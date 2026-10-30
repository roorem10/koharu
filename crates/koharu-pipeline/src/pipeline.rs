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
