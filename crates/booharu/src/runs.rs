//! Carrying on: what the pages of a chapter still need, which the Run
//! button runs when all four stages are on, as groups of the pages that
//! need the same stages, each queued as an ordinary run of Koharu's.
//!
//! A page's needs come from the records Koharu's stages leave in each
//! block's `c2pa` attributes:
//!
//! - a scanned page with no blocks needs every stage from detection, which
//!   replaces only unlocked blocks, unless detection already ran there: it
//!   records itself on everything it adds (panels and balloons as well as
//!   blocks), so a page it found no text on, or whose blocks were all
//!   deleted by hand, reads as done while one of those carries its record;
//! - a block with a blank source and no recognition record needs
//!   recognition, and the translation a block with that source would;
//! - a block with no inpainting record needs inpainting;
//! - a block with a source, no translation, and no translation record needs
//!   translation, and one the Translation stage was given back unchanged (no
//!   translation, its clean-up hidden) or with a blank source counts as
//!   translated;
//! - a sound effect the SFX mode keeps as art, or one with a subtitle
//!   (`sfx::skipped` under the lettering settings, read once per plan, as a
//!   run's snapshot reads them), needs translation only while the
//!   Translation stage would hide its clean-up to show the art
//!   (`sfx::kept_original` writing something: no translation, its clean-up
//!   showing, no kept mark); any other block carrying the kept mark with no
//!   translation needs it too, as an effect kept as art before the mode
//!   changed, whose translation shows its clean-up again.
//!
//! A stage that would leave a block as it is isn't needed for it, so a page
//! that's done reads as done however often it's run: Koharu's stages read
//! hidden blocks as any other, and leave a locked block, or one inside a
//! locked layer, out, counting it, rather than failing the page's edit;
//! detection, recognition, and inpainting read the page's scan, and do
//! nothing on a page without one; recognition reads only a source whose
//! frame lies on the page; inpainting redraws only under a mask, and writes
//! the clean-up, which can't be locked. Inside a stage nothing changes: a
//! page that needs translation is translated whole, as Koharu does, but for
//! the blocks it leaves out.
//!
//! `carry_on`, from `going`, queues the groups one after another, for the
//! desktop command and the agent tool alike.
//!
//! Translating last: a translation run sends the chapter's glossary and
//! notes, composed as it's queued from the text known then, and a run that
//! finds text before it translates would send every term instead of those
//! the chapter uses. So on a chapter with notes ([`has_notes`]), a press
//! whose plan would find text and translate runs every group without
//! Translation first, then one run of Translation over the pages that need
//! it ([`translate_last`]); with fewer stages, over every page it names.
//!
//! Reading first: a press with the Run menu's Read chapter on, whose plan
//! translates, on a chapter without a brief ([`brief_missing`]), reads the
//! whole chapter before it translates, so the Translation run sends the
//! brief: every group without Translation, then the read, then the
//! Translation run ([`read_before_translation`]), whatever the groups find.
//! The read writes nothing until it has read every page, so a press stopped
//! while it reads leaves the brief missing, and the next press reads again,
//! first when only Translation is left.

mod going;

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use booharu_core::{
    attrs::Auto,
    context::{Chapter, ChapterContext},
    settings::LetteringSettings,
    sfx,
    status::{DETECTION, INPAINTING, OCR, blocks, untranslated},
    styles::recorded,
};
use koharu_pipeline::{Event, RunId, Stage, Target};
use koharu_scene::{Layer, Page, PageId, Project, Rect, Session};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use specta::Type;
use strum::VariantArray as _;

pub use self::going::{Reading, RunHost, carry_on, group_going};

/// What the Run button carries on with: every page, or the pages chosen.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RunTarget {
    // Fields, even none, so an unknown one fails: serde reads a tagged unit
    // variant past any.
    Project {},
    Pages { pages: Vec<PageId> },
}

impl RunTarget {
    /// Koharu's target for the same pages, for an ordinary run.
    #[must_use]
    pub fn target(&self) -> Target {
        match self {
            Self::Project {} => Target::Project,
            Self::Pages { pages } => Target::Pages(pages.clone()),
        }
    }
}

/// Pages that need the same stages, which one run works on; or a press's
/// Read chapter step, which no run works on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type, JsonSchema)]
pub struct PlanGroup {
    /// In stage order; none for the read.
    pub stages: Vec<Stage>,
    /// In page order.
    pub pages: Vec<PageId>,
    /// Whether it's the press's Read chapter step, which reads the whole
    /// chapter before its Translation run: it has no stages, and every page
    /// of the project.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub read: bool,
}

/// What carrying on queues: the runs of `groups`, one after another, which
/// every `booharu_run_group` event of `group` announces, the first of them
/// `run`. No run and no groups when nothing is left to do.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RunPlan {
    pub group: u64,
    pub groups: Vec<PlanGroup>,
    /// The run going now; none while the group's read goes.
    pub run: Option<RunId>,
    /// Whether the group's Read chapter step is going, which no run is.
    pub reading: bool,
}

/// Whether a run is going on the pipeline whose events so far are `told`,
/// as `Pipeline::told` gives them: one was queued and hasn't ended. A run
/// stopped is going until the pipeline tells it ended.
#[must_use]
pub fn run_going(told: &[Event]) -> bool {
    let mut going = BTreeSet::new();
    for event in told {
        match event {
            Event::Queued { run, .. } => {
                going.insert(*run);
            }
            Event::Ended { run, .. } => {
                going.remove(run);
            }
            _ => {}
        }
    }
    !going.is_empty()
}

/// Whether `stages` holds every stage, which makes the Run button carry on
/// rather than run them all again.
#[must_use]
pub fn all_stages(stages: &[Stage]) -> bool {
    Stage::VARIANTS.iter().all(|stage| stages.contains(stage))
}

