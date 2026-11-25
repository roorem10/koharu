//! Sound effects: which ones a run leaves as the original art, the kept mark
//! Booharu leaves on a block whose clean-up it hid to keep that art, and the
//! subtitle, a small plain translation beside an effect kept as art.
//!
//! A block is a sound effect when its lettering category, as Booharu reads
//! it, is Sound Effect ([`sound_effect`]): a user's label, then a model's,
//! then Koharu's role, never whether Pick fonts automatically is on.
//! `[booharu-lettering] sfx` then says which of them a run translates
//! ([`skipped`]); one with a subtitle is never translated again, so its
//! translation doesn't come back over the art.
//!
//! An effect a run skips keeps the page's own lettering: no translation, or
//! only the placeholder a style set before translating leaves
//! ([`status::placeholder`]), its clean-up hidden, with the kept mark in its
//! hand-edit record ([`kept_original`]), so a clean-up hidden by hand can be
//! told from Booharu's own ([`hand_kept`]), and one shown again by hand from
//! one Booharu never kept ([`hand_shown`]). The mark is never the given-back
//! record, which [`crate::styles::given_back`] writes when Koharu's
//! Translation stage is given its source back, as the stage records nothing
//! there; it would have the quality check report every effect as given back
//! untranslated. A first translation typed into such an effect, on the
//! canvas, in the Story panel or through the agent, shows its clean-up
//! again ([`shown_again`]) and takes the mark off, as the record of a
//! translation typed by hand it writes carries none
//! ([`crate::styles::hand_typed`]).
//!
//! The run filters, the planner, the quality check, and the commands all
//! call these functions, never a copy of their own.

use anyhow::{Context as _, Result, bail};
use koharu_scene::{Block, Content, Direction, LanguageIdentifier, Layer, LayerId, Op, Page, Text};
use kurbo::Rect;
use serde_json::json;

use crate::{
    attrs::{self, Attr as _, Auto, Lettering, SfxSubtitle},
    lettering::{self, LetteringCategory},
    settings::{LetteringSettings, SfxMode, SubtitleStyle},
    shape, status,
};

/// The name a subtitle's layer gets.
pub const SUBTITLE_NAME: &str = "SFX subtitle";

/// The reference [`subtitle_ops`] gives the subtitle's new layer, by which
/// the edit's change names its id.
pub const SUBTITLE_REF: &str = "subtitle";

/// How much taller than wide a source with no measured direction must be to
/// read top to bottom, as Booharu's drawn boxes decide it.
const VERTICAL: f64 = 1.15;

/// Whether the block `block` on `page` is a sound effect: its lettering
/// category is Sound Effect, by a user's label, then a model's, then
/// Koharu's role, whether or not Pick fonts automatically is on. False for a
/// layer that isn't a block there.
pub fn sound_effect(page: &Page, block: LayerId, settings: &LetteringSettings) -> bool {
    lettering::effective(page, block, settings)
        .is_some_and(|effective| effective.category == LetteringCategory::Sfx)
}

/// The text layer on `page` that subtitles the block `block`, the first in
/// tree order whose `booharu.sfx_subtitle` names it; none when none does, or
/// when no block `block` is on the page, so a subtitle whose effect is gone
/// counts for nothing.
pub fn subtitle_of(page: &Page, block: LayerId) -> Option<LayerId> {
    let (layer, _) = shape::find(page, block)?;
    if !matches!(layer.content, Content::Block(_)) {
        return None;
    }
    fn search(layers: &[Layer], block: LayerId) -> Option<LayerId> {
        layers.iter().find_map(|layer| {
            let names = matches!(layer.content, Content::Text(_))
                && attrs::get::<SfxSubtitle>(layer).is_some_and(|found| found.block == block);
            if names {
                Some(layer.id)
            } else {
                search(&layer.children, block)
            }
        })
    }
    search(&page.layers, block)
}

/// Whether the block `block` on `page` has a subtitle ([`subtitle_of`]).
pub fn subtitled(page: &Page, block: LayerId) -> bool {
    subtitle_of(page, block).is_some()
}

/// Whether a run leaves the block `block` on `page` as the original art,
/// under `settings`: it has a subtitle, or it's a sound effect that
/// `settings.sfx` doesn't translate (every one for Skip, those in no
/// balloon, neither held by one nor inside one's outline, for Inside bubbles,
/// none for Translate all).
pub fn skipped(page: &Page, block: LayerId, settings: &LetteringSettings) -> bool {
    subtitled(page, block)
        || (sound_effect(page, block, settings) && mode_skips(page, block, settings.sfx))
}

/// Whether the block `block` on `page` would be left as the original art
/// with the label `label` and Koharu's role `sfx_role`, a sound effect when
/// true, in place of its own, as [`skipped`] reads one whose label and role
/// are being written.
pub fn skipped_as(
    page: &Page,
    block: LayerId,
    label: Option<&Lettering>,
    sfx_role: bool,
    settings: &LetteringSettings,
) -> bool {
    let sound_effect = lettering::effective_as(page, block, label, sfx_role, settings)
        .is_some_and(|effective| effective.category == LetteringCategory::Sfx);
    subtitled(page, block) || (sound_effect && mode_skips(page, block, settings.sfx))
}

/// Whether `mode` leaves the sound effect `block` on `page` untranslated.
fn mode_skips(page: &Page, block: LayerId, mode: SfxMode) -> bool {
    match mode {
        SfxMode::Skip => true,
        SfxMode::InsideBubbles => shape::balloon(page, block).is_none(),
        SfxMode::TranslateAll => false,
    }
}

