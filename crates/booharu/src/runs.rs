//! Carrying on: what the pages of a chapter still need, which the Run
//! button runs when all four stages are on, as groups of the pages that
//! need the same stages, each queued as an ordinary run of Koharu's.
//!
//! A page's needs come from the records Koharu's stages leave in each
//! block's `c2pa` attributes:
//!
//! - a page with no blocks needs every stage from detection, which replaces
//!   only unlocked blocks;
//! - a block with a blank source and no recognition record needs
//!   recognition, and the translation a block with that source would;
//! - a block with no inpainting record needs inpainting;
//! - a block with a source, no translation, and no translation record needs
//!   translation, and one the Translation stage was given back unchanged (no
//!   translation, its clean-up hidden) or with a blank source counts as
//!   translated.
//!
//! A stage that would leave a block as it is isn't needed for it, so a page
//! that's done reads as done however often it's run: Koharu's stages read
//! hidden blocks as any other, and can't write a locked one, or one inside a
//! locked layer, which fails its page's edit; detection, recognition, and
//! inpainting read the page's scan, and do nothing on a page without one;
//! recognition reads only a source whose frame lies on the page; inpainting
//! redraws only under a mask, and writes the clean-up, which can't be
//! locked. Inside a stage nothing changes: a page that needs translation is
//! translated whole, as Koharu does.
//!
//! `carry_on`, from `going`, queues the groups one after another, for the
//! desktop command and the agent tool alike.

mod going;

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use booharu_core::styles::recorded;
use koharu_pipeline::{RunId, Stage, Target};
use koharu_scene::{Block, Content, Layer, Page, PageId, Project, Rect, Session};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use specta::Type;
use strum::VariantArray as _;

pub use self::going::{RunHost, carry_on};

/// The `c2pa` step Koharu's Recognition stage records.
const OCR: &str = "ocr";
/// The `c2pa` step Koharu's Inpainting stage records.
const INPAINTING: &str = "inpainting";
/// The `c2pa` step Koharu's Translation stage records.
const TRANSLATION: &str = "translation";

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

/// Pages that need the same stages, which one run works on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type, JsonSchema)]
pub struct PlanGroup {
    /// In stage order.
    pub stages: Vec<Stage>,
    /// In page order.
    pub pages: Vec<PageId>,
}

/// What carrying on queues: the runs of `groups`, one after another, which
/// every `booharu_run_group` event of `group` announces, the first of them
/// `run`. No run and no groups when nothing is left to do.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RunPlan {
    pub group: u64,
    pub groups: Vec<PlanGroup>,
    pub run: Option<RunId>,
}

/// Whether `stages` holds every stage, which makes the Run button carry on
/// rather than run them all again.
#[must_use]
pub fn all_stages(stages: &[Stage]) -> bool {
    Stage::VARIANTS.iter().all(|stage| stages.contains(stage))
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
/// still need, planned; with fewer, one ordinary run of those stages on
/// every page it names, as Koharu runs them.
pub fn groups(session: &Session, target: &RunTarget, stages: &[Stage]) -> Result<Vec<PlanGroup>> {
    let pages = pages(&session.project(), target)?;
    if all_stages(stages) {
        return plan(session, &pages);
    }
    let stages: BTreeSet<Stage> = stages.iter().copied().collect();
    Ok(vec![PlanGroup {
        stages: stages.into_iter().collect(),
        pages,
    }])
}

/// The work `pages` of `session`'s project still need, as groups of the
/// pages that need the same stages, in page order, ordered by their first
/// page; empty when nothing is left.
pub fn plan(session: &Session, pages: &[PageId]) -> Result<Vec<PlanGroup>> {
    let project = session.project();
    let mut groups: Vec<PlanGroup> = Vec::new();
    let mut found: BTreeMap<BTreeSet<Stage>, usize> = BTreeMap::new();
    for page in project.pages().filter(|page| pages.contains(&page.id)) {
        let stages = needs(session, page)?;
        if stages.is_empty() {
            continue;
        }
        let at = *found.entry(stages.clone()).or_insert_with(|| {
            groups.push(PlanGroup {
                stages: stages.into_iter().collect(),
                pages: Vec::new(),
            });
            groups.len() - 1
        });
        groups[at].pages.push(page.id);
    }
    Ok(groups)
}

/// The stages `page` of `session`'s project still needs, as the module's
/// rules read them.
fn needs(session: &Session, page: &Page) -> Result<BTreeSet<Stage>> {
    let scanned = page.background.is_some();
    let blocks = blocks(page);
    if blocks.is_empty() {
        return Ok(match scanned {
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
        if !blank && untranslated(layer, block) {
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
            if untranslated(layer, block) {
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

/// `page`'s blocks in reading order, as Koharu's stages read them, each
/// with whether it or a layer it's in is locked.
fn blocks(page: &Page) -> Vec<(&Layer, &Block, bool)> {
    fn walk<'p>(layers: &'p [Layer], locked: bool, blocks: &mut Vec<(&'p Layer, &'p Block, bool)>) {
        for layer in layers {
            let locked = locked || layer.locked;
            if let Content::Block(block) = &layer.content {
                blocks.push((layer, block, locked));
            }
            walk(&layer.children, locked, blocks);
        }
    }
    let mut blocks = Vec::new();
    walk(&page.layers, false, &mut blocks);
    blocks
}

/// Whether `block` has no translation, wasn't translated, and wasn't given
/// back unchanged, which leaves no translation and hides its clean-up.
fn untranslated(layer: &Layer, block: &Block) -> bool {
    block.translation.is_none() && !recorded(layer, TRANSLATION) && block.cleanup.visible
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

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
            let project = self.session.project();
            Ok(needs(&self.session, project.page(self.page(page))?)?
                .into_iter()
                .collect())
        }

        fn plan(&self, pages: &[&str]) -> Result<Vec<(Vec<Stage>, Vec<PageId>)>> {
            let pages: Vec<PageId> = pages.iter().map(|page| self.page(page)).collect();
            Ok(plan(&self.session, &pages)?
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
                &[Stage::Translate, Stage::Recognize, Stage::Translate]
            )?,
            [PlanGroup {
                stages: vec![Stage::Recognize, Stage::Translate],
                pages: vec![p1, p2, p3],
            }]
        );
        // Every stage carries on with what the pages named still need.
        let pages = RunTarget::Pages {
            pages: vec![p3, p2],
        };
        assert_eq!(
            groups(session, &pages, &EVERY)?,
            [
                PlanGroup {
                    stages: vec![Stage::Inpaint, Stage::Translate],
                    pages: vec![p2],
                },
                PlanGroup {
                    stages: EVERY.to_vec(),
                    pages: vec![p3],
                },
            ]
        );
        assert!(all_stages(&EVERY));
        assert!(!all_stages(&EVERY[1..]));
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
}