/// Every stage, in stage order, which carries on.
#[must_use]
pub fn every_stage() -> Vec<Stage> {
    Stage::VARIANTS.to_vec()
}

/// The pages `target` names, in page order. A page the project doesn't have
/// fails, as Koharu's run on pages does.
pub fn pages(project: &Project, target: &RunTarget) -> Result<Vec<PageId>> {
    let chosen = match target {
        RunTarget::Project {} => None,
        RunTarget::Pages { pages } => {
            for &page in pages {
                project.page(page)?;
            }
            Some(pages)
        }
    };
    Ok(project
        .pages()
        .map(|page| page.id)
        .filter(|page| chosen.is_none_or(|chosen| chosen.contains(page)))
        .collect())
}

/// What `stages` on `target` queues: with every stage, the work its pages
/// still need under `settings`, the lettering settings, planned; with
/// fewer, one ordinary run of those stages on every page it names, as
/// Koharu runs them.
pub fn groups(
    session: &Session,
    target: &RunTarget,
    stages: &[Stage],
    settings: &LetteringSettings,
) -> Result<Vec<PlanGroup>> {
    let pages = pages(&session.project(), target)?;
    if all_stages(stages) {
        return plan(session, &pages, settings);
    }
    let stages: BTreeSet<Stage> = stages.iter().copied().collect();
    Ok(vec![PlanGroup {
        stages: stages.into_iter().collect(),
        pages,
        read: false,
    }])
}

/// The work `pages` of `session`'s project still need under `settings`,
/// the lettering settings, which the sound-effect rule reads, as groups of
/// the pages that need the same stages, in page order, ordered by their
/// first page; empty when nothing is left.
pub fn plan(
    session: &Session,
    pages: &[PageId],
    settings: &LetteringSettings,
) -> Result<Vec<PlanGroup>> {
    let project = session.project();
    let mut groups: Vec<PlanGroup> = Vec::new();
    let mut found: BTreeMap<BTreeSet<Stage>, usize> = BTreeMap::new();
    for page in project.pages().filter(|page| pages.contains(&page.id)) {
        let stages = needs(session, page, settings)?;
        if stages.is_empty() {
            continue;
        }
        let at = *found.entry(stages.clone()).or_insert_with(|| {
            groups.push(PlanGroup {
                stages: stages.into_iter().collect(),
                pages: Vec::new(),
                read: false,
            });
            groups.len() - 1
        });
        groups[at].pages.push(page.id);
    }
    Ok(groups)
}

/// Whether `group` translates.
fn translates(group: &PlanGroup) -> bool {
    group.stages.contains(&Stage::Translate)
}

/// `groups`, a press's plan on `project`, translating last, when one of them
/// would find text, through detection or recognition, and translate it, as
/// [`translation_last`] splits it. `None` when no group both finds text and
/// translates, which runs as planned.
#[must_use]
pub fn translate_last(project: &Project, groups: &[PlanGroup]) -> Option<Vec<PlanGroup>> {
    let finds = |group: &PlanGroup| {
        group.stages.contains(&Stage::Detect) || group.stages.contains(&Stage::Recognize)
    };
    if !groups.iter().any(|group| translates(group) && finds(group)) {
        return None;
    }
    translation_last(project, groups)
}

/// `groups`, a press's plan on `project`, with Translation last, whatever its
/// groups find: every group without Translation, in order, with those left
/// with the same stages joined at the first one's place and those left with
/// none dropped, then one group of Translation on every page a group
/// translated, in page order. `None` when no group translates.
#[must_use]
pub fn translation_last(project: &Project, groups: &[PlanGroup]) -> Option<Vec<PlanGroup>> {
    if !groups.iter().any(translates) {
        return None;
    }
    let in_order = |pages: BTreeSet<PageId>| -> Vec<PageId> {
        project
            .pages()
            .map(|page| page.id)
            .filter(|page| pages.contains(page))
            .collect()
    };
    let mut before: Vec<(Vec<Stage>, BTreeSet<PageId>)> = Vec::new();
    for group in groups {
        let stages: Vec<Stage> = group
            .stages
            .iter()
            .copied()
            .filter(|&stage| stage != Stage::Translate)
            .collect();
        if stages.is_empty() {
            continue;
        }
        match before.iter_mut().find(|(held, _)| *held == stages) {
            Some((_, pages)) => pages.extend(group.pages.iter().copied()),
            None => before.push((stages, group.pages.iter().copied().collect())),
        }
    }
    let translated = groups
        .iter()
        .filter(|group| translates(group))
        .flat_map(|group| group.pages.iter().copied())
        .collect();
    let last = PlanGroup {
        stages: vec![Stage::Translate],
        pages: in_order(translated),
        read: false,
    };
    let before = before.into_iter().map(|(stages, pages)| PlanGroup {
        stages,
        pages: in_order(pages),
        read: false,
    });
    Some(before.chain([last]).collect())
}

/// `groups`, a press's plan on `project`, reading the chapter before it
/// translates: with Translation last, as [`translation_last`] splits it
/// whatever its groups find, and just before the Translation group the
/// read, a group of no stages on every page of the project, as Read chapter
/// reads the whole chapter. `None` when no group translates, which leaves
/// the press without a read.
#[must_use]
pub fn read_before_translation(project: &Project, groups: &[PlanGroup]) -> Option<Vec<PlanGroup>> {
    let mut split = translation_last(project, groups)?;
    let read = PlanGroup {
        stages: Vec::new(),
        pages: project.pages().map(|page| page.id).collect(),
        read: true,
    };
    split.insert(split.len() - 1, read);
    Some(split)
}