/// The ops keeping the original art of the block `layer` holds, read before
/// the edit: its clean-up hidden and the kept mark in its hand-edit record,
/// for a block with no translation of its own ([`status::unwritten`]: none,
/// or only the placeholder a style set before translating leaves, which
/// stays with its style) whose clean-up shows and that carries no mark.
/// Anything else gets none: a translated block keeps its translation, and
/// one whose clean-up shows while it carries the mark had it shown by hand
/// ([`hand_shown`]). It never writes the given-back record or a `c2pa` one.
pub fn kept_original(layer: &Layer) -> Vec<Op> {
    let Content::Block(block) = &layer.content else {
        return Vec::new();
    };
    let record = Auto::of(layer);
    if !status::unwritten(layer, block) || !block.cleanup.visible || record.kept {
        return Vec::new();
    }
    let marked = Auto {
        kept: true,
        ..record
    };
    // A record read as valid stays valid with the mark, so this never fails;
    // if it did, the clean-up stays as it is rather than hidden unmarked.
    let Ok(mark) = attrs::set(layer.id, &marked) else {
        return Vec::new();
    };
    vec![
        Op::UpdateLayer {
            layer: layer.id.into(),
            patch: json!({"cleanup": {"visible": false}}),
        },
        mark,
    ]
}

/// The inverse of [`kept_original`]'s clean-up, for a first translation
/// typed into an effect kept as art, read before the edit: for a block with
/// no translation of its own ([`status::unwritten`]: none, or only a
/// placeholder) that carries the kept mark, the op showing its clean-up
/// when hidden. The mark comes off in the same edit, with the record of a
/// translation typed by hand that the typing writes
/// ([`styles::hand_typed`](crate::styles::hand_typed)), which carries none.
/// Any other layer gets none: a block with no mark has its clean-up as the
/// user left it, hidden by hand or shown, and a translated block had its
/// mark dealt with as the translation came.
pub fn shown_again(layer: &Layer) -> Vec<Op> {
    let Content::Block(block) = &layer.content else {
        return Vec::new();
    };
    if !status::unwritten(layer, block) || !Auto::of(layer).kept || block.cleanup.visible {
        return Vec::new();
    }
    vec![Op::UpdateLayer {
        layer: layer.id.into(),
        patch: json!({"cleanup": {"visible": true}}),
    }]
}

/// The op taking the kept mark off `layer`'s hand-edit record: the record
/// without it, or no record at all when nothing else is left in it; none for
/// a layer whose record carries no mark. A join that shows the block's
/// clean-up or writes it a translation takes it off; a first translation
/// typed into the block does with the marker of one typed by hand
/// ([`styles::hand_typed`](crate::styles::hand_typed)).
pub fn unmarked(layer: &Layer) -> Result<Option<Op>> {
    let record = Auto::of(layer);
    if !record.kept {
        return Ok(None);
    }
    let record = Auto {
        kept: false,
        ..record
    };
    Ok(Some(if record == Auto::default() {
        attrs::clear::<Auto>(layer.id)
    } else {
        attrs::set(layer.id, &record)?
    }))
}

/// Whether `block`, which `layer` holds, had its clean-up hidden by hand: it
/// keeps the page's lettering ([`status::kept`]), styled since or not, but
/// neither Koharu's Translation stage gave it back nor Booharu kept it as
/// art.
pub fn hand_kept(layer: &Layer, block: &Block) -> bool {
    status::kept(layer, block) && !status::given_back(layer, block) && !Auto::of(layer).kept
}

/// Whether `block`, which `layer` holds, had its clean-up shown again by
/// hand after Booharu kept it as art: it has no translation of its own
/// ([`status::unwritten`]), and its clean-up shows though it carries the
/// kept mark, which [`kept_original`] leaves alone as the user's choice.
pub fn hand_shown(layer: &Layer, block: &Block) -> bool {
    status::unwritten(layer, block) && block.cleanup.visible && Auto::of(layer).kept
}

/// Whether `source`, a sound effect's source lying at `rect` on the page,
/// reads top to bottom: its direction, which Koharu's detection measures;
/// a source with no wording read, which may hold only the scene's default
/// direction, goes by its shape, at least 1.15 times taller than wide.
pub fn vertical(source: &Text, rect: Rect) -> bool {
    match source.direction {
        Direction::Vertical => true,
        Direction::Horizontal => {
            source.content.text().trim().is_empty() && rect.height() >= VERTICAL * rect.width()
        }
    }
}

/// Where a subtitle `size` page pixels high goes beside a sound effect whose
/// source lies at `source` on a page `page` wide and high, as Booharu 0.0.4
/// placed one: a box 12 sizes wide and 2.8 high, each at most the page's,
/// a third of the size away, to the left of a vertical source and below any
/// other. When that leaves the page, it goes to the other side, right of a
/// vertical source and above any other; then it's moved onto the page.
pub fn subtitle_rect(source: Rect, vertical: bool, size: f64, page: (f64, f64)) -> Rect {
    let (width, height) = (page.0.max(0.0), page.1.max(0.0));
    let w = (size * 12.0).min(width);
    let h = (size * 2.8).min(height);
    let gap = size / 3.0;
    let (x, y) = if vertical {
        let left = source.x0 - gap - w;
        let x = if left < 0.0 { source.x1 + gap } else { left };
        (x, source.y0)
    } else {
        let below = source.y1 + gap;
        let y = if below + h > height {
            source.y0 - gap - h
        } else {
            below
        };
        (source.x0, y)
    };
    let x = x.clamp(0.0, width - w);
    let y = y.clamp(0.0, height - h);
    Rect::new(x, y, x + w, y + h)
}

