//! The series glossary and the chapter notes as the user and the agent
//! change them, which the desktop commands and the agent tools share: where
//! a term is used and Fix it everywhere, suggesting a term for a chapter and
//! accepting or dismissing it, linking a chapter to a series, and Import
//! from instructions. booharu-core's `context` holds the files and the text
//! a run sends with its chapter.
//!
//! The agent's tools work by name and source where the interface works by
//! id: the chapter as the agent reads it ([`notes`]) and in short with each
//! of its requests ([`outline`]), a series linked by its name
//! ([`link_named`]), a term saved in or removed from the linked series
//! ([`save_term`], [`delete_term`]), a suggestion accepted or dismissed by
//! its source, the Instructions' terms offered with those the series holds
//! already, and a series exported to or imported from a file it names.
//!
//! Read chapter ([`read`]) reads the whole chapter with a model that sees
//! its pages and writes the brief, suggested terms and each line's label and
//! note; Explain this line ([`explain`]) asks the same model about one line;
//! [`set_brief`] keeps the summary the user wrote.

pub mod explain;
pub mod read;

use std::{collections::BTreeSet, path::Path};

use anyhow::{Context as _, Result, bail, ensure};
use booharu_core::{
    context::{
        Chapter, ChapterBrief, ChapterContext, Extras, GlossaryEntry, MAX_TERMS, Series,
        SeriesStore, TermKind, found_terms, instruction_terms, occurs, same_source, valid_id,
    },
    edited,
};
use koharu_pipeline::{RunId, Stage, Target};
use koharu_scene::{Content, Hash, Layer, LayerId, PageId, Project};
use serde::Serialize;

/// Where a glossary term is used in a project: the blocks whose source it
/// occurs in, ignoring case and whitespace, and which of them Fix it
/// everywhere translates again.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
pub struct GlossaryUsage {
    /// Every block using it, page by page, each page's in the order it holds
    /// them.
    pub blocks: Vec<LayerId>,
    /// Those Fix it everywhere translates again: neither locked nor with a
    /// translation changed by hand, untranslated ones among them.
    pub translate: Vec<LayerId>,
    /// How many have a translation changed by hand, which it leaves alone.
    pub hand_edited: u32,
    /// How many are locked, or in a locked layer, which no stage can write.
    pub locked: u32,
}

/// Where the term `source` is used in `project`. A blank source is used
/// nowhere.
pub fn usage(project: &Project, source: &str) -> GlossaryUsage {
    fn walk(layers: &[Layer], held: bool, source: &str, usage: &mut GlossaryUsage) {
        for layer in layers {
            let locked = held || layer.locked;
            if let Content::Block(block) = &layer.content
                && occurs(&block.source.content.text(), source)
            {
                usage.blocks.push(layer.id);
                if locked {
                    usage.locked += 1;
                } else if edited(layer).text {
                    usage.hand_edited += 1;
                } else {
                    usage.translate.push(layer.id);
                }
            }
            walk(&layer.children, locked, source, usage);
        }
    }
    let mut usage = GlossaryUsage::default();
    for page in project.pages() {
        walk(&page.layers, false, source, &mut usage);
    }
    usage
}

/// What Fix it everywhere did: the run it queued, and how many blocks it
/// translates again and left alone.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
pub struct FixedTerm {
    /// The run translating them again; none when no block was left to.
    pub run: Option<RunId>,
    pub translated: u32,
    /// Blocks whose translation was changed by hand, left alone.
    pub kept_hand_edited: u32,
    /// Blocks locked, or in a locked layer, left alone.
    pub kept_locked: u32,
}

/// Fix it everywhere for the term `source` in `project`: translates again,
/// in one ordinary Translation run on those blocks, which `queue` queues,
/// every block using the term that's neither locked nor hand-edited, so each
/// follows the glossary as it is now; with none left, queues nothing.
pub fn fix_everywhere(
    project: &Project,
    source: &str,
    queue: impl FnOnce(Target, BTreeSet<Stage>) -> Result<RunId>,
) -> Result<FixedTerm> {
    let usage = usage(project, source);
    let run = match usage.translate.is_empty() {
        true => None,
        false => Some(queue(
            Target::Blocks(usage.translate.clone()),
            BTreeSet::from([Stage::Translate]),
        )?),
    };
    Ok(FixedTerm {
        run,
        translated: usage.translate.len() as u32,
        kept_hand_edited: usage.hand_edited,
        kept_locked: usage.locked,
    })
}

/// Links the chapter of the project file `project` to the series `series`
/// in `store`, or to none; a series that isn't there, or doesn't read,
/// fails, linking nothing. Answers whether the link changed.
pub fn link(project: &Path, store: &SeriesStore, series: Option<&str>) -> Result<bool> {
    if let Some(id) = series {
        store.get(id)?;
    }
    ChapterContext::update(project, |chapter| {
        let changed = chapter.series.as_deref() != series;
        chapter.series = series.map(str::to_owned);
        changed
    })
}

/// Suggests `entry` for the chapter of the project file `project`, as Read
/// chapter and the agent do, unlocked, with a new id, trimmed; its
/// translation follows it as a series term until it's accepted or
/// dismissed. Answers false, adding nothing, when the series the chapter
/// links, in `store`, or its suggestions already have a term with its
/// source. A term without a source or a target fails.
pub fn suggest(project: &Path, store: &SeriesStore, entry: GlossaryEntry) -> Result<bool> {
    let entry = suggestion(&entry.source, &entry.target, entry.kind, &entry.note);
    ensure!(
        !entry.source.is_empty() && !entry.target.is_empty(),
        "a term needs a source and a target"
    );
    let series = linked_series(project, store)?;
    ChapterContext::update(project, |chapter| {
        add_suggestion(series.as_ref(), chapter, entry)
    })
}

/// A term as a suggestion for a chapter holds it: unlocked, with a new id,
/// its source, target and note trimmed.
pub(crate) fn suggestion(source: &str, target: &str, kind: TermKind, note: &str) -> GlossaryEntry {
    GlossaryEntry {
        note: note.trim().to_owned(),
        kind,
        ..GlossaryEntry::new(source, target)
    }
}

