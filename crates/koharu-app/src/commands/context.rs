//! Chapter context: the brief, glossary and other per-project data in the sidecar.
//!
//! Owned by WO-18. The shared foundation only adds the translation hints hook.

pub(crate) mod history;
mod line_request;
pub(crate) mod lines;
pub(crate) mod reading;
pub(crate) mod second_pass;
pub(crate) mod series;
pub(crate) mod settings;

pub(crate) use lines::refresh_flags;
pub(crate) use reading::{auto_read, wants_auto_read};
pub(crate) use settings::get_context_settings;

use super::{Error, project::CurrentProject};
use anyhow::{Context as _, Result};
use booharu_context::ProjectContext;
use koharu_pipeline::TranslationHints;
use koharu_scene::Snapshot;
use serde::Serialize;
use specta::Type;
use tauri::State;

/// Builds the translation stage's extras for one pipeline run.
pub(crate) fn translation_hints(
    context: &ProjectContext,
    snapshot: &Snapshot,
) -> Result<TranslationHints> {
    let series = linked_series(context)?;
    let settings = settings::load()?;
    let mut glossary = Vec::new();
    for entry in series
        .iter()
        .flat_map(|series| &series.glossary)
        .chain(&context.suggestions)
    {
        glossary.push(koharu_pipeline::GlossaryTerm {
            hint: koharu_translator::GlossaryHint {
                source: entry.source.clone(),
                target: entry.target.clone(),
                note: (!entry.note.trim().is_empty()).then(|| entry.note.clone()),
            },
            locked: entry.locked,
        });
    }
    let mut background = series
        .as_ref()
        .map(|series| series.notes.clone())
        .unwrap_or_default();
    if let Some(brief) = &context.brief {
        background.push_str(&format!("\n{}", brief.summary));
    }
    let mut characters = std::collections::BTreeMap::new();
    for (index, page) in snapshot.pages().enumerate() {
        let names: Vec<_> = context
            .brief
            .iter()
            .flat_map(|brief| &brief.characters)
            .filter(|character| character.pages.contains(&(index as u32 + 1)))
            .map(|character| {
                let notes = series
                    .as_ref()
                    .and_then(|series| {
                        series.characters.iter().find(|entry| {
                            entry.name == character.name
                                || (!entry.original.is_empty()
                                    && entry.original == character.original)
                        })
                    })
                    .map(|entry| entry.notes.as_str())
                    .unwrap_or_default();
                format!("{} ({}): {}", character.name, character.original, notes)
            })
            .collect();
        if !names.is_empty() {
            characters.insert(
                page.id(),
                format!("Characters on this page: {}", names.join(", ")),
            );
        }
    }
    Ok(TranslationHints {
        skip: super::lettering::translation_skip(context, snapshot)?,
        glossary,
        background,
        characters,
        previous_pages: settings.previous_pages.min(2),
    })
}

#[derive(Serialize, Type)]
pub struct ChapterState {
    pub context: ProjectContext,
    pub series: Option<booharu_context::Series>,
    pub supports_tasks: bool,
    pub supports_second_pass: bool,
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_chapter_context(
    project: State<'_, CurrentProject>,
) -> std::result::Result<ChapterState, Error> {
    let projects = project.project.lock().await;
    let project = projects.as_ref().context("no project is open")?;
    refresh_flags(project)?;
    let context = project.load_context()?;
    let series = linked_series(&context)?;
    let config = koharu_pipeline::PipelineConfig::load()?;
    let supports_tasks =
        koharu_translator::Translator::supports_tasks(&config.read()?.translation.model);
    let model = settings::load()?
        .hard_line_model
        .unwrap_or(config.read()?.translation.model.clone());
    let supports_second_pass = reading::can_read_images(&model);
    Ok(ChapterState {
        context,
        series,
        supports_tasks,
        supports_second_pass,
    })
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn save_chapter_summary(
    summary: String,
    project: State<'_, CurrentProject>,
) -> std::result::Result<(), Error> {
    let projects = project.project.lock().await;
    let project = projects.as_ref().context("no project is open")?;
    project.update_context(|context| {
        let brief = context.brief.get_or_insert_with(Default::default);
        brief.summary = summary;
        brief.edited = true;
    })?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn dismiss_suggestion(
    id: String,
    project: State<'_, CurrentProject>,
) -> std::result::Result<(), Error> {
    let projects = project.project.lock().await;
    let project = projects.as_ref().context("no project is open")?;
    let source = project
        .load_context()?
        .suggestions
        .into_iter()
        .find(|term| term.id == id)
        .map(|term| term.source);
    project.update_context(|context| {
        second_pass::dismiss_suggestion(context, &id, source.as_deref())
    })?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn accept_suggestion(
    entry: booharu_context::GlossaryEntry,
    project: State<'_, CurrentProject>,
) -> std::result::Result<(), Error> {
    let projects = project.project.lock().await;
    let project = projects.as_ref().context("no project is open")?;
    let context = project.load_context()?;
    let suggested = context
        .suggestions
        .iter()
        .find(|term| term.id == entry.id)
        .context("suggestion no longer exists")?;
    let mut series =
        series::store()?.get(context.series.as_deref().context("pick a series first")?)?;
    if !(!series
        .glossary
        .iter()
        .any(|term| term.source.trim() == entry.source.trim()))
    {
        return Err(anyhow::anyhow!("term already exists in series glossary").into());
    }
    series.glossary.push(entry.clone());
    series::store()?.save(&series)?;
    project.update_context(|context| {
        second_pass::dismiss_suggestion(context, &entry.id, Some(&suggested.source))
    })?;
    Ok(())
}

pub(crate) fn operation_translates(operation: &koharu_pipeline::Operation) -> bool {
    reading::includes_translation(operation)
}

/// Splits a run that both finds text (detection or OCR) and translates into the
/// text-finding part and the rest, so Read chapter can run in between (#55).
pub(crate) fn split_for_reading(
    operation: &koharu_pipeline::Operation,
) -> Option<(koharu_pipeline::Operation, koharu_pipeline::Operation)> {
    use koharu_pipeline::{Operation, Stage};
    let stages = operation.stages().ok()?;
    let finds = |stage: &Stage| matches!(stage, Stage::Detection | Stage::Ocr);
    if !stages.contains(&Stage::Translation) || !stages.iter().any(finds) {
        return None;
    }
    let (first, rest): (Vec<_>, Vec<_>) = stages.into_iter().partition(finds);
    Some((
        Operation::Stages { stages: first },
        Operation::Stages { stages: rest },
    ))
}

pub(crate) fn linked_series(context: &ProjectContext) -> Result<Option<booharu_context::Series>> {
    match context.series.as_deref() {
        None => Ok(None),
        Some(id) => match series::store()?.get(id) {
            Ok(series) => Ok(Some(series)),
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
            {
                Ok(None)
            }
            Err(error) => Err(error),
        },
    }
}
