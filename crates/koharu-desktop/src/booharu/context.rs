//! The series glossary and chapter notes commands: the series in the
//! Booharu home's `series` folder, which the glossary editor lists, makes,
//! saves, renames, deletes, exports, and imports; the open project's
//! chapter, its link to a series and its suggested terms; where a term is
//! used and Fix it everywhere; Import from instructions; and the note on a
//! first start saying where Booharu 0.0.4's series are. The files and the
//! rules are booharu-core's `context` and the `booharu` crate's, which the
//! agent's tools share. Every write of a series file or of the open
//! project's side file tells the interface `booharu_context_changed`, and
//! every save of the Instructions `booharu_instructions_changed`.

use std::{path::PathBuf, sync::Arc};

use anyhow::ensure;
use booharu::context::{self, FixedTerm, GlossaryUsage, InstructionImport};
use booharu_core::{
    context::{
        Chapter, ChapterContext, Extras, GlossaryEntry, InstructionTerm, Series, SeriesStore,
        instruction_terms, old_series_folder,
    },
    settings::StartSettings,
};
use koharu_pipeline::PipelineConfig;
use serde::Serialize;
use specta::Type;
use tauri::{State, Window};
use tauri_runtime_cef::CefRuntime;

use super::events::{CONTEXT_CHANGED, INSTRUCTIONS_CHANGED};
use crate::{app::App, commands::Result};

/// The open project's chapter as its side file and the series it links
/// hold it now, with the glossary a translation run on it would send.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
pub struct ChapterState {
    /// The side file: the linked series' id, the brief, and the suggested
    /// terms.
    pub context: ChapterContext,
    /// The series the chapter links; none when it links none or its file is
    /// gone.
    pub series: Option<Series>,
    /// Whether the chapter links a series whose file is gone.
    pub missing: bool,
    /// The terms a run translating the chapter now sends, locked first: the
    /// series' terms and the suggestions its text uses, each source once, at
    /// most 60.
    pub glossary: Vec<GlossaryEntry>,
    /// Whether the chapter's text isn't known yet, as a page was never
    /// through detection or a block waits to be read, so a run sends every
    /// term instead of those its text uses.
    pub fresh: bool,
    /// How many terms past the first 60 a run leaves out.
    pub left_out: u32,
}

/// The Instructions as saved now, and the terms their lines write, which
/// Import from instructions offers.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
pub struct InstructionPreview {
    pub instructions: String,
    pub terms: Vec<InstructionTerm>,
}

/// A series file picked for import, and whether a series of its id is
/// already saved, which the import replaces or keeps beside it.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
pub struct SeriesImport {
    pub series: Series,
    pub exists: bool,
}

/// The note on a first start: Booharu doesn't copy Booharu 0.0.4's series,
/// and says where they are to import them from.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
pub struct FirstStartNote {
    /// Whether to show it: it wasn't dismissed.
    pub show: bool,
    /// 0.0.4's series folder, `PreviewData\settings\series` beside the
    /// running program, when it's there; none when it isn't, so the note
    /// names it beside 0.0.4's `Booharu.exe` instead.
    pub folder: Option<String>,
}

/// Every series, by name, ignoring case. A file that doesn't read is left
/// out and logged.
#[tauri::command]
#[specta::specta]
pub async fn booharu_list_series() -> Result<Vec<Series>> {
    Ok(tokio::task::spawn_blocking(|| SeriesStore::open()?.list()).await??)
}

/// Makes a series named `name`, trimmed, with nothing in it, and saves it.
/// A blank name fails.
#[tauri::command]
#[specta::specta]
pub async fn booharu_create_series(app: State<'_, Arc<App>>, name: String) -> Result<Series> {
    let app = Arc::clone(&app);
    Ok(tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let series = SeriesStore::open()?.create(&name)?;
        app.emit(CONTEXT_CHANGED, ());
        Ok(series)
    })
    .await??)
}

/// The series `id`. One whose file is gone, or doesn't read, fails.
#[tauri::command]
#[specta::specta]
pub async fn booharu_get_series(id: String) -> Result<Series> {
    Ok(tokio::task::spawn_blocking(move || SeriesStore::open()?.get(&id)).await??)
}