/// Adds `entry`, as [`suggestion`] makes it, to `chapter`'s suggestions,
/// unless `series`, the series it links, or its suggestions already have a
/// term with its source ([`same_source`]). Answers whether it was added: a
/// suggestion added before counts, so of two with one source the first
/// stays.
pub(crate) fn add_suggestion(
    series: Option<&Series>,
    chapter: &mut ChapterContext,
    entry: GlossaryEntry,
) -> bool {
    let held = series
        .iter()
        .flat_map(|series| &series.glossary)
        .any(|term| same_source(&term.source, &entry.source));
    if held || chapter.suggestion(&entry.source).is_some() {
        return false;
    }
    chapter.suggestions.push(entry);
    true
}

/// The series the chapter of the project file `project` links, in `store`;
/// none when it links none, or the series' file is gone.
pub(crate) fn linked_series(project: &Path, store: &SeriesStore) -> Result<Option<Series>> {
    match ChapterContext::load(project)?.series {
        Some(id) => store.find(&id),
        None => Ok(None),
    }
}

/// Sets the brief's summary of the chapter of the project file `project` to
/// `summary`, trimmed, as the user wrote it, so reading the chapter again
/// keeps it: makes the brief when there is none, and keeps its characters
/// and when and with which model it was read, in one write of the side
/// file. A blank summary counts as no brief for a Run press, which reads the
/// chapter again. Answers whether it changed.
pub fn set_brief(project: &Path, summary: &str) -> Result<bool> {
    let summary = summary.trim();
    ChapterContext::update(project, |chapter| {
        let brief = chapter.brief.get_or_insert_default();
        let changed = brief.summary != summary || !brief.edited;
        brief.summary = summary.to_owned();
        brief.edited = true;
        changed
    })
}

/// Accepts the suggestion of the chapter of the project file `project`
/// that `entry` names by its id into the series the chapter links, in
/// `store`, as `entry` holds it, the user's changes included, trimmed: in
/// place of the series' term with its source, keeping that term's place and
/// id, or after the others ([`Series::put`]). A locked term with its source,
/// one the user fixed, is replaced only with `replace_locked`, the user's
/// choice in the term form, which says what saving replaces; else it fails,
/// naming that term, as Booharu 0.0.4 never let a suggestion over a term.
/// The suggestion then leaves the chapter. Fails, changing nothing, when the
/// chapter has no such suggestion or links no series, the series is gone, or
/// the term lacks a source or a target. Answers the series as saved.
pub fn accept(
    project: &Path,
    store: &SeriesStore,
    entry: GlossaryEntry,
    replace_locked: bool,
) -> Result<Series> {
    let chapter = ChapterContext::load(project)?;
    ensure!(
        chapter.suggestions.iter().any(|term| term.id == entry.id),
        "the chapter has no suggested term {}; it was accepted or dismissed",
        entry.id
    );
    let id = chapter
        .series
        .context("the chapter is linked to no series; pick one first")?;
    let suggestion = entry.id.clone();
    // As the term form wrote it, trimmed, as every term is saved.
    let entry = GlossaryEntry {
        source: entry.source.trim().to_owned(),
        target: entry.target.trim().to_owned(),
        note: entry.note.trim().to_owned(),
        ..entry
    };
    let series = store.update(&id, |series| {
        if !replace_locked && let Some(held) = series.term(&entry.source).filter(|term| term.locked)
        {
            bail!(
                "the series' term \"{}\" → \"{}\" is locked, so the suggestion \"{}\" → \"{}\" \
                 doesn't replace it; edit the suggestion to save it over that term, or dismiss it",
                held.source,
                held.target,
                entry.source,
                entry.target
            );
        }
        series.put(entry);
        Ok(series.clone())
    })??;
    dismiss(project, &suggestion)?;
    Ok(series)
}

/// Dismisses the suggestion `id` of the chapter of the project file
/// `project`. Answers whether it had one.
pub fn dismiss(project: &Path, id: &str) -> Result<bool> {
    ChapterContext::update(project, |chapter| {
        let before = chapter.suggestions.len();
        chapter.suggestions.retain(|term| term.id != id);
        chapter.suggestions.len() != before
    })
}

/// What Import from instructions did.
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
pub struct InstructionImport {
    /// The series as saved.
    pub series: Series,
    /// How many terms it added.
    pub added: u32,
    /// How many it skipped, as the series already had a term with their
    /// source.
    pub skipped: u32,
    /// The Instructions without the imported terms' lines, which the caller
    /// saves; none when they stay as they are.
    pub instructions: Option<String>,
}

/// Imports the terms on the lines `lines`, counted from 0, of
/// `instructions`, the Instructions as the user previewed them, into the
/// series `series` in `store`: each a locked term of kind Term after the
/// others, skipping one whose source the series already has, as Booharu
/// 0.0.4 did; a line without a term is passed over. With `remove`, answers
/// the Instructions without those terms' lines, skipped ones included, as
/// the series holds them. Fails, changing nothing, when `saved`, the
/// Instructions as saved now, are no longer what was previewed.
pub fn import_instructions(
    store: &SeriesStore,
    series: &str,
    instructions: &str,
    saved: &str,
    lines: &[u32],
    remove: bool,
) -> Result<InstructionImport> {
    ensure!(
        saved == instructions,
        "the Instructions changed since they were read; read them again"
    );
    let terms: Vec<_> = instruction_terms(instructions)
        .into_iter()
        .filter(|term| lines.contains(&term.line))
        .collect();
    let (added, series) = store.update(series, |series| {
        let mut added = 0;
        for term in &terms {
            if series
                .glossary
                .iter()
                .any(|held| same_source(&held.source, &term.source))
            {
                continue;
            }
            series.glossary.push(GlossaryEntry {
                kind: TermKind::Term,
                locked: true,
                ..GlossaryEntry::new(&term.source, &term.target)
            });
            added += 1;
        }
        (added, series.clone())
    })?;
    let removed: BTreeSet<u32> = terms.iter().map(|term| term.line).collect();
    let instructions = (remove && !removed.is_empty()).then(|| {
        instructions
            .lines()
            .enumerate()
            .filter(|(index, _)| !removed.contains(&(*index as u32)))
            .map(|(_, line)| line)
            .collect::<Vec<_>>()
            .join("\n")
    });
    Ok(InstructionImport {
        series,
        added,
        skipped: terms.len() as u32 - added,
        instructions,
    })
}

