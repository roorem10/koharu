//! What a translation run sends with the chapter, which koharu-pipeline's
//! snapshot hook appends to the user's own Instructions in the run's
//! snapshot, never in the saved setting: Koharu's translation conversation
//! starts again whenever its instructions change, so the text is the same for
//! the same inputs, in a fixed order, with no time, id, or random value in
//! it, and while nothing changed the conversation goes on.
//!
//! - The glossary: the linked series' terms, then the chapter's suggestions,
//!   each source once, the first locked term with it winning, else the first;
//!   locked terms first, each group in stored order; only the terms whose
//!   source occurs in the source of a block on some page, ignoring case and
//!   whitespace, at most [`MAX_TERMS`]. While the chapter's text isn't known
//!   yet, as when the run detects or reads text first, a page with a scan was
//!   never through detection, or a block still waits to be read, the
//!   glossary is every term instead, locked first, as many, with those the
//!   text known so far uses first in each group, so the cap leaves out a term
//!   the chapter is known to use only after every one it isn't: a scanned
//!   page detection found nothing on leaves no record of it, and keeps the
//!   text unknown.
//! - The background, which the model reads and never translates: the series'
//!   notes, then the brief's summary.
//! - The chapter's characters from its brief, each with their pages and the
//!   voice notes of the series' character of their name, listed once for the
//!   chapter, where Booharu 0.0.4 listed a page's own.
//!
//! The wording keeps 0.0.4's: its glossary was mandatory and kept its target
//! spelling, and its background was reference only, never translated or
//! returned.

use std::{
    collections::{BTreeMap, BTreeSet, btree_map::Entry},
    ops::Range,
};

use koharu_scene::{Content, Layer, Project};
use serde::Serialize;

use super::{ChapterCharacter, ChapterContext, Character, GlossaryEntry, Series, key};
use crate::styles::recorded;

/// The most glossary terms a run sends, as Booharu 0.0.4 sent a page.
pub const MAX_TERMS: usize = 60;

/// What a translation run sends with its chapter.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Extras {
    /// The terms, locked first, each group in stored order; while the
    /// text isn't known, those the text known so far uses first in each.
    pub glossary: Vec<GlossaryEntry>,
    /// Whether the glossary is every term, as the chapter's text isn't known
    /// yet, rather than the terms found in it.
    pub fresh: bool,
    /// How many terms past the first [`MAX_TERMS`] were left out.
    pub left_out: u32,
    /// The series' notes, then the brief's summary, each trimmed, an empty
    /// one left out.
    pub background: Vec<String>,
    /// The brief's characters, in its order.
    pub characters: Vec<CharacterHint>,
}

/// A character of the chapter, as its translation reads it.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct CharacterHint {
    pub name: String,
    pub original: String,
    /// The pages they're on, counted from 1, in order.
    pub pages: Vec<u32>,
    /// The voice notes of the series' character of their name or original
    /// name.
    pub notes: String,
}

impl Extras {
    /// What a translation run on `project` sends with its chapter, as
    /// `context`, its side file, and `series`, the series it links, hold it.
    /// `finds_text` tells a run that detects or reads text before it
    /// translates, whose text isn't known yet.
    pub fn of(
        project: &Project,
        context: &ChapterContext,
        series: Option<&Series>,
        finds_text: bool,
    ) -> Self {
        let fresh = finds_text || !text_known(project);
        let terms = deduplicated(
            series
                .into_iter()
                .flat_map(|series| &series.glossary)
                .chain(&context.suggestions),
        );
        let found: Vec<&GlossaryEntry> = match fresh {
            true => used_first(project, terms),
            false => found(project, terms),
        };
        let left_out = found.len().saturating_sub(MAX_TERMS);
        let background = [
            series.map(|series| series.notes.as_str()),
            context.brief.as_ref().map(|brief| brief.summary.as_str()),
        ]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
        .collect();
        let characters = context
            .brief
            .iter()
            .flat_map(|brief| &brief.characters)
            .filter(|character| !character.name.trim().is_empty())
            .map(|character| hint(character, series))
            .collect();
        Self {
            glossary: found.into_iter().take(MAX_TERMS).cloned().collect(),
            fresh,
            left_out: left_out as u32,
            background,
            characters,
        }
    }