/// Saves `series` whole, in place of its file, and answers it as saved. A
/// blank name, a term without a source or a target, or an id that isn't a
/// series id fails, saving nothing.
#[tauri::command]
#[specta::specta]
pub async fn booharu_save_series(app: State<'_, Arc<App>>, series: Series) -> Result<Series> {
    let app = Arc::clone(&app);
    Ok(tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let store = SeriesStore::open()?;
        store.save(&series)?;
        app.emit(CONTEXT_CHANGED, ());
        store.get(&series.id)
    })
    .await??)
}

/// Names the series `id` `name`, trimmed, and answers it as saved; a blank
/// name fails.
#[tauri::command]
#[specta::specta]
pub async fn booharu_rename_series(
    app: State<'_, Arc<App>>,
    id: String,
    name: String,
) -> Result<Series> {
    let app = Arc::clone(&app);
    Ok(tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let series = SeriesStore::open()?.rename(&id, &name)?;
        app.emit(CONTEXT_CHANGED, ());
        Ok(series)
    })
    .await??)
}

/// Deletes the series `id`. Chapters linking it then link none, and keep
/// their own notes.
#[tauri::command]
#[specta::specta]
pub async fn booharu_delete_series(app: State<'_, Arc<App>>, id: String) -> Result<()> {
    let app = Arc::clone(&app);
    Ok(tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        SeriesStore::open()?.delete(&id)?;
        app.emit(CONTEXT_CHANGED, ());
        Ok(())
    })
    .await??)
}

/// Links the open project's chapter to the series `series`, or to none,
/// and answers the chapter then. A series that isn't saved, or doesn't
/// read, fails, linking nothing.
#[tauri::command]
#[specta::specta]
pub async fn booharu_link_series(
    app: State<'_, Arc<App>>,
    series: Option<String>,
) -> Result<ChapterState> {
    let app = Arc::clone(&app);
    Ok(tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        if context::link(
            &project_file(&app)?,
            &SeriesStore::open()?,
            series.as_deref(),
        )? {
            app.emit(CONTEXT_CHANGED, ());
        }
        chapter(&app)
    })
    .await??)
}

/// The open project's chapter: its side file, the series it links, and the
/// glossary a translation run on it would send. A side file or a linked
/// series' file that doesn't read fails, naming it.
#[tauri::command]
#[specta::specta]
pub async fn booharu_get_chapter_context(app: State<'_, Arc<App>>) -> Result<ChapterState> {
    let app = Arc::clone(&app);
    Ok(tokio::task::spawn_blocking(move || chapter(&app)).await??)
}

/// Accepts the open chapter's suggestion `entry` names by its id into the
/// series the chapter links, as `entry` holds it, the user's changes
/// included, in place of the series' term with its source, a locked one
/// only with `replace_locked`, the user's choice in the term form, which
/// says what saving replaces; the suggestion leaves the chapter. Answers the
/// chapter then. Fails, changing nothing, when the suggestion is gone, the
/// chapter links no series, the series' term with its source is locked and
/// `replace_locked` is off, or the term lacks a source or a target.
#[tauri::command]
#[specta::specta]
pub async fn booharu_accept_suggestion(
    app: State<'_, Arc<App>>,
    entry: GlossaryEntry,
    replace_locked: bool,
) -> Result<ChapterState> {
    let app = Arc::clone(&app);
    Ok(tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        context::accept(
            &project_file(&app)?,
            &SeriesStore::open()?,
            entry,
            replace_locked,
        )?;
        app.emit(CONTEXT_CHANGED, ());
        chapter(&app)
    })
    .await??)
}

/// Dismisses the open chapter's suggestion `id`, and answers the chapter
/// then.
#[tauri::command]
#[specta::specta]
pub async fn booharu_dismiss_suggestion(
    app: State<'_, Arc<App>>,
    id: String,
) -> Result<ChapterState> {
    let app = Arc::clone(&app);
    Ok(tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        if context::dismiss(&project_file(&app)?, &id)? {
            app.emit(CONTEXT_CHANGED, ());
        }
        chapter(&app)
    })
    .await??)
}