/// The chapter as the agent reads it: the series it links, its suggested
/// terms and brief, the terms found in its text, and what a translation run
/// on it sends.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ChapterNotes {
    /// The series the chapter links, whole: its name, notes, characters, and
    /// glossary; none when it links none, or its file is gone.
    pub series: Option<Series>,
    /// Whether the chapter links a series whose file is gone.
    pub missing: bool,
    /// The terms suggested for the chapter, which its translation follows
    /// as the series' terms until they're accepted or dismissed.
    pub suggestions: Vec<GlossaryEntry>,
    /// What Read chapter found the chapter to be about.
    pub brief: Option<ChapterBrief>,
    /// The series' terms and the suggestions whose source occurs in the
    /// source of a block, as far as the text is known ([`found_terms`]).
    pub found: Vec<GlossaryEntry>,
    /// What a run translating the chapter now adds after the Instructions,
    /// word for word; empty when it adds nothing.
    pub extras: String,
    /// Whether the chapter's text isn't known yet, as a page was never
    /// through detection or a block waits to be read, so that run sends
    /// every term rather than those found.
    pub fresh: bool,
    /// How many terms past the first 60 that run leaves out.
    pub left_out: u32,
}

/// The chapter of the project file `path`, whose project is `project`, as
/// the agent reads it, with the series it links from the store `store`
/// opens, which only a linked series needs. A side file or series file that
/// doesn't read fails, naming it.
pub fn notes(
    path: &Path,
    project: &Project,
    store: impl FnOnce() -> Result<SeriesStore>,
) -> Result<ChapterNotes> {
    let Chapter {
        context,
        series,
        missing,
    } = Chapter::read(path, store)?;
    let extras = Extras::of(project, &context, series.as_ref(), false);
    Ok(ChapterNotes {
        found: found_terms(project, &context, series.as_ref()),
        extras: extras.text(),
        fresh: extras.fresh,
        left_out: extras.left_out,
        series,
        missing,
        suggestions: context.suggestions,
        brief: context.brief,
    })
}

/// What the agent's outline says in place of the brief while the chapter
/// has text but no brief, as the runs the agent and outside apps start never
/// read the chapter on their own.
pub const NO_BRIEF: &str = "No chapter brief yet: call booharu_read_chapter before translating.";

/// The chapter in short, which goes with each of the agent's requests: the
/// name of the series it links, its brief, and the glossary terms found in
/// its text.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ChapterOutline {
    /// None when the chapter links no series, or its file is gone.
    pub series: Option<String>,
    /// Whether the chapter links a series whose file is gone.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub missing: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brief: Option<OutlinedBrief>,
    /// The terms found in the chapter's text ([`found_terms`]), at most 60.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub glossary: Vec<OutlinedTerm>,
    /// How many terms found past the first 60 were left out.
    #[serde(skip_serializing_if = "is_zero")]
    pub more: u32,
}

/// The brief as the agent's outline gives it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum OutlinedBrief {
    /// The brief, with when the chapter was read and with which model.
    Brief(ChapterBrief),
    /// [`NO_BRIEF`], while the chapter has text but no brief: none, or one
    /// whose summary is blank, which a Run press reads again.
    Unread(&'static str),
}

/// A glossary term as the agent's outline lists it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OutlinedTerm {
    pub source: String,
    pub target: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
    pub kind: TermKind,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub locked: bool,
}

/// The chapter of the project file `path`, whose project is `project`, in
/// short, with the series it links from the store `store` opens, which only
/// a linked series needs: while the chapter has text but no brief, its brief
/// is [`NO_BRIEF`]. A side file or series file that doesn't read fails,
/// naming it.
pub fn outline(
    path: &Path,
    project: &Project,
    store: impl FnOnce() -> Result<SeriesStore>,
) -> Result<ChapterOutline> {
    let Chapter {
        context,
        series,
        missing,
    } = Chapter::read(path, store)?;
    let found = found_terms(project, &context, series.as_ref());
    let pages: Vec<PageId> = project.pages().map(|page| page.id).collect();
    let unread = crate::runs::brief_missing(&context) && crate::runs::has_text(project, &pages);
    Ok(ChapterOutline {
        more: found.len().saturating_sub(MAX_TERMS) as u32,
        glossary: found
            .into_iter()
            .take(MAX_TERMS)
            .map(|term| OutlinedTerm {
                source: term.source,
                target: term.target,
                note: term.note,
                kind: term.kind,
                locked: term.locked,
            })
            .collect(),
        series: series.map(|series| series.name),
        missing,
        brief: match unread {
            true => Some(OutlinedBrief::Unread(NO_BRIEF)),
            false => context.brief.map(OutlinedBrief::Brief),
        },
    })
}

fn is_zero(count: &u32) -> bool {
    *count == 0
}

/// What linking a chapter to a series by its name did.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LinkedSeries {
    pub id: String,
    pub name: String,
    /// Whether the series was made for the link.
    pub created: bool,
    /// Whether the link changed: false when the chapter linked it already.
    pub changed: bool,
}

/// Links the chapter of the project file `project` to the series in `store`
/// named `name`, ignoring case and the whitespace around it; with `create`,
/// makes one so named when there's none. Fails, linking nothing, when
/// there's none and `create` is off, naming the series there are, or when
/// two or more have the name.
pub fn link_named(
    project: &Path,
    store: &SeriesStore,
    name: &str,
    create: bool,
) -> Result<LinkedSeries> {
    let (series, created) = match named(store, name)? {
        Some(series) => (series, false),
        None if create => (store.create(name)?, true),
        None => {
            let names: Vec<String> = store
                .list()?
                .into_iter()
                .map(|series| format!("\"{}\"", series.name))
                .collect();
            let there = match names.is_empty() {
                true => "there are none yet".to_owned(),
                false => format!("the series are {}", names.join(", ")),
            };
            bail!("there's no series named \"{}\"; {there}", name.trim());
        }
    };
    let changed = link(project, store, Some(&series.id))?;
    Ok(LinkedSeries {
        id: series.id,
        name: series.name,
        created,
        changed,
    })
}