/// The ops giving the sound effect `block` on `page` the subtitle `text` in
/// `language`, lettered as `style` says, in `font`, the default font, when
/// it names none, all for one edit: a text layer named [`SUBTITLE_NAME`],
/// added as [`SUBTITLE_REF`] right after the block in its parent, so it
/// follows the effect in reading order, framed by [`subtitle_rect`] with no
/// fit, horizontal and aligned to the start, its size `style.size` of the
/// page's shorter side and its outline a ninth of that, with
/// `booharu.sfx_subtitle` naming the block; then the block's translation
/// removed, but for only the placeholder a style set before translating
/// leaves ([`status::placeholder`]), which stays with its style as
/// [`kept_original`] keeps it, and its clean-up hidden, with the kept mark,
/// so the effect stays the original art, its record losing the marker of a
/// translation typed by hand, which goes with the translation. Fails for a
/// layer that isn't a block on `page`, or a block whose source has no frame
/// to put the subtitle beside.
pub fn subtitle_ops(
    page: &Page,
    block: LayerId,
    text: &str,
    language: &LanguageIdentifier,
    style: &SubtitleStyle,
    font: Option<&str>,
) -> Result<Vec<Op>> {
    let (layer, holder) = shape::find(page, block).context("the block isn't on the page")?;
    let Content::Block(found) = &layer.content else {
        bail!("layer {block} isn't a block");
    };
    let siblings = holder.map_or(&page.layers, |holder| &holder.children);
    let index = siblings
        .iter()
        .position(|sibling| sibling.id == block)
        .context("the block isn't among its parent's layers")?;
    let source = found
        .source
        .bounds()
        .context("the sound effect has no box to put a subtitle beside")?;
    let (width, height) = (f64::from(page.size.width), f64::from(page.size.height));
    let size = style.size * width.min(height);
    let rect = subtitle_rect(
        source,
        vertical(&found.source, source),
        size,
        (width, height),
    );
    let family = style.family.as_deref().or(font);
    let subtitle = SfxSubtitle { block };
    let add = json!({
        "op": "add_layer",
        "id": format!("${SUBTITLE_REF}"),
        "page": page.id,
        "parent": holder.map(|holder| holder.id),
        "index": index + 1,
        "layer": {
            "kind": "text",
            "name": SUBTITLE_NAME,
            "attributes": {attrs::NAMESPACE: {SfxSubtitle::KEY: subtitle}},
            "content": text,
            "language": language.to_string(),
            "direction": Direction::Horizontal,
            "style": {
                "chars": {
                    "family": family,
                    "size": size,
                    "fill": style.fill,
                    "stroke": style.stroke,
                    "stroke_width": size / 9.0,
                },
                "align": "start",
            },
            "frame": {"box": {
                "bounds": {"x": rect.x0, "y": rect.y0, "width": rect.width(), "height": rect.height()},
                "fit": "none",
            }},
        },
    });
    // The translation goes, and the marker of one typed by hand with it.
    let marked = Auto {
        kept: true,
        typed: false,
        ..Auto::of(layer)
    };
    // A style set on the effect before it's translated is the user's (D1).
    let patch = match status::placeholder(layer, found) {
        true => json!({"cleanup": {"visible": false}}),
        false => json!({"translation": null, "cleanup": {"visible": false}}),
    };
    Ok(vec![
        serde_json::from_value(add)?,
        Op::UpdateLayer {
            layer: block.into(),
            patch,
        },
        attrs::set(block, &marked)?,
    ])
}

#[cfg(test)]
mod tests {
    use koharu_scene::{Created, Edit, Rgba, Session};
    use serde_json::Value;

    use super::*;
    use crate::{
        attrs::{LetteringSource, Origin},
        status::TRANSLATION,
        styles::{self, recorded, text_hash},
        testing::{FAMILY, type_bar_style},
    };

    const RED: Rgba = Rgba([1.0, 0.0, 0.0, 1.0]);
    const YELLOW: Rgba = Rgba([1.0, 1.0, 0.0, 1.0]);

    /// A page 400 wide and 600 high holding a balloon around the block
    /// `held`, a balloon whose outline holds the block `under` though it
    /// doesn't hold it, and the block `outside`, each of Koharu's role
    /// `role`, read as `ドン` from a source 40 wide and 120 high, with no
    /// translation and its clean-up showing.
    struct Fixture {
        _dir: tempfile::TempDir,
        session: Session,
        page: koharu_scene::PageId,
        held: LayerId,
        under: LayerId,
        outside: LayerId,
        balloon: LayerId,
    }

    fn source(x: f64, y: f64, direction: &str) -> Value {
        json!({"content": "ドン", "direction": direction, "frame": {"box": {"bounds":
            {"x": x, "y": y, "width": 40.0, "height": 120.0}}}})
    }

