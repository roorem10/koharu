//! What Koharu's stages leave alone (D1, D7): a run's filters, which leave
//! out each block a stage mustn't change and say why, and what Detect again
//! keeps of a page it detects again.
//!
//! - A locked block, or one in a locked layer, is left out by every stage,
//!   and counted, rather than failing the page's edit.
//! - Reading again leaves out a source corrected by hand, and translating
//!   again a translation written or fixed by hand, or a clean-up hidden by
//!   hand on a block with no translation of its own, unless the run is Redo
//!   boxes taking them on purpose. Cleaning again never asks: it leaves out
//!   only what a lock keeps it from writing.
//! - Translating leaves out the sound effects the SFX mode keeps as art and
//!   the subtitled ones (`sfx::skipped`), which no redo lifts; a subtitle
//!   run translates the effects it names.
//! - Detect again keeps what was made or changed by hand, a split's halves,
//!   a clean-up hidden by hand, or shown again by hand on an effect kept as
//!   art, and a translation skewed by hand among it, and what is locked
//!   ([`keeps`]).
//!
//! koharu-pipeline's Booharu glue calls [`left_out`] from each stage's hook
//! and [`keeps`] from Detect's, and the redo counts read [`left_out`] too.

use koharu_scene::{Block, Content, Layer, LayerId, Page};
use serde::{Deserialize, Serialize};

use crate::{
    attrs::{self, Auto, Lettering, LetteringSource, ReviewNote, Skew},
    runs::RunStage,
    settings::LetteringSettings,
    sfx, skew,
    status::DETECTION,
    styles::{edited, recorded},
};

/// Why a stage leaves a block out.
#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    Hash,
    Serialize,
    Deserialize,
    specta::Type,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// It, a layer it's in, or for cleaning its clean-up, is locked.
    Locked,
    /// It was read or translated by hand, which only Redo boxes redoes.
    HandFixed,
    /// It's a sound effect kept as art, or one with a subtitle.
    SoundEffect,
}

/// Why `stage` leaves out the block `layer` holds on `page`, under
/// `settings`; none when it works on it, and for a layer that isn't a block.
/// `redo` says the run is Redo boxes taking hand-fixed blocks too, which
/// lifts [`Reason::HandFixed`] alone; `named_subtitle` says a subtitle run
/// names the block, which, for a sound effect, lifts both
/// [`Reason::HandFixed`] and [`Reason::SoundEffect`] when translating. A
/// lock is never lifted.
///
/// Every stage leaves out a block that is, or is in, a locked layer. Reading
/// leaves out a source corrected by hand. Translating leaves out a sound
/// effect [`sfx::skipped`] keeps as art first, as no redo would translate
/// it, then a translation written or fixed by hand and a clean-up hidden by
/// hand on a block with no translation of its own ([`sfx::hand_kept`]).
/// Cleaning leaves out a block it can't write ([`cleanable`]).
pub fn left_out(
    page: &Page,
    layer: &Layer,
    stage: RunStage,
    settings: &LetteringSettings,
    redo: bool,
    named_subtitle: bool,
) -> Option<Reason> {
    let Content::Block(block) = &layer.content else {
        return None;
    };
    if locked(page, layer.id) {
        return Some(Reason::Locked);
    }
    match stage {
        RunStage::Recognize => (!redo && edited(layer).source).then_some(Reason::HandFixed),
        RunStage::Translate => {
            if named_subtitle && sfx::sound_effect(page, layer.id, settings) {
                return None;
            }
            if sfx::skipped(page, layer.id, settings) {
                return Some(Reason::SoundEffect);
            }
            let hand_fixed = edited(layer).text || sfx::hand_kept(layer, block);
            (!redo && hand_fixed).then_some(Reason::HandFixed)
        }
        RunStage::Inpaint => (!cleanable(page, layer)).then_some(Reason::Locked),
        RunStage::Detect | RunStage::Read => None,
    }
}

/// Whether the layer `id` on `page`, or a layer it's in, is locked; false
/// for a layer that isn't there.
pub fn locked(page: &Page, id: LayerId) -> bool {
    fn search(layers: &[Layer], id: LayerId, locked: bool) -> Option<bool> {
        layers.iter().find_map(|layer| {
            let locked = locked || layer.locked;
            if layer.id == id {
                Some(locked)
            } else {
                search(&layer.children, id, locked)
            }
        })
    }
    search(&page.layers, id, false).unwrap_or(false)
}