/// The series in `store` with the id or the name `named`, a name ignoring
/// case and the whitespace around it; with none named, the series the
/// chapter of the project file `project` links. Fails when there's no such
/// series, two or more have the name, or the chapter links none.
pub fn find_series(project: &Path, store: &SeriesStore, named: Option<&str>) -> Result<Series> {
    let Some(name) = named else {
        return store.get(&linked(project)?);
    };
    if valid_id(name.trim())
        && let Some(series) = store.find(name.trim())?
    {
        return Ok(series);
    }
    self::named(store, name)?
        .with_context(|| format!("there's no series named \"{}\"", name.trim()))
}

/// The series in `store` named `name`, ignoring case and the whitespace
/// around it; none when there's none. Two or more so named fail, naming
/// their ids.
fn named(store: &SeriesStore, name: &str) -> Result<Option<Series>> {
    let wanted = name.trim().to_lowercase();
    ensure!(!wanted.is_empty(), "a series needs a name");
    let mut found: Vec<Series> = store
        .list()?
        .into_iter()
        .filter(|series| series.name.trim().to_lowercase() == wanted)
        .collect();
    if found.len() > 1 {
        let ids: Vec<&str> = found.iter().map(|series| series.id.as_str()).collect();
        bail!(
            "{} series are named \"{}\" ({}); rename one in the glossary editor first",
            found.len(),
            name.trim(),
            ids.join(", ")
        );
    }
    Ok(found.pop())
}

/// The id of the series the chapter of the project file `project` links;
/// fails when it links none.
fn linked(project: &Path) -> Result<String> {
    ChapterContext::load(project)?
        .series
        .context("the chapter is linked to no series; link one first")
}

/// A term as a caller saves it in a series: what it leaves out, a term the
/// series has with its source keeps, and a new one takes as
/// [`GlossaryEntry::new`] gives it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TermEdit {
    pub source: String,
    pub target: String,
    pub kind: Option<TermKind>,
    pub note: Option<String>,
    pub locked: Option<bool>,
}

/// What saving a term in a series did.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SavedTerm {
    /// The series' name.
    pub series: String,
    /// The term as saved.
    pub term: GlossaryEntry,
    /// Whether it took the place of a term with its source.
    pub replaced: bool,
}

/// Saves `edit` in the series the chapter of the project file `project`
/// links, in `store`: in place of its term with the same source, ignoring
/// case and whitespace, keeping that term's place and id and whatever `edit`
/// leaves out, and dropping any other with that source ([`Series::put`]);
/// else after the others. Fails, saving nothing, when the chapter links no
/// series, its file is gone, or the term lacks a source or a target.
pub fn save_term(project: &Path, store: &SeriesStore, edit: TermEdit) -> Result<SavedTerm> {
    let id = linked(project)?;
    let entry = GlossaryEntry::new(&edit.source, &edit.target);
    ensure!(
        !entry.source.is_empty() && !entry.target.is_empty(),
        "a term needs a source and a target"
    );
    store.update(&id, |series| {
        let held = series.term(&entry.source).cloned();
        let source = entry.source.clone();
        let term = GlossaryEntry {
            kind: edit
                .kind
                .or(held.as_ref().map(|term| term.kind))
                .unwrap_or_default(),
            note: edit
                .note
                .map(|note| note.trim().to_owned())
                .or_else(|| held.as_ref().map(|term| term.note.clone()))
                .unwrap_or_default(),
            locked: edit
                .locked
                .or(held.as_ref().map(|term| term.locked))
                .unwrap_or_default(),
            ..entry
        };
        series.put(term);
        SavedTerm {
            series: series.name.clone(),
            term: series.term(&source).cloned().unwrap_or_default(),
            replaced: held.is_some(),
        }
    })
}

/// Removes every term with the source `source`, ignoring case and
/// whitespace, from the series the chapter of the project file `project`
/// links, in `store`, and answers how many it removed. Fails when the
/// chapter links no series, or its file is gone.
pub fn delete_term(project: &Path, store: &SeriesStore, source: &str) -> Result<usize> {
    store.update(&linked(project)?, |series| series.remove_term(source))
}

/// Accepts the suggestion of the chapter of the project file `project` with
/// the source `source`, ignoring case and whitespace, as it holds it, into
/// the series the chapter links, in `store`, as [`accept`] does, never over
/// a locked term. Answers the series' term as saved.
pub fn accept_source(project: &Path, store: &SeriesStore, source: &str) -> Result<SavedTerm> {
    let entry = ChapterContext::load(project)?
        .suggestion(source)
        .cloned()
        .with_context(|| format!("the chapter has no suggested term \"{}\"", source.trim()))?;
    let source = entry.source.clone();
    let replaced = store.get(&linked(project)?)?.term(&source).is_some();
    let series = accept(project, store, entry, false)?;
    Ok(SavedTerm {
        term: series.term(&source).cloned().unwrap_or_default(),
        series: series.name,
        replaced,
    })
}

/// Dismisses every suggestion of the chapter of the project file `project`
/// with the source `source`, ignoring case and whitespace, and answers how
/// many it dismissed.
pub fn dismiss_source(project: &Path, source: &str) -> Result<usize> {
    ChapterContext::update(project, |chapter| {
        let before = chapter.suggestions.len();
        chapter
            .suggestions
            .retain(|term| !same_source(&term.source, source));
        before - chapter.suggestions.len()
    })
}

/// The terms the lines of the Instructions write, as Import from
/// instructions offers them to the agent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InstructionOffer {
    /// The name of the series the chapter links, which they'd go into; none
    /// when it links none, or its file is gone.
    pub series: Option<String>,
    pub terms: Vec<OfferedTerm>,
    /// The version of the Instructions the terms were read from
    /// ([`instructions_version`]), which importing them names, so it never
    /// imports the lines of Instructions changed since.
    pub version: String,
}

/// A term a line of the Instructions writes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OfferedTerm {
    /// The line, counted from 0.
    pub line: u32,
    pub source: String,
    pub target: String,
    /// Whether the linked series has a term with its source already, which
    /// importing it skips.
    pub held: bool,
}