    impl Fixture {
        fn new(role: &str) -> Self {
            let dir = tempfile::tempdir().expect("a folder");
            let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
            let block = |x, y| {
                json!({"kind": "block", "role": role, "source": source(x, y, "vertical"),
                    "attributes": {"c2pa": {"detection": {"action": "c2pa.created"},
                        "ocr": {"action": "c2pa.created"}}}})
            };
            let ops = [
                json!({"op": "add_page", "id": "$page", "index": 0,
                    "size": {"width": 400, "height": 600}}),
                json!({"op": "add_layer", "id": "$balloon", "page": "$page", "index": 0,
                    "layer": {"kind": "balloon", "outline": "M0 0 L100 0 L100 200 L0 200 Z"}}),
                json!({"op": "add_layer", "id": "$held", "page": "$page", "parent": "$balloon",
                    "index": 0, "layer": block(20.0, 20.0)}),
                json!({"op": "add_layer", "id": "$round", "page": "$page", "index": 1,
                    "layer": {"kind": "balloon",
                        "outline": "M150 0 L300 0 L300 200 L150 200 Z"}}),
                json!({"op": "add_layer", "id": "$under", "page": "$page", "index": 2,
                    "layer": block(200.0, 20.0)}),
                json!({"op": "add_layer", "id": "$outside", "page": "$page", "index": 3,
                    "layer": block(200.0, 400.0)}),
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
            let named = |name: &str| {
                change
                    .created
                    .iter()
                    .find_map(|created| match created {
                        Created::Layer {
                            id,
                            label: Some(label),
                        } if label == name => Some(*id),
                        _ => None,
                    })
                    .expect("a layer")
            };
            let page = session.project().pages().next().expect("a page").id;
            Self {
                held: named("held"),
                under: named("under"),
                outside: named("outside"),
                balloon: named("balloon"),
                _dir: dir,
                session,
                page,
            }
        }

        fn apply(&mut self, ops: Vec<Op>) -> koharu_scene::Change {
            self.session
                .apply(Edit {
                    label: "test".to_owned(),
                    ops,
                })
                .expect("the edit")
        }

        fn page(&self) -> Page {
            self.session
                .project()
                .page(self.page)
                .expect("the page")
                .clone()
        }

        fn layer(&self, id: LayerId) -> Layer {
            self.session.project().layer(id).expect("the layer").clone()
        }

        fn block(&self, id: LayerId) -> Block {
            match self.layer(id).content {
                Content::Block(block) => *block,
                other => panic!("{other:?} isn't a block"),
            }
        }

        fn label(&mut self, id: LayerId, label: Lettering) {
            self.apply(vec![attrs::set(id, &label).expect("a label")]);
        }

        /// Adds a text layer at the page's top whose `sfx_subtitle` names
        /// `block`, and answers its id.
        fn subtitle(&mut self, block: LayerId) -> LayerId {
            let add = json!({"op": "add_layer", "id": "$sub", "page": self.page, "index": 0,
                "layer": {"kind": "text", "content": "BOOM",
                    "attributes": {"booharu": {"sfx_subtitle": {"block": block}}}}});
            let change = self.apply(vec![serde_json::from_value(add).expect("an op")]);
            change
                .created
                .iter()
                .find_map(|created| match created {
                    Created::Layer { id, .. } => Some(*id),
                    _ => None,
                })
                .expect("the subtitle")
        }
    }

    fn settings(sfx: SfxMode) -> LetteringSettings {
        LetteringSettings {
            sfx,
            ..LetteringSettings::default()
        }
    }

    fn label(
        category: LetteringCategory,
        source: LetteringSource,
        confidence: Option<f32>,
    ) -> Lettering {
        Lettering {
            category,
            source,
            confidence,
            role: None,
        }
    }

    #[test]
    fn a_subtitled_effect_counts_as_handled_in_every_mode() {
        let mut fixture = Fixture::new("sfx");
        let translate_all = settings(SfxMode::TranslateAll);
        assert!(!skipped(&fixture.page(), fixture.outside, &translate_all));
        let subtitle = fixture.subtitle(fixture.outside);
        let page = fixture.page();
        assert_eq!(subtitle_of(&page, fixture.outside), Some(subtitle));
        for mode in [SfxMode::Skip, SfxMode::InsideBubbles, SfxMode::TranslateAll] {
            assert!(skipped(&page, fixture.outside, &settings(mode)), "{mode:?}");
        }
        assert!(!subtitled(&page, fixture.held));

        // A layer that isn't text naming the block isn't its subtitle.
        let raster = json!({"op": "add_layer", "page": fixture.page, "index": 0,
            "layer": {"kind": "raster",
                "attributes": {"booharu": {"sfx_subtitle": {"block": fixture.held}}}}});
        fixture.apply(vec![serde_json::from_value(raster).expect("an op")]);
        assert!(!subtitled(&fixture.page(), fixture.held));

        // A subtitle whose effect is gone counts for nothing.
        let removed = json!({"op": "remove_layer", "layer": fixture.outside});
        fixture.apply(vec![serde_json::from_value(removed).expect("an op")]);
        let page = fixture.page();
        assert_eq!(subtitle_of(&page, fixture.outside), None);
        assert!(!skipped(&page, fixture.outside, &translate_all));
    }

    #[test]
    fn each_mode_skips_its_effects_inside_and_outside_balloons() {
        let fixture = Fixture::new("sfx");
        let page = fixture.page();
        // held is held by a balloon, under lies inside one's outline, and
        // outside is in none.
        assert_eq!(
            shape::balloon(&page, fixture.held).map(|layer| layer.id),
            Some(fixture.balloon)
        );
        assert!(shape::balloon(&page, fixture.under).is_some());
        assert!(shape::balloon(&page, fixture.outside).is_none());
        let modes = [
            (SfxMode::Skip, [true, true, true]),
            (SfxMode::InsideBubbles, [false, false, true]),
            (SfxMode::TranslateAll, [false, false, false]),
        ];
        for (mode, expected) in modes {
            let answers = [fixture.held, fixture.under, fixture.outside]
                .map(|block| skipped(&page, block, &settings(mode)));
            assert_eq!(answers, expected, "{mode:?}");
        }
        // Text that isn't a sound effect is never skipped.
        let fixture = Fixture::new("text");
        let page = fixture.page();
        for block in [fixture.held, fixture.under, fixture.outside] {
            assert!(!sound_effect(&page, block, &settings(SfxMode::Skip)));
            assert!(!skipped(&page, block, &settings(SfxMode::Skip)));
        }
    }

