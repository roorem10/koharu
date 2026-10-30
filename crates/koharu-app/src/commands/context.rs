//! Chapter context: the brief, glossary and other per-project data in the sidecar.
//!
//! Owned by WO-18. The shared foundation only adds the translation hints hook.

pub(crate) mod reading;
pub(crate) mod series;
pub(crate) mod settings;

pub(crate) use reading::auto_read;
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
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_chapter_context(
    project: State<'_, CurrentProject>,
) -> std::result::Result<ChapterState, Error> {
    let projects = project.project.lock().await;
    let project = projects.as_ref().context("no project is open")?;
    let context = project.load_context()?;
    let series = linked_series(&context)?;
    let config = koharu_pipeline::PipelineConfig::load()?;
    let supports_tasks =
        koharu_translator::Translator::supports_tasks(&config.read()?.translation.model);
    Ok(ChapterState {
        context,
        series,
        supports_tasks,
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
    project.update_context(|context| context.suggestions.retain(|term| term.id != entry.id))?;
    Ok(())
}

pub(crate) fn operation_translates(operation: &koharu_pipeline::Operation) -> bool {
    reading::includes_translation(operation)
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