    /// Whether there's nothing to send, which leaves the Instructions as
    /// they are.
    pub fn is_empty(&self) -> bool {
        self.glossary.is_empty() && self.background.is_empty() && self.characters.is_empty()
    }

    /// The extras as the run's instructions end with them: fixed English
    /// headings, then a line for each term or character; empty when there
    /// are none.
    pub fn text(&self) -> String {
        let mut parts = Vec::new();
        if !self.glossary.is_empty() {
            let mut part = String::from(
                "Glossary (mandatory: use these translations and keep their spelling):",
            );
            for term in &self.glossary {
                part.push_str(&format!(
                    "\n- {} → {}",
                    line(&term.source),
                    line(&term.target)
                ));
                let note = line(&term.note);
                if !note.is_empty() {
                    part.push_str(&format!(" ({note})"));
                }
            }
            parts.push(part);
        }
        if !self.background.is_empty() {
            parts.push(format!(
                "Background (reference only: never translate or return it):\n{}",
                self.background.join("\n\n")
            ));
        }
        if !self.characters.is_empty() {
            let mut part = String::from(
                "Characters in this chapter (reference only: never translate or return them):",
            );
            for character in &self.characters {
                part.push_str(&format!("\n- {}", line(&character.name)));
                let original = line(&character.original);
                if !original.is_empty() {
                    part.push_str(&format!(" ({original})"));
                }
                match character.pages.as_slice() {
                    [] => {}
                    [page] => part.push_str(&format!(", page {page}")),
                    pages => {
                        let pages: Vec<String> = pages.iter().map(u32::to_string).collect();
                        part.push_str(&format!(", pages {}", pages.join(", ")));
                    }
                }
                let notes = line(&character.notes);
                if !notes.is_empty() {
                    part.push_str(&format!(": {notes}"));
                }
            }
            parts.push(part);
        }
        parts.join("\n\n")
    }
}

/// The terms of `series`, then the chapter's suggestions in `context`,
/// whose source occurs in the source of a block on some page of `project`,
/// as a run on a chapter whose text is known sends them, but from whatever
/// text is known now and every one of them: each source once, the first
/// locked term with it winning, else the first; locked terms first, each
/// group in stored order.
pub fn found_terms(
    project: &Project,
    context: &ChapterContext,
    series: Option<&Series>,
) -> Vec<GlossaryEntry> {
    let terms = deduplicated(
        series
            .into_iter()
            .flat_map(|series| &series.glossary)
            .chain(&context.suggestions),
    );
    found(project, terms).into_iter().cloned().collect()
}

/// The terms of `series`, then the chapter's suggestions in `context`,
/// whose source occurs in `text`, one line's source, ignoring case and
/// whitespace, as Explain this line sends them: each source once, the first
/// locked term with it winning, else the first; locked terms first, each
/// group in stored order; at most [`MAX_TERMS`].
pub fn line_terms(
    text: &str,
    context: &ChapterContext,
    series: Option<&Series>,
) -> Vec<GlossaryEntry> {
    deduplicated(
        series
            .into_iter()
            .flat_map(|series| &series.glossary)
            .chain(&context.suggestions),
    )
    .into_iter()
    .filter(|term| super::occurs(text, &term.source))
    .take(MAX_TERMS)
    .cloned()
    .collect()
}

