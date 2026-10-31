use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Result;
use koharu_scene::{EntityId, Patch};

use crate::{
    ErrorKind, PipelineConfig, PipelineError, Progress, ProgressSink, Stage, StopToken,
    accelerator::AcceleratorGate,
    progress,
    resources::ResourceMonitor,
    stages::{StageInput, Stages},
};

pub(crate) struct StageRunner {
    stages: Stages,
    translator: koharu_translator::Translator,
    accelerator: AcceleratorGate,
}

impl StageRunner {
    pub(crate) fn new(
        config: &PipelineConfig,
        translator: koharu_translator::Translator,
        device: &koharu_ml::Device,
        resources: Arc<ResourceMonitor>,
    ) -> Result<Self> {
        Ok(Self {
            stages: Stages::new(config, translator.clone(), device)?,
            translator,
            accelerator: AcceleratorGate::new(device, resources),
        })
    }

    pub(crate) async fn run_task(
        &self,
        selection: &koharu_translator::ModelSelection,
        generation: koharu_translator::GenerationConfig,
        request: koharu_translator::TaskRequest,
    ) -> Result<serde_json::Value> {
        let permit = self.accelerator.acquire().await;
        match self
            .translator
            .run_task(selection, generation, request.clone())
            .await
        {
            Ok(value) => Ok(value),
            Err(error) if is_out_of_memory(&error) => {
                drop(permit);
                let _permit = self
                    .accelerator
                    .recover(Stage::Translation, &self.stages)
                    .await;
                let result = self
                    .translator
                    .run_task(selection, generation, request)
                    .await;
                if result.is_err() {
                    self.translator.unload();
                }
                result
            }
            Err(error) => {
                self.translator.unload();
                Err(error)
            }
        }
    }

    pub(crate) fn unload_stage(&self, stage: Stage) {
        self.stages.unload(stage);
    }

    pub(crate) fn unload_models(&self) -> u32 {
        let mut count = 0;
        for stage in Stage::ALL {
            if self.stages.unload(stage) {
                count += 1;
                tracing::info!(%stage, model = self.stages.model(stage), "unloaded model");
            }
        }
        count
    }

    #[tracing::instrument(skip_all)]
    pub(crate) async fn run(&self, job: StageJob) -> StageCompletion {
        let started = Instant::now();
        let page = job.input.page();
        let model = self.stages.model(job.stage).to_owned();
        let outcome = retry_stage(
            job.retries,
            &job.stop,
            || self.run_with_recovery(&job, &model),
            |retry, attempts| {
                progress::emit(
                    job.progress.as_ref(),
                    Progress::Retrying {
                        page,
                        stage: job.stage,
                        attempt: retry + 1,
                        attempts,
                    },
                );
                retry_wait(retry, &job.stop)
            },
        )
        .await;
        StageCompletion {
            page,
            stage: job.stage,
            model,
            elapsed: started.elapsed(),
            outcome,
        }
    }

    async fn run_with_recovery(
        &self,
        job: &StageJob,
        model: &str,
    ) -> std::result::Result<StageOutcome, PipelineError> {
        if job.stop.stopped() {
            return Ok(StageOutcome::Stopped);
        }
        let skip = self.stages.skip(job.stage, &job.input).map_err(|error| {
            self.stage_error(
                job.stage,
                model,
                AttemptFailure {
                    kind: ErrorKind::Processing,
                    error,
                },
            )
        })?;
        if skip {
            return Ok(StageOutcome::Skipped);
        }
        let permit = self.accelerator.acquire().await;
        if job.stop.stopped() {
            return Ok(StageOutcome::Stopped);
        }
        let first = self.load_and_process(job, model).await;
        let failure = match first {
            Ok(outcome) => return Ok(outcome),
            Err(failure) if is_out_of_memory(&failure.error) && !job.stop.stopped() => failure,
            Err(failure) => return Err(self.stage_error(job.stage, model, failure)),
        };

        drop(permit);
        tracing::warn!(stage = %job.stage, page = %job.input.page(), error = %failure.error, "retrying stage after memory pressure");
        let _permit = self.accelerator.recover(job.stage, &self.stages).await;
        if job.stop.stopped() {
            return Ok(StageOutcome::Stopped);
        }
        match self.load_and_process(job, model).await {
            Ok(outcome) => Ok(outcome),
            Err(failure) => Err(self.stage_error(job.stage, model, failure)),
        }
    }

    async fn load_and_process(
        &self,
        job: &StageJob,
        model: &str,
    ) -> std::result::Result<StageOutcome, AttemptFailure> {
        progress::emit(
            job.progress.as_ref(),
            Progress::Loading {
                page: job.input.page(),
                stage: job.stage,
                model: model.to_owned(),
            },
        );
        self.stages
            .load(job.stage)
            .await
            .map_err(|error| AttemptFailure {
                kind: ErrorKind::ModelLoad,
                error,
            })?;
        if job.stop.stopped() {
            return Ok(StageOutcome::Stopped);
        }
        progress::emit(
            job.progress.as_ref(),
            Progress::Running {
                page: job.input.page(),
                stage: job.stage,
                model: model.to_owned(),
            },
        );
        self.stages
            .process(job.stage, job.input.clone())
            .await
            .map(|patch| {
                if patch.is_empty() {
                    StageOutcome::Skipped
                } else {
                    StageOutcome::Patch(patch)
                }
            })
            .map_err(|error| AttemptFailure {
                kind: ErrorKind::Processing,
                error,
            })
    }