/// Where the term `source` is used in the open project: the blocks whose
/// source it occurs in, ignoring case and whitespace, those Fix it
/// everywhere would translate again, and how many it would leave alone as
/// hand-edited or locked.
#[tauri::command]
#[specta::specta]
pub async fn booharu_glossary_usage(
    app: State<'_, Arc<App>>,
    source: String,
) -> Result<GlossaryUsage> {
    let project = app.session()?.lock().project();
    Ok(tokio::task::spawn_blocking(move || context::usage(&project, &source)).await?)
}

/// Fix it everywhere for the term `source`: translates again, in one
/// ordinary Translation run, every block of the open project using it that's
/// neither locked nor has a translation changed by hand, untranslated ones
/// included, so each follows the glossary as it is now. Answers the run,
/// none when no block was left to translate, and how many blocks it
/// translates and left alone.
#[tauri::command]
#[specta::specta]
pub async fn booharu_fix_term_everywhere(
    app: State<'_, Arc<App>>,
    source: String,
) -> Result<FixedTerm> {
    let app = Arc::clone(&app);
    Ok(tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let project = app.session()?.lock().project();
        context::fix_everywhere(&project, &source, |target, stages| {
            app.process(target, stages, None)
        })
    })
    .await??)
}

/// The Instructions as saved now, `[pipeline]`'s translator instructions,
/// with the terms their lines write, for Import from instructions.
#[tauri::command]
#[specta::specta]
pub fn booharu_preview_instruction_glossary() -> Result<InstructionPreview> {
    let instructions = koharu_config::get::<PipelineConfig>()?
        .translator
        .instructions;
    Ok(InstructionPreview {
        terms: instruction_terms(&instructions),
        instructions,
    })
}

/// Imports the terms on the lines `lines`, counted from 0, of
/// `instructions`, the Instructions as previewed, into the series `series`
/// as locked terms, skipping a source it already has; with `remove`, saves
/// the Instructions without those lines and tells the interface, which then
/// reads its settings again. Fails, changing nothing, when the saved
/// Instructions are no longer the ones previewed.
#[tauri::command]
#[specta::specta]
pub async fn booharu_import_instruction_glossary(
    app: State<'_, Arc<App>>,
    series: String,
    lines: Vec<u32>,
    remove: bool,
    instructions: String,
) -> Result<InstructionImport> {
    let app = Arc::clone(&app);
    Ok(tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let saved = koharu_config::get::<PipelineConfig>()?
            .translator
            .instructions;
        let imported = context::import_instructions(
            &SeriesStore::open()?,
            &series,
            &instructions,
            &saved,
            &lines,
            remove,
        )?;
        app.emit(CONTEXT_CHANGED, ());
        if let Some(kept) = &imported.instructions {
            let mut rewritten = false;
            koharu_config::update(|config: &mut PipelineConfig| {
                // In one read and write under the file's lock, so a save in
                // between is never undone.
                if config.translator.instructions == instructions {
                    config.translator.instructions.clone_from(kept);
                    rewritten = true;
                }
            })?;
            ensure!(
                rewritten,
                "the terms were imported, but the Instructions changed meanwhile and stay as \
                 they are"
            );
            app.pipeline.set_config(koharu_config::get()?);
            app.emit(INSTRUCTIONS_CHANGED, ());
        }
        Ok(imported)
    })
    .await??)
}

/// Exports the series `id` to a file the user picks, `<series name>.json`
/// to start with, in Booharu 0.0.4's shape, which 0.0.4 imports too.
/// Answers whether it was written, false when the user cancelled.
#[tauri::command]
#[specta::specta]
pub async fn booharu_export_series(window: Window<CefRuntime>, id: String) -> Result<bool> {
    let series = tokio::task::spawn_blocking(move || SeriesStore::open()?.get(&id)).await??;
    let name = sanitize_filename::sanitize(series.name.trim());
    let name = match name.trim() {
        "" => "series",
        name => name,
    };
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_parent(&window)
        .set_file_name(format!("{name}.json"))
        .add_filter("JSON", &["json"])
        .save_file()
        .await
    else {
        return Ok(false);
    };
    let path = file.path().to_owned();
    tokio::task::spawn_blocking(move || series.write_file(&path)).await??;
    Ok(true)
}

