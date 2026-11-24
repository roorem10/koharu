//! The run filters of Stage 9: what each page stage of a run leaves out of
//! a page, as booharu-core's `protect` rules say, and the counts of it that
//! the page's `Event::Skipped` tells.
//!
//! - [`Left::sent`], in the Translation and Recognition stages' reads (T1,
//!   R1), narrows a page to the blocks the stage may change, counting the
//!   rest. A translation left with no line to send still hides the clean-up
//!   of each sound effect it skips ([`Left::unsent`], T2).
//! - [`Writing`], in each stage's write, checks again against the project as
//!   the write finds it, so a block locked or fixed by hand while the model
//!   ran stays as it is, counted, rather than failing the page's edit or
//!   being written over: the Translation stage's answers (T3, T4), with the
//!   words its model picked to stress in each, a subtitle run's subtitles,
//!   and the kept art of the effects it skips; the Recognition stage's
//!   readings (R2); the Inpainting stage's clean-ups, with the kept art of
//!   the effects a translation skips (I1, I2); and Detect again's additions
//!   (D2), which go above the Original scan and the results under the
//!   page's lettering (D17, D37).
//! - [`stays`], in Detect again's removals (D1), keeps what the user made (a
//!   split's halves among it), fixed (a clean-up hidden by hand, or shown
//!   again by hand on an effect kept as art, among it), labelled, noted,
//!   subtitled, or locked, and whatever holds it.
//! - [`Left::told`] tells each page's counts once its edit is in, when any is
//!   above 0.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use booharu_core::{
    protect::{self, Reason},
    runs::RunStage,
    settings::Snapshot,
    sfx, status,
};
use icu_locale_core::LanguageIdentifier;
use koharu_scene::{Content, Layer, LayerId, Op, Page, PageId, Project, Ref};
use parking_lot::Mutex;
use tokio_util::sync::CancellationToken;

use crate::{
    Event, PipelineConfig, RunId, Stage, TranslatorConfig,
    event::Events,
    stages::{Member, Task, translate::written},
};

/// The filters of the run that queued a page stage's task, by its snapshot,
/// and what they left out of each of the task's pages so far.
#[derive(Clone)]
pub(crate) struct Left(Arc<Filters>);

struct Filters {
    stage: Stage,
    booharu: Snapshot,
    events: Events,
    /// Each page's counts, in the order the task met its pages.
    pages: Mutex<Vec<Tally>>,
}

/// What a stage left out of one page of a run, by reason.
struct Tally {
    run: RunId,
    page: PageId,
    /// Whether the run named blocks, as the page was handed to the task.
    named: bool,
    cancel: CancellationToken,
    sound_effects: u32,
    hand_fixed: u32,
    locked: u32,
}

impl Tally {
    fn new(member: &Member) -> Self {
        Self {
            run: member.run,
            page: member.page,
            named: member.blocks.is_some(),
            cancel: member.cancel.clone(),
            sound_effects: 0,
            hand_fixed: 0,
            locked: 0,
        }
    }
}

impl Left {
    /// The filters of `task`, whose run's settings are `config`'s: its first
    /// page's, which a recognition task shares with the other pages it
    /// reads, as the scheduler batches only runs whose filters match.
    pub(crate) fn new(task: &Task, config: &PipelineConfig) -> Self {
        Self::of(task.stage, config.booharu.clone(), task.events.clone())
    }

    fn of(stage: Stage, booharu: Snapshot, events: Events) -> Self {
        Self(Arc::new(Filters {
            stage,
            booharu,
            events,
            pages: Mutex::new(Vec::new()),
        }))
    }

    /// `member` with only the blocks the stage may change on its page, as
    /// the project is now, counting the others; as it is when the stage
    /// leaves out none (T1, R1). A translation sends only blocks with words,
    /// so it counts no other.
    pub(crate) fn sent(&self, member: &Member) -> Member {
        let named = self.tally(member, |tally| tally.named);
        let project = member.session.lock().project();
        let Ok(page) = project.page(member.page) else {
            return member.clone();
        };
        let translating = self.0.stage == Stage::Translate;
        let blocks = member.blocks(&project);
        let all = blocks.len();
        let mut kept = Vec::with_capacity(all);
        for (id, block) in blocks {
            let reason = project
                .layer(id)
                .ok()
                .and_then(|layer| self.why(page, layer, named));
            match reason {
                Some(reason) => {
                    if !translating || !block.source.content.text().trim().is_empty() {
                        self.count(member, reason);
                    }
                }
                None => kept.push(id),
            }
        }
        if kept.len() == all {
            return member.clone();
        }
        Member {
            blocks: Some(kept.into()),
            ..member.clone()
        }
    }

    /// Why the stage leaves out the block `layer` holds on `page`, for a run
    /// that named blocks when `named`.
    fn why(&self, page: &Page, layer: &Layer, named: bool) -> Option<Reason> {
        let booharu = &self.0.booharu;
        protect::left_out(
            page,
            layer,
            RunStage::from(self.0.stage),
            &booharu.lettering,
            booharu.redo,
            named && booharu.subtitle_run,
        )
    }

    /// `f` of the counts of `member`'s page, new ones the first time.
    fn tally<R>(&self, member: &Member, f: impl FnOnce(&mut Tally) -> R) -> R {
        let mut pages = self.0.pages.lock();
        let at = match pages
            .iter()
            .position(|tally| (tally.run, tally.page) == (member.run, member.page))
        {
            Some(at) => at,
            None => {
                pages.push(Tally::new(member));
                pages.len() - 1
            }
        };
        f(&mut pages[at])
    }

    fn count(&self, member: &Member, reason: Reason) {
        self.tally(member, |tally| match reason {
            Reason::SoundEffect => tally.sound_effects += 1,
            Reason::HandFixed => tally.hand_fixed += 1,
            Reason::Locked => tally.locked += 1,
        });
    }

    /// Tells each page's counts as one `Event::Skipped`, when any is above 0
    /// and its run didn't stop, which wrote nothing; then forgets them.
    pub(crate) fn told(&self) {
        let pages = std::mem::take(&mut *self.0.pages.lock());
        for tally in pages {
            let counted = tally.sound_effects + tally.hand_fixed + tally.locked;
            if counted == 0 || tally.cancel.is_cancelled() {
                continue;
            }
            self.0.events.send(Event::Skipped {
                run: tally.run,
                page: tally.page,
                stage: self.0.stage,
                sound_effects: tally.sound_effects,
                hand_fixed: tally.hand_fixed,
                locked: tally.locked,
                blocks: tally.named,
            });
        }
    }

    /// The end of a translation with no line to send (T2): the clean-up of
    /// each sound effect it skips hidden, so the effect keeps its art, as the
    /// stage's own edit, then the counts told.
    pub(crate) async fn unsent(&self, member: &Member) -> anyhow::Result<()> {
        let writing = self.writing(member);
        member
            .apply(self.0.stage, move |project| Ok(writing.kept(project)))
            .await?;
        self.told();
        Ok(())
    }

    /// The filters as `member`'s write checks them.
    pub(crate) fn writing(&self, member: &Member) -> Writing {
        Writing {
            left: self.clone(),
            member: member.clone(),
            picked: HashMap::new(),
        }
    }
}

/// The filters as a stage's write checks them for one page, against the
/// project as the write finds it.
pub(crate) struct Writing {
    left: Left,
    member: Member,
    /// The words the run's model picked to stress in each block's answer,
    /// by block: none but in a translation that asked for them.
    picked: HashMap<LayerId, Vec<String>>,
}

impl Writing {
    /// The write of a translation whose model picked `picked`, the words to
    /// stress in each block's answer, by block ([`super::picked`]), which
    /// [`Self::written`] hands on with the answers it writes; a block it
    /// leaves out, and a subtitle, take none.
    pub(crate) fn stressing(self, picked: HashMap<LayerId, Vec<String>>) -> Self {
        Self { picked, ..self }
    }

    fn named(&self) -> bool {
        self.left.tally(&self.member, |tally| tally.named)
    }

    /// Whether the stage leaves out `block` now, counting it if so: one
    /// locked or fixed by hand since the read stays as it is (R2, I2).
    pub(crate) fn leaves(&self, project: &Project, block: LayerId) -> bool {
        let named = self.named();
        let (Ok(page), Ok(layer)) = (project.page(self.member.page), project.layer(block)) else {
            return false;
        };
        let Some(reason) = self.left.why(page, layer, named) else {
            return false;
        };
        self.left.count(&self.member, reason);
        true
    }

    /// The Translation stage's edit of `translated`, the answers for the
    /// blocks it sent, in reading order, by `translator`, into `project` as
    /// it is now (T3, T4): an answer for a block the stage leaves out now is
    /// dropped, counted; in a subtitle run, each named sound effect's answer
    /// becomes its subtitle (`subtitle`), its words to stress unmarked; the
    /// rest go through `written`, with the words to stress picked in them,
    /// followed by the kept art of the sound effects the stage skips.
    pub(crate) fn written(
        &self,
        project: &Project,
        translated: Vec<(LayerId, LanguageIdentifier, String)>,
        translator: &TranslatorConfig,
    ) -> anyhow::Result<Vec<Op>> {
        let booharu = &self.left.0.booharu;
        let Ok(page) = project.page(self.member.page) else {
            return written(project, translated, translator, booharu, &self.picked);
        };
        let named = self.named();
        let subtitling = named && booharu.subtitle_run;
        let mut answers = Vec::with_capacity(translated.len());
        let mut subtitles = Vec::new();
        for (block, source, text) in translated {
            let Ok(layer) = project.layer(block) else {
                // `written` passes over a block that's gone.
                answers.push((block, source, text));
                continue;
            };
            if subtitling && sfx::sound_effect(page, block, &booharu.lettering) {
                subtitles.extend(self.subtitle(page, layer, &text, &translator.target)?);
                continue;
            }
            match self.left.why(page, layer, named) {
                Some(reason) => self.left.count(&self.member, reason),
                None => answers.push((block, source, text)),
            }
        }
        let mut ops = written(project, answers, translator, booharu, &self.picked)?;
        ops.extend(self.kept(project));
        ops.extend(numbered(subtitles)?);
        Ok(ops)
    }