    fn stage_error(&self, stage: Stage, model: &str, failure: AttemptFailure) -> PipelineError {
        self.stages.unload(stage);
        let message = match failure.kind {
            ErrorKind::ModelLoad => format!("failed to load {model}"),
            _ => format!("{model} failed"),
        };
        PipelineError::new(failure.kind, Some(stage), failure.error.context(message))
    }
}

struct AttemptFailure {
    kind: ErrorKind,
    error: anyhow::Error,
}

fn is_out_of_memory(error: &anyhow::Error) -> bool {
    error.chain().any(|source| {
        let message = source.to_string().to_ascii_lowercase();
        message.contains("out of memory")
            || message.contains("cuda_error_out_of_memory")
            || message.contains("not enough memory")
    })
}

pub(crate) struct StageJob {
    stage: Stage,
    input: StageInput,
    stop: StopToken,
    progress: Option<ProgressSink>,
    retries: u8,
}

impl StageJob {
    pub(crate) fn new(
        stage: Stage,
        input: StageInput,
        stop: StopToken,
        progress: Option<ProgressSink>,
        retries: u8,
    ) -> Self {
        Self {
            stage,
            input,
            stop,
            progress,
            retries,
        }
    }
}

pub(crate) enum StageOutcome {
    Patch(Patch),
    Skipped,
    Stopped,
}

pub(crate) struct StageCompletion {
    pub(crate) page: EntityId,
    pub(crate) stage: Stage,
    pub(crate) model: String,
    pub(crate) elapsed: Duration,
    pub(crate) outcome: std::result::Result<StageOutcome, PipelineError>,
}

async fn retry_stage<F, Fut, W, Wait>(
    retries: u8,
    stop: &StopToken,
    mut attempt: F,
    mut wait: W,
) -> std::result::Result<StageOutcome, PipelineError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = std::result::Result<StageOutcome, PipelineError>>,
    W: FnMut(u8, u8) -> Wait,
    Wait: std::future::Future<Output = ()>,
{
    let retries = retries.min(5);
    for retry in 0..=retries {
        if stop.stopped() {
            return Ok(StageOutcome::Stopped);
        }
        match attempt().await {
            Err(error)
                if !matches!(error.kind, ErrorKind::InvalidInput | ErrorKind::Commit)
                    && retry < retries
                    && !stop.stopped() =>
            {
                wait(retry + 1, retries + 1).await;
            }
            result => return result,
        }
    }
    unreachable!("the last attempt always returns")
}

async fn retry_wait(retry: u8, stop: &StopToken) {
    let delay = match retry {
        1 => 20,
        2 => 50,
        _ => 100,
    };
    // Release the accelerator between attempts; cancellation stays responsive during backoff.
    for _ in 0..delay {
        if stop.stopped() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU8, Ordering};

    async fn fake_stage(failures: u8, kind: ErrorKind, retries: u8) -> (u8, Vec<(u8, u8)>, bool) {
        let calls = AtomicU8::new(0);
        let mut waits = Vec::new();
        let result = retry_stage(
            retries,
            &StopToken::default(),
            || {
                let call = calls.fetch_add(1, Ordering::Relaxed);
                std::future::ready(if call < failures {
                    Err(PipelineError::new(
                        kind,
                        Some(Stage::Translation),
                        anyhow::anyhow!("returned 1 segments; expected 2"),
                    ))
                } else {
                    Ok(StageOutcome::Skipped)
                })
            },
            |retry, attempts| {
                waits.push((retry, attempts));
                std::future::ready(())
            },
        )
        .await;
        (calls.load(Ordering::Relaxed), waits, result.is_ok())
    }

    #[tokio::test]
    async fn retries_a_fake_stage_until_success_or_exhaustion() {
        assert_eq!(
            fake_stage(2, ErrorKind::Processing, 2).await,
            (3, vec![(1, 3), (2, 3)], true)
        );
        assert_eq!(
            fake_stage(10, ErrorKind::Processing, 2).await,
            (3, vec![(1, 3), (2, 3)], false)
        );
        assert_eq!(
            fake_stage(10, ErrorKind::ModelLoad, 0).await,
            (1, vec![], false)
        );
        for kind in [ErrorKind::InvalidInput, ErrorKind::Commit] {
            assert_eq!(fake_stage(2, kind, 2).await, (1, vec![], false));
        }
    }

    #[tokio::test]
    async fn stopping_during_backoff_does_not_start_another_attempt() {
        let stop = StopToken::default();
        let calls = AtomicU8::new(0);
        let result = retry_stage(
            2,
            &stop,
            || {
                calls.fetch_add(1, Ordering::Relaxed);
                std::future::ready(Err(PipelineError::new(
                    ErrorKind::Processing,
                    Some(Stage::Ocr),
                    anyhow::anyhow!("timeout"),
                )))
            },
            |_, _| {
                stop.stop();
                std::future::ready(())
            },
        )
        .await
        .unwrap();
        assert!(matches!(result, StageOutcome::Stopped));
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }
}