    #[test]
    fn a_label_decides_before_the_role_as_the_category_does() {
        let skip = settings(SfxMode::Skip);
        // A user's label wins either way.
        let mut fixture = Fixture::new("text");
        fixture.label(
            fixture.outside,
            label(LetteringCategory::Sfx, LetteringSource::User, None),
        );
        let page = fixture.page();
        assert!(sound_effect(&page, fixture.outside, &skip));
        assert!(skipped(&page, fixture.outside, &skip));
        let mut fixture = Fixture::new("sfx");
        fixture.label(
            fixture.outside,
            label(LetteringCategory::Shout, LetteringSource::User, None),
        );
        let page = fixture.page();
        assert!(!sound_effect(&page, fixture.outside, &skip));
        assert!(!skipped(&page, fixture.outside, &skip));

        // A model's Sound Effect label, a presentation category, wins at any
        // confidence, above and below the setting's.
        for confidence in [0.9, 0.2] {
            let mut fixture = Fixture::new("text");
            let model = label(
                LetteringCategory::Sfx,
                LetteringSource::Model,
                Some(confidence),
            );
            fixture.label(fixture.outside, model);
            assert!(
                sound_effect(&fixture.page(), fixture.outside, &skip),
                "{confidence}"
            );
        }
        // A model's delivery label, sure or not, never outranks Koharu's
        // sound-effect role.
        for confidence in [0.9, 0.2] {
            let mut fixture = Fixture::new("sfx");
            let model = label(
                LetteringCategory::Shout,
                LetteringSource::Model,
                Some(confidence),
            );
            fixture.label(fixture.outside, model);
            assert!(
                sound_effect(&fixture.page(), fixture.outside, &skip),
                "{confidence}"
            );
        }
        // On text, a model's delivery label never makes a sound effect.
        let mut fixture = Fixture::new("text");
        let model = label(
            LetteringCategory::Whisper,
            LetteringSource::Model,
            Some(0.9),
        );
        fixture.label(fixture.outside, model);
        assert!(!sound_effect(&fixture.page(), fixture.outside, &skip));
    }

    #[test]
    fn pick_fonts_automatically_never_changes_the_answer() {
        let mut fixture = Fixture::new("text");
        fixture.label(
            fixture.under,
            label(LetteringCategory::Sfx, LetteringSource::User, None),
        );
        let page = fixture.page();
        for mode in [SfxMode::Skip, SfxMode::InsideBubbles, SfxMode::TranslateAll] {
            for block in [fixture.held, fixture.under, fixture.outside] {
                let off = settings(mode);
                let on = LetteringSettings {
                    auto: true,
                    ..off.clone()
                };
                assert_eq!(
                    sound_effect(&page, block, &off),
                    sound_effect(&page, block, &on)
                );
                assert_eq!(skipped(&page, block, &off), skipped(&page, block, &on));
            }
        }
        assert!(sound_effect(&page, fixture.under, &settings(SfxMode::Skip)));
    }

    #[test]
    fn the_rule_reads_a_label_and_role_about_to_be_written() {
        let fixture = Fixture::new("text");
        let page = fixture.page();
        let skip = settings(SfxMode::Skip);
        let user = |category| label(category, LetteringSource::User, None);
        assert!(!skipped(&page, fixture.outside, &skip));
        // The role alone, or a user's label alone, makes a sound effect.
        assert!(skipped_as(&page, fixture.outside, None, true, &skip));
        assert!(skipped_as(
            &page,
            fixture.outside,
            Some(&user(LetteringCategory::Sfx)),
            false,
            &skip
        ));
        // A user's other category wins over the role.
        assert!(!skipped_as(
            &page,
            fixture.outside,
            Some(&user(LetteringCategory::Shout)),
            true,
            &skip
        ));
        // The mode still decides, as for one written.
        let inside = settings(SfxMode::InsideBubbles);
        assert!(!skipped_as(&page, fixture.held, None, true, &inside));
        assert!(skipped_as(&page, fixture.outside, None, true, &inside));
    }

    #[test]
    fn keeping_the_original_hides_the_clean_up_with_the_mark_once() {
        let mut fixture = Fixture::new("sfx");
        let id = fixture.outside;
        let ops = kept_original(&fixture.layer(id));
        assert_eq!(ops.len(), 2, "{ops:?}");
        fixture.apply(ops);
        let layer = fixture.layer(id);
        let block = fixture.block(id);
        assert!(!block.cleanup.visible && block.translation.is_none());
        assert_eq!(attrs::get::<Auto>(&layer).map(|auto| auto.kept), Some(true));
        // Never a given-back record or a stage's.
        assert_eq!(Auto::of(&layer).text, None);
        assert!(!recorded(&layer, TRANSLATION));
        assert!(!status::given_back(&layer, &block));
        // Kept already, it writes nothing more.
        assert!(kept_original(&layer).is_empty());

        // Shown again by hand while marked, it's left alone.
        fixture.apply(vec![Op::UpdateLayer {
            layer: id.into(),
            patch: json!({"cleanup": {"visible": true}}),
        }]);
        assert!(kept_original(&fixture.layer(id)).is_empty());

        // A translated block keeps its translation.
        let other = fixture.held;
        fixture.apply(vec![Op::SetText {
            layer: other.into(),
            text: "BOOM".to_owned(),
        }]);
        assert!(kept_original(&fixture.layer(other)).is_empty());
        // And a layer that isn't a block has nothing to keep.
        assert!(kept_original(&fixture.layer(fixture.balloon)).is_empty());
    }