    /// The ops giving the sound effect `layer` on `page` its answer `text`,
    /// in `language`, as its subtitle: none when it's locked now, counted;
    /// when it has a translation of some words now, counted as fixed by
    /// hand; when it got a subtitle since the read; when the answer is its
    /// source, both trimmed; or when its source has no box to put one
    /// beside, as Add subtitle refuses one; each leaves it as it is.
    fn subtitle(
        &self,
        page: &Page,
        layer: &Layer,
        text: &str,
        language: &LanguageIdentifier,
    ) -> anyhow::Result<Option<Vec<Op>>> {
        let Content::Block(block) = &layer.content else {
            return Ok(None);
        };
        if protect::locked(page, layer.id) {
            self.left.count(&self.member, Reason::Locked);
            return Ok(None);
        }
        // Add subtitle runs one only for an effect with no words of its own
        // to subtitle it with, so a translation now was typed, or written by
        // another run, while the model ran. The subtitle would take it away:
        // it stays, and Add subtitle makes the subtitle from it (D1).
        let translated = block
            .translation
            .as_ref()
            .is_some_and(|translation| !translation.content.text().trim().is_empty());
        if translated {
            self.left.count(&self.member, Reason::HandFixed);
            return Ok(None);
        }
        if sfx::subtitled(page, layer.id)
            || text.trim() == block.source.content.text().trim()
            || block.source.bounds().is_none()
        {
            return Ok(None);
        }
        let booharu = &self.left.0.booharu;
        sfx::subtitle_ops(
            page,
            layer.id,
            text,
            language,
            &booharu.lettering.subtitle,
            booharu.font.as_deref(),
        )
        .map(Some)
    }

    /// The kept art of each sound effect a translation skips among the
    /// blocks of the page the stage works on: its clean-up hidden with the
    /// kept mark, as `sfx::kept_original` keeps one with no translation, so
    /// it stays hidden whichever of cleaning and translating comes first
    /// (T2, T3, I2). A translation keeps only effects with words, as it
    /// sends no other.
    pub(crate) fn kept(&self, project: &Project) -> Vec<Op> {
        let Ok(page) = project.page(self.member.page) else {
            return Vec::new();
        };
        let named = self.named();
        let booharu = &self.left.0.booharu;
        let translating = self.left.0.stage == Stage::Translate;
        self.member
            .blocks(project)
            .into_iter()
            .filter(|(_, block)| !translating || !block.source.content.text().trim().is_empty())
            .filter_map(|(id, _)| project.layer(id).ok())
            .filter(|layer| {
                let skipped = protect::left_out(
                    page,
                    layer,
                    RunStage::Translate,
                    &booharu.lettering,
                    booharu.redo,
                    named && booharu.subtitle_run,
                );
                skipped == Some(Reason::SoundEffect)
            })
            .flat_map(sfx::kept_original)
            .collect()
    }

    /// Detect again's additions to `page`, `additions` as Koharu's stage
    /// made them, but for each new block whose source lies over a block that
    /// stays, one locked or kept (`protect::keeps`), by at least half the
    /// smaller of their areas, with its mask, and each new panel or balloon
    /// that held nothing else, up to the top; the rest numbered again within
    /// each parent from 0 (D2), but the page's own from
    /// `booharu_core::cleanup::detect_start` of the layers this Detect's
    /// removals leave, so the new layers go above the Original scan and the
    /// results that stay under the page's lettering, and never cover them
    /// (D17, D37). Each block that stays is counted, as locked or as kept by
    /// hand.
    pub(crate) fn added(&self, page: &Page, additions: Vec<Op>) -> Vec<Op> {
        let mut staying = Vec::new();
        for (layer, block, locked) in status::blocks(page) {
            let reason = match locked {
                true => Reason::Locked,
                false if protect::keeps(page, layer) => Reason::HandFixed,
                false => continue,
            };
            self.left.count(&self.member, reason);
            staying.extend(block.source.bounds());
        }
        let remaining: Vec<&Layer> = page
            .layers
            .iter()
            .filter(|layer| remains(page, layer))
            .collect();
        let start = booharu_core::cleanup::detect_start(&remaining);
        unrepeated(additions, &staying, start)
    }
}

/// Whether the top-level `layer` of `page` is still there after Detect's
/// removals, by the rule `removals` (detect/mod.rs) applies: a locked layer,
/// one that stays, and any but a panel, balloon or block are.
fn remains(page: &Page, layer: &Layer) -> bool {
    layer.locked
        || stays(page, layer)
        || !matches!(
            layer.content,
            Content::Panel(_) | Content::Balloon(_) | Content::Block(_)
        )
}

/// Whether Detect again leaves the unlocked `layer` on `page` in place (D1):
/// it keeps it (`protect::keeps`), or a layer inside it, at any depth, is
/// kept or locked. Its removals then go on into what it holds.
pub(crate) fn stays(page: &Page, layer: &Layer) -> bool {
    protect::keeps(page, layer) || layer.children.iter().any(|child| stays(page, child))
}

/// A new layer of Detect again's additions.
struct New {
    label: String,
    parent: Option<String>,
    container: bool,
    children: usize,
    held: usize,
    dropped: bool,
}

/// `additions` but for each new block whose source lies over one of
/// `staying` by at least half the smaller of their areas, with its parts'
/// pixels, and each new panel or balloon whose layers were all dropped, up
/// to the top; the rest numbered again within each parent from 0, in order,
/// but the page's own top level from `start`.
fn unrepeated(additions: Vec<Op>, staying: &[kurbo::Rect], start: usize) -> Vec<Op> {
    let mut new: Vec<New> = additions
        .iter()
        .filter_map(|op| {
            let Op::AddLayer {
                id: Some(id),
                parent,
                layer,
                ..
            } = op
            else {
                return None;
            };
            let repeats = match &layer.content {
                Content::Block(block) => block
                    .source
                    .bounds()
                    .is_some_and(|bounds| staying.iter().any(|&kept| over(bounds, kept))),
                _ => false,
            };
            Some(New {
                label: id.label()?.to_owned(),
                parent: parent.as_ref().and_then(Ref::label).map(str::to_owned),
                container: matches!(layer.content, Content::Panel(_) | Content::Balloon(_)),
                children: 0,
                held: 0,
                dropped: repeats,
            })
        })
        .collect();
    let at: HashMap<String, usize> = new
        .iter()
        .enumerate()
        .map(|(at, layer)| (layer.label.clone(), at))
        .collect();
    // A parent comes before its layers, so going back meets each layer
    // before its parent.
    for index in (0..new.len()).rev() {
        let layer = &mut new[index];
        if layer.container && layer.children > 0 && layer.held == 0 {
            layer.dropped = true;
        }
        let held = usize::from(!layer.dropped);
        let parent = layer.parent.as_ref().and_then(|parent| at.get(parent));
        if let Some(&parent) = parent {
            new[parent].children += 1;
            new[parent].held += held;
        }
    }
    let dropped: HashSet<&str> = new
        .iter()
        .filter(|layer| layer.dropped)
        .map(|layer| layer.label.as_str())
        .collect();
    // A new layer's label, or a part's of a new block, as `$d3.mask`.
    let gone = |layer: &Ref| {
        layer
            .label()
            .and_then(|label| label.split('.').next())
            .is_some_and(|label| dropped.contains(label))
    };
    let mut places: HashMap<Option<String>, usize> = HashMap::from([(None, start)]);
    additions
        .into_iter()
        .filter_map(|op| match op {
            Op::AddLayer {
                id,
                page,
                parent,
                layer,
                ..
            } => {
                if id.as_ref().is_some_and(gone) {
                    return None;
                }
                let place = places
                    .entry(parent.as_ref().map(ToString::to_string))
                    .or_default();
                let index = *place;
                *place += 1;
                Some(Op::AddLayer {
                    id,
                    page,
                    parent,
                    index,
                    layer,
                })
            }
            Op::WritePixels { ref layer, .. } if gone(layer) => None,
            op => Some(op),
        })
        .collect()
}

/// Whether `a` and `b` overlap by at least half the smaller of their areas.
fn over(a: kurbo::Rect, b: kurbo::Rect) -> bool {
    let (a, b) = (a.abs(), b.abs());
    let smaller = a.area().min(b.area());
    smaller > 0.0 && a.intersect(b).area() >= smaller / 2.0
}

