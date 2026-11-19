//! The series glossary and the chapter's notes, as the agent works with
//! them: the open chapter read whole, a series linked by its name, terms
//! saved in or deleted from the linked series, terms suggested for the
//! chapter and suggestions accepted or dismissed by their source, the
//! Instructions' glossary lines moved into the series, Fix it everywhere,
//! and a series exported to or imported from a file. Each is worked out by
//! `booharu::context`, as the app's commands work it out, on the series
//! folder and the Instructions the app's host gives, and every write of a
//! series file or the open project's side file is told to the app, which
//! tells its interface, as the commands' writes are. The chapter in short
//! goes with each of the agent's requests, in the project's outline
//! (`outlined`).

use std::{path::PathBuf, sync::Arc};

use booharu::context::{
    self, ChapterNotes, FixedTerm, ImportedSeries, InstructionOffer, LinkedSeries, SavedTerm,
    TermEdit,
};
use booharu_core::{
    context::{GlossaryEntry, TermKind},
    settings::BooharuPreferences,
};
use koharu_pipeline::Request;
use rig_core::{
    completion::ToolAnnotations,
    tool::{Tool, ToolContext, ToolExecutionError},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use super::BooharuHost;
use crate::tools::{NoArgs, Tools, failed, project, schema};

/// `outline`, the project's outline each of the agent's requests goes
/// with, with the chapter in short under `booharu`: the name of the series
/// it's linked to, its brief, and the glossary terms found in its text, or
/// why it can't be read.
pub(crate) fn outlined(tools: &Tools, mut outline: Value) -> Value {
    let host = host(tools);
    let (path, project) = {
        let session = tools.session.lock();
        (session.path().to_owned(), session.project())
    };
    let chapter = match context::outline(&path, &project, || host.series()) {
        Ok(chapter) => json!(chapter),
        Err(error) => json!({"error": format!("{error:#}")}),
    };
    if let Value::Object(fields) = &mut outline {
        fields.insert("booharu".to_owned(), chapter);
    }
    outline
}

pub(super) struct GetChapterContext;

impl Tool for GetChapterContext {
    const NAME: &'static str = "booharu_get_chapter_context";
    type Args = NoArgs;
    type Output = ChapterNotes;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "The open chapter's notes, which every translation run on it follows: the series it is \
         linked to, whole (`series`, with its `name`, the `notes` every chapter's translation \
         reads as background, its `characters` and how they speak, and its `glossary`), or none, \
         and whether that series' file is gone (`missing`); the terms suggested for this chapter \
         (`suggestions`), which its translation follows as the series' terms until the user \
         accepts them into the series or dismisses them; the `brief` Read chapter wrote, what \
         happens and who is in it; the glossary terms `found` in the chapter's text, locked ones \
         first; and `extras`, word for word what a run translating the chapter now adds after \
         the user's Instructions: the glossary terms the chapter uses, at most 60 (`left_out` \
         past those), the background, and the characters. While the chapter's text isn't known \
         yet (`fresh`), as a page was never through detection or a block waits to be read, a \
         run sends every term instead, as does a run that detects or reads text before it \
         translates. Needs a project open, as every tool here does."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<NoArgs>()
    }

    fn annotations(&self) -> ToolAnnotations {
        ToolAnnotations::default().with_read_only(true)
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        _: NoArgs,
    ) -> Result<ChapterNotes, Self::Error> {
        blocking(context, |tools, host| {
            let (path, project) = {
                let session = tools.session.lock();
                (session.path().to_owned(), session.project())
            };
            context::notes(&path, &project, || host.series())
        })
        .await
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct LinkSeriesArgs {
    /// The series' name, ignoring case.
    name: String,
    /// Whether to make a series of that name when there's none.
    #[serde(default)]
    create: bool,
}

pub(super) struct LinkSeries;

impl Tool for LinkSeries {
    const NAME: &'static str = "booharu_link_series";
    type Args = LinkSeriesArgs;
    type Output = LinkedSeries;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Link the open chapter to the series named `name`, ignoring case, so its translation \
         follows that series' glossary, notes, and characters; with `create`, make an empty \
         series of that name when there's none, for booharu_save_glossary_entry to fill. Fails, \
         linking nothing, when there's no series of that name and `create` is off, naming the \
         series there are, or when two series have the name, which the user tells apart in the \
         glossary editor. Answers the series' `id` and `name`, whether it was `created`, and \
         whether the link `changed`."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<LinkSeriesArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: LinkSeriesArgs,
    ) -> Result<LinkedSeries, Self::Error> {
        blocking(context, move |tools, host| {
            let linked =
                context::link_named(&file(tools), &host.series()?, &args.name, args.create)?;
            if linked.created || linked.changed {
                host.context_changed();
            }
            Ok(linked)
        })
        .await
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct SaveGlossaryEntryArgs {
    /// The term as the pages write it.
    source: String,
    /// The translation every page keeps to.
    target: String,
    /// What kind of term it is.
    #[serde(default)]
    kind: Option<TermKind>,
    /// A note the translation reads beside it, such as who or what it is.
    #[serde(default)]
    note: Option<String>,
    /// Whether the term is fixed, which only the user decides.
    #[serde(default)]
    locked: Option<bool>,
}

pub(super) struct SaveGlossaryEntry;

impl Tool for SaveGlossaryEntry {
    const NAME: &'static str = "booharu_save_glossary_entry";
    type Args = SaveGlossaryEntryArgs;
    type Output = SavedTerm;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Save a term in the glossary of the series the open chapter is linked to: its `source` \
         as the pages write it and the `target` every translation keeps to, with its `kind`, a \
         `note` the translation reads beside it, and whether it is `locked`. A term with the \
         same source, ignoring case and whitespace, is replaced in place, keeping what you leave \
         out; else the term is added after the others, unlocked, of kind `other`, with no note, \
         where you leave those out. A locked term is one the user fixed: it goes before the \
         others, wins over one with the same source, and Booharu's automatic steps never replace \
         it, so change or lock one only when the user asks. Lines translated before keep their \
         translation: booharu_fix_term_everywhere translates the lines using the term again. \
         Fails when the chapter is linked to no series, which booharu_link_series links. \
         Answers the series' name, the term as saved, and whether it `replaced` one."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<SaveGlossaryEntryArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: SaveGlossaryEntryArgs,
    ) -> Result<SavedTerm, Self::Error> {
        let edit = TermEdit {
            source: args.source,
            target: args.target,
            kind: args.kind,
            note: args.note,
            locked: args.locked,
        };
        blocking(context, move |tools, host| {
            let saved = context::save_term(&file(tools), &host.series()?, edit)?;
            host.context_changed();
            Ok(saved)
        })
        .await
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceArgs {
    /// The term's source, ignoring case and whitespace.
    source: String,
}

/// How many terms a tool removed.
#[derive(Debug, Serialize)]
pub(super) struct Removed {
    removed: usize,
}

pub(super) struct DeleteGlossaryEntry;

impl Tool for DeleteGlossaryEntry {
    const NAME: &'static str = "booharu_delete_glossary_entry";
    type Args = SourceArgs;
    type Output = Removed;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Delete the term with the `source`, ignoring case and whitespace, from the glossary of \
         the series the open chapter is linked to, each copy of it. A locked term is one the \
         user fixed: delete one only when the user asks. Fails when the chapter is linked to no \
         series. Answers how many terms it `removed`, 0 when the series has none with that \
         source."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<SourceArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: SourceArgs,
    ) -> Result<Removed, Self::Error> {
        blocking(context, move |tools, host| {
            let removed = context::delete_term(&file(tools), &host.series()?, &args.source)?;
            if removed > 0 {
                host.context_changed();
            }
            Ok(Removed { removed })
        })
        .await
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct AddGlossaryEntryArgs {
    /// The term as the pages write it.
    source: String,
    /// The translation the chapter should keep to.
    target: String,
    /// What kind of term it is.
    #[serde(default)]
    kind: TermKind,
    /// A note the translation reads beside it, such as who or what it is.
    #[serde(default)]
    note: String,
}

/// Whether a term was suggested.
#[derive(Debug, Serialize)]
pub(super) struct Suggested {
    suggested: bool,
}

pub(super) struct AddGlossaryEntry;

impl Tool for AddGlossaryEntry {
    const NAME: &'static str = "booharu_add_glossary_entry";
    type Args = AddGlossaryEntryArgs;
    type Output = Suggested;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Suggest a glossary term for the open chapter, as Read chapter does: its `source` as the \
         pages write it, the `target` its translation should keep to, its `kind`, and a `note`. \
         The chapter's translation follows a suggestion as it follows the series' terms, until \
         the user accepts it into the series or dismisses it. It never replaces a term, locked \
         or not: when the series the chapter is linked to, or the chapter's suggestions, have \
         one with that source already, ignoring case and whitespace, it adds nothing and answers \
         `suggested` false. Use it for a name or term you find while translating; \
         booharu_save_glossary_entry changes the series' glossary when the user asks."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<AddGlossaryEntryArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: AddGlossaryEntryArgs,
    ) -> Result<Suggested, Self::Error> {
        blocking(context, move |tools, host| {
            let entry = GlossaryEntry {
                kind: args.kind,
                note: args.note,
                ..GlossaryEntry::new(&args.source, &args.target)
            };
            let suggested = context::suggest(&file(tools), &host.series()?, entry)?;
            if suggested {
                host.context_changed();
            }
            Ok(Suggested { suggested })
        })
        .await
    }
}

pub(super) struct AcceptGlossarySuggestion;

impl Tool for AcceptGlossarySuggestion {
    const NAME: &'static str = "booharu_accept_glossary_suggestion";
    type Args = SourceArgs;
    type Output = SavedTerm;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Accept the open chapter's suggested term with the `source`, ignoring case and \
         whitespace, into the glossary of the series the chapter is linked to, as it was \
         suggested, in place of the series' term with that source when it has an unlocked one; \
         the suggestion then leaves the chapter. A locked term is one the user fixed: when the \
         series' term with that source is locked, it fails, changing nothing, so tell the user \
         both, and booharu_save_glossary_entry replaces the term only when they ask. Fails too \
         when the chapter has no such suggestion or is linked to no series. Answers the series' \
         name, the term as saved, and whether it `replaced` one."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<SourceArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: SourceArgs,
    ) -> Result<SavedTerm, Self::Error> {
        blocking(context, move |tools, host| {
            let accepted = context::accept_source(&file(tools), &host.series()?, &args.source)?;
            host.context_changed();
            Ok(accepted)
        })
        .await
    }
}

/// How many suggestions a tool dismissed.
#[derive(Debug, Serialize)]
pub(super) struct Dismissed {
    dismissed: usize,
}

pub(super) struct DismissGlossarySuggestion;

impl Tool for DismissGlossarySuggestion {
    const NAME: &'static str = "booharu_dismiss_glossary_suggestion";
    type Args = SourceArgs;
    type Output = Dismissed;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Dismiss the open chapter's suggested term with the `source`, ignoring case and \
         whitespace, so its translation no longer follows it. Answers how many suggestions it \
         `dismissed`, 0 when the chapter has none with that source."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<SourceArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: SourceArgs,
    ) -> Result<Dismissed, Self::Error> {
        blocking(context, move |tools, host| {
            let dismissed = context::dismiss_source(&file(tools), &args.source)?;
            if dismissed > 0 {
                host.context_changed();
            }
            Ok(Dismissed { dismissed })
        })
        .await
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ImportInstructionGlossaryArgs {
    /// False to read the terms the Instructions' lines write; true to
    /// import those on `lines`.
    apply: bool,
    /// With `apply`, the lines to import, counted from 0, as reading them
    /// answered.
    #[serde(default)]
    lines: Vec<u32>,
    /// With `apply`, whether to delete the imported lines from the
    /// Instructions.
    #[serde(default)]
    remove: bool,
    /// With `apply`, the `version` of the Instructions reading the terms
    /// answered, so the lines of Instructions changed since are never
    /// imported.
    #[serde(default)]
    version: Option<String>,
}

/// What Import from instructions answers the agent: the terms offered, or
/// what importing them did.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(super) enum InstructionAnswer {
    Offer(InstructionOffer),
    Imported {
        /// The series' name.
        series: String,
        added: u32,
        skipped: u32,
        /// Whether the lines were deleted from the Instructions.
        removed: bool,
    },
}

pub(super) struct ImportInstructionGlossary;

impl Tool for ImportInstructionGlossary {
    const NAME: &'static str = "booharu_import_instruction_glossary";
    type Args = ImportInstructionGlossaryArgs;
    type Output = InstructionAnswer;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Move glossary lines of the translator's Instructions, such as `ヒナ = Hina`, into the \
         glossary of the series the open chapter is linked to, as Import from instructions \
         does. With `apply` false, reads the Instructions as saved now and answers the terms \
         their lines write, each with its `line`, counted from 0, its `source` and `target`, and \
         whether the series has a term with that source already (`held`), which importing \
         skips, with the series' name and the Instructions' `version`; it changes nothing. With \
         `apply` true, imports the terms on `lines` of the Instructions of that `version`, as \
         locked terms of kind `term`, skipping a source the series has; with `remove`, also \
         deletes those lines, skipped ones included, from the Instructions. Instructions \
         changed since that `version` was read are refused, importing nothing, as a line may \
         hold another term now: read the terms again and show them to the user again. Read the \
         terms first, show them to the user, and import only the lines they want, removing \
         them only when they agree. Fails when the chapter is linked to no series. Answers how \
         many terms were `added` and `skipped`, and whether the lines were `removed`."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<ImportInstructionGlossaryArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: ImportInstructionGlossaryArgs,
    ) -> Result<InstructionAnswer, Self::Error> {
        let version = match (args.apply, args.version) {
            (false, _) => String::new(),
            (true, _) if args.lines.is_empty() => {
                return Err(ToolExecutionError::invalid_args(
                    "name the `lines` to import, as reading the terms answered them",
                ));
            }
            (true, Some(version)) => version,
            (true, None) => {
                return Err(ToolExecutionError::invalid_args(
                    "name the `version` reading the terms answered, so lines of Instructions \
                     changed since are never imported",
                ));
            }
        };
        blocking(context, move |tools, host| {
            let (path, store, read) = (file(tools), host.series()?, host.instructions()?);
            if !args.apply {
                return Ok(InstructionAnswer::Offer(context::offer_instructions(
                    &path, &store, &read,
                )?));
            }
            let imported = context::import_linked_instructions(
                &path,
                &store,
                &read,
                &version,
                &args.lines,
                args.remove,
            )?;
            if imported.added > 0 {
                host.context_changed();
            }
            let removed = match &imported.instructions {
                Some(kept) => {
                    anyhow::ensure!(
                        host.save_instructions(&tools.pipeline, &read, kept)?,
                        "the terms were imported, but the Instructions changed meanwhile and \
                         stay as they are"
                    );
                    // So the app's Settings, and its other saves of them,
                    // never put the lines back.
                    host.instructions_changed();
                    true
                }
                None => false,
            };
            Ok(InstructionAnswer::Imported {
                series: imported.series.name,
                added: imported.added,
                skipped: imported.skipped,
                removed,
            })
        })
        .await
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct FixTermEverywhereArgs {
    /// The glossary term's source, ignoring case and whitespace.
    term: String,
}

pub(super) struct FixTermEverywhere;

impl Tool for FixTermEverywhere {
    const NAME: &'static str = "booharu_fix_term_everywhere";
    type Args = FixTermEverywhereArgs;
    type Output = FixedTerm;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Translate again every line of the open project whose source uses the glossary term \
         `term`, ignoring case and whitespace, untranslated ones included, so each follows the \
         glossary as it is now, as Fix it everywhere does: one ordinary Translation run on those \
         blocks. Call it after changing a term the chapter was translated with. A line whose \
         translation was changed by hand, by the user or by you, is left alone, and so is a \
         locked line, or one in a locked layer. Answers the `run`, none when no line was left to \
         translate, which you follow with wait, and how many lines it `translated` and left \
         alone as `kept_hand_edited` and `kept_locked`; tell the user about any it left alone."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<FixTermEverywhereArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: FixTermEverywhereArgs,
    ) -> Result<FixedTerm, Self::Error> {
        blocking(context, move |tools, host| {
            let project = tools.session.lock().project();
            context::fix_everywhere(&project, &args.term, |target, stages| {
                let request = Request {
                    target,
                    stages,
                    config: None,
                };
                host.start_run(&tools.pipeline, &tools.session, request)?
                    .ok_or_else(|| anyhow::anyhow!("the project closed"))
            })
        })
        .await
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ExportSeriesArgs {
    /// The series' id or name, ignoring case; the series the chapter is
    /// linked to when left out.
    #[serde(default)]
    series: Option<String>,
    /// The file to write, an absolute path, such as one ending in `.json`.
    path: String,
    /// Whether to replace a file already at `path`.
    #[serde(default)]
    overwrite: bool,
}

/// The file a series was exported to.
#[derive(Debug, Serialize)]
pub(super) struct Exported {
    path: String,
    /// The series' name.
    name: String,
    /// How many terms it has.
    terms: usize,
}

pub(super) struct ExportSeries;

impl Tool for ExportSeries {
    const NAME: &'static str = "booharu_export_series";
    type Args = ExportSeriesArgs;
    type Output = Exported;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Export a series, its glossary, notes, and characters, to a file in Booharu 0.0.4's \
         shape, which Booharu 0.0.4 and this Booharu import: the series with the id or name \
         `series`, ignoring case, or, left out, the series the open chapter is linked to, to \
         `path`, an absolute path. A file already at `path` is replaced only with `overwrite`: \
         ask the user before you set it. Answers the `path`, the series' `name`, and how many \
         `terms` it has."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<ExportSeriesArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: ExportSeriesArgs,
    ) -> Result<Exported, Self::Error> {
        blocking(context, move |tools, host| {
            let series =
                context::find_series(&file(tools), &host.series()?, args.series.as_deref())?;
            let path = PathBuf::from(&args.path);
            context::export_series(&series, &path, args.overwrite)?;
            Ok(Exported {
                path: args.path,
                terms: series.glossary.len(),
                name: series.name,
            })
        })
        .await
    }
}

/// What importing a series of an id already saved does.
#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(super) enum OnConflict {
    /// Save the file's series in place of the saved one.
    Replace,
    /// Save the file's series beside the saved one, with a new id.
    KeepBoth,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ImportSeriesArgs {
    /// The series file, an absolute path.
    path: String,
    /// What to do when a series of the file's id is saved already.
    on_conflict: OnConflict,
}

pub(super) struct ImportSeries;

impl Tool for ImportSeries {
    const NAME: &'static str = "booharu_import_series";
    type Args = ImportSeriesArgs;
    type Output = ImportedSeries;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Import a series from a file at `path`, an absolute path to a Booharu 0.0.4 or 0.0.5 \
         export or series file. When a series of the file's id is saved already, \
         `on_conflict` decides: `replace` saves the file's series in its place, `keep_both` \
         saves it beside it with a new id. A file that isn't a series imports nothing. It \
         links no chapter: booharu_link_series does. Answers the series' `id` as saved, its \
         `name`, how many `terms` it has, and whether one of its file's id `existed`."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<ImportSeriesArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: ImportSeriesArgs,
    ) -> Result<ImportedSeries, Self::Error> {
        let replace = matches!(args.on_conflict, OnConflict::Replace);
        blocking(context, move |_, host| {
            let imported =
                context::import_series(&host.series()?, &PathBuf::from(&args.path), replace)?;
            host.context_changed();
            Ok(imported)
        })
        .await
    }
}

/// Runs `work` with the tools on the open project and the app's host, on a
/// blocking thread, as the files and the host block; an error reaches the
/// model word for word.
pub(super) async fn blocking<T: Send + 'static>(
    context: &ToolContext,
    work: impl FnOnce(&Tools, &dyn BooharuHost) -> anyhow::Result<T> + Send + 'static,
) -> Result<T, ToolExecutionError> {
    let tools = project(context)?;
    tokio::task::spawn_blocking(move || work(&tools, &*host(&tools)))
        .await
        .map_err(failed)?
        .map_err(|error| failed(format!("{error:#}")))
}

/// The open project's file, which its side file sits beside.
pub(super) fn file(tools: &Tools) -> PathBuf {
    tools.session.lock().path().to_owned()
}

/// The app's host, else, outside the app, Booharu's defaults: the series in
/// the Booharu home, the Instructions in the settings file, runs on the
/// tools' own pipeline, and no Read chapter.
pub(super) fn host(tools: &Tools) -> Arc<dyn BooharuHost> {
    tools.booharu.clone().unwrap_or_else(|| Arc::new(Unhosted))
}

/// No app, whose settings the glossary tools never read.
struct Unhosted;

impl BooharuHost for Unhosted {
    fn preferences(&self) -> anyhow::Result<BooharuPreferences> {
        anyhow::bail!("Booharu's settings aren't available here, outside the app")
    }

    fn save_section(&self, _: &str, _: Map<String, Value>) -> anyhow::Result<BooharuPreferences> {
        anyhow::bail!("Booharu's settings aren't available here, outside the app")
    }

    fn font_families(&self) -> anyhow::Result<Vec<String>> {
        anyhow::bail!("Koharu's fonts aren't available here, outside the app")
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet},
        sync::atomic::{AtomicUsize, Ordering},
    };

    use booharu_core::context::{ChapterContext, SeriesStore};
    use koharu_pipeline::{Pipeline, PipelineConfig, RunId, Stage, Target};
    use koharu_renderer::{Renderer, RendererConfig};
    use koharu_scene::{Created, Edit, LayerId, Session};
    use parking_lot::Mutex;

    use super::*;
    use crate::McpServer;

    /// Every glossary and chapter tool, and whether it only reads.
    const TOOLS: [(&str, bool); 13] = [
        ("booharu_get_chapter_context", true),
        ("booharu_link_series", false),
        ("booharu_save_glossary_entry", false),
        ("booharu_delete_glossary_entry", false),
        ("booharu_add_glossary_entry", false),
        ("booharu_accept_glossary_suggestion", false),
        ("booharu_dismiss_glossary_suggestion", false),
        ("booharu_import_instruction_glossary", false),
        ("booharu_fix_term_everywhere", false),
        ("booharu_export_series", false),
        ("booharu_import_series", false),
        ("booharu_read_chapter", false),
        ("booharu_set_chapter_brief", false),
    ];

    /// The app as the glossary tools reach it: its series in a folder of
    /// the test's, its Instructions in memory, the runs it was asked to
    /// start, which it never starts, as a translation would load a model,
    /// how many writes it was told of, and how many saves of the
    /// Instructions.
    struct Glossary {
        series: SeriesStore,
        instructions: Mutex<String>,
        started: Mutex<Vec<(Target, BTreeSet<Stage>)>>,
        told: AtomicUsize,
        instructions_told: AtomicUsize,
    }

    impl BooharuHost for Glossary {
        fn preferences(&self) -> anyhow::Result<BooharuPreferences> {
            anyhow::bail!("nothing here reads settings")
        }

        fn save_section(
            &self,
            _: &str,
            _: Map<String, Value>,
        ) -> anyhow::Result<BooharuPreferences> {
            anyhow::bail!("nothing here saves settings")
        }

        fn font_families(&self) -> anyhow::Result<Vec<String>> {
            Ok(Vec::new())
        }

        fn context_changed(&self) {
            self.told.fetch_add(1, Ordering::Relaxed);
        }

        fn instructions_changed(&self) {
            self.instructions_told.fetch_add(1, Ordering::Relaxed);
        }

        fn series(&self) -> anyhow::Result<SeriesStore> {
            Ok(self.series.clone())
        }

        fn instructions(&self) -> anyhow::Result<String> {
            Ok(self.instructions.lock().clone())
        }

        fn save_instructions(&self, _: &Pipeline, read: &str, kept: &str) -> anyhow::Result<bool> {
            let mut instructions = self.instructions.lock();
            if *instructions != read {
                return Ok(false);
            }
            kept.clone_into(&mut instructions);
            Ok(true)
        }

        fn start_run(
            &self,
            _: &Pipeline,
            _: &Arc<Mutex<Session>>,
            request: Request,
        ) -> anyhow::Result<Option<RunId>> {
            self.started.lock().push((request.target, request.stages));
            Ok(Some(RunId(7)))
        }
    }

    /// A chapter of one page holding the layers some ops add, in the app
    /// [`Glossary`] stands for, its series in the chapter's folder: the
    /// tools on it, the app, and the layers by name.
    struct Chapter {
        dir: tempfile::TempDir,
        tools: Tools,
        app: Arc<Glossary>,
        layers: BTreeMap<String, LayerId>,
    }

    impl Chapter {
        fn new(ops: Vec<Value>) -> Self {
            let dir = tempfile::tempdir().expect("a folder");
            let mut session = Session::create(dir.path().join("Ch.1.khr")).expect("a project");
            let page = json!({"op": "add_page", "id": "$page", "index": 0,
                "size": {"width": 400, "height": 400}});
            let ops = std::iter::once(page)
                .chain(ops)
                .map(|op| serde_json::from_value(op).expect("an op"))
                .collect();
            let change = session
                .apply(Edit {
                    label: "test".to_owned(),
                    ops,
                })
                .expect("the page");
            let layers = change
                .created
                .into_iter()
                .filter_map(|created| match created {
                    Created::Layer {
                        id,
                        label: Some(label),
                    } => Some((label, id)),
                    _ => None,
                })
                .collect();
            let app = Arc::new(Glossary {
                series: SeriesStore::new(dir.path().join("series")),
                instructions: Mutex::default(),
                started: Mutex::default(),
                told: AtomicUsize::new(0),
                instructions_told: AtomicUsize::new(0),
            });
            let tools = Tools::new(
                Arc::new(Mutex::new(session)),
                Pipeline::new(PipelineConfig::default()),
                Renderer::new(RendererConfig::default()),
            )
            .with_booharu(Arc::clone(&app) as Arc<dyn BooharuHost>);
            Self {
                dir,
                tools,
                app,
                layers,
            }
        }

        fn store(&self) -> &SeriesStore {
            &self.app.series
        }

        fn told(&self) -> usize {
            self.app.told.load(Ordering::Relaxed)
        }

        async fn call<T: Tool>(&self, tool: T, args: Value) -> Result<T::Output, T::Error> {
            let args = serde_json::from_value(args).expect("the arguments");
            tool.call(&mut self.tools.tool_context(), args).await
        }
    }

    /// The op adding the block `name`, found and read by Koharu's stages, at
    /// `index` of `parent`, the page when none, reading `source`.
    fn block(name: &str, parent: Option<&str>, index: usize, source: &str) -> Value {
        let x = 10 + 40 * index;
        json!({"op": "add_layer", "id": format!("${name}"), "page": "$page",
        "parent": parent.map(|parent| format!("${parent}")), "index": index,
        "layer": {"kind": "block", "source": {
            "frame": {"box": {"bounds": {"x": x, "y": 10, "width": 30, "height": 100}}},
            "content": source,
        }, "attributes": {"c2pa": {
            "detection": {"action": "c2pa.created"},
            "ocr": {"action": "c2pa.created"},
        }}}})
    }

    #[test]
    fn every_glossary_and_chapter_tool_is_offered_inlined_and_marked_as_it_reads() {
        let definitions = Tools::toolset().tool_definitions();
        for (name, reads) in TOOLS {
            let tool = definitions
                .iter()
                .find(|tool| tool.name.as_str() == name)
                .unwrap_or_else(|| panic!("{name} isn't offered"));
            assert!(!tool.parameters.to_string().contains("$ref"), "{name}");
            let read_only = tool.annotations.read_only_hint == Some(true);
            assert_eq!(read_only, reads, "{name}");
        }
    }

    #[tokio::test]
    async fn adding_a_term_never_replaces_a_series_term_or_a_locked_one() {
        let chapter = Chapter::new(Vec::new());
        let mut series = chapter.store().create("Blue Archive").expect("a series");
        series.glossary = vec![
            GlossaryEntry {
                locked: true,
                ..GlossaryEntry::new("ヒナ", "Hina")
            },
            GlossaryEntry::new("先生", "Sensei"),
        ];
        chapter.store().save(&series).expect("saved");
        let linked = chapter
            .call(LinkSeries, json!({"name": " blue ARCHIVE "}))
            .await;
        assert!(linked.expect("linked").changed);
        let told = chapter.told();
        for source in ["ヒナ", "ヒ ナ", "先 生"] {
            let args = json!({"source": source, "target": "Teacher"});
            let added = chapter
                .call(AddGlossaryEntry, args)
                .await
                .expect("an answer");
            assert!(!added.suggested, "{source}");
        }
        let args = json!({"source": "シャーレ", "target": "Schale", "kind": "place"});
        let added = chapter
            .call(AddGlossaryEntry, args)
            .await
            .expect("an answer");
        assert!(added.suggested);
        let args = json!({"source": "シャーレ", "target": "Shale"});
        let again = chapter
            .call(AddGlossaryEntry, args)
            .await
            .expect("an answer");
        assert!(!again.suggested, "a suggestion it has");
        assert_eq!(chapter.store().get(&series.id).expect("the series"), series);
        let chapter_file = file(&chapter.tools);
        let suggestions = ChapterContext::load(&chapter_file)
            .expect("the side file")
            .suggestions;
        let suggested: Vec<(&str, &str, TermKind, bool)> = suggestions
            .iter()
            .map(|term| (&*term.source, &*term.target, term.kind, term.locked))
            .collect();
        assert_eq!(suggested, [("シャーレ", "Schale", TermKind::Place, false)]);
        assert_eq!(chapter.told() - told, 1, "told of its one write");
    }

    #[tokio::test]
    async fn accepting_a_suggestion_never_replaces_a_locked_term() {
        let chapter = Chapter::new(Vec::new());
        let args = json!({"name": "Blue Archive", "create": true});
        chapter.call(LinkSeries, args).await.expect("linked");
        let args = json!({"source": "先生", "target": "Teacher"});
        let added = chapter
            .call(AddGlossaryEntry, args)
            .await
            .expect("an answer");
        assert!(added.suggested);
        // The user then locks a term of that source.
        let args = json!({"source": "先生", "target": "Sensei", "locked": true});
        chapter.call(SaveGlossaryEntry, args).await.expect("saved");

        let args = json!({"source": " 先 生 "});
        let refused = chapter
            .call(AcceptGlossarySuggestion, args)
            .await
            .expect_err("a locked term");
        assert!(refused.message().contains("is locked"), "{refused}");
        let id = linked_id(&chapter);
        let glossary = chapter.store().get(&id).expect("the series").glossary;
        let kept: Vec<(&str, bool)> = glossary
            .iter()
            .map(|term| (&*term.target, term.locked))
            .collect();
        assert_eq!(kept, [("Sensei", true)]);
        let suggestions = ChapterContext::load(&file(&chapter.tools))
            .expect("the side file")
            .suggestions;
        assert_eq!(suggestions.len(), 1, "the suggestion stays");
    }

    #[tokio::test]
    async fn fixing_a_term_everywhere_skips_hand_fixed_and_locked_lines() {
        let chapter = Chapter::new(vec![
            // Translated by the Translation stage.
            block("auto", None, 0, "ヒナです"),
            json!({"op": "set_attribute", "layer": "$auto", "namespace": "c2pa",
                "key": "translation", "value": {"action": "c2pa.created"}}),
            json!({"op": "set_text", "layer": "$auto", "text": "It's Hina"}),
            // Not translated yet.
            block("untranslated", None, 1, "ひ な と ヒ ナ"),
            // Translated by hand.
            block("typed", None, 2, "ヒナ!"),
            json!({"op": "set_text", "layer": "$typed", "text": "Hina!"}),
            // Locked, and in a locked group.
            block("locked", None, 3, "ヒナ…"),
            json!({"op": "add_layer", "id": "$group", "page": "$page", "index": 4,
                "layer": {"kind": "group"}}),
            block("grouped", Some("group"), 0, "ヒナ。"),
            json!({"op": "update_layer", "layer": "$locked", "patch": {"locked": true}}),
            json!({"op": "update_layer", "layer": "$group", "patch": {"locked": true}}),
        ]);
        let fixed = chapter
            .call(FixTermEverywhere, json!({"term": " ヒ ナ "}))
            .await
            .expect("fixed");
        assert_eq!(
            fixed,
            FixedTerm {
                run: Some(RunId(7)),
                translated: 2,
                kept_hand_edited: 1,
                kept_locked: 2,
            }
        );
        let id = |name: &str| chapter.layers[name];
        assert_eq!(
            *chapter.app.started.lock(),
            [(
                Target::Blocks(vec![id("auto"), id("untranslated")]),
                BTreeSet::from([Stage::Translate])
            )]
        );
        // With no line left to translate, it starts no run.
        let none = chapter
            .call(FixTermEverywhere, json!({"term": "ヒナ…"}))
            .await
            .expect("an answer");
        assert_eq!((none.run, none.kept_locked), (None, 1));
        assert_eq!(chapter.app.started.lock().len(), 1);
    }

    #[tokio::test]
    async fn export_refuses_a_file_there_without_overwrite_and_import_keeps_both() {
        let chapter = Chapter::new(Vec::new());
        let args = json!({"name": "Blue Archive", "create": true});
        let linked = chapter.call(LinkSeries, args).await.expect("linked");
        assert!(linked.created);
        let args = json!({"source": "先生", "target": "Sensei"});
        chapter.call(SaveGlossaryEntry, args).await.expect("saved");
        let file = chapter.dir.path().join("Blue Archive.json");
        let path = file.to_str().expect("a path");
        let exported = chapter
            .call(ExportSeries, json!({"path": path}))
            .await
            .expect("exported");
        assert_eq!(
            (exported.name.as_str(), exported.terms),
            ("Blue Archive", 1)
        );
        let there = chapter
            .call(
                ExportSeries,
                json!({"series": "BLUE ARCHIVE", "path": path}),
            )
            .await
            .expect_err("refused");
        assert!(there.message().contains("is there already"), "{there}");
        let args = json!({"series": linked.id, "path": path, "overwrite": true});
        chapter.call(ExportSeries, args).await.expect("replaced");
        let args = json!({"path": "Blue Archive.json", "overwrite": true});
        let relative = chapter.call(ExportSeries, args).await.expect_err("refused");
        assert!(relative.message().contains("absolute"), "{relative}");

        let told = chapter.told();
        let args = json!({"path": path, "on_conflict": "keep_both"});
        let kept = chapter.call(ImportSeries, args).await.expect("imported");
        assert!(
            kept.existed && kept.id != linked.id && kept.terms == 1,
            "{kept:?}"
        );
        let names: Vec<String> = chapter
            .store()
            .list()
            .expect("the series")
            .into_iter()
            .map(|series| series.name)
            .collect();
        assert_eq!(names, ["Blue Archive", "Blue Archive"]);
        assert_eq!(chapter.told() - told, 1);
        let args = json!({"path": path, "on_conflict": "replace"});
        let replaced = chapter.call(ImportSeries, args).await.expect("imported");
        assert_eq!(replaced.id, linked.id);
        assert_eq!(chapter.store().list().expect("the series").len(), 2);
    }

    #[tokio::test]
    async fn the_context_document_includes_the_linked_series_and_the_found_terms() {
        let chapter = Chapter::new(vec![block("hina", None, 0, "ヒナ先生!")]);
        let outline = chapter.tools.context().expect("the outline");
        assert_eq!(
            outline["booharu"],
            json!({"series": null, "brief": context::NO_BRIEF}),
            "{outline}"
        );
        assert!(outline["pages"].is_array(), "Koharu's outline is kept");
        let args = json!({"name": "Blue Archive", "create": true});
        chapter.call(LinkSeries, args).await.expect("linked");
        for (source, target) in [("先生", "Sensei"), ("リムル", "Rimuru")] {
            let args = json!({"source": source, "target": target, "locked": true});
            chapter.call(SaveGlossaryEntry, args).await.expect("saved");
        }
        let args = json!({"source": "ヒナ", "target": "Hina", "kind": "name"});
        chapter
            .call(AddGlossaryEntry, args)
            .await
            .expect("suggested");
        let outline = chapter.tools.context().expect("the outline");
        assert_eq!(
            outline["booharu"],
            json!({"series": "Blue Archive", "brief": context::NO_BRIEF, "glossary": [
                {"source": "先生", "target": "Sensei", "kind": "other", "locked": true},
                {"source": "ヒナ", "target": "Hina", "kind": "name"},
            ]})
        );
        let read = chapter
            .call(GetChapterContext, json!({}))
            .await
            .expect("the chapter");
        let series = read.series.as_ref().expect("the linked series");
        assert_eq!(
            (series.name.as_str(), series.glossary.len()),
            ("Blue Archive", 2)
        );
        let found: Vec<&str> = read.found.iter().map(|term| &*term.source).collect();
        assert_eq!(found, ["先生", "ヒナ"]);
        assert!(read.extras.contains("- 先生 → Sensei"), "{}", read.extras);
        // A side file that doesn't read says why, and the outline goes on.
        fs_err::write(booharu_core::context::side_file(&file(&chapter.tools)), "{")
            .expect("written");
        let outline = chapter.tools.context().expect("the outline");
        assert!(outline["booharu"]["error"].is_string(), "{outline}");
    }

    #[tokio::test]
    async fn the_instructions_lines_move_into_the_series_once_read() {
        let chapter = Chapter::new(Vec::new());
        let instructions = "Use natural English.\nヒナ=Hina\n先生 = Sensei";
        instructions.clone_into(&mut chapter.app.instructions.lock());
        let args = json!({"name": "Blue Archive", "create": true});
        chapter.call(LinkSeries, args).await.expect("linked");
        let args = json!({"source": "先生", "target": "Teacher"});
        chapter.call(SaveGlossaryEntry, args).await.expect("saved");
        let read = chapter
            .call(ImportInstructionGlossary, json!({"apply": false}))
            .await
            .expect("the terms");
        let version = context::instructions_version(instructions);
        assert_eq!(
            json!(read),
            json!({"series": "Blue Archive", "terms": [
                {"line": 1, "source": "ヒナ", "target": "Hina", "held": false},
                {"line": 2, "source": "先生", "target": "Sensei", "held": true},
            ], "version": version})
        );
        let args = json!({"apply": true, "version": version});
        let refused = chapter.call(ImportInstructionGlossary, args).await;
        assert!(refused.is_err(), "no lines named");
        let args = json!({"apply": true, "lines": [1]});
        let refused = chapter.call(ImportInstructionGlossary, args).await;
        assert!(refused.is_err(), "no version named");

        // The user changes the Instructions in Settings after the agent read
        // the terms: line 1 now writes アコ, which is never imported in ヒナ's
        // place, nor deleted.
        let edited = "Use natural English.\nアコ = Ako";
        edited.clone_into(&mut chapter.app.instructions.lock());
        let args = json!({"apply": true, "lines": [1], "remove": true, "version": version});
        let stale = chapter
            .call(ImportInstructionGlossary, args)
            .await
            .expect_err("changed since they were read");
        assert!(stale.message().contains("read them again"), "{stale}");
        assert_eq!(*chapter.app.instructions.lock(), edited);
        let id = linked_id(&chapter);
        let terms = chapter.store().get(&id).expect("the series").glossary;
        assert_eq!(terms.len(), 1, "nothing imported");
        let instructions_told = || chapter.app.instructions_told.load(Ordering::Relaxed);
        assert_eq!(instructions_told(), 0);

        instructions.clone_into(&mut chapter.app.instructions.lock());
        let args = json!({"apply": true, "lines": [1, 2], "remove": true, "version": version});
        let imported = chapter
            .call(ImportInstructionGlossary, args)
            .await
            .expect("imported");
        assert_eq!(
            json!(imported),
            json!({"series": "Blue Archive", "added": 1, "skipped": 1, "removed": true})
        );
        assert_eq!(*chapter.app.instructions.lock(), "Use natural English.");
        // The app is told the Instructions were saved, so its interface reads
        // them again and its next save of the settings never puts the lines
        // back.
        assert_eq!(instructions_told(), 1);
    }

    /// The id of the series the chapter is linked to.
    fn linked_id(chapter: &Chapter) -> String {
        ChapterContext::load(&file(&chapter.tools))
            .expect("the side file")
            .series
            .expect("a linked series")
    }

    /// An outside app lists Booharu's tools as it reaches Booharu's MCP
    /// server, the series' export and import among them (D32).
    #[tokio::test]
    async fn mcp_lists_the_export_and_import_tools() {
        let chapter = Chapter::new(Vec::new());
        let server = McpServer::start(0).await.expect("a server");
        server.open(Some(chapter.tools.clone()));
        let client = reqwest::Client::new();
        let url = format!("http://{}/mcp", server.address());
        let post = |session: Option<&str>, message: Value| {
            let request = client
                .post(&url)
                .header("Accept", "application/json, text/event-stream")
                .json(&message);
            match session {
                Some(session) => request.header("Mcp-Session-Id", session),
                None => request,
            }
            .send()
        };
        let initialize = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1"},
        }});
        let started = post(None, initialize).await.expect("a session");
        let session = started.headers()["mcp-session-id"]
            .to_str()
            .expect("its id")
            .to_owned();
        let initialized = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        post(Some(&session), initialized)
            .await
            .expect("the session starts");
        let list = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"});
        // The answer comes as a server-sent event.
        let events = post(Some(&session), list)
            .await
            .expect("an answer")
            .text()
            .await
            .expect("events");
        let listed = events
            .lines()
            .find_map(|line| {
                let message = serde_json::from_str::<Value>(line.strip_prefix("data:")?).ok()?;
                (message["id"] == json!(2)).then(|| message["result"]["tools"].clone())
            })
            .unwrap_or_else(|| panic!("no tools in {events}"));
        for name in ["booharu_export_series", "booharu_import_series"] {
            let tool = listed
                .as_array()
                .into_iter()
                .flatten()
                .find(|tool| tool["name"] == json!(name))
                .unwrap_or_else(|| panic!("{name} isn't listed in {listed}"));
            assert_ne!(tool["annotations"]["readOnlyHint"], json!(true), "{name}");
            assert!(
                tool["inputSchema"]["properties"]["path"].is_object(),
                "{tool}"
            );
        }
    }
}
