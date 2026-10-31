use super::*;

#[test]
fn configuration_ignores_unknown_fields() {
    let config = toml::from_str::<PipelineConfig>("legacy_limit = 1").unwrap();
    assert_eq!(config, PipelineConfig::default());
}

#[tokio::test]
async fn stop_is_a_successful_partial_result() {
    let pipeline = pipeline(Default::default());
    let stop = StopToken::default();
    stop.stop();
    let request = Request {
        stop,
        ..Request::default()
    };
    let mut committer = RejectCommitter;

    let report = pipeline
        .execute(
            koharu_scene::Session::memory().await.unwrap().snapshot(),
            request,
            &mut committer,
        )
        .await
        .unwrap();

    assert_eq!(report.status, RunStatus::Stopped);
    assert_eq!(report.completed, 0);
}

#[tokio::test]
async fn stop_after_a_page_keeps_completed_progress() {
    let translation = TranslationConfig {
        model: koharu_translator::ModelSelection {
            provider: koharu_translator::Provider::OpenAi,
            model: Some("gpt-5.6-luna".to_owned()),
            quantization: None,
            vision: true,
            reasoning: true,
        },
        ..Default::default()
    };
    let pipeline = pipeline(translation);
    let mut session = koharu_scene::Session::memory().await.unwrap();
    let patch = session
        .snapshot()
        .patch(|edit| {
            edit.add_page(
                koharu_scene::PageDraft::new("one", 1.0, 1.0),
                koharu_scene::At::End,
            )?;
            edit.add_page(
                koharu_scene::PageDraft::new("two", 1.0, 1.0),
                koharu_scene::At::End,
            )?;
            Ok(())
        })
        .unwrap();
    session.commit(patch).await.unwrap();
    let stop = StopToken::default();
    let progress_stop = stop.clone();
    let request = Request {
        operation: Operation::Only {
            stage: Stage::Translation,
        },
        stop,
        progress: Some(std::sync::Arc::new(move |event| {
            if matches!(event, Progress::Skipped { .. }) {
                progress_stop.stop();
            }
        })),
        ..Request::default()
    };
    let mut committer = RejectCommitter;

    let report = pipeline
        .execute(session.snapshot(), request, &mut committer)
        .await
        .unwrap();

    assert_eq!(report.status, RunStatus::Stopped);
    assert_eq!(report.completed, 1);
    assert_eq!(report.total, 2);
}

#[tokio::test]
async fn translation_leaves_hinted_layers_alone() {
    // A hosted provider without a key fails on any real request, so reaching
    // the commit proves the skipped line was never sent.
    let pipeline = pipeline(TranslationConfig {
        model: koharu_translator::ModelSelection {
            provider: koharu_translator::Provider::OpenAi,
            model: Some("gpt-5.6-luna".to_owned()),
            quantization: None,
            vision: false,
            reasoning: false,
        },
        ..Default::default()
    });
    let mut session = koharu_scene::Session::memory().await.unwrap();
    let mut layer = None;
    let patch = session
        .snapshot()
        .patch(|edit| {
            let page = edit.add_page(
                koharu_scene::PageDraft::new("one", 1.0, 1.0),
                koharu_scene::At::End,
            )?;
            let content = edit.add_text_content(page, koharu_scene::At::End)?;
            edit.set(
                content,
                &koharu_scene::SourceText {
                    text: koharu_scene::Authored::user("ドン".to_owned()),
                    language: None,
                },
            )?;
            layer = Some(edit.add_text_layer(
                page,
                koharu_scene::At::End,
                content,
                &koharu_scene::TextLayout {
                    origin: koharu_scene::Origin::User,
                    kind: koharu_scene::TextLayoutKind::Paragraph,
                    angle_degrees: None,
                    warp: None,
                },
            )?);
            Ok(())
        })
        .unwrap();
    session.commit(patch).await.unwrap();
    let request = Request {
        operation: Operation::Only {
            stage: Stage::Translation,
        },
        translation_hints: std::sync::Arc::new(crate::TranslationHints {
            skip: [layer.unwrap()].into(),
            ..Default::default()
        }),
        ..Request::default()
    };
    let snapshot = session.snapshot();
    let mut committer = SessionCommitter(&mut session);

    let report = pipeline
        .execute(snapshot, request, &mut committer)
        .await
        .unwrap();

    assert_eq!(report.status, RunStatus::Completed);
    let snapshot = session.snapshot();
    let content = snapshot
        .text_layer(layer.unwrap())
        .unwrap()
        .content()
        .unwrap();
    assert!(
        snapshot
            .component::<koharu_scene::Translation>(content.id())
            .unwrap()
            .is_none()
    );
}

struct SessionCommitter<'a>(&'a mut koharu_scene::Session);

#[async_trait::async_trait]
impl Committer for SessionCommitter<'_> {
    async fn commit(&mut self, output: StageOutput) -> anyhow::Result<koharu_scene::Snapshot> {
        Ok(self.0.commit(output.patch).await?.snapshot)
    }
}

struct RejectCommitter;