/// The glossary terms `translation`, one line's, leaves out of `source`, the
/// line's source: of the terms of `series`, then the chapter's suggestions
/// in `context`, whose source occurs in `source` ([`line_terms`]), each one
/// whose target doesn't occur in the translation, ignoring case and
/// whitespace ([`occurs`](super::occurs)), in the order they first occur in
/// the source. A term found only inside a longer one's occurrences, such as
/// 王 inside 王子, is the longer one's. A blank translation, or one that is
/// its source, as a line given back unchanged is, leaves out nothing: there
/// is no translation to check.
pub fn slips(
    source: &str,
    translation: &str,
    context: &ChapterContext,
    series: Option<&Series>,
) -> Vec<GlossaryEntry> {
    if translation.trim().is_empty() || super::same_source(translation, source) {
        return Vec::new();
    }
    let text = key(source);
    let terms = line_terms(source, context, series);
    let found: Vec<Vec<Range<usize>>> = terms
        .iter()
        .map(|term| occurrences(&text, &key(&term.source)))
        .collect();
    let inside_longer = |index: usize, at: &Range<usize>| {
        found.iter().enumerate().any(|(other, theirs)| {
            other != index
                && theirs.iter().any(|over| {
                    over.len() > at.len() && over.start <= at.start && at.end <= over.end
                })
        })
    };
    let mut slips: Vec<(usize, GlossaryEntry)> = terms
        .iter()
        .zip(&found)
        .enumerate()
        .filter(|(index, (_, mine))| !mine.iter().all(|at| inside_longer(*index, at)))
        .filter(|(_, (term, _))| !super::occurs(translation, &term.target))
        .map(|(_, (term, mine))| (mine.first().map_or(usize::MAX, |at| at.start), term.clone()))
        .collect();
    slips.sort_by_key(|(first, _)| *first);
    slips.into_iter().map(|(_, term)| term).collect()
}

/// Where `needle` occurs in `text`, overlapping occurrences too, as byte
/// ranges in order.
fn occurrences(text: &str, needle: &str) -> Vec<Range<usize>> {
    if needle.is_empty() {
        return Vec::new();
    }
    text.char_indices()
        .map(|(start, _)| start)
        .filter(|start| text[*start..].starts_with(needle))
        .map(|start| start..start + needle.len())
        .collect()
}

/// `instructions`, the user's own, with `extras` after them, one blank line
/// between, as a run's snapshot holds them; with no extras, `instructions`
/// byte for byte, so the run goes as Koharu's would.
pub fn instructions(instructions: &str, extras: &Extras) -> String {
    let text = extras.text();
    if text.is_empty() {
        return instructions.to_owned();
    }
    match instructions.trim_end() {
        "" => text,
        own => format!("{own}\n\n{text}"),
    }
}

/// Whether `project`'s text is known: every page with a scan was through
/// detection, which leaves its record on whatever it adds, and no block
/// there still waits to be read, its source blank without recognition's
/// record, unless it's locked, which no stage can write. A page without a
/// scan has no text a stage could find.
pub fn text_known(project: &Project) -> bool {
    project
        .pages()
        .filter(|page| page.background.is_some())
        .all(|page| detected(&page.layers) && !unread(&page.layers, false))
}

/// Whether detection left its record on any of `layers` or the layers in
/// them.
fn detected(layers: &[Layer]) -> bool {
    layers
        .iter()
        .any(|layer| recorded(layer, "detection") || detected(&layer.children))
}

/// Whether a block among `layers` or in them waits to be read: a blank
/// source no recognition recorded, in no locked layer; `locked` tells
/// whether a layer holding them is.
fn unread(layers: &[Layer], locked: bool) -> bool {
    layers.iter().any(|layer| {
        let locked = locked || layer.locked;
        let blank = matches!(&layer.content,
            Content::Block(block) if block.source.content.text().trim().is_empty());
        (blank && !locked && !recorded(layer, "ocr")) || unread(&layer.children, locked)
    })
}

/// The terms of `terms` whose source occurs in the source of a block on
/// some page of `project`, ignoring case and whitespace, in their order.
fn found<'t>(project: &Project, terms: Vec<&'t GlossaryEntry>) -> Vec<&'t GlossaryEntry> {
    let texts = sources(project);
    terms
        .into_iter()
        .filter(|term| used(term, &texts))
        .collect()
}

/// `terms`, locked first, as [`deduplicated`] leaves them, with those whose
/// source occurs in the source of a block on some page of `project` before
/// the others in each group, each part in their order.
fn used_first<'t>(project: &Project, mut terms: Vec<&'t GlossaryEntry>) -> Vec<&'t GlossaryEntry> {
    let texts = sources(project);
    // A stable sort, so each part keeps its order.
    terms.sort_by_cached_key(|term| (!term.locked, !used(term, &texts)));
    terms
}