    #[test]
    fn showing_a_kept_effect_again_undoes_keeping_it_and_nothing_else() {
        let mut fixture = Fixture::new("sfx");
        let shown = |layer: &Layer| shown_again(layer);
        let json = |ops: &[Op]| serde_json::to_value(ops).expect("JSON");
        let show = |id: LayerId| Op::UpdateLayer {
            layer: id.into(),
            patch: json!({"cleanup": {"visible": true}}),
        };
        // The record the typing writes in the same edit, which takes the
        // mark off.
        let typed = |layer: &Layer| styles::hand_typed(layer).expect("the record");
        let marker = |before: Auto| Auto {
            typed: true,
            ..before
        };

        // Kept as art with nothing else recorded: its clean-up shows, and the
        // record holds only the marker of a translation typed by hand.
        let id = fixture.outside;
        let ops = kept_original(&fixture.layer(id));
        fixture.apply(ops);
        let ops = shown(&fixture.layer(id));
        assert_eq!(json(&ops), json(&[show(id)]));
        let record = typed(&fixture.layer(id));
        fixture.apply(ops.into_iter().chain([record]).collect());
        let layer = fixture.layer(id);
        assert!(fixture.block(id).cleanup.visible);
        assert_eq!(attrs::get::<Auto>(&layer), Some(marker(Auto::default())));
        assert!(shown(&layer).is_empty());

        // A record holding more keeps it, as it was before the mark.
        let id = fixture.under;
        let source = styles::recognized(&fixture.layer(id), "ドン").expect("the record");
        fixture.apply(vec![source]);
        let before = Auto::of(&fixture.layer(id));
        let ops = kept_original(&fixture.layer(id));
        fixture.apply(ops);
        let ops = shown(&fixture.layer(id));
        assert_eq!(json(&ops), json(&[show(id)]));
        let record = typed(&fixture.layer(id));
        fixture.apply(ops.into_iter().chain([record]).collect());
        assert!(fixture.block(id).cleanup.visible);
        assert_eq!(attrs::get::<Auto>(&fixture.layer(id)), Some(marker(before)));

        // Marked, but its clean-up shown by hand: there's nothing to show,
        // and the typing's record alone takes the mark off.
        let id = fixture.held;
        let ops = kept_original(&fixture.layer(id));
        fixture.apply(ops);
        fixture.apply(vec![show(id)]);
        assert!(shown(&fixture.layer(id)).is_empty());
        let record = typed(&fixture.layer(id));
        fixture.apply(vec![record]);
        assert!(!Auto::of(&fixture.layer(id)).kept);

        // A clean-up hidden by hand, with no mark, stays hidden.
        let mut fixture = Fixture::new("sfx");
        let id = fixture.outside;
        fixture.apply(vec![Op::UpdateLayer {
            layer: id.into(),
            patch: json!({"cleanup": {"visible": false}}),
        }]);
        assert!(hand_kept(&fixture.layer(id), &fixture.block(id)));
        assert!(shown(&fixture.layer(id)).is_empty());

        // A translated block is left as it is, even one still marked.
        let id = fixture.under;
        let ops = kept_original(&fixture.layer(id));
        fixture.apply(ops);
        fixture.apply(vec![Op::SetText {
            layer: id.into(),
            text: "BOOM".to_owned(),
        }]);
        assert!(Auto::of(&fixture.layer(id)).kept);
        assert!(shown(&fixture.layer(id)).is_empty());

        // And a layer that isn't a block has no clean-up to show.
        assert!(shown(&fixture.layer(fixture.balloon)).is_empty());
    }

    #[test]
    fn a_clean_up_hidden_by_hand_is_told_from_booharus_and_the_stages() {
        let mut fixture = Fixture::new("sfx");
        let hide = |id: LayerId| Op::UpdateLayer {
            layer: id.into(),
            patch: json!({"cleanup": {"visible": false}}),
        };
        // By hand.
        let id = fixture.outside;
        fixture.apply(vec![hide(id)]);
        assert!(hand_kept(&fixture.layer(id), &fixture.block(id)));
        // Kept by Booharu as art.
        let id = fixture.under;
        let ops = kept_original(&fixture.layer(id));
        fixture.apply(ops);
        assert!(!hand_kept(&fixture.layer(id), &fixture.block(id)));
        // Given back by the Translation stage.
        let id = fixture.held;
        let given = styles::given_back(&fixture.layer(id)).expect("the record");
        fixture.apply(vec![hide(id), given]);
        let layer = fixture.layer(id);
        assert_eq!(Auto::of(&layer).text, Some(text_hash("")));
        assert!(!hand_kept(&layer, &fixture.block(id)));
        // A translated block keeps nothing.
        let mut fixture = Fixture::new("sfx");
        let id = fixture.outside;
        fixture.apply(vec![
            Op::SetText {
                layer: id.into(),
                text: "BOOM".to_owned(),
            },
            hide(id),
        ]);
        assert!(!hand_kept(&fixture.layer(id), &fixture.block(id)));
    }

    #[test]
    fn a_clean_up_shown_again_by_hand_on_an_effect_kept_as_art_is_told() {
        let mut fixture = Fixture::new("sfx");
        let id = fixture.outside;
        let shown = |fixture: &Fixture| hand_shown(&fixture.layer(id), &fixture.block(id));
        // Untouched, its clean-up shows with no mark.
        assert!(!shown(&fixture));
        // Kept as art: hidden, with the mark.
        let ops = kept_original(&fixture.layer(id));
        fixture.apply(ops);
        assert!(!shown(&fixture));
        // Shown again by hand, the mark left: the user's choice, which
        // keeping the art leaves alone.
        fixture.apply(vec![Op::UpdateLayer {
            layer: id.into(),
            patch: json!({"cleanup": {"visible": true}}),
        }]);
        assert!(shown(&fixture));
        assert!(kept_original(&fixture.layer(id)).is_empty());
        assert!(!hand_kept(&fixture.layer(id), &fixture.block(id)));
        // Translated, it's no effect kept as art any more.
        fixture.apply(vec![Op::SetText {
            layer: id.into(),
            text: "BOOM".to_owned(),
        }]);
        assert!(!shown(&fixture));
    }