fn pipeline(translation: TranslationConfig) -> Pipeline {
    let config = PipelineConfig {
        translation,
        ..PipelineConfig::default()
    };
    Pipeline::from_config(
        koharu_config::Config::memory(config),
        koharu_config::Config::memory(koharu_translator::ProvidersConfig::default()),
        koharu_ml::Device::cpu(),
    )
    .unwrap()
}

#[async_trait::async_trait]
impl Committer for RejectCommitter {
    async fn commit(&mut self, _output: StageOutput) -> anyhow::Result<koharu_scene::Snapshot> {
        anyhow::bail!("stopped execution must not commit")
    }
}

#[test]
fn operations_expand_to_the_supported_workflows() {
    assert_eq!(
        Operation::Through {
            stage: Stage::Translation,
        }
        .stages()
        .unwrap(),
        vec![Stage::Detection, Stage::Ocr, Stage::Translation],
    );
    assert_eq!(
        Operation::Through {
            stage: Stage::Inpainting,
        }
        .stages()
        .unwrap(),
        vec![Stage::Detection, Stage::Inpainting],
    );
    assert_eq!(
        Operation::Only {
            stage: Stage::Translation,
        }
        .stages()
        .unwrap(),
        vec![Stage::Translation],
    );
    assert_eq!(
        Operation::Stages {
            stages: vec![Stage::Translation, Stage::Detection, Stage::Translation],
        }
        .stages()
        .unwrap(),
        vec![Stage::Detection, Stage::Translation],
    );
}

#[tokio::test]
async fn a_failed_page_does_not_block_other_pages_but_stop_run_does() {
    for stop_run in [false, true] {
        let pipeline = pipeline(TranslationConfig {
            model: koharu_translator::ModelSelection {
                provider: koharu_translator::Provider::OpenAi,
                model: Some("test".into()),
                quantization: None,
                vision: false,
                reasoning: false,
            },
            ..Default::default()
        });
        let mut session = koharu_scene::Session::memory().await.unwrap();
        let mut first = None;
        let patch = session
            .snapshot()
            .patch(|edit| {
                let page = edit.add_page(
                    koharu_scene::PageDraft::new("bad", 1.0, 1.0),
                    koharu_scene::At::End,
                )?;
                first = Some(page);
                let text = edit.add_text_content(page, koharu_scene::At::End)?;
                edit.set(
                    text,
                    &koharu_scene::SourceText {
                        text: koharu_scene::Authored::user("hello".into()),
                        language: None,
                    },
                )?;
                edit.add_text_layer(
                    page,
                    koharu_scene::At::End,
                    text,
                    &koharu_scene::TextLayout {
                        origin: koharu_scene::Origin::User,
                        kind: koharu_scene::TextLayoutKind::Paragraph,
                        angle_degrees: None,
                        warp: None,
                    },
                )?;
                edit.add_page(
                    koharu_scene::PageDraft::new("good", 1.0, 1.0),
                    koharu_scene::At::End,
                )?;
                Ok(())
            })
            .unwrap();
        session.commit(patch).await.unwrap();
        let request = Request {
            operation: Operation::Only {
                stage: Stage::Translation,
            },
            policy: RunPolicy {
                retries: 0,
                on_page_failure: if stop_run {
                    OnPageFailure::StopRun
                } else {
                    OnPageFailure::ContinueWithOtherPages
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let snapshot = session.snapshot();
        let result = pipeline
            .execute(snapshot, request, &mut SessionCommitter(&mut session))
            .await;
        if stop_run {
            assert!(result.is_err());
        } else {
            let report = result.unwrap();
            assert_eq!(report.status, RunStatus::CompletedWithProblems);
            assert_eq!(report.completed, 2);
            assert_eq!(report.failures.len(), 1);
            assert_eq!(report.failures[0].page, first.unwrap());
            assert_eq!(report.failures[0].label, "bad");
            assert_eq!(report.failures[0].stage, Stage::Translation);
        }
    }
}

#[tokio::test]
async fn a_graceful_stop_finishes_the_current_page_item_before_stopping() {
    let pipeline = pipeline(TranslationConfig {
        model: koharu_translator::ModelSelection {
            provider: koharu_translator::Provider::OpenAi,
            model: Some("test".into()),
            quantization: None,
            vision: false,
            reasoning: false,
        },
        ..Default::default()
    });
    let mut session = koharu_scene::Session::memory().await.unwrap();
    let patch = session
        .snapshot()
        .patch(|edit| {
            for name in ["one", "two"] {
                edit.add_page(
                    koharu_scene::PageDraft::new(name, 1.0, 1.0),
                    koharu_scene::At::End,
                )?;
            }
            Ok(())
        })
        .unwrap();
    session.commit(patch).await.unwrap();
    let stop = StopToken::default();
    let requested = stop.clone();
    let request = Request {
        operation: Operation::Only {
            stage: Stage::Translation,
        },
        stop,
        progress: Some(std::sync::Arc::new(move |event| {
            if matches!(event, Progress::Skipped { .. }) {
                requested.finish_current_page();
            }
        })),
        ..Default::default()
    };
    let report = pipeline
        .execute(session.snapshot(), request, &mut RejectCommitter)
        .await
        .unwrap();
    assert_eq!(report.status, RunStatus::Stopped);
    assert_eq!(report.completed, 1);
}
