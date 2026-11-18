//! The series glossary and the chapter notes as the user and the agent
//! change them, which the desktop commands and the agent tools share: where
//! a term is used and Fix it everywhere, suggesting a term for a chapter and
//! accepting or dismissing it, linking a chapter to a series, and Import
//! from instructions. booharu-core's `context` holds the files and the text
//! a run sends with its chapter.

use std::{collections::BTreeSet, path::Path};

use anyhow::{Context as _, Result, ensure};
use booharu_core::{
    context::{
        ChapterContext, GlossaryEntry, Series, SeriesStore, TermKind, instruction_terms, occurs,
        same_source,
    },
    edited,
};
use koharu_pipeline::{RunId, Stage, Target};
use koharu_scene::{Content, Layer, LayerId, Project};
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
    let entry = GlossaryEntry {
        note: entry.note.trim().to_owned(),
        kind: entry.kind,
        ..GlossaryEntry::new(&entry.source, &entry.target)
    };
    ensure!(
        !entry.source.is_empty() && !entry.target.is_empty(),
        "a term needs a source and a target"
    );
    let linked = ChapterContext::load(project)?.series;
    let series = match linked {
        Some(id) => store.find(&id)?,
        None => None,
    };
    let held = |source: &str| {
        series
            .iter()
            .flat_map(|series| &series.glossary)
            .any(|term| same_source(&term.source, source))
    };
    if held(&entry.source) {
        return Ok(false);
    }
    ChapterContext::update(project, |chapter| {
        if chapter.suggestion(&entry.source).is_some() {
            return false;
        }
        chapter.suggestions.push(entry);
        true
    })
}

/// Accepts the suggestion of the chapter of the project file `project`
/// that `entry` names by its id into the series the chapter links, in
/// `store`, as `entry` holds it, the user's changes included: in place of the
/// series' term with its source, keeping that term's place and id, or after
/// the others ([`Series::put`]). The suggestion then leaves the chapter.
/// Fails, changing nothing, when the chapter has no such suggestion or links
/// no series, the series is gone, or the term lacks a source or a target.
/// Answers the series as saved.
pub fn accept(project: &Path, store: &SeriesStore, entry: GlossaryEntry) -> Result<Series> {
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
    let series = store.update(&id, |series| {
        series.put(entry);
        series.clone()
    })?;
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
        assert!(accept(path, &store, suggestion.clone()).is_err());
        assert!(link(path, &store, Some("blue-archive-000000000000")).is_err());
        assert!(link(path, &store, Some(&series.id))?);
        assert!(!link(path, &store, Some(&series.id))?);

        // Its source is the series' first term's: it takes that term's place.
        let edited = GlossaryEntry {
            target: "Sensei".to_owned(),
            locked: true,
            ..suggestion
        };
        let saved = accept(path, &store, edited.clone())?;
        assert_eq!(saved, store.get(&series.id)?);
        assert_eq!(saved.glossary.len(), 2);
        assert_eq!(
            saved.glossary[0],
            GlossaryEntry {
                id: held,
                ..edited.clone()
            }
        );
        assert!(ChapterContext::load(path)?.suggestions.is_empty());
        // Accepted once, it's gone.
        assert!(accept(path, &store, edited).is_err());
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
}