/// Whether `chapter` has notes a translation run sends with it, so a press
/// on it translates last: a linked series whose file is there, suggested
/// terms, or a brief.
#[must_use]
pub fn has_notes(chapter: &Chapter) -> bool {
    chapter.series.is_some()
        || !chapter.context.suggestions.is_empty()
        || chapter.context.brief.is_some()
}

/// Whether the chapter `context` holds has no brief to translate by, none or
/// one whose summary is blank, as Booharu 0.0.4 read it, so a press with
/// Read chapter on reads the chapter before it translates.
#[must_use]
pub fn brief_missing(context: &ChapterContext) -> bool {
    context
        .brief
        .as_ref()
        .is_none_or(|brief| brief.summary.trim().is_empty())
}

/// Whether a block on `pages` of `project` has text Read chapter can read:
/// a source that isn't blank, as recognition writes it.
#[must_use]
pub fn has_text(project: &Project, pages: &[PageId]) -> bool {
    project
        .pages()
        .filter(|page| pages.contains(&page.id))
        .flat_map(blocks)
        .any(|(_, block, _)| !block.source.content.text().trim().is_empty())
}

/// The pages of `pages` in `session`'s project that still need translation
/// under `settings`, the lettering settings, as the module's rules read
/// them, in page order.
pub fn to_translate(
    session: &Session,
    pages: &[PageId],
    settings: &LetteringSettings,
) -> Result<Vec<PageId>> {
    let project = session.project();
    let mut found = Vec::new();
    for page in project.pages().filter(|page| pages.contains(&page.id)) {
        if needs(session, page, settings)?.contains(&Stage::Translate) {
            found.push(page.id);
        }
    }
    Ok(found)
}

/// The stages `page` of `session`'s project still needs under `settings`,
/// the lettering settings, as the module's rules read them.
fn needs(session: &Session, page: &Page, settings: &LetteringSettings) -> Result<BTreeSet<Stage>> {
    let scanned = page.background.is_some();
    let blocks = blocks(page);
    if blocks.is_empty() {
        // Detection that ran and found no text leaves its panels and
        // balloons; running it again would find the same, or bring back
        // blocks deleted by hand.
        return Ok(match scanned && !detected(&page.layers) {
            true => Stage::VARIANTS.iter().copied().collect(),
            false => BTreeSet::new(),
        });
    }
    let (width, height) = (page.size.width, page.size.height);
    let mut needs = BTreeSet::new();
    for (layer, block, locked) in blocks {
        if locked {
            continue;
        }
        let blank = block.source.content.text().trim().is_empty();
        // A sound effect kept as art is translated only to hide its
        // clean-up; any other block when untranslated, or kept as art once
        // and not any more.
        let translates = match sfx::skipped(page, layer.id, settings) {
            true => !sfx::kept_original(layer).is_empty(),
            false => {
                untranslated(layer, block) || (block.translation.is_none() && Auto::of(layer).kept)
            }
        };
        if !blank && translates {
            needs.insert(Stage::Translate);
        }
        // Recognition reads only a source whose frame lies on the page.
        let readable = block
            .source
            .bounds()
            .and_then(|bounds| Rect::covering(bounds, width, height))
            .is_some();
        if blank && scanned && readable && !recorded(layer, OCR) {
            needs.insert(Stage::Recognize);
            if translates {
                needs.insert(Stage::Translate);
            }
        }
        // Inpainting writes the clean-up, and only under the mask, which is
        // read last, as it's decoded.
        if scanned
            && !block.cleanup.locked
            && !recorded(layer, INPAINTING)
            && session
                .tiles(block.mask.id, None)?
                .coverage(width, height)
                .is_some()
        {
            needs.insert(Stage::Inpaint);
        }
    }
    Ok(needs)
}