/// `subtitles`, each one's ops, as one edit's: the first keeps
/// `sfx::SUBTITLE_REF`, and each other gets one of its own; they go in from
/// the last place among a parent's layers, so adding one never moves the
/// place another goes, right after its effect.
fn numbered(mut subtitles: Vec<Vec<Op>>) -> anyhow::Result<Vec<Op>> {
    let place = |ops: &Vec<Op>| match ops.first() {
        Some(Op::AddLayer { index, .. }) => *index,
        _ => 0,
    };
    subtitles.sort_by_key(|ops| std::cmp::Reverse(place(ops)));
    let mut all = Vec::new();
    for (at, mut ops) in subtitles.into_iter().enumerate() {
        if at > 0
            && let Some(Op::AddLayer { id, .. }) = ops.first_mut()
        {
            let label = format!("${}-{at}", sfx::SUBTITLE_REF);
            *id = Some(Ref::try_from(label).map_err(anyhow::Error::msg)?);
        }
        all.extend(ops);
    }
    Ok(all)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet},
        io::Cursor,
        time::Duration,
    };

    use anyhow::{Context as _, Result};
    use booharu_core::{
        attrs::{self, Auto, ReviewNote, SfxSubtitle},
        emphasis::EmphasisSettings,
        settings::{LetteringSettings, SfxMode},
        styles::{given_back, recognized, recorded},
    };
    use icu_locale_core::langid;
    use image::{ImageFormat, Rgb, RgbImage};
    use koharu_scene::{Block, Created, Edit, Hash, RichText, Session, Span};
    use parking_lot::RwLock;
    use petgraph::stable_graph::NodeIndex;
    use serde_json::{Value, json};

    use super::*;
    use crate::{
        Outcome, Target,
        scheduler::{Command, Plan, Run, Scheduler},
        stages::detect::removals,
    };

    /// The source wording of every block.
    const SOURCE: &str = "どうして";
    /// A page's side, in pixels.
    const SIDE: u32 = 200;

    /// A block of Koharu's `role`, its source's box 20 by 40 at `x`, `y`,
    /// reading [`SOURCE`], as Koharu's stages detect and read one.
    fn block(role: &str, (x, y): (f64, f64)) -> Value {
        json!({"kind": "block", "role": role, "source": {
            "frame": {"box": {"bounds": {"x": x, "y": y, "width": 20.0, "height": 40.0}}},
            "content": SOURCE,
        }, "attributes": {"c2pa": {
            "detection": {"action": "c2pa.created"},
            "ocr": {"action": "c2pa.created"},
        }}})
    }

    /// A panel or a balloon, `kind`, over `outline`, as Koharu's Detection
    /// stage makes one.
    fn detected(kind: &str, outline: &str) -> Value {
        json!({"kind": kind, "outline": outline,
            "attributes": {"c2pa": {"detection": {"action": "c2pa.created"}}}})
    }

    /// The op adding `layer` as `$label` at `index` among `parent`'s layers,
    /// or the page `$p`'s.
    fn add(label: &str, parent: Option<&str>, index: usize, layer: Value) -> Value {
        json!({"op": "add_layer", "id": format!("${label}"), "page": "$p",
            "parent": parent.map(|parent| format!("${parent}")), "index": index, "layer": layer})
    }

    /// The op giving the block `$label` a mask over the 20 by 40 box at `x`,
    /// `y`.
    fn mask(label: &str, (x, y): (i32, i32)) -> Value {
        json!({"op": "write_pixels", "layer": format!("${label}.mask"),
            "rect": {"x": x, "y": y, "width": 20, "height": 40}, "pixels": vec![255u8; 20 * 40]})
    }

    /// A run's snapshot with sound effects under `sfx`, Redo boxes' flag
    /// `redo`, and a subtitle run's `subtitle_run`.
    fn snapshot(sfx: SfxMode, redo: bool, subtitle_run: bool) -> Snapshot {
        Snapshot {
            lettering: LetteringSettings {
                sfx,
                ..LetteringSettings::default()
            },
            redo,
            subtitle_run,
            ..Snapshot::default()
        }
    }

    fn skip() -> Snapshot {
        snapshot(SfxMode::Skip, false, false)
    }

    /// Each `Event::Skipped` `told` holds: its stage, its counts of sound
    /// effects, hand fixes, and locks, and whether the run named blocks.
    fn skipped(told: &[Event]) -> Vec<(Stage, u32, u32, u32, bool)> {
        told.iter()
            .filter_map(|event| match *event {
                Event::Skipped {
                    stage,
                    sound_effects,
                    hand_fixed,
                    locked,
                    blocks,
                    ..
                } => Some((stage, sound_effects, hand_fixed, locked, blocks)),
                _ => None,
            })
            .collect()
    }

    /// `(label, text)`, each as the Translation stage's model answers for
    /// the block `label`.
    fn answers(
        fixture: &Fixture,
        said: &[(&str, &str)],
    ) -> Vec<(LayerId, LanguageIdentifier, String)> {
        said.iter()
            .map(|(label, text)| (fixture.id(label), langid!("ja"), (*text).to_owned()))
            .collect()
    }

    /// A project of one page, 200 pixels square, with a white scan when
    /// asked, holding what the ops given add on `$p`, each block's wording
    /// recorded as the Recognition stage records it, so a corrected one reads
    /// as fixed by hand.
    struct Fixture {
        _dir: tempfile::TempDir,
        session: Arc<Mutex<Session>>,
        page: PageId,
        ids: BTreeMap<String, LayerId>,
    }

    impl Fixture {
        fn new(scanned: bool, ops: Vec<Value>) -> Result<Self> {
            let dir = tempfile::tempdir()?;
            let mut session = Session::create(dir.path().join("test.khr"))?;
            let mut page = json!({"op": "add_page", "id": "$p", "index": 0,
                "size": {"width": SIDE, "height": SIDE}});
            let mut edit = Vec::new();
            if scanned {
                let mut scan = Vec::new();
                RgbImage::from_pixel(SIDE, SIDE, Rgb([255; 3]))
                    .write_to(&mut Cursor::new(&mut scan), ImageFormat::Png)?;
                // A page with a scan takes its size.
                page["size"] = Value::Null;
                page["background"] = json!(Hash::of(&scan));
                edit.push(Op::AddFile { bytes: scan });
            }
            edit.push(serde_json::from_value(page)?);
            for op in ops {
                edit.push(serde_json::from_value(op)?);
            }
            let change = session.apply(Edit {
                label: "test".to_owned(),
                ops: edit,
            })?;
            let ids = change
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
            let page = change.pages.added[0];
            let project = session.project();
            let records = project
                .blocks(page)?
                .into_iter()
                .map(|(id, block)| recognized(project.layer(id)?, &block.source.content.text()))
                .collect::<Result<Vec<_>>>()?;
            session.apply(Edit {
                label: "test".to_owned(),
                ops: records,
            })?;
            Ok(Self {
                _dir: dir,
                session: Arc::new(Mutex::new(session)),
                page,
                ids,
            })
        }

        fn id(&self, label: &str) -> LayerId {
            self.ids[label]
        }

        fn label(&self, id: LayerId) -> String {
            self.ids
                .iter()
                .find_map(|(label, at)| (*at == id).then(|| label.clone()))
                .unwrap_or_else(|| id.to_string())
        }

        fn project(&self) -> Project {
            self.session.lock().project()
        }

        fn page(&self) -> Result<Page> {
            Ok(self.project().page(self.page)?.clone())
        }

        fn layer(&self, label: &str) -> Result<Layer> {
            Ok(self.project().layer(self.id(label))?.clone())
        }

        fn block(&self, label: &str) -> Result<Block> {
            match self.layer(label)?.content {
                Content::Block(block) => Ok(*block),
                _ => anyhow::bail!("{label} isn't a block"),
            }
        }

        fn translation(&self, label: &str) -> Result<Option<String>> {
            Ok(self
                .block(label)?
                .translation
                .map(|translation| translation.content.text()))
        }

        fn edit(&self, ops: Vec<Op>) -> Result<()> {
            self.session.lock().apply(Edit {
                label: "test".to_owned(),
                ops,
            })?;
            Ok(())
        }

        fn op(&self, op: Value) -> Result<()> {
            self.edit(vec![serde_json::from_value(op)?])
        }

        fn lock(&self, label: &str) -> Result<()> {
            self.op(json!({"op": "update_layer", "layer": self.id(label),
                "patch": {"locked": true}}))
        }

        /// Types `text` as the translation of the block `label`, by hand.
        fn type_in(&self, label: &str, text: &str) -> Result<()> {
            self.edit(vec![Op::SetText {
                layer: self.id(label).into(),
                text: text.to_owned(),
            }])
        }

        /// Corrects the source of the block `label` by hand.
        fn correct(&self, label: &str) -> Result<()> {
            self.op(json!({"op": "update_layer", "layer": self.id(label),
                "patch": {"source": {"content": "なぜ"}}}))
        }

        /// The page as run 1 hands it to a task, limited to the blocks
        /// `named` when it names some.
        fn member(&self, named: Option<&[&str]>) -> Member {
            Member {
                run: RunId(1),
                job: NodeIndex::new(0),
                page: self.page,
                index: 0,
                blocks: named.map(|named| named.iter().map(|label| self.id(label)).collect()),
                session: Arc::clone(&self.session),
                cancel: CancellationToken::new(),
            }
        }

        /// The blocks `member` has a task work on.
        fn sent(&self, member: &Member) -> BTreeSet<String> {
            member
                .blocks(&self.project())
                .into_iter()
                .map(|(id, _)| self.label(id))
                .collect()
        }

        /// Every layer on the page, in the tree at any depth.
        fn layers(&self) -> Result<BTreeSet<String>> {
            fn walk(layers: &[Layer], found: &mut Vec<LayerId>) {
                for layer in layers {
                    found.push(layer.id);
                    walk(&layer.children, found);
                }
            }
            let mut found = Vec::new();
            walk(&self.page()?.layers, &mut found);
            Ok(found.into_iter().map(|id| self.label(id)).collect())
        }

        /// What run 1, of `stages` on `target` with the snapshot `booharu`,
        /// told once it ended done, on a scheduler of its own, queued as
        /// `Pipeline::run` queues a run but with no setting read.
        async fn run(
            &self,
            target: Target,
            stages: &[Stage],
            booharu: Snapshot,
        ) -> Result<Vec<Event>> {
            let events = Events::new();
            let config = PipelineConfig {
                booharu,
                ..PipelineConfig::default()
            };
            let commands = Scheduler::spawn(Arc::new(RwLock::new(config.clone())), events.clone());
            let stages: BTreeSet<Stage> = stages.iter().copied().collect();
            let project = self.project();
            let plan = Plan::new(&project, &target, &stages)?;
            let run = RunId(1);
            events.send(Event::Queued {
                run,
                steps: plan.pages() * stages.len(),
                stages: stages.iter().copied().collect(),
                pages: true,
            });
            let queued = Run::new(
                run,
                Arc::clone(&self.session),
                project.id(),
                config,
                plan,
                &stages,
            );
            commands
                .send(Command::Run(queued))
                .map_err(|_| anyhow::anyhow!("the scheduler ended"))?;
            let told = events
                .history(run, Duration::from_secs(60))
                .await
                .context("a history")?;
            let done = Event::Ended {
                run,
                outcome: Outcome::Done,
            };
            anyhow::ensure!(told.last() == Some(&done), "{told:#?}");
            Ok(told)
        }
    }

    // S9-e: the run filters of the Translation and Recognition stages.

    #[tokio::test]
    async fn a_translation_with_no_line_to_send_still_hides_its_effects_clean_up_and_tells_why()
    -> Result<()> {
        let fixture = Fixture::new(
            false,
            vec![
                add("effect", None, 0, block("sfx", (10.0, 10.0))),
                add("typed", None, 1, block("text", (40.0, 10.0))),
                add("locked", None, 2, block("text", (70.0, 10.0))),
                // An effect with no words, which a translation never sends.
                add(
                    "blank",
                    None,
                    3,
                    json!({"kind": "block", "role": "sfx", "source": {}}),
                ),
            ],
        )?;
        fixture.type_in("typed", "Hm")?;
        fixture.lock("locked")?;
        // No model loads: every line is left out before the request.
        let told = fixture
            .run(Target::Project, &[Stage::Translate], skip())
            .await?;
        assert_eq!(skipped(&told), [(Stage::Translate, 1, 1, 1, false)]);
        assert!(told.iter().any(|event| matches!(event,
            Event::Skipped { run: RunId(1), page, .. } if *page == fixture.page)));
        // The effect keeps its art, in the stage's own edit.
        assert!(!fixture.block("effect")?.cleanup.visible);
        assert!(Auto::of(&fixture.layer("effect")?).kept);
        assert_eq!(fixture.translation("effect")?, None);
        assert!(fixture.block("blank")?.cleanup.visible);
        // What was fixed by hand or locked stays as it was.
        assert_eq!(fixture.translation("typed")?.as_deref(), Some("Hm"));
        assert_eq!(fixture.translation("locked")?, None);
        Ok(())
    }

    #[tokio::test]
    async fn reading_leaves_out_blocks_corrected_by_hand_or_locked_and_tells_why() -> Result<()> {
        let fixture = Fixture::new(
            true,
            vec![
                add("corrected", None, 0, block("text", (10.0, 10.0))),
                add("locked", None, 1, block("text", (60.0, 10.0))),
            ],
        )?;
        fixture.correct("corrected")?;
        fixture.lock("locked")?;
        // No model loads: the page has nothing left to read.
        let told = fixture
            .run(Target::Project, &[Stage::Recognize], skip())
            .await?;
        assert_eq!(skipped(&told), [(Stage::Recognize, 0, 1, 1, false)]);
        assert_eq!(fixture.block("corrected")?.source.content.text(), "なぜ");
        Ok(())
    }

    #[test]
    fn redo_boxes_lifts_a_hand_fix_but_never_a_lock_or_a_sound_effects_skip() -> Result<()> {
        let fixture = Fixture::new(
            false,
            vec![
                add("plain", None, 0, block("text", (10.0, 10.0))),
                add("corrected", None, 1, block("text", (40.0, 10.0))),
                add("typed", None, 2, block("text", (70.0, 10.0))),
                add("effect", None, 3, block("sfx", (100.0, 10.0))),
                add("locked", None, 4, block("text", (130.0, 10.0))),
            ],
        )?;
        fixture.correct("corrected")?;
        fixture.type_in("typed", "Hm")?;
        fixture.lock("locked")?;
        let sent = |stage: Stage, redo: bool| {
            let events = Events::new();
            let left = Left::of(stage, snapshot(SfxMode::Skip, redo, false), events.clone());
            let sent = fixture.sent(&left.sent(&fixture.member(None)));
            left.told();
            (sent, skipped(&events.told()))
        };
        let set = |labels: &[&str]| labels.iter().map(|label| (*label).to_owned()).collect();
        assert_eq!(
            sent(Stage::Recognize, false),
            (
                set(&["plain", "typed", "effect"]),
                vec![(Stage::Recognize, 0, 1, 1, false)]
            )
        );
        assert_eq!(
            sent(Stage::Recognize, true),
            (
                set(&["plain", "corrected", "typed", "effect"]),
                vec![(Stage::Recognize, 0, 0, 1, false)]
            )
        );
        assert_eq!(
            sent(Stage::Translate, false),
            (
                set(&["plain", "corrected"]),
                vec![(Stage::Translate, 1, 1, 1, false)]
            )
        );
        assert_eq!(
            sent(Stage::Translate, true),
            (
                set(&["plain", "corrected", "typed"]),
                vec![(Stage::Translate, 1, 0, 1, false)]
            )
        );
        Ok(())
    }

    #[test]
    fn a_lock_or_a_hand_fix_made_while_the_model_ran_leaves_its_block_as_it_is() -> Result<()> {
        let layers = || {
            vec![
                add("a", None, 0, block("text", (10.0, 10.0))),
                add("b", None, 1, block("text", (40.0, 10.0))),
                add("c", None, 2, block("text", (70.0, 10.0))),
            ]
        };
        // A translation reads every block; while its model runs, b is locked
        // and c's translation typed by hand.
        let fixture = Fixture::new(false, layers())?;
        let events = Events::new();
        let left = Left::of(Stage::Translate, skip(), events.clone());
        let member = fixture.member(None);
        assert_eq!(left.sent(&member).blocks, None, "it leaves out none");
        fixture.lock("b")?;
        fixture.type_in("c", "By hand")?;
        let said = answers(&fixture, &[("a", "A!"), ("b", "B!"), ("c", "C!")]);
        let ops = left.writing(&member).written(
            &fixture.project(),
            said,
            &TranslatorConfig::default(),
        )?;
        // The page's edit goes in, neither failing on b nor writing over c.
        fixture.edit(ops)?;
        left.told();
        assert_eq!(fixture.translation("a")?.as_deref(), Some("A!"));
        assert_eq!(fixture.translation("b")?, None);
        assert_eq!(fixture.translation("c")?.as_deref(), Some("By hand"));
        assert_eq!(
            skipped(&events.told()),
            [(Stage::Translate, 0, 1, 1, false)]
        );

        // A reading the same way: a locked, b corrected by hand meanwhile.
        let fixture = Fixture::new(false, layers())?;
        let events = Events::new();
        let left = Left::of(Stage::Recognize, skip(), events.clone());
        let member = left.sent(&fixture.member(None));
        assert_eq!(member.blocks, None, "it leaves out none");
        fixture.lock("a")?;
        fixture.correct("b")?;
        let writing = left.writing(&member);
        let project = fixture.project();
        let left_out: Vec<bool> = ["a", "b", "c"]
            .iter()
            .map(|label| writing.leaves(&project, fixture.id(label)))
            .collect();
        assert_eq!(left_out, [true, true, false]);
        left.told();
        assert_eq!(
            skipped(&events.told()),
            [(Stage::Recognize, 0, 1, 1, false)]
        );
        Ok(())
    }

    // S9-e: a sound effect kept as art, by translating and by cleaning.

    #[tokio::test]
    async fn a_skipped_effects_clean_up_stays_hidden_whichever_of_cleaning_and_translating_is_first()
    -> Result<()> {
        for first in [Stage::Translate, Stage::Inpaint] {
            let fixture = Fixture::new(
                true,
                vec![
                    add("effect", None, 0, block("sfx", (40.0, 40.0))),
                    mask("effect", (40, 40)),
                ],
            )?;
            let then = match first {
                Stage::Translate => Stage::Inpaint,
                _ => Stage::Translate,
            };
            // Neither loads a model: the clean-up is plain paper, and the
            // translation has no line to send.
            for stage in [first, then] {
                let told = fixture.run(Target::Project, &[stage], skip()).await?;
                let tells = match stage {
                    Stage::Translate => vec![(Stage::Translate, 1, 0, 0, false)],
                    _ => Vec::new(),
                };
                assert_eq!(skipped(&told), tells, "{stage:?}, {first:?} first");
                assert!(
                    !fixture.block("effect")?.cleanup.visible,
                    "after {stage:?}, {first:?} first"
                );
                assert!(Auto::of(&fixture.layer("effect")?).kept);
            }
            let cleaned = fixture.layer("effect")?;
            assert!(recorded(&cleaned, status::INPAINTING), "{first:?} first");

            // Set to Translate all, a run translates it, and its clean-up
            // shows under its translation.
            let left = Left::of(
                Stage::Translate,
                snapshot(SfxMode::TranslateAll, false, false),
                Events::new(),
            );
            let member = fixture.member(None);
            assert_eq!(left.sent(&member).blocks, None, "it's sent");
            let said = answers(&fixture, &[("effect", "BAM")]);
            let ops = left.writing(&member).written(
                &fixture.project(),
                said,
                &TranslatorConfig::default(),
            )?;
            fixture.edit(ops)?;
            assert_eq!(fixture.translation("effect")?.as_deref(), Some("BAM"));
            assert!(fixture.block("effect")?.cleanup.visible);
            assert!(!Auto::of(&fixture.layer("effect")?).kept);
        }
        Ok(())
    }

    #[tokio::test]
    async fn cleaning_leaves_a_locked_clean_up_or_block_as_it_is_and_tells_why() -> Result<()> {
        let fixture = Fixture::new(
            true,
            vec![
                add("free", None, 0, block("text", (10.0, 10.0))),
                mask("free", (10, 10)),
                add("part", None, 1, block("text", (70.0, 10.0))),
                mask("part", (70, 10)),
                add("whole", None, 2, block("text", (130.0, 10.0))),
                mask("whole", (130, 10)),
            ],
        )?;
        fixture.op(json!({"op": "update_layer", "layer": fixture.id("part"),
            "patch": {"cleanup": {"locked": true}}}))?;
        fixture.lock("whole")?;
        // The page's edit goes in: no model loads, as the clean-ups are
        // plain paper, and no locked layer is written.
        let told = fixture
            .run(Target::Project, &[Stage::Inpaint], skip())
            .await?;
        assert_eq!(skipped(&told), [(Stage::Inpaint, 0, 0, 2, false)]);
        let cleaned = |label| {
            fixture
                .layer(label)
                .map(|layer| recorded(&layer, status::INPAINTING))
        };
        assert!(cleaned("free")?);
        assert!(!cleaned("part")?);
        assert!(!cleaned("whole")?);
        Ok(())
    }

    #[test]
    fn a_page_tells_nothing_left_out_once_and_a_stopped_run_tells_nothing() -> Result<()> {
        let fixture = Fixture::new(
            false,
            vec![
                add("plain", None, 0, block("text", (10.0, 10.0))),
                add("locked", None, 1, block("text", (40.0, 10.0))),
            ],
        )?;
        let events = Events::new();
        let left = Left::of(Stage::Recognize, skip(), events.clone());
        // Nothing left out tells nothing.
        left.sent(&fixture.member(None));
        left.told();
        assert!(events.told().is_empty());
        fixture.lock("locked")?;
        // Each run's page tells its own, once; a stopped run's, nothing.
        let other = Member {
            run: RunId(2),
            ..fixture.member(Some(&["locked"]))
        };
        let stopped = Member {
            run: RunId(3),
            ..fixture.member(None)
        };
        stopped.cancel.cancel();
        for member in [fixture.member(None), other, stopped] {
            left.sent(&member);
        }
        left.told();
        left.told();
        let told: Vec<(RunId, bool)> = events
            .told()
            .into_iter()
            .filter_map(|event| match event {
                Event::Skipped {
                    run,
                    locked: 1,
                    blocks,
                    ..
                } => Some((run, blocks)),
                _ => None,
            })
            .collect();
        assert_eq!(told, [(RunId(1), false), (RunId(2), true)]);
        Ok(())
    }

    // S9-g: a subtitle run.

    #[test]
    fn a_subtitle_run_writes_each_named_effects_answer_as_its_subtitle() -> Result<()> {
        let fixture = Fixture::new(
            false,
            vec![
                add("effect", None, 0, block("sfx", (20.0, 20.0))),
                add("hidden", None, 1, block("sfx", (80.0, 20.0))),
                add("talk", None, 2, block("text", (140.0, 20.0))),
            ],
        )?;
        // One effect's clean-up was hidden by hand.
        fixture.op(json!({"op": "update_layer", "layer": fixture.id("hidden"),
            "patch": {"cleanup": {"visible": false}}}))?;
        let events = Events::new();
        let left = Left::of(
            Stage::Translate,
            snapshot(SfxMode::Skip, false, true),
            events.clone(),
        );
        let member = fixture.member(Some(&["effect", "hidden", "talk"]));
        // Each named effect is sent, skipped or kept by hand as it is.
        assert_eq!(left.sent(&member).blocks, member.blocks);
        let said = answers(
            &fixture,
            &[("effect", "BAM"), ("hidden", "THUD"), ("talk", "Why?")],
        );
        let ops = left.writing(&member).written(
            &fixture.project(),
            said,
            &TranslatorConfig::default(),
        )?;
        // Two subtitles in one edit, each after its own effect.
        fixture.edit(ops)?;
        left.told();
        assert!(events.told().is_empty(), "nothing left out");
        let page = fixture.page()?;
        let project = fixture.project();
        for (label, text) in [("effect", "BAM"), ("hidden", "THUD")] {
            let subtitle = sfx::subtitle_of(&page, fixture.id(label)).context("a subtitle")?;
            let layer = project.layer(subtitle)?;
            assert_eq!(layer.name, sfx::SUBTITLE_NAME);
            let Content::Text(written) = &layer.content else {
                anyhow::bail!("{label}'s subtitle isn't text");
            };
            assert_eq!(written.content.text(), text);
            // The effect stays the original art.
            assert_eq!(fixture.translation(label)?, None);
            assert!(!fixture.block(label)?.cleanup.visible);
            assert!(Auto::of(&fixture.layer(label)?).kept);
            let after = page
                .layers
                .iter()
                .position(|layer| layer.id == fixture.id(label))
                .context("the effect")?;
            assert_eq!(page.layers[after + 1].id, subtitle, "{label}");
        }
        // A named block that isn't an effect is translated as any other.
        assert_eq!(fixture.translation("talk")?.as_deref(), Some("Why?"));
        Ok(())
    }

    #[test]
    fn a_subtitle_run_writes_none_for_an_answer_that_gives_the_source_back_or_a_changed_effect()
    -> Result<()> {
        let fixture = Fixture::new(
            false,
            vec![
                add("same", None, 0, block("sfx", (20.0, 20.0))),
                add("locks", None, 1, block("sfx", (80.0, 20.0))),
                add("gets", None, 2, block("sfx", (140.0, 20.0))),
            ],
        )?;
        let events = Events::new();
        let left = Left::of(
            Stage::Translate,
            snapshot(SfxMode::Skip, false, true),
            events.clone(),
        );
        let member = fixture.member(Some(&["same", "locks", "gets"]));
        assert_eq!(left.sent(&member).blocks, member.blocks);
        // While the model ran, one effect was locked and another subtitled.
        fixture.lock("locks")?;
        let style = LetteringSettings::default().subtitle;
        let page = fixture.page()?;
        let ops = sfx::subtitle_ops(
            &page,
            fixture.id("gets"),
            "POW",
            &langid!("en"),
            &style,
            None,
        )?;
        fixture.edit(ops)?;
        let before = fixture.page()?;
        let said = answers(
            &fixture,
            &[
                ("same", &format!(" {SOURCE} ")),
                ("locks", "BAM"),
                ("gets", "THUD"),
            ],
        );
        let ops = left.writing(&member).written(
            &fixture.project(),
            said,
            &TranslatorConfig::default(),
        )?;
        assert!(ops.is_empty(), "{ops:?}");
        left.told();
        assert_eq!(skipped(&events.told()), [(Stage::Translate, 0, 0, 1, true)]);
        assert_eq!(fixture.page()?, before);
        assert!(fixture.block("same")?.cleanup.visible, "as it was");

        // A run naming no blocks ignores the flag.
        let events = Events::new();
        let left = Left::of(
            Stage::Translate,
            snapshot(SfxMode::Skip, false, true),
            events.clone(),
        );
        assert!(fixture.sent(&left.sent(&fixture.member(None))).is_empty());
        left.told();
        assert_eq!(
            skipped(&events.told()),
            [(Stage::Translate, 2, 0, 1, false)]
        );
        Ok(())
    }

    #[test]
    fn a_subtitle_run_leaves_an_effect_whose_source_lost_its_box_as_it_is() -> Result<()> {
        let fixture = Fixture::new(
            false,
            vec![
                add("effect", None, 0, block("sfx", (20.0, 20.0))),
                add("talk", None, 1, block("text", (80.0, 20.0))),
            ],
        )?;
        let events = Events::new();
        let left = Left::of(
            Stage::Translate,
            snapshot(SfxMode::Skip, false, true),
            events.clone(),
        );
        let member = fixture.member(Some(&["effect", "talk"]));
        assert_eq!(left.sent(&member).blocks, member.blocks);
        // While the model ran, the effect's source became point text, with
        // no box to put a subtitle beside.
        fixture.op(json!({"op": "update_layer", "layer": fixture.id("effect"),
            "patch": {"source": {"frame": "point"}}}))?;
        let said = answers(&fixture, &[("effect", "BAM"), ("talk", "Why?")]);
        // No error to fail the page's edit, which the run would retry.
        let ops = left.writing(&member).written(
            &fixture.project(),
            said,
            &TranslatorConfig::default(),
        )?;
        fixture.edit(ops)?;
        let page = fixture.page()?;
        assert_eq!(sfx::subtitle_of(&page, fixture.id("effect")), None);
        assert_eq!(fixture.translation("effect")?, None);
        assert!(fixture.block("effect")?.cleanup.visible, "as it was");
        // The page's other answers are written.
        assert_eq!(fixture.translation("talk")?.as_deref(), Some("Why?"));
        Ok(())
    }

    #[test]
    fn a_subtitle_run_keeps_a_translation_typed_while_it_ran_and_counts_it_fixed_by_hand()
    -> Result<()> {
        let fixture = Fixture::new(
            false,
            vec![
                add("typed", None, 0, block("sfx", (20.0, 20.0))),
                add("still", None, 1, block("sfx", (80.0, 20.0))),
            ],
        )?;
        let events = Events::new();
        let left = Left::of(
            Stage::Translate,
            snapshot(SfxMode::Skip, false, true),
            events.clone(),
        );
        let member = fixture.member(Some(&["typed", "still"]));
        assert_eq!(left.sent(&member).blocks, member.blocks);
        // While the model ran, one effect's translation was typed by hand.
        fixture.type_in("typed", "Thump")?;
        let said = answers(&fixture, &[("typed", "BAM"), ("still", "THUD")]);
        let ops = left.writing(&member).written(
            &fixture.project(),
            said,
            &TranslatorConfig::default(),
        )?;
        fixture.edit(ops)?;
        left.told();
        assert_eq!(skipped(&events.told()), [(Stage::Translate, 0, 1, 0, true)]);
        let page = fixture.page()?;
        // The typed words stay, with no subtitle, and the clean-up as it was.
        assert_eq!(sfx::subtitle_of(&page, fixture.id("typed")), None);
        assert_eq!(fixture.translation("typed")?.as_deref(), Some("Thump"));
        assert!(fixture.block("typed")?.cleanup.visible);
        assert!(!Auto::of(&fixture.layer("typed")?).kept);
        // The effect still without one gets its subtitle as before.
        let subtitle = sfx::subtitle_of(&page, fixture.id("still")).context("a subtitle")?;
        let project = fixture.project();
        let Content::Text(written) = &project.layer(subtitle)?.content else {
            anyhow::bail!("the subtitle isn't text");
        };
        assert_eq!(written.content.text(), "THUD");
        assert_eq!(fixture.translation("still")?, None);
        assert!(!fixture.block("still")?.cleanup.visible);
        assert!(Auto::of(&fixture.layer("still")?).kept);
        Ok(())
    }

    #[test]
    fn a_subtitled_effect_is_left_out_of_every_later_translation() -> Result<()> {
        let fixture = Fixture::new(
            false,
            vec![
                add("effect", None, 0, block("sfx", (20.0, 20.0))),
                add("talk", None, 1, block("text", (80.0, 20.0))),
            ],
        )?;
        let left = Left::of(
            Stage::Translate,
            snapshot(SfxMode::Skip, false, true),
            Events::new(),
        );
        let member = fixture.member(Some(&["effect"]));
        let said = answers(&fixture, &[("effect", "BAM")]);
        let ops = left.writing(&member).written(
            &fixture.project(),
            said,
            &TranslatorConfig::default(),
        )?;
        fixture.edit(ops)?;
        assert!(sfx::subtitled(&fixture.page()?, fixture.id("effect")));
        for mode in [SfxMode::Skip, SfxMode::InsideBubbles, SfxMode::TranslateAll] {
            for redo in [false, true] {
                let events = Events::new();
                let left = Left::of(
                    Stage::Translate,
                    snapshot(mode, redo, false),
                    events.clone(),
                );
                let sent = fixture.sent(&left.sent(&fixture.member(None)));
                assert_eq!(sent, BTreeSet::from(["talk".to_owned()]), "{mode:?}");
                left.told();
                assert_eq!(
                    skipped(&events.told()),
                    [(Stage::Translate, 1, 0, 0, false)]
                );
            }
        }

        // Subtitled while the model ran, its answer is dropped too.
        let fixture = Fixture::new(
            false,
            vec![add("effect", None, 0, block("sfx", (20.0, 20.0)))],
        )?;
        let events = Events::new();
        let all = snapshot(SfxMode::TranslateAll, false, false);
        let left = Left::of(Stage::Translate, all, events.clone());
        let member = fixture.member(None);
        assert_eq!(left.sent(&member).blocks, None, "Translate all sends it");
        let style = LetteringSettings::default().subtitle;
        let ops = sfx::subtitle_ops(
            &fixture.page()?,
            fixture.id("effect"),
            "POW",
            &langid!("en"),
            &style,
            None,
        )?;
        fixture.edit(ops)?;
        let said = answers(&fixture, &[("effect", "BAM")]);
        let ops = left.writing(&member).written(
            &fixture.project(),
            said,
            &TranslatorConfig::default(),
        )?;
        assert!(ops.is_empty(), "{ops:?}");
        left.told();
        assert_eq!(
            skipped(&events.told()),
            [(Stage::Translate, 1, 0, 0, false)]
        );
        Ok(())
    }

    // S10-e: the words a translation's model picked to stress, as the write
    // hands them on.

    #[test]
    fn the_write_marks_an_answers_picks_but_none_in_a_subtitle_or_a_block_left_out() -> Result<()> {
        // A subtitle run of an effect and two lines, its model picking a
        // word in each answer, the last line locked while the model ran;
        // written with its picks when `stressing`.
        let written = |stressing: bool| -> Result<Fixture> {
            let fixture = Fixture::new(
                false,
                vec![
                    add("effect", None, 0, block("sfx", (20.0, 20.0))),
                    add("talk", None, 1, block("text", (80.0, 20.0))),
                    add("locked", None, 2, block("text", (140.0, 20.0))),
                ],
            )?;
            let booharu = Snapshot {
                emphasis: Some(EmphasisSettings::default()),
                ..snapshot(SfxMode::Skip, false, true)
            };
            let left = Left::of(Stage::Translate, booharu, Events::new());
            let member = fixture.member(Some(&["effect", "talk", "locked"]));
            fixture.lock("locked")?;
            let said = answers(
                &fixture,
                &[("effect", "BAM"), ("talk", "No way!"), ("locked", "Why?")],
            );
            let picked = HashMap::from([
                (fixture.id("effect"), vec!["BAM".to_owned()]),
                (fixture.id("talk"), vec!["No".to_owned()]),
                (fixture.id("locked"), vec!["Why".to_owned()]),
            ]);
            let writing = match stressing {
                true => left.writing(&member).stressing(picked),
                false => left.writing(&member),
            };
            let ops = writing.written(&fixture.project(), said, &TranslatorConfig::default())?;
            fixture.edit(ops)?;
            Ok(fixture)
        };
        let spans = |fixture: &Fixture| -> Result<Vec<Span>> {
            let translation = fixture.block("talk")?.translation;
            Ok(translation.context("talk's translation")?.content.0)
        };
        let subtitle = |fixture: &Fixture| -> Result<RichText> {
            let page = fixture.page()?;
            let id = sfx::subtitle_of(&page, fixture.id("effect")).context("a subtitle")?;
            match &fixture.project().layer(id)?.content {
                Content::Text(subtitle) => Ok(subtitle.content.clone()),
                _ => anyhow::bail!("the subtitle isn't text"),
            }
        };

        let stressed = written(true)?;
        let talk = spans(&stressed)?;
        let words: Vec<_> = talk.iter().map(|span| span.insert.as_str()).collect();
        assert_eq!(words, ["No", " way!"]);
        assert_eq!(talk[0].attributes.weight, Some(700));
        assert_eq!(Auto::of(&stressed.layer("talk")?).marks.len(), 1);
        // The subtitle is written as it is with no picks, and the block
        // locked meanwhile gets nothing.
        let plain = written(false)?;
        assert_eq!(subtitle(&stressed)?, subtitle(&plain)?);
        assert_eq!(subtitle(&stressed)?.text(), "BAM");
        assert_eq!(stressed.translation("effect")?, None);
        assert_eq!(stressed.translation("locked")?, None);
        assert!(Auto::of(&stressed.layer("locked")?).marks.is_empty());

        // Written with none, the line is marked nowhere.
        let talk = spans(&plain)?;
        assert_eq!(talk.len(), 1, "{talk:?}");
        assert!(Auto::of(&plain.layer("talk")?).marks.is_empty());
        Ok(())
    }

    // S9-f: Detect again.

    /// A page of what Detect again keeps and what it doesn't: a detected
    /// balloon holding a block corrected by hand, an untouched one, and a
    /// locked one; beside it a block drawn by hand, one a user labelled, one
    /// with a reviewer's note, a sound effect with its subtitle, a locked
    /// block, and an untouched one; and a detected panel holding a text layer
    /// and an untouched block.
    fn kept_and_not() -> Result<Fixture> {
        let mut drawn = block("text", (10.0, 120.0));
        drawn["attributes"] = json!({});
        let mut labelled = block("text", (40.0, 120.0));
        labelled["attributes"]["booharu"] =
            json!({"lettering": {"category": "shout", "source": "user"}});
        let fixture = Fixture::new(
            false,
            vec![
                add(
                    "balloon",
                    None,
                    0,
                    detected("balloon", "M0 0 L100 0 L100 100 L0 100 Z"),
                ),
                add("corrected", Some("balloon"), 0, block("text", (10.0, 10.0))),
                add("untouched", Some("balloon"), 1, block("text", (40.0, 10.0))),
                add("locked_in", Some("balloon"), 2, block("text", (70.0, 10.0))),
                add("drawn", None, 1, drawn),
                add("labelled", None, 2, labelled),
                add("noted", None, 3, block("text", (70.0, 120.0))),
                add("effect", None, 4, block("sfx", (100.0, 120.0))),
                add(
                    "subtitle",
                    None,
                    5,
                    json!({"kind": "text", "content": "BAM"}),
                ),
                add("locked", None, 6, block("text", (130.0, 120.0))),
                add("plain", None, 7, block("text", (160.0, 120.0))),
                add(
                    "panel",
                    None,
                    8,
                    detected("panel", "M0 160 L200 160 L200 200 L0 200 Z"),
                ),
                add(
                    "typed",
                    Some("panel"),
                    0,
                    json!({"kind": "text", "content": "Hi"}),
                ),
                add("in_panel", Some("panel"), 1, block("text", (10.0, 160.0))),
            ],
        )?;
        fixture.correct("corrected")?;
        fixture.edit(vec![
            attrs::set(fixture.id("noted"), &ReviewNote("Check".to_owned()))?,
            attrs::set(
                fixture.id("subtitle"),
                &SfxSubtitle {
                    block: fixture.id("effect"),
                },
            )?,
        ])?;
        fixture.lock("locked_in")?;
        fixture.lock("locked")?;
        Ok(fixture)
    }

    #[test]
    fn detect_again_keeps_what_was_made_fixed_labelled_noted_subtitled_or_locked_by_hand()
    -> Result<()> {
        let fixture = kept_and_not()?;
        let page = fixture.page()?;
        let mut ops = Vec::new();
        removals(&page, &page.layers, &mut ops);
        // The edit goes in: the balloon holding a locked block stays, and no
        // locked layer is removed.
        fixture.edit(ops)?;
        let kept: BTreeSet<String> = [
            "balloon",
            "corrected",
            "locked_in",
            "drawn",
            "labelled",
            "noted",
            "effect",
            "subtitle",
            "locked",
            "panel",
            "typed",
        ]
        .map(str::to_owned)
        .into();
        assert_eq!(fixture.layers()?, kept);
        Ok(())
    }

    /// The op adding a new layer as Koharu's Detection stage does, as `$label`
    /// at `index` among `parent`'s new layers, or the page's.
    fn found(
        page: PageId,
        label: &str,
        parent: Option<&str>,
        index: usize,
        layer: Value,
    ) -> Result<Op> {
        Ok(serde_json::from_value(
            json!({"op": "add_layer", "id": format!("${label}"),
            "page": page, "parent": parent.map(|parent| format!("${parent}")), "index": index,
            "layer": layer}),
        )?)
    }

    /// A new block's source box, 20 by 40 at `x`, `y`, as detected.
    fn lettering((x, y): (f64, f64)) -> Value {
        json!({"kind": "block", "source": {"frame": {"box": {"bounds":
            {"x": x, "y": y, "width": 20.0, "height": 40.0}}}},
            "attributes": {"c2pa": {"detection": {"action": "c2pa.created"}}}})
    }

    fn found_mask(label: &str, (x, y): (i32, i32)) -> Result<Op> {
        Ok(serde_json::from_value(mask(label, (x, y)))?)
    }

    /// A new layer: its label, its parent's, and its index.
    type Added = (String, Option<String>, usize);

    /// Each new layer `ops` add, and each layer whose pixels they write.
    fn shape(ops: &[Op]) -> (Vec<Added>, Vec<String>) {
        let label = |at: &Ref| at.label().unwrap_or_default().to_owned();
        let added = ops
            .iter()
            .filter_map(|op| match op {
                Op::AddLayer {
                    id: Some(id),
                    parent,
                    index,
                    ..
                } => Some((label(id), parent.as_ref().map(label), *index)),
                _ => None,
            })
            .collect();
        let written = ops
            .iter()
            .filter_map(|op| match op {
                Op::WritePixels { layer, .. } => Some(label(layer)),
                _ => None,
            })
            .collect();
        (added, written)
    }

    #[test]
    fn detect_again_adds_no_block_over_one_that_stays_nor_a_container_left_empty() -> Result<()> {
        let fixture = Fixture::new(
            false,
            vec![
                add("kept", None, 0, block("text", (10.0, 10.0))),
                add("locked", None, 1, block("text", (100.0, 10.0))),
                add("plain", None, 2, block("text", (150.0, 10.0))),
            ],
        )?;
        fixture.correct("kept")?;
        fixture.lock("locked")?;
        let at = fixture.page;
        let additions = vec![
            found(
                at,
                "d0",
                None,
                0,
                detected("panel", "M0 0 L200 0 L200 200 L0 200 Z"),
            )?,
            found(
                at,
                "d1",
                Some("d0"),
                0,
                detected("balloon", "M0 0 L60 0 L60 60 L0 60 Z"),
            )?,
            // Over the block corrected by hand.
            found(at, "d2", Some("d1"), 0, lettering((12.0, 12.0)))?,
            found_mask("d2", (12, 12))?,
            found(at, "d3", Some("d0"), 1, lettering((60.0, 100.0)))?,
            found_mask("d3", (60, 100))?,
            found(
                at,
                "d4",
                None,
                1,
                detected("balloon", "M95 5 L130 5 L130 60 L95 60 Z"),
            )?,
            // Over the locked block.
            found(at, "d5", Some("d4"), 0, lettering((105.0, 15.0)))?,
            found_mask("d5", (105, 15))?,
            // Over the untouched block, which goes.
            found(at, "d6", None, 2, lettering((150.0, 10.0)))?,
            found_mask("d6", (150, 10))?,
            // A balloon detected empty stays so.
            found(
                at,
                "d7",
                None,
                3,
                detected("balloon", "M0 120 L40 120 L40 150 L0 150 Z"),
            )?,
            // Over the corrected block by less than half.
            found(at, "d8", None, 4, lettering((25.0, 10.0)))?,
        ];
        let events = Events::new();
        let left = Left::of(Stage::Detect, Snapshot::default(), events.clone());
        let page = fixture.page()?;
        let added = left.writing(&fixture.member(None)).added(&page, additions);
        let own = |label: &str| label.to_owned();
        assert_eq!(
            shape(&added),
            (
                vec![
                    (own("d0"), None, 0),
                    (own("d3"), Some(own("d0")), 0),
                    (own("d6"), None, 1),
                    (own("d7"), None, 2),
                    (own("d8"), None, 3),
                ],
                vec![own("d3.mask"), own("d6.mask")],
            )
        );
        left.told();
        assert_eq!(skipped(&events.told()), [(Stage::Detect, 0, 1, 1, false)]);
        // Removing what goes and adding the rest is one edit that goes in.
        let mut ops = Vec::new();
        removals(&page, &page.layers, &mut ops);
        ops.extend(added);
        fixture.edit(ops)?;
        let page = fixture.page()?;
        let top: Vec<String> = page
            .layers
            .iter()
            .map(|layer| fixture.label(layer.id))
            .collect();
        assert_eq!(top.len(), 6, "{top:?}");
        assert_eq!(top[4..], ["kept".to_owned(), "locked".to_owned()]);
        Ok(())
    }

    #[test]
    fn a_page_whose_every_block_stays_gets_nothing_new_and_fails_nothing() -> Result<()> {
        let mut drawn = block("text", (100.0, 10.0));
        drawn["attributes"] = json!({});
        let fixture = Fixture::new(
            false,
            vec![
                add(
                    "balloon",
                    None,
                    0,
                    detected("balloon", "M0 0 L80 0 L80 60 L0 60 Z"),
                ),
                add("corrected", Some("balloon"), 0, block("text", (10.0, 10.0))),
                add("locked", Some("balloon"), 1, block("text", (40.0, 10.0))),
                add("drawn", None, 1, drawn),
            ],
        )?;
        fixture.correct("corrected")?;
        fixture.lock("locked")?;
        let at = fixture.page;
        // What detection finds again: the same balloon and blocks.
        let additions = vec![
            found(
                at,
                "d0",
                None,
                0,
                detected("balloon", "M0 0 L80 0 L80 60 L0 60 Z"),
            )?,
            found(at, "d1", Some("d0"), 0, lettering((10.0, 10.0)))?,
            found_mask("d1", (10, 10))?,
            found(at, "d2", Some("d0"), 1, lettering((40.0, 10.0)))?,
            found_mask("d2", (40, 10))?,
            found(at, "d3", None, 1, lettering((100.0, 10.0)))?,
            found_mask("d3", (100, 10))?,
        ];
        let events = Events::new();
        let left = Left::of(Stage::Detect, Snapshot::default(), events.clone());
        let page = fixture.page()?;
        let mut ops = Vec::new();
        removals(&page, &page.layers, &mut ops);
        ops.extend(left.writing(&fixture.member(None)).added(&page, additions));
        assert!(ops.is_empty(), "{ops:?}");
        left.told();
        assert_eq!(skipped(&events.told()), [(Stage::Detect, 0, 2, 1, false)]);
        Ok(())
    }

    #[test]
    fn detect_again_keeps_a_clean_up_hidden_by_hand_but_not_one_booharu_or_the_translator_hid()
    -> Result<()> {
        let fixture = Fixture::new(
            false,
            vec![
                add("by_hand", None, 0, block("sfx", (10.0, 10.0))),
                add("as_art", None, 1, block("sfx", (60.0, 10.0))),
                add("given", None, 2, block("text", (110.0, 10.0))),
            ],
        )?;
        let hide = |label: &str| -> Result<Op> {
            Ok(serde_json::from_value(
                json!({"op": "update_layer", "layer": fixture.id(label),
                    "patch": {"cleanup": {"visible": false}}}),
            )?)
        };
        // Each block's clean-up hidden, with no translation: by hand, by
        // Booharu keeping the art with its mark, and by the Translation stage
        // given the source back.
        fixture.edit(vec![hide("by_hand")?])?;
        fixture.edit(sfx::kept_original(&fixture.layer("as_art")?))?;
        fixture.edit(vec![hide("given")?, given_back(&fixture.layer("given")?)?])?;
        let at = fixture.page;
        // What detection finds again: the same three blocks.
        let additions = vec![
            found(at, "d0", None, 0, lettering((10.0, 10.0)))?,
            found_mask("d0", (10, 10))?,
            found(at, "d1", None, 1, lettering((60.0, 10.0)))?,
            found_mask("d1", (60, 10))?,
            found(at, "d2", None, 2, lettering((110.0, 10.0)))?,
            found_mask("d2", (110, 10))?,
        ];
        let events = Events::new();
        let left = Left::of(Stage::Detect, Snapshot::default(), events.clone());
        let page = fixture.page()?;
        let mut ops = Vec::new();
        removals(&page, &page.layers, &mut ops);
        let added = left.writing(&fixture.member(None)).added(&page, additions);
        // No new block over the one hidden by hand; the others are replaced.
        let own = |label: &str| label.to_owned();
        assert_eq!(
            shape(&added),
            (
                vec![(own("d1"), None, 0), (own("d2"), None, 1)],
                vec![own("d1.mask"), own("d2.mask")],
            )
        );
        ops.extend(added);
        fixture.edit(ops)?;
        left.told();
        assert_eq!(skipped(&events.told()), [(Stage::Detect, 0, 1, 0, false)]);
        let layers = fixture.layers()?;
        assert!(layers.contains("by_hand"), "{layers:?}");
        assert!(!layers.contains("as_art") && !layers.contains("given"));
        assert_eq!(layers.len(), 3, "{layers:?}");
        // It keeps its clean-up hidden, so cleaning again never shows it.
        let kept = fixture.block("by_hand")?;
        assert!(!kept.cleanup.visible && kept.translation.is_none());
        Ok(())
    }

    #[test]
    fn detect_again_keeps_a_clean_up_shown_by_hand_on_an_effect_kept_as_art() -> Result<()> {
        let fixture = Fixture::new(
            false,
            vec![
                add("shown", None, 0, block("sfx", (10.0, 10.0))),
                add("as_art", None, 1, block("sfx", (60.0, 10.0))),
            ],
        )?;
        // Both kept as art by Booharu, with the mark, then one's clean-up
        // shown again by hand, which keeping the art leaves alone.
        for label in ["shown", "as_art"] {
            fixture.edit(sfx::kept_original(&fixture.layer(label)?))?;
        }
        fixture.edit(vec![serde_json::from_value(
            json!({"op": "update_layer", "layer": fixture.id("shown"),
                "patch": {"cleanup": {"visible": true}}}),
        )?])?;
        let at = fixture.page;
        // What detection finds again: the same two blocks.
        let additions = vec![
            found(at, "d0", None, 0, lettering((10.0, 10.0)))?,
            found_mask("d0", (10, 10))?,
            found(at, "d1", None, 1, lettering((60.0, 10.0)))?,
            found_mask("d1", (60, 10))?,
        ];
        let events = Events::new();
        let left = Left::of(Stage::Detect, Snapshot::default(), events.clone());
        let page = fixture.page()?;
        let mut ops = Vec::new();
        removals(&page, &page.layers, &mut ops);
        let added = left.writing(&fixture.member(None)).added(&page, additions);
        // No new block over the one shown by hand; the other is replaced.
        let own = |label: &str| label.to_owned();
        assert_eq!(
            shape(&added),
            (vec![(own("d1"), None, 0)], vec![own("d1.mask")])
        );
        ops.extend(added);
        fixture.edit(ops)?;
        left.told();
        assert_eq!(skipped(&events.told()), [(Stage::Detect, 0, 1, 0, false)]);
        let layers = fixture.layers()?;
        assert!(layers.contains("shown"), "{layers:?}");
        assert!(!layers.contains("as_art"));
        assert_eq!(layers.len(), 2, "{layers:?}");
        // Its clean-up still shows, and keeping the art leaves it so.
        assert!(fixture.block("shown")?.cleanup.visible);
        assert!(sfx::kept_original(&fixture.layer("shown")?).is_empty());
        Ok(())
    }

    #[test]
    fn detect_again_keeps_a_splits_halves_and_adds_no_block_over_them() -> Result<()> {
        // A block detection found, split in two by hand: each half keeps the
        // detection record it was copied with, its own record saying it's a
        // half of a block split by hand, and is read again.
        let fixture = Fixture::new(
            false,
            vec![
                add("first", None, 0, block("text", (30.0, 10.0))),
                add("second", None, 1, block("text", (10.0, 10.0))),
            ],
        )?;
        for label in ["first", "second"] {
            let half = Auto {
                split: true,
                ..Auto::of(&fixture.layer(label)?)
            };
            fixture.edit(vec![attrs::set(fixture.id(label), &half)?])?;
            assert!(!booharu_core::edited(&fixture.layer(label)?).hand_edited());
        }
        // What detection finds again: the block as it was before the split.
        let at = fixture.page;
        let merged = || -> Result<Vec<Op>> {
            let block = json!({"kind": "block", "source": {"frame": {"box": {"bounds":
                {"x": 10.0, "y": 10.0, "width": 40.0, "height": 40.0}}}},
                "attributes": {"c2pa": {"detection": {"action": "c2pa.created"}}}});
            Ok(vec![
                found(at, "d0", None, 0, block)?,
                serde_json::from_value(json!({"op": "write_pixels", "layer": "$d0.mask",
                    "rect": {"x": 10, "y": 10, "width": 40, "height": 40},
                    "pixels": vec![255u8; 40 * 40]}))?,
            ])
        };
        // Untouched since, then with one half's translation fixed by hand,
        // which once left the other's line with no block at all.
        for fixed in [false, true] {
            if fixed {
                fixture.type_in("second", "Who?")?;
            }
            let events = Events::new();
            let left = Left::of(Stage::Detect, Snapshot::default(), events.clone());
            let page = fixture.page()?;
            let mut ops = Vec::new();
            removals(&page, &page.layers, &mut ops);
            ops.extend(left.writing(&fixture.member(None)).added(&page, merged()?));
            assert!(ops.is_empty(), "{ops:?}");
            left.told();
            assert_eq!(skipped(&events.told()), [(Stage::Detect, 0, 2, 0, false)]);
        }
        Ok(())
    }

    /// The hash of the scan a scanned fixture's page shows, which an image
    /// layer may show too.
    fn scan() -> Result<Hash> {
        let mut scan = Vec::new();
        RgbImage::from_pixel(SIDE, SIDE, Rgb([255; 3]))
            .write_to(&mut Cursor::new(&mut scan), ImageFormat::Png)?;
        Ok(Hash::of(&scan))
    }

    /// The Original scan, showing the page's scan, as page levels adds it:
    /// hidden, locked, and marked.
    fn original() -> Result<Value> {
        Ok(
            json!({"kind": "image", "file": scan()?, "name": "Original scan", "visible": false,
            "locked": true, "attributes": {"booharu": {"original_scan": true}}}),
        )
    }

    /// A Remove stroke's or a Generative Fill's result, as the Inpainting
    /// stage records it.
    fn result() -> Value {
        json!({"kind": "raster", "attributes": {"c2pa": {
            "inpainting": {"action": "c2pa.created"}}}})
    }

    /// What detection finds on any page here: a panel holding a block, and
    /// a free block, none over a block the pages hold.
    fn finds(at: PageId) -> Result<Vec<Op>> {
        Ok(vec![
            found(
                at,
                "d0",
                None,
                0,
                detected("panel", "M0 90 L200 90 L200 200 L0 200 Z"),
            )?,
            found(at, "d1", Some("d0"), 0, lettering((10.0, 100.0)))?,
            found_mask("d1", (10, 100))?,
            found(at, "d2", None, 1, lettering((60.0, 100.0)))?,
            found_mask("d2", (60, 100))?,
        ])
    }

    #[test]
    fn detect_again_adds_its_layers_above_the_original_scan_and_the_results_under_the_lettering()
    -> Result<()> {
        // From the bottom up: the Original scan, two results with a block
        // between them, which goes, a locked block, which stays, and a free
        // text.
        let fixture = Fixture::new(
            true,
            vec![
                add("original", None, 0, original()?),
                add("first", None, 1, result()),
                add("plain", None, 2, block("text", (10.0, 10.0))),
                add("second", None, 3, result()),
                add("locked", None, 4, block("text", (100.0, 10.0))),
                add("free", None, 5, json!({"kind": "text", "content": "Hi"})),
            ],
        )?;
        fixture.lock("locked")?;
        let events = Events::new();
        let left = Left::of(Stage::Detect, Snapshot::default(), events.clone());
        let page = fixture.page()?;
        let added = left
            .writing(&fixture.member(None))
            .added(&page, finds(fixture.page)?);
        // The block that goes counts for nothing: the new top-level layers
        // start above the second result, and the panel's block keeps its
        // place in the panel.
        let own = |label: &str| label.to_owned();
        assert_eq!(
            shape(&added),
            (
                vec![
                    (own("d0"), None, 3),
                    (own("d1"), Some(own("d0")), 0),
                    (own("d2"), None, 4),
                ],
                vec![own("d1.mask"), own("d2.mask")],
            )
        );
        left.told();
        // Removing what goes and adding the rest is one edit that goes in.
        let mut ops = Vec::new();
        removals(&page, &page.layers, &mut ops);
        ops.extend(added);
        fixture.edit(ops)?;
        let page = fixture.page()?;
        let top: Vec<String> = page
            .layers
            .iter()
            .map(|layer| fixture.label(layer.id))
            .collect();
        assert_eq!(top.len(), 7, "{top:?}");
        assert_eq!(top[..3], ["original", "first", "second"].map(own));
        assert!(matches!(page.layers[3].content, Content::Panel(_)));
        assert!(matches!(page.layers[4].content, Content::Block(_)));
        assert_eq!(top[5..], ["locked", "free"].map(own));
        Ok(())
    }

    #[test]
    fn with_no_original_scan_nor_result_detect_again_numbers_from_0_as_koharus_does() -> Result<()>
    {
        let fixture = Fixture::new(
            false,
            vec![
                add("draw", None, 0, json!({"kind": "raster"})),
                add("free", None, 1, json!({"kind": "text", "content": "Hi"})),
            ],
        )?;
        let left = Left::of(Stage::Detect, Snapshot::default(), Events::new());
        let page = fixture.page()?;
        let added = left
            .writing(&fixture.member(None))
            .added(&page, finds(fixture.page)?);
        let own = |label: &str| label.to_owned();
        assert_eq!(
            shape(&added).0,
            [
                (own("d0"), None, 0),
                (own("d1"), Some(own("d0")), 0),
                (own("d2"), None, 1),
            ]
        );
        Ok(())
    }

    #[test]
    fn the_layers_detect_again_starts_among_are_those_its_removals_leave() -> Result<()> {
        let fixture = Fixture::new(
            true,
            vec![
                add("original", None, 0, original()?),
                add("result", None, 1, result()),
                add("draw", None, 2, json!({"kind": "raster"})),
                add(
                    "panel",
                    None,
                    3,
                    detected("panel", "M0 0 L90 0 L90 90 L0 90 Z"),
                ),
                add(
                    "locked_panel",
                    None,
                    4,
                    detected("panel", "M100 0 L200 0 L200 90 Z"),
                ),
                add(
                    "holding",
                    None,
                    5,
                    detected("balloon", "M0 0 L80 0 L80 60 Z"),
                ),
                add("held", Some("holding"), 0, block("text", (10.0, 10.0))),
                add(
                    "balloon",
                    None,
                    6,
                    detected("balloon", "M0 100 L40 100 L40 150 Z"),
                ),
                add("plain", None, 7, block("text", (100.0, 100.0))),
                add("corrected", None, 8, block("text", (150.0, 100.0))),
                add("locked", None, 9, block("text", (150.0, 150.0))),
                add("group", None, 10, json!({"kind": "group"})),
                add("grouped", Some("group"), 0, block("text", (100.0, 150.0))),
                add("free", None, 11, json!({"kind": "text", "content": "Hi"})),
            ],
        )?;
        fixture.lock("locked_panel")?;
        fixture.lock("locked")?;
        fixture.correct("held")?;
        fixture.correct("corrected")?;
        let page = fixture.page()?;
        let mut ops = Vec::new();
        removals(&page, &page.layers, &mut ops);
        let removed: Vec<Ref> = ops
            .iter()
            .filter_map(|op| match op {
                Op::RemoveLayer { layer } => Some(layer.clone()),
                _ => None,
            })
            .collect();
        let left: Vec<String> = page
            .layers
            .iter()
            .filter(|layer| remains(&page, layer))
            .map(|layer| fixture.label(layer.id))
            .collect();
        let unremoved: Vec<String> = page
            .layers
            .iter()
            .filter(|layer| !removed.contains(&Ref::from(layer.id)))
            .map(|layer| fixture.label(layer.id))
            .collect();
        assert_eq!(left, unremoved);
        assert_eq!(
            left,
            [
                "original",
                "result",
                "draw",
                "locked_panel",
                "holding",
                "corrected",
                "locked",
                "group",
                "free"
            ]
            .map(str::to_owned)
        );
        Ok(())
    }
}
