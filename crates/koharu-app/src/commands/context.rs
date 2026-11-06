//! Chapter context: the brief, glossary and other per-project data in the sidecar.
//!
//! Owned by WO-18. The shared foundation only adds the translation hints hook.

pub(crate) mod history;
pub(crate) mod agent;
pub(crate) mod lines;
pub(crate) mod reading;
pub(crate) mod series;
pub(crate) mod settings;
pub(crate) mod sync;

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
    let mut sources = std::collections::BTreeSet::new();
    for entry in series
        .iter()
        .flat_map(|series| &series.glossary)
        .chain(&context.suggestions)
    {
        if !sources.insert(entry.source.trim().to_owned()) {
            continue;
        }
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
                let canonical = series
                    .as_ref()
                    .and_then(|series| {
                        series.characters.iter().find(|entry| {
                            entry.name.trim() == character.name.trim()
                                || (!entry.original.trim().is_empty()
                                    && entry.original.trim() == character.original.trim())
                        })
                    });
                match canonical {
                    Some(entry) => format!("{} ({}): {}", entry.name, entry.original, entry.notes),
                    None => format!("{} ({})", character.name, character.original),
                }
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
    pub term_conflicts: Vec<booharu_context::TermConflict>,
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
    let term_conflicts = series.as_ref()
        .map(|series| booharu_context::term_conflicts(&context, series))
        .transpose()?.unwrap_or_default();
    let config = koharu_pipeline::PipelineConfig::load()?;
    let supports_tasks =
        koharu_translator::Translator::supports_tasks(&config.read()?.translation.model);
    Ok(ChapterState {
        context,
        series,
        supports_tasks,
        term_conflicts,
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
    project.update_context(|context| context.suggestions.retain(|term| term.id != id))?;
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
    if !(context.suggestions.iter().any(|term| term.id == entry.id)) {
        return Err(anyhow::anyhow!("suggestion no longer exists").into());
    }
    series::store()?.update(context.series.as_deref().context("pick a series first")?, |series| {
        anyhow::ensure!(!series.glossary.iter().any(|term| term.source.trim() == entry.source.trim()),
            "term already exists in series glossary");
        series.glossary.push(entry.clone());
        Ok(())
    })?;
    project.update_context(|context| context.suggestions.retain(|term| term.id != entry.id))?;
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