/// Whether Koharu's Inpainting stage can write the clean-up of the block
/// `layer` holds on `page`: the block, the layers it's in, and its clean-up
/// part are all unlocked, as a locked one refuses the stage's pixels. False
/// for a layer that isn't a block.
pub fn cleanable(page: &Page, layer: &Layer) -> bool {
    match &layer.content {
        Content::Block(block) => !locked(page, layer.id) && !block.cleanup.locked,
        _ => false,
    }
}

/// Whether Detect again keeps `layer`, on `page`, rather than replace it
/// with what it detects: a locked layer; a block changed by hand (its
/// source, translation, or style, or drawn by hand, as
/// [`edited`](crate::edited) reports it), cut from a block split by hand,
/// though it keeps detection's record ([`Auto::split`]), whose clean-up was
/// hidden by hand ([`sfx::hand_kept`]) or shown again by hand after Booharu
/// kept it as art ([`sfx::hand_shown`]), labelled by a user, given a
/// reviewer's note, subtitled, or whose translation shows a skew
/// ([`skewed`]); a panel or balloon Koharu's Detection
/// stage didn't make; and any text layer, the T tool's and subtitles. Anything else, a block, panel, or balloon detection made and
/// nobody changed among them, is detected again, a block whose clean-up
/// Booharu hid to keep it as art (the kept mark) or the Translation stage
/// hid giving it back among them, as no hand hid those.
pub fn keeps(page: &Page, layer: &Layer) -> bool {
    if layer.locked {
        return true;
    }
    match &layer.content {
        Content::Block(block) => {
            edited(layer).hand_edited()
                || Auto::of(layer).split
                || sfx::hand_kept(layer, block)
                || sfx::hand_shown(layer, block)
                || attrs::get::<Lettering>(layer)
                    .is_some_and(|label| label.source == LetteringSource::User)
                || attrs::get::<ReviewNote>(layer).is_some()
                || sfx::subtitled(page, layer.id)
                || skewed(layer, block)
        }
        Content::Panel(_) | Content::Balloon(_) => !recorded(layer, DETECTION),
        Content::Text(_) => true,
        _ => false,
    }
}

/// Whether the translation of `block`, `layer`'s, shows a skew
/// ([`skew::skewed`]), which only the FX window, by hand, sets. Not a hand
/// edit by [`edited`]'s reckoning, so Apply and the runs restyle the box as
/// any other, but Detect again keeps it.
pub fn skewed(layer: &Layer, block: &Block) -> bool {
    block
        .translation
        .as_ref()
        .is_some_and(|translation| skew::skewed(translation, attrs::get::<Skew>(layer).as_ref()))
}

#[cfg(test)]
mod tests {
    use koharu_scene::{Created, Edit, Op, Session};
    use serde_json::{Value, json};

    use super::*;
    use crate::{
        attrs::SfxSubtitle,
        settings::SfxMode,
        status,
        styles::{given_back, hand_typed, recognized, text_hash},
        testing::type_bar_style,
    };