    #[test]
    fn an_effect_styled_before_its_translated_is_kept_and_shown_as_one_with_none() {
        let mut fixture = Fixture::new("sfx");
        let id = fixture.outside;
        let json = |ops: &[Op]| serde_json::to_value(ops).expect("JSON");
        let show = Op::UpdateLayer {
            layer: id.into(),
            patch: json!({"cleanup": {"visible": true}}),
        };
        fixture.apply(vec![type_bar_style(id)]);
        assert!(status::placeholder(&fixture.layer(id), &fixture.block(id)));
        assert!(!hand_kept(&fixture.layer(id), &fixture.block(id)));

        // Kept as art: hidden with the mark, the style staying.
        let ops = kept_original(&fixture.layer(id));
        assert_eq!(ops.len(), 2, "{ops:?}");
        fixture.apply(ops);
        let (layer, block) = (fixture.layer(id), fixture.block(id));
        assert!(!block.cleanup.visible && Auto::of(&layer).kept);
        let family = block
            .translation
            .as_ref()
            .and_then(|translation| translation.style.chars.family.as_deref());
        assert_eq!(family, Some(FAMILY));
        assert!(!hand_kept(&layer, &block) && !hand_shown(&layer, &block));
        assert!(kept_original(&layer).is_empty());
        // A first translation typed in shows the clean-up, its record taking
        // the mark off.
        let ops = shown_again(&layer);
        assert_eq!(json(&ops), json(std::slice::from_ref(&show)));

        // Shown again by hand, the mark left: the user's choice, with no
        // clean-up left to show.
        fixture.apply(vec![show]);
        let (layer, block) = (fixture.layer(id), fixture.block(id));
        assert!(hand_shown(&layer, &block));
        assert!(kept_original(&layer).is_empty());
        assert!(shown_again(&layer).is_empty());

        // Hidden by hand with no mark: kept by hand.
        let id = fixture.under;
        fixture.apply(vec![
            type_bar_style(id),
            Op::UpdateLayer {
                layer: id.into(),
                patch: json!({"cleanup": {"visible": false}}),
            },
        ]);
        let (layer, block) = (fixture.layer(id), fixture.block(id));
        assert!(hand_kept(&layer, &block));
        assert!(kept_original(&layer).is_empty());
        assert!(shown_again(&layer).is_empty());
    }

    #[test]
    fn an_effect_styled_before_its_translated_keeps_its_style_under_a_subtitle() {
        let mut fixture = Fixture::new("sfx");
        let id = fixture.held;
        fixture.apply(vec![type_bar_style(id)]);
        let english: LanguageIdentifier = "en".parse().expect("a tag");
        let ops = subtitle_ops(
            &fixture.page(),
            id,
            "Boom",
            &english,
            &SubtitleStyle::default(),
            None,
        )
        .expect("the ops");
        fixture.apply(ops);

        // Kept as art under its subtitle, the style set on it staying.
        let (layer, block) = (fixture.layer(id), fixture.block(id));
        assert!(subtitled(&fixture.page(), id));
        assert!(!block.cleanup.visible && Auto::of(&layer).kept);
        assert!(status::placeholder(&layer, &block));
        let family = block
            .translation
            .as_ref()
            .and_then(|translation| translation.style.chars.family.as_deref());
        assert_eq!(family, Some(FAMILY));
    }

    #[test]
    fn a_subtitle_takes_a_translation_typed_by_hand_away_with_its_marker() {
        let mut fixture = Fixture::new("sfx");
        let id = fixture.held;
        let marker = styles::hand_typed(&fixture.layer(id)).expect("the record");
        fixture.apply(vec![
            Op::SetText {
                layer: id.into(),
                text: "BOOM".to_owned(),
            },
            marker,
        ]);
        assert!(Auto::of(&fixture.layer(id)).typed);
        let english: LanguageIdentifier = "en".parse().expect("a tag");
        let ops = subtitle_ops(
            &fixture.page(),
            id,
            "Boom",
            &english,
            &SubtitleStyle::default(),
            None,
        )
        .expect("the ops");
        fixture.apply(ops);
        let (layer, block) = (fixture.layer(id), fixture.block(id));
        assert!(block.translation.is_none() && !block.cleanup.visible);
        let record = Auto::of(&layer);
        assert!(record.kept && !record.typed);
    }

    #[test]
    fn subtitles_stay_on_the_page_at_both_edges() {
        let page = (500.0, 500.0);
        for vertical in [false, true] {
            for source in [
                Rect::new(0.0, 0.0, 30.0, 80.0),
                Rect::new(490.0, 490.0, 500.0, 500.0),
            ] {
                let rect = subtitle_rect(source, vertical, 18.0, page);
                assert!(rect.x0 >= 0.0 && rect.y0 >= 0.0, "{rect:?}");
                assert!(rect.x1 <= 500.0 && rect.y1 <= 500.0, "{rect:?}");
            }
        }
        let source = Rect::new(300.0, 100.0, 320.0, 180.0);
        assert!(subtitle_rect(source, true, 18.0, page).x0 < source.x0);
        assert!(subtitle_rect(source, false, 18.0, page).y0 > source.y0);
    }

    #[test]
    fn a_subtitle_is_sized_and_spaced_by_its_size() {
        let page = (1200.0, 1600.0);
        let source = Rect::new(600.0, 100.0, 660.0, 300.0);
        // Left of a vertical source, a third of the size away, its top at the
        // source's.
        let high = 18.0 * 2.8;
        let left = subtitle_rect(source, true, 18.0, page);
        assert_eq!(left, Rect::new(378.0, 100.0, 594.0, 100.0 + high));
        // Below any other, its left at the source's.
        let below = subtitle_rect(source, false, 18.0, page);
        assert_eq!(below, Rect::new(600.0, 306.0, 816.0, 306.0 + high));
        // Never wider or higher than the page.
        let tiny = subtitle_rect(Rect::new(0.0, 0.0, 5.0, 5.0), false, 18.0, (100.0, 40.0));
        assert_eq!((tiny.width(), tiny.height()), (100.0, 40.0));
    }