/// Whether the source of `term` occurs in one of `texts`, block sources as
/// terms are compared.
fn used(term: &GlossaryEntry, texts: &[String]) -> bool {
    let source = key(&term.source);
    texts.iter().any(|text| text.contains(&source))
}

/// The source of every block on every page, as terms are compared.
fn sources(project: &Project) -> Vec<String> {
    fn walk(layers: &[Layer], texts: &mut Vec<String>) {
        for layer in layers {
            if let Content::Block(block) = &layer.content {
                texts.push(key(&block.source.content.text()));
            }
            walk(&layer.children, texts);
        }
    }
    let mut texts = Vec::new();
    for page in project.pages() {
        walk(&page.layers, &mut texts);
    }
    texts
}

/// `terms` with each source once, the first locked term with it kept, else
/// the first; locked terms first, each group in their order. A term without
/// a source or a target, which only a hand-edited side file holds, is left
/// out.
fn deduplicated<'t>(terms: impl Iterator<Item = &'t GlossaryEntry>) -> Vec<&'t GlossaryEntry> {
    let terms: Vec<&GlossaryEntry> = terms
        .filter(|term| !term.source.trim().is_empty() && !term.target.trim().is_empty())
        .collect();
    let mut kept = BTreeMap::new();
    for (index, term) in terms.iter().enumerate() {
        match kept.entry(key(&term.source)) {
            Entry::Vacant(slot) => {
                slot.insert(index);
            }
            Entry::Occupied(mut slot) => {
                if term.locked && !terms[*slot.get()].locked {
                    slot.insert(index);
                }
            }
        }
    }
    let kept: BTreeSet<usize> = kept.into_values().collect();
    let (locked, unlocked): (Vec<_>, Vec<_>) = terms
        .into_iter()
        .enumerate()
        .filter(|(index, _)| kept.contains(index))
        .map(|(_, term)| term)
        .partition(|term| term.locked);
    locked.into_iter().chain(unlocked).collect()
}

/// `character` of the brief as its translation reads it, with the voice
/// notes of `series`' character of the same name or original name.
fn hint(character: &ChapterCharacter, series: Option<&Series>) -> CharacterHint {
    let same = |a: &str, b: &str| !a.trim().is_empty() && a.trim() == b.trim();
    let named = |entry: &&Character| {
        same(&entry.name, &character.name) || same(&entry.original, &character.original)
    };
    let notes = series
        .and_then(|series| series.characters.iter().find(named))
        .map_or("", |entry| entry.notes.trim());
    let mut pages = character.pages.clone();
    pages.sort_unstable();
    pages.dedup();
    CharacterHint {
        name: character.name.trim().to_owned(),
        original: character.original.trim().to_owned(),
        pages,
        notes: notes.to_owned(),
    }
}

