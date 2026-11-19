//! Explain this line (T12) as the app runs it: the model Explain this line
//! uses (`booharu::models`) explains one block of the open project, as
//! `booharu::context::explain` asks, and its answer goes into the project's
//! agent file as a note, which tells `booharu_agent_history_changed`. It
//! writes nothing to the scene. It's a job of `booharu::jobs`, so Stop and
//! closing the project end it, but tells no `booharu_job`: its command
//! answers.

use std::{path::PathBuf, sync::Arc};

use anyhow::Context as _;
use booharu::{
    calls::{self, Stops},
    context::explain::{self, ExplainAnswer},
    jobs::{self, JobKind},
    models, runs,
};
use booharu_core::context::{Chapter, SeriesStore};
use koharu_agent::AgentConfig;
use koharu_llm::Size;
use koharu_scene::LayerId;
use tauri::State;

use super::{
    events::AGENT_HISTORY_CHANGED,
    read_chapter::{connect, settings},
};
use crate::{app::App, commands::Result};

/// The context one of Koharu's own models is loaded with to explain a line:
/// the page, the line and its terms, and the answer.
const CONTEXT: u32 = 16_384;

/// What a stopped explanation says.
const STOPPED: &str = "Explain this line was stopped.";

/// Explains the line `layer` of the open project, a block with source text,
/// with the model Explain this line uses: the Read chapter model, else the
/// translation model when it's a general model, else the agent's. Sends the
/// page with it when the model reads images. One of Koharu's own models is
/// loaded for it, once the pipeline unloads its own while no run goes, and
/// dropped as it answers. Answers the explanation and two or three other
/// translations, with the line as it was asked about and the model that
/// answered, and adds them to the project's agent file as a note, unless
/// that file can't be read or written, which is logged and never fails the
/// answer. Fails with no model to ask, an answer that doesn't read, or once
/// Stop or closing the project stops it.
#[tauri::command]
#[specta::specta]
pub async fn booharu_explain_line(
    app: State<'_, Arc<App>>,
    layer: LayerId,
) -> Result<ExplainAnswer> {
    Ok(explain_line(Arc::clone(&app), layer).await?)
}

/// [`booharu_explain_line`]'s work.
async fn explain_line(app: Arc<App>, layer: LayerId) -> anyhow::Result<ExplainAnswer> {
    let job = jobs::start(JobKind::Explain)?;
    let stops = Stops::new(job.cancel().clone(), app.pipeline.clone());
    let settings = settings()?;
    let agent = koharu_config::get::<AgentConfig>()?.model;
    let listed = koharu_llm::models().await;
    let reference = models::explain_model(
        &listed,
        settings.reading.model.as_deref(),
        &settings.translation,
        agent.as_deref(),
    )
    .context(
        "No model can explain this line: choose a Read chapter model in Settings > Pipeline, or \
         a model for the agent.",
    )?;
    let session = app.session()?;
    let (path, project) = {
        let session = session.lock();
        (session.path().to_owned(), session.project())
    };
    let line = explain::line(&project, layer)?;
    let text = {
        let (path, line) = (path.clone(), line.clone());
        let target = settings.target_language;
        tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
            let chapter = Chapter::read(&path, SeriesStore::open)?;
            Ok(explain::request(
                &line,
                &chapter.context,
                chapter.series.as_ref(),
                &target,
            ))
        })
        .await??
    };
    if models::local(&reference) && !runs::run_going(&app.pipeline.told()) {
        // As Read chapter does: the pipeline's models free their memory.
        app.pipeline.unload();
    }
    let size = Size {
        sequences: 1,
        context: CONTEXT,
        images: models::listed(&listed, &reference).is_none_or(|model| model.images),
    };
    let model = connect(&reference, size, &stops)
        .await
        .with_context(|| format!("Explain this line couldn't load {reference}"))?
        .context(STOPPED)?;
    calls::usable(&model, &reference, false)?;
    let answer = explain::explain(&model, &reference, session, line, text, &stops).await;
    drop(model);
    let answer = answer?.context(STOPPED)?;
    if note(path, answer.clone()).await {
        app.emit(AGENT_HISTORY_CHANGED, ());
    }
    Ok(answer)
}

/// Adds `answer` to the agent file of the project file `path` as a note, as
/// the chat shows it, and answers whether it did. The note is the chat's,
/// the answer the user's, already paid for: a note that can't be written,
/// as when the agent file doesn't read, is logged, and the answer stands.
async fn note(path: PathBuf, answer: ExplainAnswer) -> bool {
    match tokio::task::spawn_blocking(move || explain::record(&path, &answer)).await {
        Ok(Ok(noted)) => noted,
        Ok(Err(error)) => {
            tracing::warn!("Explain this line's answer isn't in the agent's chat: {error:#}");
            false
        }
        Err(error) => {
            tracing::warn!("Explain this line's answer isn't in the agent's chat: {error}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use koharu_scene::Uuid;

    use super::*;

    #[tokio::test]
    async fn an_answer_stands_when_its_note_cant_be_written() {
        let dir = tempfile::tempdir().expect("a folder");
        let project = dir.path().join("Ch.1.khr");
        std::fs::write(&project, b"").expect("a project file");
        let file = booharu_core::context::agent_file(&project);
        std::fs::write(&file, "{").expect("a broken agent file");
        let answer = ExplainAnswer {
            layer: LayerId(Uuid::now_v7()),
            source: "ヒナです".to_owned(),
            translation: "It's Hina.".to_owned(),
            explanation: "She introduces herself.".to_owned(),
            alternatives: vec!["Hina here.".to_owned(), "I'm Hina.".to_owned()],
            model: "openai/gpt-5.4".to_owned(),
        };
        assert!(!note(project.clone(), answer.clone()).await);
        assert_eq!(std::fs::read_to_string(&file).expect("kept"), "{");
        std::fs::remove_file(&file).expect("removed");
        assert!(note(project, answer).await, "noted once the file reads");
    }
}
