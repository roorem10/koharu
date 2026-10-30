use anyhow::Context as _;
use booharu_context::{
    GlossaryEntry, InstructionTerm, Series, SeriesStore, TermKind, instruction_terms,
};
use serde::Serialize;
use specta::Type;
use tauri::State;

use super::super::{Error, project::CurrentProject};

pub(super) fn store() -> anyhow::Result<SeriesStore> {
    let config = koharu_config::path()?;
    Ok(SeriesStore::new(
        config
            .parent()
            .context("settings directory is missing")?
            .join("series"),
    ))
}

#[tauri::command]
#[specta::specta]
pub(crate) fn list_series() -> Result<Vec<Series>, Error> {
    Ok(store()?.list()?)
}

#[tauri::command]
#[specta::specta]
pub(crate) fn create_series(name: String) -> Result<Series, Error> {
    let series = Series::new(name)?;
    store()?.save(&series)?;
    Ok(series)
}

#[tauri::command]
#[specta::specta]
pub(crate) fn get_series(id: String) -> Result<Series, Error> {
    Ok(store()?.get(&id)?)
}

#[tauri::command]
#[specta::specta]
pub(crate) fn save_series(series: Series) -> Result<Series, Error> {
    store()?.save(&series)?;
    Ok(series)
}

#[tauri::command]
#[specta::specta]
pub(crate) fn rename_series(id: String, name: String) -> Result<Series, Error> {
    let mut series = store()?.get(&id)?;
    series.name = name.trim().into();
    store()?.save(&series)?;
    Ok(series)
}

#[tauri::command]
#[specta::specta]
pub(crate) fn delete_series(id: String) -> Result<(), Error> {
    Ok(store()?.delete(&id)?)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn set_project_series(
    id: Option<String>,
    project: State<'_, CurrentProject>,
) -> Result<(), Error> {
    if let Some(id) = &id {
        store()?.get(id)?;
    }
    let projects = project.project.lock().await;
    let project = projects.as_ref().context("no project is open")?;
    project.update_context(|context| {
        context.series = id;
    })?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn export_series(id: String, title: String) -> Result<bool, Error> {
    let series = store()?.get(&id)?;
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title(title)
        .add_filter("JSON", &["json"])
        .set_file_name(format!("{}.json", series.id))
        .save_file()
        .await
    else {
        return Ok(false);
    };
    booharu_context::atomic_json(file.path(), &series)?;
    Ok(true)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn preview_series_import(title: String) -> Result<Option<Series>, Error> {
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title(title)
        .add_filter("JSON", &["json"])
        .pick_file()
        .await
    else {
        return Ok(None);
    };
    let series: Series = serde_json::from_slice(&std::fs::read(file.path())?)?;
    Ok(Some(series))
}

#[tauri::command]
#[specta::specta]
pub(crate) fn import_series(series: Series, replace: bool) -> Result<Series, Error> {
    Ok(store()?.import(&serde_json::to_string(&series)?, replace)?)
}

#[derive(Serialize, Type)]
pub struct InstructionPreview {
    pub instructions: String,
    pub terms: Vec<InstructionTerm>,
}

#[tauri::command]
#[specta::specta]
pub(crate) fn preview_instruction_glossary() -> Result<InstructionPreview, Error> {
    let config = koharu_pipeline::PipelineConfig::load()?;
    let instructions = config
        .read()?
        .translation
        .instructions
        .clone()
        .unwrap_or_default();
    let terms = instruction_terms(&instructions);
    Ok(InstructionPreview {
        instructions,
        terms,
    })
}

#[tauri::command]
#[specta::specta]
pub(crate) fn import_instruction_glossary(
    id: String,
    instructions: String,
    lines: Vec<u32>,
    remove: bool,
) -> Result<Series, Error> {
    let config = koharu_pipeline::PipelineConfig::load()?;
    let mut config = config.write()?;
    if !(config
        .translation
        .instructions
        .as_deref()
        .unwrap_or_default()
        == instructions)
    {
        return Err(anyhow::anyhow!("instructions changed; preview again").into());
    }
    let mut series = store()?.get(&id)?;
    let selected: Vec<_> = instruction_terms(&instructions)
        .into_iter()
        .filter(|entry| lines.contains(&entry.line))
        .collect();
    for entry in &selected {
        if !series
            .glossary
            .iter()
            .any(|term| term.source.trim() == entry.source)
        {
            series.glossary.push(GlossaryEntry {
                id: koharu_scene::EntityId::new().to_string(),
                source: entry.source.clone(),
                target: entry.target.clone(),
                locked: true,
                kind: TermKind::Term,
                note: String::new(),
            });
        }
    }
    store()?.save(&series)?;
    if remove {
        let selected: std::collections::BTreeSet<_> =
            selected.iter().map(|term| term.line).collect();
        config.translation.instructions = Some(
            instructions
                .lines()
                .enumerate()
                .filter(|(index, _)| !selected.contains(&(*index as u32)))
                .map(|(_, line)| line)
                .collect::<Vec<_>>()
                .join("\n"),
        );
        config.save()?;
    }
    Ok(series)
}