/// `text` on one line, each run of whitespace a space.
fn line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use anyhow::Context as _;
    use image::{ImageFormat, Rgb, RgbImage};
    use koharu_scene::{Created, Edit, Hash, LayerId, Op, Session};
    use serde_json::{Value, json};

    use super::*;
    use crate::context::ChapterBrief;

    /// A chapter of one scanned page with the blocks the ops of [`block`]
    /// add, by their names.
    struct Page {
        _dir: tempfile::TempDir,
        session: Session,
        blocks: BTreeMap<String, LayerId>,
    }

    impl Page {
        /// A page holding a block for each of `sources`, detected and read
        /// as Koharu's stages record them.
        fn read(sources: &[&str]) -> anyhow::Result<Self> {
            let blocks = sources.iter().enumerate();
            Self::new(
                blocks
                    .map(|(index, source)| block(index, source, &["detection", "ocr"]))
                    .collect(),
            )
        }

        fn new(blocks: Vec<Value>) -> anyhow::Result<Self> {
            let dir = tempfile::tempdir()?;
            let mut session = Session::create(dir.path().join("test.khr"))?;
            let mut scan = Vec::new();
            RgbImage::from_pixel(400, 400, Rgb([255; 3]))
                .write_to(&mut Cursor::new(&mut scan), ImageFormat::Png)?;
            let page =
                json!({"op": "add_page", "id": "$page", "index": 0, "background": Hash::of(&scan)});
            let mut ops = vec![Op::AddFile { bytes: scan }];
            for value in [page].into_iter().chain(blocks) {
                ops.push(serde_json::from_value(value)?);
            }
            let change = session.apply(Edit {
                label: "test".to_owned(),
                ops,
            })?;
            let blocks = change
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
                _dir: dir,
                session,
                blocks,
            })
        }

        fn project(&self) -> Project {
            self.session.project()
        }
    }

    /// The op adding a block at `index` of the page, its source `source`,
    /// with the `c2pa` records of `steps`.
    fn block(index: usize, source: &str, steps: &[&str]) -> Value {
        let c2pa: serde_json::Map<String, Value> = steps
            .iter()
            .map(|step| ((*step).to_owned(), json!({"action": "c2pa.created"})))
            .collect();
        let y = 10 + 30 * index;
        json!({"op": "add_layer", "id": format!("$block{index}"), "page": "$page", "index": index,
        "layer": {"kind": "block", "attributes": {"c2pa": c2pa}, "source": {
            "frame": {"box": {"bounds": {"x": 10, "y": y, "width": 100, "height": 20}}},
            "content": source,
        }}})
    }

    fn term(source: &str, target: &str, locked: bool) -> GlossaryEntry {
        GlossaryEntry {
            locked,
            ..GlossaryEntry::new(source, target)
        }
    }

    fn series(terms: Vec<GlossaryEntry>) -> Series {
        Series {
            glossary: terms,
            ..Series::new("Blue Archive").expect("a series")
        }
    }

    fn sources(extras: &Extras) -> Vec<&str> {
        extras.glossary.iter().map(|term| &*term.source).collect()
    }

    fn slipped(source: &str, translation: &str, series: &Series) -> Vec<String> {
        slips(
            source,
            translation,
            &ChapterContext::default(),
            Some(series),
        )
        .into_iter()
        .map(|term| term.source)
        .collect()
    }

    #[test]
    fn a_line_slips_the_terms_its_translation_leaves_out() {
        let demon = series(vec![term("魔王", "Demon King", true)]);
        assert_eq!(slipped("魔王です", "The king", &demon), ["魔王"]);
        assert!(slipped("魔王です", "The Demon King", &demon).is_empty());
        // Case and the lettering's line breaks don't count.
        assert!(slipped("魔\n王です", "THE DEMON\nKING!", &demon).is_empty());

        let rimuru = series(vec![term("リムル", "Rimuru", false)]);
        assert_eq!(slipped("リムル様！", "Lord Rimiru!", &rimuru), ["リムル"]);
        assert!(slipped("リムル様！", "LORD RIMURU!", &rimuru).is_empty());

        // A target in the source's script.
        let back = series(vec![term("Demon King", "魔王", false)]);
        assert!(slipped("The Demon King!", "魔王だ", &back).is_empty());
        assert_eq!(slipped("The Demon King!", "魔玉だ", &back), ["Demon King"]);

        // A chapter's suggestion slips as a series term does.
        let suggested = ChapterContext {
            suggestions: vec![term("勇者", "Hero", false)],
            ..ChapterContext::default()
        };
        let found = slips("勇者と魔王", "The king and him", &suggested, Some(&demon));
        let found: Vec<&str> = found.iter().map(|term| &*term.source).collect();
        assert_eq!(found, ["勇者", "魔王"], "in the order they occur");
    }

    #[test]
    fn the_longest_term_found_wins() {
        let both = series(vec![
            term("王子", "Prince", false),
            term("王", "King", false),
        ]);
        assert!(slipped("王子様", "Prince!", &both).is_empty());
        // 王 found on its own as well slips there.
        assert_eq!(slipped("王子と王", "The prince and him", &both), ["王"]);
        assert_eq!(slipped("王と王子", "Them", &both), ["王", "王子"]);
    }

    #[test]
    fn a_blank_or_unchanged_translation_slips_nothing() {
        let demon = series(vec![term("魔王", "Demon King", true)]);
        assert!(slipped("魔王です", "", &demon).is_empty());
        assert!(slipped("魔王です", " \n ", &demon).is_empty());
        assert!(
            slipped("魔王です", " 魔王 です", &demon).is_empty(),
            "given back"
        );
        assert!(slipped("こんにちは", "Hello", &demon).is_empty(), "no term");
        let blank = series(vec![term("魔王", " ", false)]);
        assert!(
            slipped("魔王です", "The king", &blank).is_empty(),
            "no target"
        );
        let none = ChapterContext::default();
        assert!(slips("魔王です", "The king", &none, None).is_empty());
    }

    #[test]
    fn with_no_context_the_instructions_stay_byte_for_byte() -> anyhow::Result<()> {
        let page = Page::read(&["ヒナです。"])?;
        let none = Extras::of(&page.project(), &ChapterContext::default(), None, false);
        assert!(none.is_empty());
        assert_eq!(none.text(), "");
        for own in ["", "Keep honorifics.\n", "  Use natural English. "] {
            assert_eq!(instructions(own, &none), own);
        }
        // Terms the chapter never uses send nothing either.
        let unused = series(vec![term("リムル", "Rimuru", true)]);
        let extras = Extras::of(
            &page.project(),
            &ChapterContext::default(),
            Some(&unused),
            false,
        );
        assert!(extras.is_empty());
        assert_eq!(
            instructions("Keep honorifics.\n", &extras),
            "Keep honorifics.\n"
        );
        Ok(())
    }

    #[test]
    fn the_extras_follow_the_instructions_after_a_blank_line() -> anyhow::Result<()> {
        let page = Page::read(&["ヒナ先生、シャーレへ"])?;
        let mut series = series(vec![
            GlossaryEntry {
                note: "a club,\n  not a tray".to_owned(),
                ..term("シャーレ", "Schale", false)
            },
            term("先生", "Sensei", true),
        ]);
        series.notes = "Students of Kivotos.".to_owned();
        series.characters.push(Character {
            name: "Hina".to_owned(),
            original: "ヒナ".to_owned(),
            notes: "Curt, tired.".to_owned(),
            ..Character::default()
        });
        let context = ChapterContext {
            brief: Some(ChapterBrief {
                summary: " Hina meets Sensei. ".to_owned(),
                characters: vec![
                    ChapterCharacter {
                        name: "Hina".to_owned(),
                        original: String::new(),
                        pages: vec![3, 1, 3],
                    },
                    ChapterCharacter {
                        name: "Ako".to_owned(),
                        original: "アコ".to_owned(),
                        pages: vec![2],
                    },
                ],
                ..ChapterBrief::default()
            }),
            ..ChapterContext::default()
        };
        let extras = Extras::of(&page.project(), &context, Some(&series), false);
        let text = "Glossary (mandatory: use these translations and keep their spelling):\n\
                    - 先生 → Sensei\n\
                    - シャーレ → Schale (a club, not a tray)\n\
                    \n\
                    Background (reference only: never translate or return it):\n\
                    Students of Kivotos.\n\
                    \n\
                    Hina meets Sensei.\n\
                    \n\
                    Characters in this chapter (reference only: never translate or return them):\n\
                    - Hina, pages 1, 3: Curt, tired.\n\
                    - Ako (アコ), page 2";
        assert_eq!(extras.text(), text);
        assert_eq!(
            instructions("Keep honorifics.\n\n", &extras),
            format!("Keep honorifics.\n\n{text}")
        );
        assert_eq!(instructions(" \n", &extras), text);
        // The same inputs give the same text, so the conversation goes on.
        let again = Extras::of(
            &page.project(),
            &context.clone(),
            Some(&series.clone()),
            false,
        );
        assert_eq!(
            instructions("Keep honorifics.", &again),
            instructions("Keep honorifics.", &extras)
        );
        Ok(())
    }

    #[test]
    fn locked_terms_go_first_and_each_source_once() -> anyhow::Result<()> {
        let page = Page::read(&["ヒナ と 魔王", "SCHALE で 先生 と"])?;
        let series = series(vec![
            term("ヒナ", "Hina", false),
            term("先生", "Teacher", false),
            term("魔王", "Demon King", true),
            term("先 生", "Sensei", true),
            term("ヒナ", "Hinata", false),
            term("リムル", "Rimuru", true),
        ]);
        let context = ChapterContext {
            suggestions: vec![
                term("Schale", "Shale", false),
                term("ひな", "Hina-chan", true),
            ],
            ..ChapterContext::default()
        };
        let extras = Extras::of(&page.project(), &context, Some(&series), false);
        assert!(!extras.fresh);
        assert_eq!(sources(&extras), ["魔王", "先 生", "ヒナ", "Schale"]);
        let targets: Vec<&str> = extras.glossary.iter().map(|term| &*term.target).collect();
        assert_eq!(targets, ["Demon King", "Sensei", "Hina", "Shale"]);
        Ok(())
    }

    #[test]
    fn the_terms_found_are_those_the_known_text_uses_however_many() -> anyhow::Result<()> {
        let linked = series(vec![
            term("先生", "Sensei", false),
            term("ヒナ", "Hina", false),
            term("リムル", "Rimuru", true),
            term("ヒ ナ", "Hina!", true),
        ]);
        let context = ChapterContext {
            suggestions: vec![term("ひな", "Hina-chan", false), term("アコ", "Ako", false)],
            ..ChapterContext::default()
        };
        // A block still waits to be read, so a run would send every term,
        // but the terms found are those the text known uses.
        let page = Page::new(vec![
            block(0, "ヒナです。ひな", &["detection", "ocr"]),
            block(1, "", &["detection"]),
        ])?;
        assert!(!text_known(&page.project()));
        let found = found_terms(&page.project(), &context, Some(&linked));
        let found: Vec<&str> = found.iter().map(|term| &*term.target).collect();
        assert_eq!(found, ["Hina!", "Hina-chan"]);
        // Past 60 too.
        let text: String = (0..70).map(|index| format!("語{index:02} ")).collect();
        let page = Page::read(&[&text])?;
        let terms = (0..70)
            .map(|index| term(&format!("語{index:02}"), &format!("word {index}"), false))
            .collect();
        let many = found_terms(
            &page.project(),
            &ChapterContext::default(),
            Some(&series(terms)),
        );
        assert_eq!(many.len(), 70);
        Ok(())
    }

    #[test]
    fn at_most_60_terms_are_sent() -> anyhow::Result<()> {
        let text: String = (0..70).map(|index| format!("語{index:02} ")).collect();
        let page = Page::read(&[&text])?;
        let terms = (0..70)
            .map(|index| {
                term(
                    &format!("語{index:02}"),
                    &format!("word {index}"),
                    index >= 65,
                )
            })
            .collect();
        let extras = Extras::of(
            &page.project(),
            &ChapterContext::default(),
            Some(&series(terms)),
            false,
        );
        assert_eq!(extras.glossary.len(), MAX_TERMS);
        assert_eq!(extras.left_out, 10);
        let kept = sources(&extras);
        assert_eq!(
            kept[..5],
            ["語65", "語66", "語67", "語68", "語69"],
            "locked first"
        );
        assert_eq!(kept[5], "語00");
        assert_eq!(kept[59], "語54");
        Ok(())
    }

    #[test]
    fn a_fresh_chapter_sends_every_term_locked_first() -> anyhow::Result<()> {
        let series = series(vec![
            term("先生", "Sensei", false),
            term("ヒナ", "Hina", false),
            term("リムル", "Rimuru", true),
        ]);
        let context = ChapterContext::default();
        let page = Page::read(&["ヒナです。"])?;
        let found = Extras::of(&page.project(), &context, Some(&series), false);
        assert_eq!(sources(&found), ["ヒナ"]);

        // A run that detects or reads first doesn't know the text yet: every
        // term, locked first, and the one the text known uses first after.
        let fresh = Extras::of(&page.project(), &context, Some(&series), true);
        assert!(fresh.fresh);
        assert_eq!(sources(&fresh), ["リムル", "ヒナ", "先生"]);

        // Nor does one where a block waits to be read...
        let mut waiting = Page::new(vec![
            block(0, "ヒナです。", &["detection", "ocr"]),
            block(1, "", &["detection"]),
        ])?;
        assert!(!text_known(&waiting.project()));
        let extras = Extras::of(&waiting.project(), &context, Some(&series), false);
        assert_eq!((extras.fresh, sources(&extras).len()), (true, 3));
        // ...unless it's locked, which no stage can write.
        let lock = json!({"op": "update_layer", "layer": waiting.blocks["block1"],
            "patch": {"locked": true}});
        waiting.session.apply(Edit {
            label: "test".to_owned(),
            ops: vec![serde_json::from_value(lock)?],
        })?;
        assert!(text_known(&waiting.project()));
        // A block read as nothing is read.
        let blank = Page::new(vec![block(0, " ", &["detection", "ocr"])])?;
        assert!(text_known(&blank.project()));

        // Nor one with a scanned page detection never went through.
        let undetected = Page::new(Vec::new())?;
        assert!(!text_known(&undetected.project()));
        let extras = Extras::of(&undetected.project(), &context, Some(&series), false);
        assert_eq!(sources(&extras), ["リムル", "先生", "ヒナ"]);
        Ok(())
    }

    #[test]
    fn a_fresh_chapter_sends_at_most_60_terms_those_its_text_uses_first() -> anyhow::Result<()> {
        // 70 terms, the last five locked, of which the text uses 語62 and 語68.
        let terms = (0..70)
            .map(|index| {
                term(
                    &format!("語{index:02}"),
                    &format!("word {index}"),
                    index >= 65,
                )
            })
            .collect();
        let series = series(terms);
        let mut chapter = Page::read(&["語62 と 語68"])?;
        // A second scanned page, which detection found nothing on, or never
        // went through: no record tells which, so the text isn't known.
        let scan = chapter
            .project()
            .pages()
            .find_map(|page| page.background)
            .context("a scan")?
            .file;
        let blank = json!({"op": "add_page", "id": "$blank", "index": 1, "background": scan});
        chapter.session.apply(Edit {
            label: "test".to_owned(),
            ops: vec![serde_json::from_value(blank)?],
        })?;
        assert!(!text_known(&chapter.project()));
        for finds_text in [false, true] {
            let extras = Extras::of(
                &chapter.project(),
                &ChapterContext::default(),
                Some(&series),
                finds_text,
            );
            assert!(extras.fresh);
            assert_eq!(extras.glossary.len(), MAX_TERMS);
            assert_eq!(extras.left_out, 10);
            let kept = sources(&extras);
            assert_eq!(
                kept[..6],
                ["語68", "語65", "語66", "語67", "語69", "語62"],
                "locked first, and in each group those the text uses first"
            );
            // Then the others in stored order, those past the cap left out,
            // never 語62, past it in stored order.
            assert_eq!(kept[6], "語00");
            assert_eq!(kept[59], "語53");
        }
        Ok(())
    }

    #[test]
    fn a_lines_terms_are_those_its_source_uses() {
        let series = series(vec![
            term("ヒナ", "Hina", false),
            term("先生", "Sensei", false),
            term("ヒナ", "Hinata", true),
            term("ゲヘナ", "Gehenna", false),
        ]);
        let chapter = ChapterContext {
            suggestions: vec![
                term("風紀委員", "the Prefects", false),
                term("先 生", "Teach", false),
            ],
            ..ChapterContext::default()
        };
        let terms = line_terms("ヒナ 先生、風紀委員です", &chapter, Some(&series));
        let found: Vec<(&str, &str)> = terms
            .iter()
            .map(|term| (&*term.source, &*term.target))
            .collect();
        assert_eq!(
            found,
            [
                ("ヒナ", "Hinata"),
                ("先生", "Sensei"),
                ("風紀委員", "the Prefects")
            ],
            "the locked term wins and goes first, a suggestion with a series term's source doesn't \
             come, and a term the line doesn't use is left out"
        );
        assert!(line_terms("", &chapter, Some(&series)).is_empty());
    }
}