/// The terms the lines of `instructions` write, each with whether the
/// series the chapter of the project file `project` links, in `store`, has
/// one with its source already.
pub fn offer_instructions(
    project: &Path,
    store: &SeriesStore,
    instructions: &str,
) -> Result<InstructionOffer> {
    let series = match ChapterContext::load(project)?.series {
        Some(id) => store.find(&id)?,
        None => None,
    };
    let terms = instruction_terms(instructions)
        .into_iter()
        .map(|term| OfferedTerm {
            held: series
                .as_ref()
                .is_some_and(|series| series.term(&term.source).is_some()),
            line: term.line,
            source: term.source,
            target: term.target,
        })
        .collect();
    Ok(InstructionOffer {
        series: series.map(|series| series.name),
        terms,
        version: instructions_version(instructions),
    })
}

/// The version of `instructions`, whole: the first 8 bytes of its BLAKE3
/// hash as 16 hex digits, the same in every build. Instructions changed in
/// any way, even a line moved, have another.
pub fn instructions_version(instructions: &str) -> String {
    Hash::of(instructions.as_bytes()).as_bytes()[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Imports the terms on the lines `lines` of `instructions`, the
/// Instructions as saved now, into the series the chapter of the project
/// file `project` links, in `store`, as [`import_instructions`] does, while
/// they're still the `version` the lines were read from
/// ([`offer_instructions`]): Instructions changed since fail, importing
/// nothing, as a line's number may name another term now. The caller saves
/// the Instructions it answers only while they're still `instructions`.
pub fn import_linked_instructions(
    project: &Path,
    store: &SeriesStore,
    instructions: &str,
    version: &str,
    lines: &[u32],
    remove: bool,
) -> Result<InstructionImport> {
    ensure!(
        instructions_version(instructions) == version.trim(),
        "the Instructions changed since they were read; read them again"
    );
    let id = linked(project)?;
    import_instructions(store, &id, instructions, instructions, lines, remove)
}

/// Writes `series` to the file `path`, an absolute path, as an export in
/// Booharu 0.0.4's shape, which 0.0.4 imports too. A file already there is
/// replaced only with `overwrite`.
pub fn export_series(series: &Series, path: &Path, overwrite: bool) -> Result<()> {
    ensure!(
        path.is_absolute(),
        "\"{}\" isn't an absolute path",
        path.display()
    );
    ensure!(
        overwrite || !path.exists(),
        "{} is there already",
        path.display()
    );
    series.write_file(path)
}

/// What importing a series file did.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ImportedSeries {
    /// The series' id as saved: a new one when it was kept beside the
    /// series of its file's id.
    pub id: String,
    pub name: String,
    /// How many terms it has.
    pub terms: u32,
    /// Whether a series of its file's id was saved already, which the import
    /// replaced or kept beside it.
    pub existed: bool,
}

/// Imports the series file `path`, an absolute path to a Booharu 0.0.4 or
/// 0.0.5 export or series file, into `store`: in place of the series of its
/// id with `replace`, else, when that id is taken, beside it with a new id.
/// A file that isn't a series fails, importing nothing.
pub fn import_series(store: &SeriesStore, path: &Path, replace: bool) -> Result<ImportedSeries> {
    ensure!(
        path.is_absolute(),
        "\"{}\" isn't an absolute path",
        path.display()
    );
    let series = Series::read_file(path)?;
    let existed = store.exists(&series.id)?;
    let series = store.import(series, replace)?;
    Ok(ImportedSeries {
        terms: series.glossary.len() as u32,
        id: series.id,
        name: series.name,
        existed,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use koharu_scene::{Created, Edit, Session};
    use serde_json::{Value, json};

    use super::*;

    /// A project of one page with the layers `ops` add, by their names.
    struct Chapter {
        dir: tempfile::TempDir,
        session: Session,
        layers: BTreeMap<String, LayerId>,
    }

    impl Chapter {
        fn new(ops: Vec<Value>) -> Result<Self> {
            let dir = tempfile::tempdir()?;
            let mut session = Session::create(dir.path().join("Ch.1.khr"))?;
            let page = json!({"op": "add_page", "id": "$page", "index": 0,
                "size": {"width": 400, "height": 400}});
            let change = session.apply(Edit {
                label: "test".to_owned(),
                ops: std::iter::once(page)
                    .chain(ops)
                    .map(serde_json::from_value)
                    .collect::<Result<_, _>>()?,
            })?;
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
            Ok(Self {
                dir,
                session,
                layers,
            })
        }

        fn path(&self) -> &Path {
            self.session.path()
        }

        fn store(&self) -> SeriesStore {
            SeriesStore::new(self.dir.path().join("series"))
        }
    }

    /// The op adding the block `name`, detected and read, at `index` of
    /// `parent` (the page when none), reading `source`.
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
    fn fix_it_everywhere_translates_again_all_but_hand_edited_and_locked_blocks() -> Result<()> {
        let mut ops = vec![
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
            block("locked", None, 3, "ヒナ?"),
            json!({"op": "add_layer", "id": "$group", "page": "$page", "index": 4,
                "layer": {"kind": "group"}}),
            block("grouped", Some("group"), 0, "ヒナ…"),
            // Without the term.
            block("other", None, 5, "先生"),
        ];
        ops.extend([
            json!({"op": "update_layer", "layer": "$locked", "patch": {"locked": true}}),
            json!({"op": "update_layer", "layer": "$group", "patch": {"locked": true}}),
        ]);
        let chapter = Chapter::new(ops)?;
        let id = |name: &str| chapter.layers[name];
        let project = chapter.session.project();

        let found = usage(&project, " ヒ ナ ");
        assert_eq!(found.blocks.len(), 5);
        assert!(!found.blocks.contains(&id("other")));
        assert_eq!(found.translate, [id("auto"), id("untranslated")]);
        assert_eq!((found.hand_edited, found.locked), (1, 2));

        let mut queued = None;
        let fixed = fix_everywhere(&project, "ヒナ", |target, stages| {
            queued = Some((target, stages));
            Ok(RunId(7))
        })?;
        assert_eq!(
            fixed,
            FixedTerm {
                run: Some(RunId(7)),
                translated: 2,
                kept_hand_edited: 1,
                kept_locked: 2,
            }
        );
        let (target, stages) = queued.context("a run")?;
        assert!(
            matches!(&target, Target::Blocks(blocks) if *blocks == [id("auto"), id("untranslated")])
        );
        assert_eq!(stages, BTreeSet::from([Stage::Translate]));

        // With no block left to translate, no run.
        let fixed = fix_everywhere(&project, "ヒナ?", |_, _| anyhow::bail!("no run"))?;
        assert_eq!((fixed.run, fixed.kept_locked), (None, 1));
        assert_eq!(usage(&project, "  "), GlossaryUsage::default());
        Ok(())
    }

    #[test]
    fn fix_it_everywhere_translates_a_box_styled_before_its_translated() -> Result<()> {
        let mut chapter = Chapter::new(vec![block("styled", None, 0, "ヒナです")])?;
        let styled = chapter.layers["styled"];
        // A style picked before translating leaves a placeholder, no
        // translation changed by hand.
        chapter.session.apply(Edit {
            label: "test".to_owned(),
            ops: vec![crate::testing::type_bar_style(styled)],
        })?;
        let found = usage(&chapter.session.project(), "ヒナ");
        assert_eq!(found.translate, [styled]);
        assert_eq!((found.hand_edited, found.locked), (0, 0));
        Ok(())
    }

    #[test]
    fn an_accepted_suggestion_replaces_the_series_term_with_its_source() -> Result<()> {
        let chapter = Chapter::new(Vec::new())?;
        let (path, store) = (chapter.path(), chapter.store());
        let mut series = store.create("Blue Archive")?;
        series.glossary = vec![
            GlossaryEntry::new("先生", "Teacher"),
            GlossaryEntry::new("シャーレ", "Schale"),
        ];
        store.save(&series)?;
        let held = series.glossary[0].id.clone();

        // A suggestion needs a linked series to go into.
        assert!(suggest(
            path,
            &store,
            GlossaryEntry::new(" 先 生 ", "Sensei")
        )?);
        let suggestion = ChapterContext::load(path)?.suggestions[0].clone();
        assert_eq!(suggestion.source, "先 生");
        assert!(accept(path, &store, suggestion.clone(), false).is_err());
        assert!(link(path, &store, Some("blue-archive-000000000000")).is_err());
        assert!(link(path, &store, Some(&series.id))?);
        assert!(!link(path, &store, Some(&series.id))?);

        // Its source is the series' first term's: it takes that term's place,
        // as the user edited it, trimmed.
        let edited = GlossaryEntry {
            target: " Sensei ".to_owned(),
            note: " the teacher\n".to_owned(),
            locked: true,
            ..suggestion
        };
        let saved = accept(path, &store, edited.clone(), false)?;
        assert_eq!(saved, store.get(&series.id)?);
        assert_eq!(saved.glossary.len(), 2);
        assert_eq!(
            saved.glossary[0],
            GlossaryEntry {
                id: held,
                target: "Sensei".to_owned(),
                note: "the teacher".to_owned(),
                ..edited.clone()
            }
        );
        assert!(ChapterContext::load(path)?.suggestions.is_empty());
        // Accepted once, it's gone.
        assert!(accept(path, &store, edited, true).is_err());
        // A term the series has is no suggestion.
        assert!(!suggest(
            path,
            &store,
            GlossaryEntry::new("シャーレ", "Shale")
        )?);
        assert!(suggest(path, &store, GlossaryEntry::new("ヒナ", "")).is_err());
        assert!(link(path, &store, None)?);
        Ok(())
    }

    #[test]
    fn a_suggestion_replaces_a_locked_term_only_when_the_user_chose_to() -> Result<()> {
        let chapter = Chapter::new(Vec::new())?;
        let (path, store) = (chapter.path(), chapter.store());
        let series = store.create("Blue Archive")?;
        link(path, &store, Some(&series.id))?;
        // Suggested before the user saved a locked term of its source.
        assert!(suggest(
            path,
            &store,
            GlossaryEntry::new("先生", "Teacher")
        )?);
        let locked = GlossaryEntry {
            locked: true,
            note: "a title".to_owned(),
            ..GlossaryEntry::new("先 生", "Sensei")
        };
        store.update(&series.id, |series| series.glossary.push(locked.clone()))?;
        let suggestion = ChapterContext::load(path)?.suggestions[0].clone();

        // Neither the agent nor a click on Accept puts it over that term.
        let refused = accept_source(path, &store, "先生").expect_err("a locked term");
        assert!(
            format!("{refused:#}").contains("\"先 生\" → \"Sensei\" is locked"),
            "{refused:#}"
        );
        assert!(accept(path, &store, suggestion.clone(), false).is_err());
        assert_eq!(
            store.get(&series.id)?.glossary,
            std::slice::from_ref(&locked)
        );
        assert_eq!(
            ChapterContext::load(path)?.suggestions,
            std::slice::from_ref(&suggestion),
            "the suggestion stays"
        );

        // The user's choice in the term form, which says what it replaces,
        // does.
        let saved = accept(path, &store, suggestion.clone(), true)?;
        assert_eq!(
            saved.glossary,
            [GlossaryEntry {
                id: locked.id,
                ..suggestion
            }]
        );
        assert!(ChapterContext::load(path)?.suggestions.is_empty());
        Ok(())
    }

    #[test]
    fn a_dismissed_suggestion_leaves_the_chapter() -> Result<()> {
        let chapter = Chapter::new(Vec::new())?;
        let (path, store) = (chapter.path(), chapter.store());
        assert!(suggest(path, &store, GlossaryEntry::new("ヒナ", "Hina"))?);
        assert!(!suggest(
            path,
            &store,
            GlossaryEntry::new("ヒ ナ", "Hinata")
        )?);
        let id = ChapterContext::load(path)?.suggestions[0].id.clone();
        assert!(dismiss(path, &id)?);
        assert!(!dismiss(path, &id)?);
        assert!(ChapterContext::load(path)?.suggestions.is_empty());
        Ok(())
    }

    #[test]
    fn importing_from_instructions_adds_locked_terms_and_can_remove_their_lines() -> Result<()> {
        let chapter = Chapter::new(Vec::new())?;
        let store = chapter.store();
        let mut series = store.create("Blue Archive")?;
        series.glossary.push(GlossaryEntry::new("魔王", "Overlord"));
        store.save(&series)?;
        let instructions = "Use natural English.\nヒナ=Hina\n魔王 = Demon King\n村 → Village\n\
                            町: Town";

        // Instructions changed since the preview write nothing.
        let changed = format!("{instructions}\nKeep honorifics.");
        assert!(
            import_instructions(&store, &series.id, instructions, &changed, &[1], true).is_err()
        );
        assert_eq!(store.get(&series.id)?, series);

        let imported = import_instructions(
            &store,
            &series.id,
            instructions,
            instructions,
            &[0, 1, 2, 3],
            true,
        )?;
        assert_eq!((imported.added, imported.skipped), (2, 1));
        assert_eq!(imported.series, store.get(&series.id)?);
        let terms: Vec<(&str, &str, bool, TermKind)> = imported
            .series
            .glossary
            .iter()
            .map(|term| (&*term.source, &*term.target, term.locked, term.kind))
            .collect();
        assert_eq!(
            terms,
            [
                ("魔王", "Overlord", false, TermKind::Other),
                ("ヒナ", "Hina", true, TermKind::Term),
                ("村", "Village", true, TermKind::Term),
            ]
        );
        assert_eq!(
            imported.instructions.as_deref(),
            Some("Use natural English.\n町: Town")
        );
        // Kept, the Instructions stay as they are.
        let kept =
            import_instructions(&store, &series.id, instructions, instructions, &[4], false)?;
        assert_eq!((kept.added, kept.instructions), (1, None));
        Ok(())
    }

    #[test]
    fn the_agent_reads_the_chapter_whole_and_in_short() -> Result<()> {
        let chapter = Chapter::new(vec![block("hina", None, 0, "ヒナ先生!")])?;
        let (path, store) = (chapter.path(), chapter.store());
        let project = chapter.session.project();
        let opened = || Ok(chapter.store());
        let none = notes(path, &project, opened)?;
        assert!(none.series.is_none() && none.found.is_empty() && none.extras.is_empty());
        assert_eq!(
            json!(outline(path, &project, opened)?),
            json!({"series": null, "brief": NO_BRIEF})
        );

        let mut series = store.create("Blue Archive")?;
        series.notes = "Students of Kivotos.".to_owned();
        series.glossary = vec![
            GlossaryEntry {
                locked: true,
                note: "a title".to_owned(),
                ..GlossaryEntry::new("先生", "Sensei")
            },
            GlossaryEntry::new("リムル", "Rimuru"),
        ];
        store.save(&series)?;
        link(path, &store, Some(&series.id))?;
        suggest(path, &store, GlossaryEntry::new("ヒナ", "Hina"))?;
        let read = notes(path, &project, opened)?;
        assert_eq!(read.series.as_ref(), Some(&series));
        let found: Vec<&str> = read.found.iter().map(|term| &*term.source).collect();
        assert_eq!(found, ["先生", "ヒナ"]);
        assert_eq!(read.suggestions.len(), 1);
        assert!(!read.fresh);
        assert!(
            read.extras.contains("- 先生 → Sensei (a title)")
                && read.extras.contains("Students of Kivotos.")
                && !read.extras.contains("Rimuru"),
            "{}",
            read.extras
        );
        assert_eq!(
            json!(outline(path, &project, opened)?),
            json!({"series": "Blue Archive", "brief": NO_BRIEF, "glossary": [
                {"source": "先生", "target": "Sensei", "note": "a title", "kind": "other",
                    "locked": true},
                {"source": "ヒナ", "target": "Hina", "kind": "other"},
            ]})
        );
        // A series whose file is gone.
        store.delete(&series.id)?;
        let gone = outline(path, &project, opened)?;
        assert!(gone.missing && gone.series.is_none());
        Ok(())
    }

    #[test]
    fn a_series_is_linked_by_its_name_or_made() -> Result<()> {
        let chapter = Chapter::new(Vec::new())?;
        let (path, store) = (chapter.path(), chapter.store());
        let none = link_named(path, &store, "Blue Archive", false).expect_err("none");
        assert!(
            format!("{none:#}").contains("there are none yet"),
            "{none:#}"
        );
        let made = link_named(path, &store, " Blue Archive ", true)?;
        assert!(made.created && made.changed && made.name == "Blue Archive");
        let again = link_named(path, &store, "blue archive", true)?;
        assert_eq!(
            (again.id.as_str(), again.created, again.changed),
            (&*made.id, false, false)
        );
        assert_eq!(ChapterContext::load(path)?.series, Some(made.id.clone()));
        let other = link_named(path, &store, "Slime", false).expect_err("none");
        assert!(
            format!("{other:#}").contains("\"Blue Archive\""),
            "{other:#}"
        );
        // By id, by name, or the linked one.
        let series = store.get(&made.id)?;
        assert_eq!(find_series(path, &store, None)?, series);
        assert_eq!(find_series(path, &store, Some(&made.id))?, series);
        assert_eq!(find_series(path, &store, Some("BLUE ARCHIVE"))?, series);
        // Two of one name are told apart by the user, not guessed.
        store.create("blue archive")?;
        assert!(link_named(path, &store, "Blue Archive", false).is_err());
        assert!(find_series(path, &store, Some("Blue Archive")).is_err());
        Ok(())
    }

    #[test]
    fn a_term_saved_keeps_what_it_leaves_out() -> Result<()> {
        let chapter = Chapter::new(Vec::new())?;
        let (path, store) = (chapter.path(), chapter.store());
        let edit = |source: &str, target: &str| TermEdit {
            source: source.to_owned(),
            target: target.to_owned(),
            ..TermEdit::default()
        };
        assert!(
            save_term(path, &store, edit("ヒナ", "Hina")).is_err(),
            "no series"
        );
        let series = store.create("Blue Archive")?;
        link(path, &store, Some(&series.id))?;
        let saved = save_term(
            path,
            &store,
            TermEdit {
                kind: Some(TermKind::Name),
                note: Some(" the prefect ".to_owned()),
                locked: Some(true),
                ..edit(" ヒナ ", "Hina")
            },
        )?;
        assert!(!saved.replaced && saved.series == "Blue Archive");
        let first = saved.term;
        assert_eq!(
            (first.note.as_str(), first.kind, first.locked),
            ("the prefect", TermKind::Name, true)
        );
        let saved = save_term(path, &store, edit("ヒ ナ", "Hina-san"))?;
        assert!(saved.replaced);
        assert_eq!(
            saved.term,
            GlossaryEntry {
                source: "ヒ ナ".to_owned(),
                target: "Hina-san".to_owned(),
                ..first
            }
        );
        assert_eq!(store.get(&series.id)?.glossary, [saved.term]);
        assert!(save_term(path, &store, edit("先生", " ")).is_err());
        assert_eq!(delete_term(path, &store, "ヒナ")?, 1);
        assert_eq!(delete_term(path, &store, "ヒナ")?, 0);
        Ok(())
    }

    #[test]
    fn a_suggestion_is_accepted_or_dismissed_by_its_source() -> Result<()> {
        let chapter = Chapter::new(Vec::new())?;
        let (path, store) = (chapter.path(), chapter.store());
        let series = store.create("Blue Archive")?;
        link(path, &store, Some(&series.id))?;
        suggest(path, &store, GlossaryEntry::new("ヒナ", "Hina"))?;
        suggest(path, &store, GlossaryEntry::new("アコ", "Ako"))?;
        let accepted = accept_source(path, &store, "ヒ ナ")?;
        assert_eq!(
            (accepted.term.target.as_str(), accepted.replaced),
            ("Hina", false)
        );
        assert_eq!(store.get(&series.id)?.glossary, [accepted.term]);
        assert!(
            accept_source(path, &store, "ヒナ").is_err(),
            "accepted already"
        );
        assert_eq!(dismiss_source(path, "アコ")?, 1);
        assert_eq!(dismiss_source(path, "アコ")?, 0);
        assert!(ChapterContext::load(path)?.suggestions.is_empty());
        Ok(())
    }

    #[test]
    fn the_instructions_terms_are_offered_with_those_the_series_holds() -> Result<()> {
        let chapter = Chapter::new(Vec::new())?;
        let (path, store) = (chapter.path(), chapter.store());
        let instructions = "Use natural English.\nヒナ=Hina\n魔王 = Demon King";
        let offer = offer_instructions(path, &store, instructions)?;
        assert_eq!(offer.series, None);
        assert!(offer.terms.iter().all(|term| !term.held));
        let version = offer.version;
        assert_eq!(version, instructions_version(instructions));
        assert_eq!(version.len(), 16);
        assert!(
            import_linked_instructions(path, &store, instructions, &version, &[1], false).is_err()
        );
        let mut series = store.create("Blue Archive")?;
        series.glossary.push(GlossaryEntry::new("魔王", "Overlord"));
        store.save(&series)?;
        link(path, &store, Some(&series.id))?;
        let offer = offer_instructions(path, &store, instructions)?;
        assert_eq!(offer.series.as_deref(), Some("Blue Archive"));
        let held: Vec<(u32, bool)> = offer
            .terms
            .iter()
            .map(|term| (term.line, term.held))
            .collect();
        assert_eq!(held, [(1, false), (2, true)]);

        // Line 1 deleted since they were read: line 1 now writes another
        // term, which is never imported in its place.
        let edited = "Use natural English.\n魔王 = Demon King";
        let refused = import_linked_instructions(path, &store, edited, &offer.version, &[1], true)
            .expect_err("changed since they were read");
        assert!(
            format!("{refused:#}").contains("read them again"),
            "{refused:#}"
        );
        assert_eq!(store.get(&series.id)?, series);

        let imported =
            import_linked_instructions(path, &store, instructions, &offer.version, &[1, 2], true)?;
        assert_eq!((imported.added, imported.skipped), (1, 1));
        assert_eq!(
            imported.instructions.as_deref(),
            Some("Use natural English.")
        );
        Ok(())
    }

    #[test]
    fn a_series_exports_to_a_new_file_and_imports_beside_its_own() -> Result<()> {
        let chapter = Chapter::new(Vec::new())?;
        let store = chapter.store();
        let mut series = store.create("Blue Archive")?;
        series.glossary.push(GlossaryEntry::new("先生", "Sensei"));
        store.save(&series)?;
        let file = chapter.dir.path().join("Blue Archive.json");
        export_series(&series, &file, false)?;
        assert_eq!(Series::read_file(&file)?, series);
        assert!(
            export_series(&series, &file, false).is_err(),
            "there already"
        );
        export_series(&series, &file, true)?;
        assert!(export_series(&series, Path::new("Blue Archive.json"), true).is_err());

        let kept = import_series(&store, &file, false)?;
        assert!(kept.existed && kept.id != series.id && kept.terms == 1);
        assert_eq!(store.list()?.len(), 2);
        let replaced = import_series(&store, &file, true)?;
        assert_eq!(
            (replaced.id.as_str(), replaced.existed),
            (&*series.id, true)
        );
        assert_eq!(store.list()?.len(), 2);
        assert!(import_series(&store, Path::new("Blue Archive.json"), false).is_err());
        Ok(())
    }

    #[test]
    fn the_brief_the_user_writes_keeps_what_the_read_found() -> Result<()> {
        let chapter = Chapter::new(Vec::new())?;
        let path = chapter.path();
        assert!(set_brief(path, "  Hina meets Sensei.  ")?);
        let brief = ChapterContext::load(path)?.brief.context("a brief")?;
        assert_eq!(
            (brief.summary.as_str(), brief.edited, brief.read_at),
            ("Hina meets Sensei.", true, None),
            "made when there was none"
        );
        let read = ChapterBrief {
            summary: "Read".to_owned(),
            characters: vec![booharu_core::context::ChapterCharacter {
                name: "Hina".to_owned(),
                original: "ヒナ".to_owned(),
                pages: vec![1, 3],
            }],
            edited: false,
            read_at: Some("2026-11-19T10:00:00Z".to_owned()),
            read_model: Some("openai/gpt-5.4".to_owned()),
        };
        ChapterContext::update(path, |context| context.brief = Some(read.clone()))?;
        assert!(set_brief(path, "Hina meets Sensei.")?);
        let brief = ChapterContext::load(path)?.brief.context("a brief")?;
        assert_eq!(
            brief,
            ChapterBrief {
                summary: "Hina meets Sensei.".to_owned(),
                edited: true,
                ..read
            },
            "the characters and when and how it was read stay"
        );
        assert!(
            !set_brief(path, "Hina meets Sensei. ")?,
            "the same, trimmed"
        );
        Ok(())
    }
}