/// Whether Koharu's Detection stage left its record on any of `layers` or
/// the layers in them.
fn detected(layers: &[Layer]) -> bool {
    layers
        .iter()
        .any(|layer| recorded(layer, DETECTION) || detected(&layer.children))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use booharu_core::{
        context::{ChapterBrief, GlossaryEntry, SeriesStore},
        settings::SfxMode,
    };
    use image::{ImageFormat, Rgb, RgbImage};
    use koharu_scene::{Created, Edit, Hash, LayerId, Op, Uuid};
    use serde_json::{Value, json};
    use tempfile::TempDir;

    use super::*;

    const SIZE: u32 = 400;

    /// A chapter built in code: its pages and layers by name.
    struct Chapter {
        _dir: TempDir,
        session: Session,
        pages: BTreeMap<String, PageId>,
        layers: BTreeMap<String, LayerId>,
    }

    impl Chapter {
        /// The pages `pages` in order, each named and scanned or not, with
        /// the layers `ops` add.
        fn new(pages: &[(&str, bool)], ops: Vec<Value>) -> Result<Self> {
            let dir = tempfile::tempdir()?;
            let mut session = Session::create(dir.path().join("test.khr"))?;
            let mut scan = Vec::new();
            RgbImage::from_pixel(SIZE, SIZE, Rgb([255; 3]))
                .write_to(&mut Cursor::new(&mut scan), ImageFormat::Png)?;
            let file = Hash::of(&scan);
            let added = pages.iter().enumerate().map(|(index, (name, scanned))| {
                let id = format!("${name}");
                match scanned {
                    true => json!({"op": "add_page", "id": id, "index": index, "background": file}),
                    false => json!({"op": "add_page", "id": id, "index": index,
                        "size": {"width": SIZE, "height": SIZE}}),
                }
            });
            let mut edit = vec![Op::AddFile { bytes: scan }];
            for op in added.chain(ops) {
                edit.push(serde_json::from_value(op)?);
            }
            let change = session.apply(Edit {
                label: "test".to_owned(),
                ops: edit,
            })?;
            let mut chapter = Self {
                _dir: dir,
                session,
                pages: BTreeMap::new(),
                layers: BTreeMap::new(),
            };
            for created in change.created {
                match created {
                    Created::Page {
                        id,
                        label: Some(label),
                    } => {
                        chapter.pages.insert(label, id);
                    }
                    Created::Layer {
                        id,
                        label: Some(label),
                    } => {
                        chapter.layers.insert(label, id);
                    }
                    _ => {}
                }
            }
            Ok(chapter)
        }

        fn page(&self, name: &str) -> PageId {
            self.pages[name]
        }

        fn needs(&self, page: &str) -> Result<Vec<Stage>> {
            self.needs_under(page, &LetteringSettings::default())
        }

        fn needs_under(&self, page: &str, settings: &LetteringSettings) -> Result<Vec<Stage>> {
            let project = self.session.project();
            Ok(
                needs(&self.session, project.page(self.page(page))?, settings)?
                    .into_iter()
                    .collect(),
            )
        }

        fn plan(&self, pages: &[&str]) -> Result<Vec<(Vec<Stage>, Vec<PageId>)>> {
            let pages: Vec<PageId> = pages.iter().map(|page| self.page(page)).collect();
            Ok(plan(&self.session, &pages, &LetteringSettings::default())?
                .into_iter()
                .map(|group| (group.stages, group.pages))
                .collect())
        }
    }

    /// The ops adding the block `name` to `page`, at `index` of it, its
    /// source `wording` framed in the `index`th column, with the `c2pa`
    /// records of `steps` and, when `masked`, a mask over its frame.
    fn block(
        page: &str,
        name: &str,
        index: usize,
        wording: &str,
        steps: &[&str],
        masked: bool,
    ) -> Vec<Value> {
        let x = 10 + 50 * index as i32;
        let c2pa: serde_json::Map<String, Value> = steps
            .iter()
            .map(|step| ((*step).to_owned(), json!({"action": "c2pa.created"})))
            .collect();
        let mut ops = vec![
            json!({"op": "add_layer", "id": format!("${name}"), "page": format!("${page}"),
            "index": index, "layer": {"kind": "block", "source": {
                "frame": {"box": {"bounds": {"x": x, "y": 20, "width": 40, "height": 100}}},
                "content": wording,
            }, "attributes": {"c2pa": c2pa}}}),
        ];
        if masked {
            ops.push(json!({"op": "write_pixels", "layer": format!("${name}.mask"),
                "rect": {"x": x, "y": 20, "width": 40, "height": 100}, "pixels": vec![255u8; 4000]}));
        }
        ops
    }

    /// The op giving block `name` the translation `text`.
    fn translated(name: &str, text: &str) -> Value {
        json!({"op": "set_text", "layer": format!("${name}"), "text": text})
    }

    /// The op the Translation stage writes on block `name` when it was given
    /// the source back unchanged.
    fn given_back(name: &str) -> Value {
        json!({"op": "update_layer", "layer": format!("${name}"),
            "patch": {"translation": null, "cleanup": {"visible": false}}})
    }

    /// The op patching layer `name`.
    fn patched(name: &str, patch: Value) -> Value {
        json!({"op": "update_layer", "layer": format!("${name}"), "patch": patch})
    }

    const EVERY: [Stage; 4] = [
        Stage::Detect,
        Stage::Recognize,
        Stage::Inpaint,
        Stage::Translate,
    ];
    const NOTHING: [Stage; 0] = [];

    #[test]
    fn a_fresh_page_needs_every_stage() -> Result<()> {
        let chapter = Chapter::new(
            &[("fresh", true), ("detected", true)],
            [
                block("detected", "a", 0, "", &["detection"], true),
                block("detected", "b", 1, "", &["detection"], true),
            ]
            .concat(),
        )?;
        assert_eq!(chapter.needs("fresh")?, EVERY);
        assert_eq!(
            chapter.needs("detected")?,
            [Stage::Recognize, Stage::Inpaint, Stage::Translate]
        );
        Ok(())
    }

    #[test]
    fn a_page_stopped_after_recognition_needs_inpainting_and_translation() -> Result<()> {
        let chapter = Chapter::new(
            &[("read", true)],
            [
                block("read", "a", 0, "どうして", &["detection", "ocr"], true),
                block("read", "b", 1, "ここに", &["detection", "ocr"], true),
            ]
            .concat(),
        )?;
        assert_eq!(chapter.needs("read")?, [Stage::Inpaint, Stage::Translate]);
        Ok(())
    }

    #[test]
    fn a_page_with_every_record_needs_nothing() -> Result<()> {
        let done = ["detection", "ocr", "inpainting", "translation"];
        let mut ops = [
            block("done", "a", 0, "どうして", &done, true),
            block("done", "b", 1, "ここに", &done, true),
        ]
        .concat();
        ops.extend([translated("a", "Why?"), translated("b", "Here?")]);
        let chapter = Chapter::new(&[("done", true)], ops)?;
        assert_eq!(chapter.needs("done")?, NOTHING);
        assert_eq!(chapter.plan(&["done"])?, Vec::new());
        Ok(())
    }

    #[test]
    fn a_blank_source_is_read_then_counts_as_translated() -> Result<()> {
        let chapter = Chapter::new(
            &[("unread", true), ("read_blank", true)],
            [
                // Never read: it's read, then translated.
                block("unread", "a", 0, "", &["detection", "inpainting"], true),
                // Read as nothing: nothing to translate.
                block(
                    "read_blank",
                    "b",
                    0,
                    " ",
                    &["detection", "ocr", "inpainting"],
                    true,
                ),
            ]
            .concat(),
        )?;
        assert_eq!(
            chapter.needs("unread")?,
            [Stage::Recognize, Stage::Translate]
        );
        assert_eq!(chapter.needs("read_blank")?, NOTHING);
        Ok(())
    }

    #[test]
    fn a_translation_given_back_unchanged_counts_as_translated() -> Result<()> {
        let read = ["detection", "ocr", "inpainting"];
        let mut ops = [
            block("p", "kept", 0, "OK!", &read, true),
            block("q", "untouched", 0, "OK!", &read, true),
            block("r", "typed", 0, "どうして", &read, true),
        ]
        .concat();
        ops.extend([given_back("kept"), translated("typed", "Why?")]);
        let chapter = Chapter::new(&[("p", true), ("q", true), ("r", true)], ops)?;
        assert_eq!(chapter.needs("p")?, NOTHING);
        assert_eq!(chapter.needs("q")?, [Stage::Translate]);
        // A translation typed by hand, without the stage's record, is kept.
        assert_eq!(chapter.needs("r")?, NOTHING);
        Ok(())
    }

    #[test]
    fn a_page_without_blocks_needs_every_stage_if_it_has_a_scan() -> Result<()> {
        let chapter = Chapter::new(
            &[("scanned", true), ("blank", false), ("typed", false)],
            [
                block("typed", "a", 0, "どうして", &[], true),
                block("typed", "b", 1, "", &[], true),
            ]
            .concat(),
        )?;
        assert_eq!(chapter.needs("scanned")?, EVERY);
        // Without a scan, nothing can be found, read, or cleaned there, but
        // typed wording is translated.
        assert_eq!(chapter.needs("blank")?, NOTHING);
        assert_eq!(chapter.needs("typed")?, [Stage::Translate]);
        Ok(())
    }

    /// The op adding a `kind` layer (a panel or a balloon) named `name` to
    /// `page`, inside layer `parent` if one is named, with the `c2pa`
    /// record detection leaves when `detected`.
    fn outlined(page: &str, name: &str, kind: &str, parent: Option<&str>, detected: bool) -> Value {
        let c2pa = match detected {
            true => json!({"detection": {"action": "c2pa.created"}}),
            false => json!({}),
        };
        json!({"op": "add_layer", "id": format!("${name}"), "page": format!("${page}"),
            "parent": parent.map(|parent| format!("${parent}")), "index": 0,
            "layer": {"kind": kind, "outline": "M10 10L390 10L390 390L10 390Z",
            "attributes": {"c2pa": c2pa}}})
    }

    #[test]
    fn a_page_detection_found_no_text_on_needs_nothing() -> Result<()> {
        let chapter = Chapter::new(
            &[("panels", true), ("balloon", true), ("drawn", true)],
            vec![
                // What detection leaves on a page with panels and no text.
                outlined("panels", "p1", "panel", None, true),
                // A balloon detection found, in a panel drawn by hand, after
                // its blocks were deleted.
                outlined("balloon", "p2", "panel", None, false),
                outlined("balloon", "b2", "balloon", Some("p2"), true),
                // Only a panel drawn by hand: detection never ran here.
                outlined("drawn", "p3", "panel", None, false),
            ],
        )?;
        assert_eq!(chapter.needs("panels")?, NOTHING);
        assert_eq!(chapter.needs("balloon")?, NOTHING);
        assert_eq!(chapter.needs("drawn")?, EVERY);
        assert!(chapter.plan(&["panels", "balloon"])?.is_empty());
        Ok(())
    }

    #[test]
    fn what_koharus_stages_leave_as_it_is_isnt_needed() -> Result<()> {
        let mut ops = [
            // No mask: inpainting skips it.
            block("unmasked", "a", 0, "どうして", &["ocr", "translation"], false),
            // A point source: recognition can't read it.
            vec![json!({"op": "add_layer", "id": "$b", "page": "$point", "index": 0,
                "layer": {"kind": "block", "source": {"content": ""},
                    "attributes": {"c2pa": {"inpainting": {"action": "c2pa.created"}}}}})],
            // Locked, or inside a locked group: the stages can't write it.
            block("locked", "c", 0, "", &[], true),
            vec![json!({"op": "add_layer", "id": "$g", "page": "$grouped", "index": 0,
                "layer": {"kind": "group"}})],
            vec![json!({"op": "add_layer", "id": "$d", "page": "$grouped", "parent": "$g",
                "index": 0, "layer": {"kind": "block", "source": {"content": "どうして",
                    "frame": {"box": {"bounds": {"x": 10, "y": 20, "width": 40, "height": 100}}}}}})],
            // A locked clean-up: inpainting can't write it.
            block("cleanup_locked", "e", 0, "どうして", &["ocr", "translation"], true),
            // Hidden: read, cleaned, and translated as any other.
            block("hidden", "f", 0, "", &[], true),
        ]
        .concat();
        ops.extend([
            patched("c", json!({"locked": true})),
            patched("g", json!({"locked": true})),
            patched("e", json!({"cleanup": {"locked": true}})),
            patched("f", json!({"visible": false})),
        ]);
        let chapter = Chapter::new(
            &[
                ("unmasked", true),
                ("point", true),
                ("locked", true),
                ("grouped", true),
                ("cleanup_locked", true),
                ("hidden", true),
            ],
            ops,
        )?;
        for page in ["unmasked", "point", "locked", "grouped", "cleanup_locked"] {
            assert_eq!(chapter.needs(page)?, NOTHING, "{page}");
        }
        assert_eq!(
            chapter.needs("hidden")?,
            [Stage::Recognize, Stage::Inpaint, Stage::Translate]
        );
        Ok(())
    }

    #[test]
    fn a_sound_effect_kept_as_art_is_translated_only_to_hide_its_clean_up() -> Result<()> {
        // On each page a sound effect read and cleaned, with no translation:
        // its clean-up showing, kept as art with the mark, or hidden by hand.
        let mut ops = Vec::new();
        for (page, name) in [("shown", "a"), ("kept", "b"), ("hidden", "c")] {
            ops.extend(block(
                page,
                name,
                0,
                "ドン",
                &["detection", "ocr", "inpainting"],
                false,
            ));
            ops.push(patched(name, json!({"role": "sfx"})));
        }
        ops.push(patched("c", json!({"cleanup": {"visible": false}})));
        let mut chapter = Chapter::new(&[("shown", true), ("kept", true), ("hidden", true)], ops)?;
        let project = chapter.session.project();
        let kept = sfx::kept_original(project.layer(chapter.layers["b"])?);
        chapter.session.apply(Edit {
            label: "test".to_owned(),
            ops: kept,
        })?;

        // Kept as art: translated only while its clean-up shows, which the
        // Translation stage hides; one hidden by hand stays as it is.
        assert_eq!(chapter.needs("shown")?, [Stage::Translate]);
        assert_eq!(chapter.needs("kept")?, NOTHING);
        assert_eq!(chapter.needs("hidden")?, NOTHING);
        assert_eq!(chapter.plan(&["kept", "hidden"])?, Vec::new());

        // With every sound effect translated, the one kept as art before
        // needs its translation too, which shows its clean-up again.
        let all = LetteringSettings {
            sfx: SfxMode::TranslateAll,
            ..LetteringSettings::default()
        };
        assert_eq!(chapter.needs_under("shown", &all)?, [Stage::Translate]);
        assert_eq!(chapter.needs_under("kept", &all)?, [Stage::Translate]);
        assert_eq!(chapter.needs_under("hidden", &all)?, NOTHING);
        Ok(())
    }

    #[test]
    fn a_locked_sound_effect_is_left_out_whatever_it_needs() -> Result<()> {
        let mut ops = block("p", "a", 0, "ドン", &["detection", "ocr"], false);
        ops.push(patched("a", json!({"role": "sfx", "locked": true})));
        let chapter = Chapter::new(&[("p", true)], ops)?;
        let all = LetteringSettings {
            sfx: SfxMode::TranslateAll,
            ..LetteringSettings::default()
        };
        assert_eq!(chapter.needs("p")?, NOTHING);
        assert_eq!(chapter.needs_under("p", &all)?, NOTHING);
        Ok(())
    }

    #[test]
    fn pages_that_need_the_same_stages_run_together_by_their_first_page() -> Result<()> {
        let read = ["detection", "ocr", "inpainting"];
        let done = ["detection", "ocr", "inpainting", "translation"];
        let mut ops = [
            block("p1", "a", 0, "どうして", &read, true),
            block("p3", "c", 0, "ここに", &read, true),
            block("p4", "d", 0, "OK!", &done, true),
            block("p5", "e", 0, "まだ", &read, true),
        ]
        .concat();
        ops.push(translated("d", "OK!"));
        let pages = [
            ("p1", true),
            ("p2", true),
            ("p3", true),
            ("p4", true),
            ("p5", true),
            ("p6", true),
        ];
        let chapter = Chapter::new(&pages, ops)?;
        let page = |name| chapter.page(name);
        assert_eq!(
            chapter.plan(&["p1", "p2", "p3", "p4", "p5", "p6"])?,
            [
                (
                    vec![Stage::Translate],
                    vec![page("p1"), page("p3"), page("p5")]
                ),
                (EVERY.to_vec(), vec![page("p2"), page("p6")]),
            ]
        );
        // Only the pages asked for, still in page order.
        assert_eq!(
            chapter.plan(&["p6", "p4", "p3"])?,
            [
                (vec![Stage::Translate], vec![page("p3")]),
                (EVERY.to_vec(), vec![page("p6")]),
            ]
        );
        Ok(())
    }

    #[test]
    fn fewer_stages_run_once_on_every_page_named() -> Result<()> {
        let chapter = Chapter::new(
            &[("p1", true), ("p2", true), ("p3", true)],
            block("p2", "a", 0, "どうして", &["detection", "ocr"], true),
        )?;
        let (p1, p2, p3) = (chapter.page("p1"), chapter.page("p2"), chapter.page("p3"));
        let session = &chapter.session;
        assert_eq!(
            groups(
                session,
                &RunTarget::Project {},
                &[Stage::Translate, Stage::Recognize, Stage::Translate],
                &LetteringSettings::default(),
            )?,
            [PlanGroup {
                stages: vec![Stage::Recognize, Stage::Translate],
                pages: vec![p1, p2, p3],
                read: false,
            }]
        );
        // Every stage carries on with what the pages named still need.
        let pages = RunTarget::Pages {
            pages: vec![p3, p2],
        };
        assert_eq!(
            groups(session, &pages, &EVERY, &LetteringSettings::default())?,
            [
                PlanGroup {
                    stages: vec![Stage::Inpaint, Stage::Translate],
                    pages: vec![p2],
                    read: false,
                },
                PlanGroup {
                    stages: EVERY.to_vec(),
                    pages: vec![p3],
                    read: false,
                },
            ]
        );
        assert!(all_stages(&EVERY));
        assert!(!all_stages(&EVERY[1..]));
        Ok(())
    }

    #[test]
    fn a_plan_that_finds_text_and_translates_translates_last() -> Result<()> {
        let mut ops = [
            // Translation alone.
            block(
                "p1",
                "a",
                0,
                "どうして",
                &["detection", "ocr", "inpainting"],
                true,
            ),
            // Recognition, inpainting, and the translation of what it reads.
            block("p3", "c", 0, "", &["detection"], true),
            // Inpainting alone.
            block(
                "p4",
                "d",
                0,
                "OK!",
                &["detection", "ocr", "translation"],
                true,
            ),
            // Recognition and inpainting, its translation recorded.
            block("p6", "f", 0, "", &["detection", "translation"], true),
        ]
        .concat();
        ops.push(translated("d", "OK!"));
        let names = ["p1", "p2", "p3", "p4", "p5", "p6"];
        let chapter = Chapter::new(&names.map(|name| (name, true)), ops)?;
        let page = |name: &str| chapter.page(name);
        let group = |stages: &[Stage], pages: &[&str]| PlanGroup {
            stages: stages.to_vec(),
            pages: pages.iter().map(|name| page(name)).collect(),
            read: false,
        };
        let (session, project) = (&chapter.session, chapter.session.project());
        let planned = groups(
            session,
            &RunTarget::Project {},
            &EVERY,
            &LetteringSettings::default(),
        )?;
        let (read, inpaint) = (Stage::Recognize, Stage::Inpaint);
        assert_eq!(
            planned,
            [
                group(&[Stage::Translate], &["p1"]),
                group(&EVERY, &["p2", "p5"]),
                group(&[read, inpaint, Stage::Translate], &["p3"]),
                group(&[inpaint], &["p4"]),
                group(&[read, inpaint], &["p6"]),
            ]
        );
        // Every group without Translation first, those left alike joined,
        // then one Translation run on every page a group translated.
        assert_eq!(
            translate_last(&project, &planned),
            Some(vec![
                group(&EVERY[..3], &["p2", "p5"]),
                group(&[read, inpaint], &["p3", "p6"]),
                group(&[inpaint], &["p4"]),
                group(&[Stage::Translate], &["p1", "p2", "p3", "p5"]),
            ])
        );
        // A plan that finds no text it translates runs as planned.
        let pages = [page("p1"), page("p4"), page("p6")];
        let settings = LetteringSettings::default();
        assert_eq!(
            translate_last(&project, &plan(session, &pages, &settings)?),
            None
        );
        // The pages that still need translation, in page order.
        let pages = names.map(page);
        assert_eq!(
            to_translate(
                session,
                &[pages[5], pages[4], pages[3], pages[2], pages[0]],
                &settings,
            )?,
            [page("p1"), page("p3"), page("p5")]
        );
        Ok(())
    }

    #[test]
    fn a_preset_that_reads_and_translates_translates_last_too() -> Result<()> {
        let chapter = Chapter::new(
            &[("p1", true), ("p2", true)],
            block("p2", "a", 0, "どうして", &["detection", "ocr"], true),
        )?;
        let (p1, p2) = (chapter.page("p1"), chapter.page("p2"));
        let (session, project) = (&chapter.session, chapter.session.project());
        let preset = groups(
            session,
            &RunTarget::Project {},
            &[Stage::Recognize, Stage::Translate],
            &LetteringSettings::default(),
        )?;
        assert_eq!(
            translate_last(&project, &preset),
            Some(vec![
                PlanGroup {
                    stages: vec![Stage::Recognize],
                    pages: vec![p1, p2],
                    read: false,
                },
                PlanGroup {
                    stages: vec![Stage::Translate],
                    pages: vec![p1, p2],
                    read: false,
                },
            ])
        );
        // One that finds no text, or translates none, runs as it is.
        let others: [&[Stage]; 3] = [
            &[Stage::Translate],
            &[Stage::Inpaint, Stage::Translate],
            &EVERY[..3],
        ];
        for stages in others {
            let preset = groups(
                session,
                &RunTarget::Project {},
                stages,
                &LetteringSettings::default(),
            )?;
            assert_eq!(translate_last(&project, &preset), None, "{stages:?}");
        }
        Ok(())
    }

    #[test]
    fn a_press_that_reads_translates_last_whatever_it_finds() -> Result<()> {
        let chapter = Chapter::new(
            &[("p1", true), ("p2", true), ("p3", true)],
            [
                // Inpainting and translation left.
                block("p1", "a", 0, "どうして", &["detection", "ocr"], true),
                // Translation alone.
                block(
                    "p2",
                    "b",
                    0,
                    "ここに",
                    &["detection", "ocr", "inpainting"],
                    true,
                ),
            ]
            .concat(),
        )?;
        let (p1, p2, p3) = (chapter.page("p1"), chapter.page("p2"), chapter.page("p3"));
        let (session, project) = (&chapter.session, chapter.session.project());
        let group = |stages: &[Stage], pages: Vec<PageId>| PlanGroup {
            stages: stages.to_vec(),
            pages,
            read: false,
        };
        // Nothing is found, so the press translates as planned, but split
        // around the read: every group without Translation, the read of
        // every page of the project, then one Translation run.
        let left = groups(
            session,
            &RunTarget::Pages {
                pages: vec![p1, p2],
            },
            &EVERY,
            &LetteringSettings::default(),
        )?;
        assert_eq!(
            left,
            [
                group(&[Stage::Inpaint, Stage::Translate], vec![p1]),
                group(&[Stage::Translate], vec![p2]),
            ]
        );
        assert_eq!(translate_last(&project, &left), None);
        assert_eq!(
            read_before_translation(&project, &left),
            Some(vec![
                group(&[Stage::Inpaint], vec![p1]),
                PlanGroup {
                    stages: Vec::new(),
                    pages: vec![p1, p2, p3],
                    read: true,
                },
                group(&[Stage::Translate], vec![p1, p2]),
            ])
        );
        // With Translation alone left, the read comes first.
        let translating = [group(&[Stage::Translate], vec![p2])];
        let split = read_before_translation(&project, &translating).expect("a split");
        assert!(split[0].read && split[0].stages.is_empty(), "{split:?}");
        assert_eq!(split[1..], translating);
        // A plan that doesn't translate neither splits nor reads.
        let cleaning = [group(&[Stage::Inpaint], vec![p1])];
        assert_eq!(translation_last(&project, &cleaning), None);
        assert_eq!(read_before_translation(&project, &cleaning), None);
        Ok(())
    }

    #[test]
    fn a_chapter_has_notes_with_a_series_suggested_terms_or_a_brief() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let project = dir.path().join("Ch.1.khr");
        let store = SeriesStore::new(dir.path().join("series"));
        let opened = || -> Result<SeriesStore> { Ok(store.clone()) };
        let none = || -> Result<SeriesStore> { anyhow::bail!("a chapter linking no series") };
        // The chapter as its side file and series give it, which the test's
        // own `Chapter` hides.
        use booharu_core::context::Chapter as Notes;
        assert!(!has_notes(&Notes::read(&project, none)?));
        let series = store.create("Blue Archive")?;
        ChapterContext::update(&project, |context| {
            context.series = Some(series.id.clone());
        })?;
        assert!(has_notes(&Notes::read(&project, opened)?));
        // A series whose file is gone sends nothing.
        store.delete(&series.id)?;
        assert!(!has_notes(&Notes::read(&project, opened)?));
        ChapterContext::update(&project, |context| {
            context
                .suggestions
                .push(GlossaryEntry::new("先生", "Sensei"));
        })?;
        assert!(has_notes(&Notes::read(&project, opened)?));
        ChapterContext::update(&project, |context| {
            context.series = None;
            context.suggestions.clear();
            context.brief = Some(ChapterBrief {
                summary: "Hina meets Sensei.".to_owned(),
                ..ChapterBrief::default()
            });
        })?;
        assert!(has_notes(&Notes::read(&project, none)?));
        Ok(())
    }

    #[test]
    fn a_brief_is_missing_without_one_or_with_a_blank_summary() {
        let briefed = |summary: &str| ChapterContext {
            brief: Some(ChapterBrief {
                summary: summary.to_owned(),
                edited: true,
                ..ChapterBrief::default()
            }),
            ..ChapterContext::default()
        };
        assert!(brief_missing(&ChapterContext::default()));
        assert!(brief_missing(&briefed(" \n")));
        assert!(!brief_missing(&briefed("Hina meets Sensei.")));
    }

    #[test]
    fn a_page_has_text_to_read_once_a_block_has_a_source() -> Result<()> {
        let chapter = Chapter::new(
            &[("blank", true), ("unread", true), ("read", true)],
            [
                block("unread", "a", 0, " ", &["detection"], true),
                block("read", "b", 0, "どうして", &["detection", "ocr"], true),
            ]
            .concat(),
        )?;
        let project = chapter.session.project();
        let page = |name| chapter.page(name);
        assert!(!has_text(&project, &[page("blank"), page("unread")]));
        assert!(has_text(&project, &[page("unread"), page("read")]));
        assert!(!has_text(&project, &[]));
        Ok(())
    }

    #[test]
    fn a_target_names_its_pages_in_page_order() -> Result<()> {
        let chapter = Chapter::new(&[("p1", false), ("p2", false), ("p3", false)], Vec::new())?;
        let project = chapter.session.project();
        let (p1, p2, p3) = (chapter.page("p1"), chapter.page("p2"), chapter.page("p3"));
        assert_eq!(pages(&project, &RunTarget::Project {})?, [p1, p2, p3]);
        let chosen = RunTarget::Pages {
            pages: vec![p3, p1, p3],
        };
        assert_eq!(pages(&project, &chosen)?, [p1, p3]);
        assert_eq!(chosen.target(), Target::Pages(vec![p3, p1, p3]));
        assert_eq!(RunTarget::Project {}.target(), Target::Project);
        // A page the project doesn't have fails, as Koharu's run does.
        let gone = RunTarget::Pages {
            pages: vec![p1, PageId(Uuid::nil())],
        };
        assert!(pages(&project, &gone).is_err());
        Ok(())
    }

    #[test]
    fn a_target_reads_as_the_interface_sends_it() -> Result<()> {
        let page = PageId(Uuid::nil());
        let read = |value: Value| serde_json::from_value::<RunTarget>(value);
        assert_eq!(read(json!({"kind": "project"}))?, RunTarget::Project {});
        assert_eq!(
            read(json!({"kind": "pages", "pages": [page]}))?,
            RunTarget::Pages { pages: vec![page] }
        );
        for wrong in [
            json!({"kind": "project", "pages": []}),
            json!({"kind": "pages", "pages": [], "stages": []}),
            json!({"kind": "blocks", "blocks": []}),
            json!({"kind": "pages"}),
        ] {
            assert!(read(wrong.clone()).is_err(), "{wrong}");
        }
        Ok(())
    }

    #[test]
    fn a_run_is_going_from_queued_until_it_ends() {
        let queued = |run| Event::Queued {
            run: RunId(run),
            steps: 1,
            stages: vec![Stage::Detect],
            pages: true,
        };
        let ended = |run| Event::Ended {
            run: RunId(run),
            outcome: koharu_pipeline::Outcome::Stopped,
        };
        assert!(!run_going(&[]));
        assert!(run_going(&[queued(1)]));
        assert!(run_going(&[queued(1), queued(2), ended(1)]));
        assert!(!run_going(&[queued(1), queued(2), ended(2), ended(1)]));
    }
}