    /// A page holding a detected balloon around the detected block `held`,
    /// a detected panel, the detected blocks `text` and `effect`, a sound
    /// effect, each read as Koharu's stages read them, and a text layer.
    struct Fixture {
        _dir: tempfile::TempDir,
        session: Session,
        page: koharu_scene::PageId,
        ids: std::collections::BTreeMap<String, LayerId>,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("a folder");
            let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
            let c2pa = json!({"c2pa": {"detection": {"action": "c2pa.created"},
                "ocr": {"action": "c2pa.created"}}});
            let block = |role: &str, x: f64| {
                json!({"kind": "block", "role": role, "attributes": c2pa,
                    "source": {"content": "どうして", "frame": {"box": {"bounds":
                        {"x": x, "y": 20.0, "width": 40.0, "height": 120.0}}}}})
            };
            let detected = json!({"c2pa": {"detection": {"action": "c2pa.created"}}});
            let ops = [
                json!({"op": "add_page", "id": "$page", "index": 0,
                    "size": {"width": 400, "height": 400}}),
                json!({"op": "add_layer", "id": "$balloon", "page": "$page", "index": 0,
                    "layer": {"kind": "balloon", "attributes": detected,
                        "outline": "M0 0 L100 0 L100 200 L0 200 Z"}}),
                json!({"op": "add_layer", "id": "$held", "page": "$page", "parent": "$balloon",
                    "index": 0, "layer": block("text", 20.0)}),
                json!({"op": "add_layer", "id": "$panel", "page": "$page", "index": 1,
                    "layer": {"kind": "panel", "attributes": detected,
                        "outline": "M0 250 L400 250 L400 400 L0 400 Z"}}),
                json!({"op": "add_layer", "id": "$text", "page": "$page", "index": 2,
                    "layer": block("text", 150.0)}),
                json!({"op": "add_layer", "id": "$effect", "page": "$page", "index": 3,
                    "layer": block("sfx", 250.0)}),
                json!({"op": "add_layer", "id": "$typed", "page": "$page", "index": 4,
                    "layer": {"kind": "text", "content": "Hi"}}),
            ];
            let change = session
                .apply(Edit {
                    label: "test".to_owned(),
                    ops: ops
                        .into_iter()
                        .map(|op| serde_json::from_value(op).expect("an op"))
                        .collect(),
                })
                .expect("a page");
            let ids = change
                .created
                .iter()
                .filter_map(|created| match created {
                    Created::Layer {
                        id,
                        label: Some(label),
                    } => Some((label.clone(), *id)),
                    _ => None,
                })
                .collect();
            let page = session.project().pages().next().expect("a page").id;
            let mut fixture = Self {
                _dir: dir,
                session,
                page,
                ids,
            };
            // Each block's source as the Recognition stage records it.
            let ops = ["held", "text", "effect"]
                .map(|name| recognized(&fixture.layer(name), "どうして").expect("the record"));
            fixture.apply(ops.into());
            fixture
        }

        fn id(&self, name: &str) -> LayerId {
            self.ids[name]
        }

        fn apply(&mut self, ops: Vec<Op>) {
            self.session
                .apply(Edit {
                    label: "test".to_owned(),
                    ops,
                })
                .expect("the edit");
        }

        fn op(&mut self, op: Value) {
            self.apply(vec![serde_json::from_value(op).expect("an op")]);
        }

        fn page(&self) -> Page {
            self.session
                .project()
                .page(self.page)
                .expect("the page")
                .clone()
        }

        fn layer(&self, name: &str) -> Layer {
            self.session
                .project()
                .layer(self.id(name))
                .expect("the layer")
                .clone()
        }

        /// Why `stage` leaves out the block `name`, under `settings`.
        fn left_out(
            &self,
            name: &str,
            stage: RunStage,
            settings: &LetteringSettings,
            redo: bool,
            named_subtitle: bool,
        ) -> Option<Reason> {
            left_out(
                &self.page(),
                &self.layer(name),
                stage,
                settings,
                redo,
                named_subtitle,
            )
        }