    #[test]
    fn a_subtitle_that_would_leave_the_page_goes_to_the_other_side() {
        let page = (500.0, 500.0);
        // Too near the left edge for a vertical source: to its right.
        let source = Rect::new(20.0, 100.0, 60.0, 300.0);
        let right = subtitle_rect(source, true, 18.0, page);
        assert_eq!((right.x0, right.y0), (66.0, 100.0));
        // Too near the bottom for any other: above it.
        let source = Rect::new(100.0, 420.0, 300.0, 480.0);
        let above = subtitle_rect(source, false, 18.0, page);
        assert_eq!(above.x0, 100.0);
        assert!((above.y1 - 414.0).abs() < 1e-9, "{above:?}");
        // With no room either side, it's moved onto the page.
        let source = Rect::new(0.0, 0.0, 500.0, 500.0);
        for vertical in [false, true] {
            let rect = subtitle_rect(source, vertical, 18.0, page);
            assert!(rect.x0 >= 0.0 && rect.y0 >= 0.0 && rect.x1 <= 500.0 && rect.y1 <= 500.0);
        }
    }

    #[test]
    fn a_sources_direction_says_where_its_subtitle_goes() {
        let text = |direction: &str, content: &str| -> Text {
            serde_json::from_value(json!({"content": content, "direction": direction}))
                .expect("a text")
        };
        let tall = Rect::new(0.0, 0.0, 40.0, 120.0);
        let wide = Rect::new(0.0, 0.0, 120.0, 40.0);
        assert!(vertical(&text("vertical", "ドン"), wide));
        assert!(!vertical(&text("horizontal", "ドン"), tall));
        // With no wording read, its shape decides.
        assert!(vertical(&text("horizontal", ""), tall));
        assert!(!vertical(&text("horizontal", " "), wide));
    }

    #[test]
    fn a_subtitle_is_added_after_its_effect_which_is_kept_as_art() {
        let mut fixture = Fixture::new("sfx");
        let id = fixture.held;
        fixture.apply(vec![Op::SetText {
            layer: id.into(),
            text: "BOOM".to_owned(),
        }]);
        let style = SubtitleStyle {
            family: None,
            size: 0.02,
            fill: RED,
            stroke: YELLOW,
        };
        let english: LanguageIdentifier = "en".parse().expect("a tag");
        let ops = subtitle_ops(
            &fixture.page(),
            id,
            "Boom",
            &english,
            &style,
            Some("CCWildWords"),
        )
        .expect("the ops");
        let steps = fixture.session.history().steps.len();
        let change = fixture.apply(ops);
        assert_eq!(fixture.session.history().steps.len(), steps + 1, "one edit");
        let subtitle = change
            .created
            .iter()
            .find_map(|created| match created {
                Created::Layer {
                    id,
                    label: Some(label),
                } if label == SUBTITLE_REF => Some(*id),
                _ => None,
            })
            .expect("the subtitle");

        // Right after the block in its balloon.
        let balloon = fixture.layer(fixture.balloon);
        let order: Vec<LayerId> = balloon.children.iter().map(|layer| layer.id).collect();
        assert_eq!(order, [id, subtitle]);
        let layer = fixture.layer(subtitle);
        assert_eq!(layer.name, SUBTITLE_NAME);
        assert_eq!(
            attrs::get::<SfxSubtitle>(&layer),
            Some(SfxSubtitle { block: id })
        );
        let Content::Text(text) = &layer.content else {
            panic!("{:?} isn't text", layer.content);
        };
        assert_eq!(text.content.text(), "Boom");
        assert_eq!(text.language, english);
        assert_eq!(text.direction, Direction::Horizontal);
        assert_eq!(text.style.align, Some(koharu_scene::Align::Start));
        // 2% of the page's shorter side, 400, is 8 pixels.
        let chars = &text.style.chars;
        assert_eq!(chars.family.as_deref(), Some("CCWildWords"));
        assert_eq!(chars.size, Some(8.0));
        assert_eq!((chars.fill, chars.stroke), (Some(RED), Some(YELLOW)));
        assert_eq!(chars.stroke_width, Some(8.0 / 9.0));
        assert_eq!((chars.outer_stroke, chars.outer_stroke_width), (None, None));
        // Left of the vertical source, a third of the size away.
        let koharu_scene::Frame::Box { bounds, fit, .. } = text.frame else {
            panic!("{:?} isn't a box", text.frame);
        };
        assert_eq!(fit, koharu_scene::Fit::None);
        let expected = subtitle_rect(
            Rect::new(20.0, 20.0, 60.0, 140.0),
            true,
            8.0,
            (400.0, 600.0),
        );
        assert_eq!(kurbo::Rect::from(bounds), expected);

        // The effect keeps the art, marked, and counts as subtitled.
        let block = fixture.block(id);
        assert!(block.translation.is_none() && !block.cleanup.visible);
        let record = Auto::of(&fixture.layer(id));
        assert!(record.kept);
        assert_eq!((record.origin, record.text), (Origin::Automatic, None));
        assert_eq!(subtitle_of(&fixture.page(), id), Some(subtitle));

        // The subtitle's own family wins over the default font.
        let style = SubtitleStyle {
            family: Some("Anime Ace".to_owned()),
            ..style
        };
        let ops = subtitle_ops(
            &fixture.page(),
            fixture.outside,
            "Boom",
            &english,
            &style,
            None,
        )
        .expect("the ops");
        let added = json!(ops[0]);
        assert_eq!(
            added["layer"]["style"]["chars"]["family"],
            json!("Anime Ace")
        );
        assert_eq!(added["parent"], Value::Null);
        assert_eq!(added["index"], json!(4));
        // A layer that isn't a block gets none.
        assert!(
            subtitle_ops(
                &fixture.page(),
                fixture.balloon,
                "Boom",
                &english,
                &style,
                None
            )
            .is_err()
        );
    }
}