/// Asks the user for a series file to import, a Booharu 0.0.4 or 0.0.5
/// export or series file, starting in 0.0.4's series folder when it's beside
/// the running program, and answers its series once checked, with whether
/// a series of its id is saved already; none when the user cancelled. A
/// file that isn't a series fails.
#[tauri::command]
#[specta::specta]
pub async fn booharu_preview_series_import(
    window: Window<CefRuntime>,
) -> Result<Option<SeriesImport>> {
    let mut dialog = rfd::AsyncFileDialog::new()
        .set_parent(&window)
        .add_filter("JSON", &["json"]);
    if let Some(folder) = old_series_folder() {
        dialog = dialog.set_directory(folder);
    }
    let Some(file) = dialog.pick_file().await else {
        return Ok(None);
    };
    let path = file.path().to_owned();
    Ok(tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let series = Series::read_file(&path)?;
        let exists = SeriesStore::open()?.exists(&series.id)?;
        Ok(Some(SeriesImport { series, exists }))
    })
    .await??)
}

/// Saves `series`, a file's being imported: in place of the series of its
/// id with `replace`, else, when that id is taken, beside it with a new id.
/// Answers it as saved.
#[tauri::command]
#[specta::specta]
pub async fn booharu_import_series(
    app: State<'_, Arc<App>>,
    series: Series,
    replace: bool,
) -> Result<Series> {
    let app = Arc::clone(&app);
    Ok(tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let series = SeriesStore::open()?.import(series, replace)?;
        app.emit(CONTEXT_CHANGED, ());
        Ok(series)
    })
    .await??)
}

/// Whether to show the note on a first start, and where Booharu 0.0.4's
/// series are, when they're beside the running program.
#[tauri::command]
#[specta::specta]
pub fn booharu_get_first_start_note() -> Result<FirstStartNote> {
    let start = koharu_config::get::<StartSettings>()?;
    Ok(FirstStartNote {
        show: !start.fresh_start_note_seen,
        folder: old_series_folder().map(|folder| folder.display().to_string()),
    })
}

/// Dismisses the note on a first start for good, saving `[booharu-start]`
/// `fresh_start_note_seen` alone.
#[tauri::command]
#[specta::specta]
pub fn booharu_dismiss_first_start_note() -> Result<()> {
    koharu_config::update(|start: &mut StartSettings| start.fresh_start_note_seen = true)?;
    Ok(())
}

/// The open project's file.
fn project_file(app: &App) -> anyhow::Result<PathBuf> {
    Ok(app.session()?.lock().path().to_owned())
}

/// The open project's chapter as its files hold it now.
fn chapter(app: &App) -> anyhow::Result<ChapterState> {
    let (path, project) = {
        let session = app.session()?;
        let session = session.lock();
        (session.path().to_owned(), session.project())
    };
    let Chapter {
        context,
        series,
        missing,
    } = Chapter::read(&path, SeriesStore::open)?;
    let extras = Extras::of(&project, &context, series.as_ref(), false);
    Ok(ChapterState {
        context,
        series,
        missing,
        glossary: extras.glossary,
        fresh: extras.fresh,
        left_out: extras.left_out,
    })
}

#[cfg(test)]
mod tests {
    use booharu_core::context::{agent_file, side_file};

    use crate::library::Library;

    #[test]
    fn deleting_a_project_deletes_its_side_files() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let library = Library::new(dir.path().to_owned())?;
        drop(library.create("Ch.1")?);
        let project = dir.path().join("Ch.1.khr");
        fs_err::write(side_file(&project), "{}")?;
        fs_err::write(agent_file(&project), "{}")?;
        library.delete("Ch.1")?;
        assert!(!project.exists());
        assert!(!side_file(&project).exists() && !agent_file(&project).exists());
        // A project without them deletes as before.
        drop(library.create("Ch.2")?);
        library.delete("Ch.2")?;
        assert_eq!(library.list()?, Vec::<String>::new());
        Ok(())
    }
}