        /// Translates the block `name` as the Translation stage records one.
        fn translate(&mut self, name: &str, text: &str) {
            let id = self.id(name);
            let record = Auto {
                text: Some(text_hash(text)),
                ..Auto::of(&self.layer(name))
            };
            self.apply(vec![
                Op::SetText {
                    layer: id.into(),
                    text: text.to_owned(),
                },
                Op::SetAttribute {
                    layer: id.into(),
                    namespace: "c2pa".to_owned(),
                    key: "translation".to_owned(),
                    value: Some(json!({"action": "c2pa.translated"})),
                },
                attrs::set(id, &record).expect("the record"),
            ]);
        }
    }

    const STAGES: [RunStage; 5] = [
        RunStage::Detect,
        RunStage::Recognize,
        RunStage::Inpaint,
        RunStage::Read,
        RunStage::Translate,
    ];

    fn translate_all() -> LetteringSettings {
        LetteringSettings {
            sfx: SfxMode::TranslateAll,
            ..LetteringSettings::default()
        }
    }

    #[test]
    fn untouched_blocks_are_left_out_by_no_stage() {
        let fixture = Fixture::new();
        let settings = translate_all();
        for name in ["held", "text", "effect"] {
            for stage in STAGES {
                assert_eq!(fixture.left_out(name, stage, &settings, false, false), None);
            }
        }
        // A layer that isn't a block is no stage's.
        let page = fixture.page();
        let panel = fixture.layer("panel");
        assert_eq!(
            left_out(&page, &panel, RunStage::Translate, &settings, false, false),
            None
        );
    }

    #[test]
    fn every_stage_leaves_out_a_locked_block_even_on_a_redo() {
        let mut fixture = Fixture::new();
        let settings = translate_all();
        fixture.op(json!({"op": "update_layer", "layer": fixture.id("text"),
            "patch": {"locked": true}}));
        fixture.op(json!({"op": "update_layer", "layer": fixture.id("balloon"),
            "patch": {"locked": true}}));
        for name in ["text", "held"] {
            for stage in STAGES {
                for (redo, named) in [(false, false), (true, false), (true, true)] {
                    assert_eq!(
                        fixture.left_out(name, stage, &settings, redo, named),
                        Some(Reason::Locked),
                        "{name} {stage:?}"
                    );
                }
            }
        }
        let page = fixture.page();
        assert!(locked(&page, fixture.id("held")) && locked(&page, fixture.id("balloon")));
        assert!(!locked(&page, fixture.id("effect")));
    }

    #[test]
    fn cleaning_leaves_out_a_locked_clean_up_and_nothing_fixed_by_hand() {
        let mut fixture = Fixture::new();
        let settings = translate_all();
        fixture.translate("text", "Why?");
        fixture.apply(vec![Op::SetText {
            layer: fixture.id("text").into(),
            text: "Why, though?".to_owned(),
        }]);
        assert_eq!(
            fixture.left_out("text", RunStage::Inpaint, &settings, false, false),
            None,
            "cleaning again never asks"
        );
        assert_eq!(
            fixture.left_out("text", RunStage::Translate, &settings, false, false),
            Some(Reason::HandFixed)
        );
        fixture.op(json!({"op": "update_layer", "layer": fixture.id("effect"),
            "patch": {"cleanup": {"locked": true}}}));
        assert!(!cleanable(&fixture.page(), &fixture.layer("effect")));
        assert_eq!(
            fixture.left_out("effect", RunStage::Inpaint, &settings, true, false),
            Some(Reason::Locked)
        );
        // Only cleaning writes the clean-up.
        assert_eq!(
            fixture.left_out("effect", RunStage::Translate, &settings, false, false),
            None
        );
    }

    #[test]
    fn reading_and_translating_leave_out_what_was_fixed_by_hand_but_a_redo() {
        let mut fixture = Fixture::new();
        let settings = translate_all();
        // A source corrected by hand.
        fixture.op(json!({"op": "update_layer", "layer": fixture.id("text"),
            "patch": {"source": {"content": "なぜ"}}}));
        assert_eq!(
            fixture.left_out("text", RunStage::Recognize, &settings, false, false),
            Some(Reason::HandFixed)
        );
        assert_eq!(
            fixture.left_out("text", RunStage::Recognize, &settings, true, false),
            None
        );
        // Its translation is still the stage's to write.
        assert_eq!(
            fixture.left_out("text", RunStage::Translate, &settings, false, false),
            None
        );

        // A translation fixed by hand.
        fixture.translate("held", "Why?");
        assert_eq!(
            fixture.left_out("held", RunStage::Translate, &settings, false, false),
            None
        );
        fixture.apply(vec![Op::SetText {
            layer: fixture.id("held").into(),
            text: "How come?".to_owned(),
        }]);
        assert_eq!(
            fixture.left_out("held", RunStage::Translate, &settings, false, false),
            Some(Reason::HandFixed)
        );
        assert_eq!(
            fixture.left_out("held", RunStage::Translate, &settings, true, false),
            None
        );
        assert_eq!(
            fixture.left_out("held", RunStage::Recognize, &settings, false, false),
            None
        );

        // A clean-up hidden by hand on a block with no translation.
        fixture.op(json!({"op": "update_layer", "layer": fixture.id("effect"),
            "patch": {"cleanup": {"visible": false}}}));
        assert_eq!(
            fixture.left_out("effect", RunStage::Translate, &settings, false, false),
            Some(Reason::HandFixed)
        );
        assert_eq!(
            fixture.left_out("effect", RunStage::Translate, &settings, true, false),
            None
        );
    }

    #[test]
    fn a_box_styled_before_its_translated_is_still_translated() {
        let mut fixture = Fixture::new();
        let settings = translate_all();
        let translating = |fixture: &Fixture, name: &str, redo: bool| {
            fixture.left_out(name, RunStage::Translate, &settings, redo, false)
        };
        // Styled in the type bar before it's translated: the stage still
        // translates it, while Detect again keeps the values set by hand.
        fixture.apply(vec![type_bar_style(fixture.id("held"))]);
        assert_eq!(translating(&fixture, "held", false), None);
        assert!(keeps(&fixture.page(), &fixture.layer("held")));

        // Words typed into it are the user's.
        fixture.apply(vec![Op::SetText {
            layer: fixture.id("held").into(),
            text: "Why?".to_owned(),
        }]);
        assert_eq!(
            translating(&fixture, "held", false),
            Some(Reason::HandFixed)
        );

        // Its clean-up hidden by hand: the page's lettering, kept by hand.
        fixture.apply(vec![type_bar_style(fixture.id("text"))]);
        fixture.op(json!({"op": "update_layer", "layer": fixture.id("text"),
            "patch": {"cleanup": {"visible": false}}}));
        assert_eq!(
            translating(&fixture, "text", false),
            Some(Reason::HandFixed)
        );
        // A redo takes both.
        assert_eq!(translating(&fixture, "held", true), None);
        assert_eq!(translating(&fixture, "text", true), None);
    }

    #[test]
    fn a_translation_typed_by_hand_and_cleared_is_left_alone_but_by_a_redo() {
        let mut fixture = Fixture::new();
        let settings = translate_all();
        let translating = |fixture: &Fixture, name: &str, redo: bool| {
            fixture.left_out(name, RunStage::Translate, &settings, redo, false)
        };
        let set = |id: LayerId, text: &str| Op::SetText {
            layer: id.into(),
            text: text.to_owned(),
        };
        // A first translation typed by hand, with the marker in its step,
        // then cleared: blank on purpose, a hand edit Translate leaves alone
        // unless Redo boxes takes it.
        let held = fixture.id("held");
        let marker = hand_typed(&fixture.layer("held")).expect("the record");
        fixture.apply(vec![set(held, "Why?"), marker]);
        fixture.apply(vec![set(held, "")]);
        assert_eq!(
            translating(&fixture, "held", false),
            Some(Reason::HandFixed)
        );
        assert_eq!(translating(&fixture, "held", true), None);
        // Styled since, it's still one.
        fixture.apply(vec![type_bar_style(held)]);
        assert_eq!(
            translating(&fixture, "held", false),
            Some(Reason::HandFixed)
        );

        // The same with no marker, as a project saved before Booharu kept
        // one holds it, or as Koharu's agent edit tool types it: a
        // placeholder, which Translate fills.
        let text = fixture.id("text");
        fixture.apply(vec![set(text, "Why?")]);
        fixture.apply(vec![set(text, "")]);
        assert_eq!(translating(&fixture, "text", false), None);
    }

    #[test]
    fn what_the_planner_translates_is_never_left_out_as_fixed_by_hand() {
        // Each block the planner sends to the Translation stage, as it has
        // no translation of its own and wasn't given back, or carries the
        // kept mark, the stage must translate, whether it's styled in the
        // type bar, kept as art, given back, or shown again by hand.
        type Step = fn(&mut Fixture, &str);
        fn style(fixture: &mut Fixture, name: &str) {
            fixture.apply(vec![type_bar_style(fixture.id(name))]);
        }
        fn moved(fixture: &mut Fixture, name: &str) {
            fixture.op(json!({"op": "transform_layer", "layer": fixture.id(name),
                "transform": [[1.0, 0.0, 6.0], [0.0, 1.0, 4.0]]}));
        }
        fn blank(fixture: &mut Fixture, name: &str) {
            fixture.apply(vec![Op::SetText {
                layer: fixture.id(name).into(),
                text: " ".to_owned(),
            }]);
        }
        /// As a first translation is typed by hand, the marker in its step.
        fn typed(fixture: &mut Fixture, name: &str) {
            let marker = hand_typed(&fixture.layer(name)).expect("the record");
            fixture.apply(vec![
                Op::SetText {
                    layer: fixture.id(name).into(),
                    text: "Why?".to_owned(),
                },
                marker,
            ]);
        }
        fn keep(fixture: &mut Fixture, name: &str) {
            let ops = sfx::kept_original(&fixture.layer(name));
            fixture.apply(ops);
        }
        fn clean_up(fixture: &mut Fixture, name: &str, visible: bool) {
            fixture.op(json!({"op": "update_layer", "layer": fixture.id(name),
                "patch": {"cleanup": {"visible": visible}}}));
        }
        fn show(fixture: &mut Fixture, name: &str) {
            clean_up(fixture, name, true);
        }
        fn hide(fixture: &mut Fixture, name: &str) {
            clean_up(fixture, name, false);
        }
        /// As the Translation stage gives back a line, its translation
        /// removed.
        fn give_back(fixture: &mut Fixture, name: &str) {
            let record = given_back(&fixture.layer(name)).expect("the record");
            let removed = json!({"op": "update_layer", "layer": fixture.id(name),
                "patch": {"translation": null, "cleanup": {"visible": false}}});
            fixture.apply(vec![
                serde_json::from_value(removed).expect("an op"),
                record,
            ]);
        }
        /// As the Translation stage gives back a line, its placeholder kept.
        fn give_back_styled(fixture: &mut Fixture, name: &str) {
            let record = given_back(&fixture.layer(name)).expect("the record");
            hide(fixture, name);
            fixture.apply(vec![record]);
        }
        let sequences: [&[Step]; 9] = [
            &[style, moved, blank, hide, show],
            &[blank, style, keep],
            &[keep, style, show, hide],
            &[style, keep, show, keep],
            &[give_back, style, show],
            &[style, give_back, show, style],
            &[style, give_back_styled, show],
            &[style, typed, blank, keep, hide, show],
            &[give_back, typed, blank, style],
        ];
        let names = ["held", "text", "effect"];
        let mut planned = 0;
        for (sequence, steps) in sequences.into_iter().enumerate() {
            let mut fixture = Fixture::new();
            for (index, step) in steps.iter().enumerate() {
                for name in names {
                    step(&mut fixture, name);
                }
                for name in names {
                    let layer = fixture.layer(name);
                    let Content::Block(block) = &layer.content else {
                        panic!("{name} isn't a block");
                    };
                    let translates = status::untranslated(&layer, block)
                        || (status::unwritten(&layer, block) && Auto::of(&layer).kept);
                    if !translates {
                        continue;
                    }
                    planned += 1;
                    for settings in [translate_all(), LetteringSettings::default()] {
                        assert_ne!(
                            fixture.left_out(name, RunStage::Translate, &settings, false, false),
                            Some(Reason::HandFixed),
                            "{name} after step {index} of sequence {sequence}"
                        );
                    }
                }
            }
        }
        assert!(planned >= 40, "only {planned} blocks to translate");
    }

    #[test]
    fn a_sound_effect_kept_as_art_is_left_out_whatever_a_redo_says() {
        let mut fixture = Fixture::new();
        let skip = LetteringSettings::default();
        assert_eq!(skip.sfx, SfxMode::Skip);
        for redo in [false, true] {
            assert_eq!(
                fixture.left_out("effect", RunStage::Translate, &skip, redo, false),
                Some(Reason::SoundEffect)
            );
        }
        // Other stages still read and clean it.
        for stage in [RunStage::Recognize, RunStage::Inpaint] {
            assert_eq!(fixture.left_out("effect", stage, &skip, false, false), None);
        }
        // Kept as art, it's the effect's skip that leaves it out, not a hand
        // edit, so no redo asks about it.
        let ops = sfx::kept_original(&fixture.layer("effect"));
        fixture.apply(ops);
        assert_eq!(
            fixture.left_out("effect", RunStage::Translate, &skip, false, false),
            Some(Reason::SoundEffect)
        );
        // Translate all translates it, kept mark and all.
        assert_eq!(
            fixture.left_out(
                "effect",
                RunStage::Translate,
                &translate_all(),
                false,
                false
            ),
            None
        );
    }

    #[test]
    fn a_subtitle_run_translates_the_effects_it_names() {
        let mut fixture = Fixture::new();
        let skip = LetteringSettings::default();
        assert_eq!(
            fixture.left_out("effect", RunStage::Translate, &skip, false, true),
            None,
            "skipped, but named"
        );
        // Hidden by hand too.
        fixture.op(json!({"op": "update_layer", "layer": fixture.id("effect"),
            "patch": {"cleanup": {"visible": false}}}));
        assert_eq!(
            fixture.left_out("effect", RunStage::Translate, &skip, false, true),
            None
        );
        // A named block that isn't a sound effect follows the usual rules.
        fixture.translate("text", "Why?");
        fixture.apply(vec![Op::SetText {
            layer: fixture.id("text").into(),
            text: "How come?".to_owned(),
        }]);
        assert_eq!(
            fixture.left_out("text", RunStage::Translate, &skip, false, true),
            Some(Reason::HandFixed)
        );
    }

    #[test]
    fn detect_again_keeps_what_was_made_fixed_labelled_noted_or_locked_by_hand() {
        let mut fixture = Fixture::new();
        let page = fixture.page();
        // Detected and untouched, it goes; text and what detection didn't
        // make stay.
        for name in ["balloon", "held", "panel", "text", "effect"] {
            assert!(!keeps(&page, &fixture.layer(name)), "{name}");
        }
        assert!(keeps(&page, &fixture.layer("typed")));

        // Fixed by hand.
        fixture.op(json!({"op": "update_layer", "layer": fixture.id("text"),
            "patch": {"source": {"content": "なぜ"}}}));
        assert!(keeps(&fixture.page(), &fixture.layer("text")));
        // A user's label, never a model's.
        let label = |source| Lettering {
            category: crate::lettering::LetteringCategory::Shout,
            source,
            confidence: None,
            role: None,
        };
        let held = fixture.id("held");
        fixture.apply(vec![
            attrs::set(held, &label(LetteringSource::Model)).expect("a label"),
        ]);
        assert!(!keeps(&fixture.page(), &fixture.layer("held")));
        fixture.apply(vec![
            attrs::set(held, &label(LetteringSource::User)).expect("a label"),
        ]);
        assert!(keeps(&fixture.page(), &fixture.layer("held")));
        // A reviewer's note.
        let effect = fixture.id("effect");
        fixture.apply(vec![
            attrs::set(effect, &ReviewNote("Check".to_owned())).expect("a note"),
        ]);
        assert!(keeps(&fixture.page(), &fixture.layer("effect")));
        fixture.apply(vec![attrs::clear::<ReviewNote>(effect)]);
        assert!(!keeps(&fixture.page(), &fixture.layer("effect")));
        // A subtitle.
        let typed = fixture.id("typed");
        fixture.apply(vec![
            attrs::set(typed, &SfxSubtitle { block: effect }).expect("a subtitle"),
        ]);
        assert!(keeps(&fixture.page(), &fixture.layer("effect")));
        // A lock.
        fixture.op(json!({"op": "update_layer", "layer": fixture.id("panel"),
            "patch": {"locked": true}}));
        assert!(keeps(&fixture.page(), &fixture.layer("panel")));

        // Drawn by hand: a block, panel, or balloon detection didn't make.
        let drawn = json!({"op": "add_layer", "id": "$drawn", "page": fixture.page, "index": 0,
            "layer": {"kind": "block", "source": {"frame": {"box": {"bounds":
                {"x": 300.0, "y": 300.0, "width": 40.0, "height": 40.0}}}}}});
        let made = json!({"op": "add_layer", "id": "$made", "page": fixture.page, "index": 0,
            "layer": {"kind": "balloon", "outline": "M300 0 L400 0 L400 100 L300 100 Z"}});
        let change = fixture
            .session
            .apply(Edit {
                label: "test".to_owned(),
                ops: vec![
                    serde_json::from_value(drawn).expect("an op"),
                    serde_json::from_value(made).expect("an op"),
                ],
            })
            .expect("drawn");
        let page = fixture.page();
        let mut checked = 0;
        for created in change.created {
            // The drawn block's parts are named after it, but aren't in the
            // tree.
            if let Created::Layer {
                id,
                label: Some(label),
            } = created
                && (label == "drawn" || label == "made")
            {
                let layer = fixture
                    .session
                    .project()
                    .layer(id)
                    .expect("a layer")
                    .clone();
                assert!(keeps(&page, &layer), "{:?}", layer.content);
                checked += 1;
            }
        }
        assert_eq!(checked, 2);
    }

    #[test]
    fn detect_again_keeps_a_split_half_though_detection_made_it() {
        let mut fixture = Fixture::new();
        let settings = translate_all();
        // A split's half keeps the detection record it was copied with, and
        // its record says it's a half of a block split by hand, which counts
        // as no other hand edit.
        let held = fixture.id("held");
        let half = Auto {
            split: true,
            ..Auto::of(&fixture.layer("held"))
        };
        fixture.apply(vec![attrs::set(held, &half).expect("the record")]);
        let layer = fixture.layer("held");
        assert!(recorded(&layer, DETECTION));
        assert!(!edited(&layer).hand_edited());
        assert!(keeps(&fixture.page(), &layer));
        // Every stage still reads, cleans, and translates it, as the split's
        // own run does.
        for stage in STAGES {
            assert_eq!(
                fixture.left_out("held", stage, &settings, false, false),
                None
            );
        }
        // Translated as the Translation stage records one, it stays kept.
        fixture.translate("held", "Why?");
        assert!(keeps(&fixture.page(), &fixture.layer("held")));
    }

    #[test]
    fn detect_again_keeps_a_clean_up_hidden_by_hand_but_not_booharus_or_the_translators() {
        let mut fixture = Fixture::new();
        let hide = |id: LayerId| Op::UpdateLayer {
            layer: id.into(),
            patch: json!({"cleanup": {"visible": false}}),
        };
        // Hidden by hand on a block with no translation: kept, though nothing
        // else about it changed.
        let effect = fixture.id("effect");
        fixture.apply(vec![hide(effect)]);
        let layer = fixture.layer("effect");
        assert!(!edited(&layer).hand_edited());
        assert!(keeps(&fixture.page(), &layer));

        // Hidden by Booharu, with the kept mark, to keep the art: replaced.
        let ops = sfx::kept_original(&fixture.layer("text"));
        assert!(!ops.is_empty());
        fixture.apply(ops);
        let layer = fixture.layer("text");
        assert!(Auto::of(&layer).kept);
        assert!(!keeps(&fixture.page(), &layer));

        // Hidden by the Translation stage, given the source back: replaced.
        let held = fixture.id("held");
        let given = given_back(&fixture.layer("held")).expect("the record");
        fixture.apply(vec![hide(held), given]);
        let layer = fixture.layer("held");
        assert_eq!(Auto::of(&layer).text, Some(text_hash("")));
        assert!(!keeps(&fixture.page(), &layer));
    }

    #[test]
    fn detect_again_keeps_a_clean_up_shown_by_hand_on_an_effect_kept_as_art() {
        let mut fixture = Fixture::new();
        // Kept as art by Booharu, its clean-up hidden with the kept mark:
        // replaced, as no hand chose that.
        let ops = sfx::kept_original(&fixture.layer("effect"));
        assert!(!ops.is_empty());
        fixture.apply(ops);
        assert!(!keeps(&fixture.page(), &fixture.layer("effect")));
        // Its clean-up shown again by hand, the mark left: kept, though
        // nothing else about it changed, as keeping the art leaves that
        // choice alone.
        let effect = fixture.id("effect");
        fixture.apply(vec![Op::UpdateLayer {
            layer: effect.into(),
            patch: json!({"cleanup": {"visible": true}}),
        }]);
        let layer = fixture.layer("effect");
        assert!(!edited(&layer).hand_edited());
        assert!(sfx::kept_original(&layer).is_empty());
        assert!(keeps(&fixture.page(), &layer));
    }

    #[test]
    fn detect_again_keeps_a_translation_skewed_by_hand() {
        let mut fixture = Fixture::new();
        fixture.translate("text", "Why?");
        assert!(!keeps(&fixture.page(), &fixture.layer("text")));
        // Skewed in the FX window: its transform and the skew's record.
        let id = fixture.id("text");
        let skew_to = |fixture: &mut Fixture, x: f64| {
            let layer = fixture.layer("text");
            let Content::Block(block) = &layer.content else {
                panic!("not a block");
            };
            let translation = block.translation.as_ref().expect("a translation");
            let pivot = skew::pivot(&translation.frame);
            let record = attrs::get::<Skew>(&layer);
            let transform = skew::reskewed(translation.transform, record.as_ref(), (x, 0.0), pivot);
            let record = if x == 0.0 {
                attrs::clear::<Skew>(id)
            } else {
                let skew = Skew {
                    x,
                    y: 0.0,
                    pivot: [pivot.x, pivot.y],
                };
                attrs::set(id, &skew).expect("the record")
            };
            fixture.apply(vec![
                Op::UpdateLayer {
                    layer: id.into(),
                    patch: json!({"translation": {"transform": transform}}),
                },
                record,
            ]);
        };
        skew_to(&mut fixture, 20.0);
        let layer = fixture.layer("text");
        assert!(!edited(&layer).hand_edited(), "a skew is no hand edit");
        let Content::Block(block) = &layer.content else {
            panic!("not a block");
        };
        assert!(skewed(&layer, block));
        assert!(keeps(&fixture.page(), &layer));
        // Skewed back to none: detected again.
        skew_to(&mut fixture, 0.0);
        assert!(!keeps(&fixture.page(), &fixture.layer("text")));
    }
}
