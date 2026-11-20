//! Text effects on boxes: the skew the FX window sets on whole boxes, as
//! one undo step labelled [`LABEL`] ([`set_skew`]), and the effects each
//! text on a page carries beyond its lettering default, which the fx tag
//! shows ([`page_effects`]).
//!
//! A skew is composed into a text's transform about its pivot and recorded
//! beside it (`booharu_core::skew`), so it shows again as it was set and
//! comes off exactly, after a move, a turn or a resize too. Reset to
//! lettering default takes it off with the rest of the look
//! (`styles::reset_lettering`, through [`unskewed`]).

use std::collections::BTreeSet;

use anyhow::{Result, ensure};
use booharu_core::{
    attrs::{self, Attr, Skew},
    effects::{TextEffects, text_effects},
    resolver,
    settings::LetteringSettings,
    skew,
};
use koharu_scene::{Content, Edit, Layer, LayerId, Op, Page, Project, Text, Transform};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::styles::locked;

/// The undo step's label, which the History panel translates.
pub const LABEL: &str = "booharu_set_skew";

/// How close an angle asked for may be to the one a text shows and still
/// change nothing, in degrees.
const SAME: f64 = 1e-9;

/// What [`set_skew`] does: its edit, none when nothing changes, and how
/// many layers it skews, leaves alone as locked, and finds untranslated.
#[derive(Clone, Debug, Default)]
pub struct SkewChange {
    /// The edit, one undo step labelled [`LABEL`].
    pub edit: Option<Edit>,
    /// Text layers and translated blocks it skews.
    pub changed: u32,
    /// Text layers and blocks it leaves as they are because they, or a
    /// layer holding them, are locked.
    pub locked: u32,
    /// Blocks without a translation, which it leaves alone, as skewing one
    /// would shear its source.
    pub untranslated: u32,
}

impl SkewChange {
    /// Its numbers, which the desktop's command answers.
    pub fn counts(&self) -> SkewCounts {
        SkewCounts {
            changed: self.changed,
            locked: self.locked,
            untranslated: self.untranslated,
        }
    }
}

/// How many layers a skew changed, left alone as locked, and found
/// untranslated. A layer showing the skew already counts in none.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct SkewCounts {
    /// Text layers and translated blocks skewed.
    pub changed: u32,
    /// Layers left as they are because they, or a layer holding them, are
    /// locked.
    pub locked: u32,
    /// Blocks without a translation, which have no lettering to skew.
    pub untranslated: u32,
}

/// The edit skewing each of `layers` in `project`, text layers and
/// translated blocks, `x` degrees along its width and `y` along its height
/// in place of the skew it shows, as one undo step labelled [`LABEL`], with
/// its numbers; none when every layer shows that skew already.
///
/// An axis left out keeps each layer's own: the skew it shows along that
/// axis, brought into −60 to 60 when it was read from a shear made
/// elsewhere, which happens only when the other axis changes. Each layer
/// changed gets an absolute transform, its own with the skew it shows taken
/// off about its record's pivot and the new one composed in about its pivot
/// now (`skew::reskewed`), so its position, turn and size stay, and the
/// record of the new skew, removed at 0 and 0. A locked layer, or one
/// inside a locked layer, is counted and left, as is a block without a
/// translation; any other kind of layer is left uncounted. A layer named
/// twice counts once, and a layer not in `project` fails. An angle outside
/// −60 to 60, or not a number, refuses the whole call.
pub fn set_skew(
    project: &Project,
    layers: &[LayerId],
    x: Option<f64>,
    y: Option<f64>,
) -> Result<SkewChange> {
    for angle in [x, y].into_iter().flatten() {
        ensure!(
            skew::in_range(angle),
            "a skew of {angle}° isn't from -{limit}° to {limit}°",
            limit = skew::LIMIT
        );
    }
    let mut change = SkewChange::default();
    let mut ops = Vec::new();
    let mut seen = BTreeSet::new();
    for &id in layers {
        if !seen.insert(id) {
            continue;
        }
        let layer = project.layer(id)?;
        let (text, block) = match &layer.content {
            Content::Block(block) => (block.translation.as_ref(), true),
            Content::Text(text) => (Some(&**text), false),
            _ => continue,
        };
        if locked(project, layer)? {
            change.locked += 1;
            continue;
        }
        let Some(text) = text else {
            change.untranslated += 1;
            continue;
        };
        let skewed = skewed(layer, text, block, x, y)?;
        change.changed += u32::from(!skewed.is_empty());
        ops.extend(skewed);
    }
    change.edit = (!ops.is_empty()).then(|| Edit {
        label: LABEL.to_owned(),
        ops,
    });
    Ok(change)
}

/// The effects of each text layer and translated block on `page` that
/// carries any, under `settings` and `font`, the default font, as the fx tag
/// shows them (`booharu_core::effects`): one entry for each, with the skew
/// it shows, depth first, which is reading order. The page's lettering size
/// is worked out once.
pub fn page_effects(
    page: &Page,
    settings: &LetteringSettings,
    font: Option<&str>,
) -> Result<Vec<TextEffects>> {
    fn walk(
        page: &Page,
        layers: &[Layer],
        settings: &LetteringSettings,
        font: Option<&str>,
        median: Option<f64>,
        found: &mut Vec<TextEffects>,
    ) -> Result<()> {
        for layer in layers {
            found.extend(text_effects(page, layer, settings, font, median)?);
            walk(page, &layer.children, settings, font, median, found)?;
        }
        Ok(())
    }
    let median = resolver::lettering_size(&page.layers);
    let mut found = Vec::new();
    walk(page, &page.layers, settings, font, median, &mut found)?;
    Ok(found)
}

/// The ops Reset to lettering default takes the skew of `layer` off with,
/// its text `text`, a block's translation when `block`: its transform with
/// the skew it shows taken off, about its trusted record's pivot, and its
/// record removed; none when it shows no skew and holds no record.
pub(crate) fn unskewed(layer: &Layer, text: &Text, block: bool) -> Vec<Op> {
    let record = attrs::get::<Skew>(layer);
    let mut ops = Vec::new();
    if skew::shown(text, record.as_ref()) != (0.0, 0.0) {
        let pivot = skew::pivot(&text.frame);
        let transform = skew::reskewed(text.transform, record.as_ref(), (0.0, 0.0), pivot);
        ops.push(transformed(layer.id, block, transform));
    }
    ops.extend(forgotten(layer));
    ops
}

/// The ops giving `layer`, its text `text`, a block's translation when
/// `block`, the skew `x` and `y`, an axis left out keeping its own, with its
/// record; none when it shows the angles asked for already.
fn skewed(
    layer: &Layer,
    text: &Text,
    block: bool,
    x: Option<f64>,
    y: Option<f64>,
) -> Result<Vec<Op>> {
    let record = attrs::get::<Skew>(layer);
    let shown = skew::shown(text, record.as_ref());
    let asked =
        |angle: Option<f64>, shown: f64| angle.is_some_and(|angle| (angle - shown).abs() > SAME);
    if !asked(x, shown.0) && !asked(y, shown.1) {
        return Ok(Vec::new());
    }
    let new = (
        x.unwrap_or_else(|| skew::clamped(shown.0)),
        y.unwrap_or_else(|| skew::clamped(shown.1)),
    );
    let pivot = skew::pivot(&text.frame);
    let transform = skew::reskewed(text.transform, record.as_ref(), new, pivot);
    let mut ops = vec![transformed(layer.id, block, transform)];
    if new == (0.0, 0.0) {
        ops.extend(forgotten(layer));
    } else {
        let record = Skew {
            x: new.0,
            y: new.1,
            pivot: [pivot.x, pivot.y],
        };
        ops.push(attrs::set(layer.id, &record)?);
    }
    Ok(ops)
}

/// The op drawing the text of `layer`, a block's translation when `block`,
/// with `transform`.
fn transformed(layer: LayerId, block: bool, transform: Transform) -> Op {
    let patch = match block {
        true => json!({"translation": {"transform": transform}}),
        false => json!({"transform": transform}),
    };
    Op::UpdateLayer {
        layer: layer.into(),
        patch,
    }
}

/// The op removing `layer`'s skew record when it holds one, one that
/// doesn't read included.
fn forgotten(layer: &Layer) -> Option<Op> {
    layer
        .attributes
        .get(attrs::NAMESPACE)
        .is_some_and(|keys| keys.contains_key(Skew::KEY))
        .then(|| attrs::clear::<Skew>(layer.id))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use anyhow::{Context as _, bail};
    use booharu_core::{
        effects::{SkewAngles, TextEffect},
        emphasis::Marking,
    };
    use koharu_scene::{
        CharStyle, Created, Gradient, GradientKind, GradientStop, LanguageIdentifier, PageId, Rgba,
        Session, Span,
    };
    use kurbo::{Affine, Point};
    use tempfile::TempDir;

    use super::*;

    /// The default font.
    const FONT: &str = "CCWildWords";

    const YELLOW: Rgba = Rgba([1.0, 1.0, 0.0, 1.0]);

    /// A page 400 by 400 holding the blocks `a` and `b`, translated as
    /// Koharu's Translation stage translates with Booharu's hook, the
    /// untranslated block `bare`, the locked group `g` holding the
    /// translated block `held`, the text layer `t`, and the raster `paint`.
    struct Fixture {
        _dir: TempDir,
        session: Session,
        layers: BTreeMap<String, LayerId>,
        page: PageId,
    }

    impl Fixture {
        fn new() -> Result<Self> {
            let dir = tempfile::tempdir()?;
            let mut session = Session::create(dir.path().join("test.khr"))?;
            let block = |x: f64| {
                json!({"kind": "block", "source": {
                    "frame": {"box": {"bounds": {"x": x, "y": 20.0, "width": 40.0, "height": 120.0}}},
                    "content": "どうして",
                    "style": {"chars": {"size": 32.0}},
                }, "attributes": {"c2pa": {
                    "detection": {"action": "c2pa.created"},
                    "ocr": {"action": "c2pa.created"},
                }}})
            };
            let ops = [
                json!({"op": "add_page", "id": "$p", "index": 0, "size": {"width": 400, "height": 400}}),
                json!({"op": "add_layer", "id": "$a", "page": "$p", "index": 0, "layer": block(10.0)}),
                json!({"op": "add_layer", "id": "$b", "page": "$p", "index": 1, "layer": block(60.0)}),
                json!({"op": "add_layer", "id": "$bare", "page": "$p", "index": 2, "layer": block(110.0)}),
                json!({"op": "add_layer", "id": "$g", "page": "$p", "index": 3, "layer": {"kind": "group"}}),
                json!({"op": "add_layer", "id": "$held", "page": "$p", "parent": "$g", "index": 0,
                    "layer": block(160.0)}),
                json!({"op": "add_layer", "id": "$t", "page": "$p", "index": 4, "layer": {
                    "kind": "text",
                    "content": "Hi!",
                    "style": {"chars": {"family": FONT, "size": 20.0}},
                    "frame": {"box": {"bounds": {"x": 200.0, "y": 200.0, "width": 80.0, "height": 40.0}}},
                }}),
                json!({"op": "add_layer", "id": "$paint", "page": "$p", "index": 5,
                    "layer": {"kind": "raster"}}),
            ];
            let change = session.apply(Edit {
                label: "test".to_owned(),
                ops: ops
                    .into_iter()
                    .map(serde_json::from_value)
                    .collect::<Result<_, _>>()?,
            })?;
            let layers = change
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
            let mut fixture = Self {
                _dir: dir,
                session,
                layers,
                page: change.pages.added[0],
            };
            fixture.translate(&["a", "b", "held"])?;
            let g = fixture.id("g");
            fixture.edit(vec![Op::UpdateLayer {
                layer: g.into(),
                patch: json!({"locked": true}),
            }])?;
            Ok(fixture)
        }

        fn id(&self, name: &str) -> LayerId {
            self.layers[name]
        }

        fn layer(&self, name: &str) -> Result<Layer> {
            Ok(self.session.project().layer(self.id(name))?.clone())
        }

        /// The text of `name`: a block's translation or a text layer's.
        fn text(&self, name: &str) -> Result<Text> {
            match self.layer(name)?.content {
                Content::Block(block) => block.translation.context("a translation"),
                Content::Text(text) => Ok(*text),
                _ => bail!("{name} has no text"),
            }
        }

        fn record(&self, name: &str) -> Result<Option<Skew>> {
            Ok(attrs::get(&self.layer(name)?))
        }

        fn edit(&mut self, ops: Vec<Op>) -> Result<()> {
            self.session.apply(Edit {
                label: "test".to_owned(),
                ops,
            })?;
            Ok(())
        }

        /// Translates `names` as Koharu's Translation stage does, with
        /// Booharu's hook giving them the default look.
        fn translate(&mut self, names: &[&str]) -> Result<()> {
            let project = self.session.project();
            let english: LanguageIdentifier = "en".parse()?;
            let mut ops = Vec::new();
            for name in names {
                let block = self.id(name);
                let placed = koharu_pipeline::placement(&project, block, &english)?;
                ops.push(Op::SetText {
                    layer: block.into(),
                    text: "Why?".to_owned(),
                });
                ops.push(placed.clone());
                ops.extend(booharu_core::styles::translated(
                    &project,
                    block,
                    "Why?",
                    &placed,
                    &LetteringSettings::default(),
                    Some(FONT),
                    &Marking::default(),
                )?);
                ops.push(Op::SetAttribute {
                    layer: block.into(),
                    namespace: "c2pa".to_owned(),
                    key: "translation".to_owned(),
                    value: Some(json!({"action": "c2pa.translated"})),
                });
            }
            self.edit(ops)
        }

        /// Skews `names` as [`set_skew`] does, applying its edit, and
        /// answers its numbers.
        fn skew(&mut self, names: &[&str], x: Option<f64>, y: Option<f64>) -> Result<SkewCounts> {
            let layers: Vec<LayerId> = names.iter().map(|name| self.id(name)).collect();
            let change = set_skew(&self.session.project(), &layers, x, y)?;
            assert_eq!(change.edit.is_some(), change.changed > 0);
            if let Some(edit) = change.edit.clone() {
                assert_eq!(edit.label, LABEL);
                self.session.apply(edit)?;
            }
            Ok(change.counts())
        }

        /// The page's text effects under the default look.
        fn effects(&self) -> Result<Vec<TextEffects>> {
            let project = self.session.project();
            page_effects(
                project.page(self.page)?,
                &LetteringSettings::default(),
                Some(FONT),
            )
        }
    }

    /// The skew of `x` and `y` degrees about `pivot`.
    fn about(pivot: Point, x: f64, y: f64) -> Affine {
        let to = Affine::translate(pivot.to_vec2());
        to * skew::matrix(x, y) * to.inverse()
    }

    fn close(a: Transform, b: Affine) -> bool {
        Affine::from(a)
            .as_coeffs()
            .iter()
            .zip(b.as_coeffs())
            .all(|(a, b)| (a - b).abs() <= 1e-9)
    }

    fn gradient() -> Gradient {
        Gradient {
            kind: GradientKind::Linear,
            angle: 90.0,
            stops: vec![
                GradientStop {
                    offset: 0.0,
                    color: YELLOW,
                },
                GradientStop {
                    offset: 1.0,
                    color: Rgba([1.0, 0.0, 0.0, 1.0]),
                },
            ],
        }
    }

    #[test]
    fn a_skew_over_two_boxes_is_one_undo_step() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let before = fixture.session.project();
        let steps = fixture.session.history().steps.len();
        let first = [fixture.text("a")?, fixture.text("t")?];

        // A block named twice counts once.
        let counts = fixture.skew(&["a", "t", "a"], Some(20.0), Some(-5.0))?;
        assert_eq!(
            counts,
            SkewCounts {
                changed: 2,
                ..SkewCounts::default()
            }
        );
        assert_eq!(fixture.session.history().steps.len(), steps + 1);
        assert_eq!(
            fixture.session.history().steps.last().map(String::as_str),
            Some(LABEL)
        );
        for (name, first) in ["a", "t"].into_iter().zip(&first) {
            // Composed in about the box's centre, which stays put.
            let pivot = skew::pivot(&first.frame);
            let text = fixture.text(name)?;
            let skewed = Affine::from(first.transform) * about(pivot, 20.0, -5.0);
            assert!(close(text.transform, skewed), "{name}");
            let record = fixture.record(name)?;
            assert_eq!(
                record,
                Some(Skew {
                    x: 20.0,
                    y: -5.0,
                    pivot: [pivot.x, pivot.y],
                }),
                "{name}"
            );
            assert_eq!(skew::shown(&text, record.as_ref()), (20.0, -5.0), "{name}");
        }

        fixture.session.undo()?;
        assert!(fixture.session.project() == before);
        Ok(())
    }

    #[test]
    fn locked_and_untranslated_boxes_are_counted_and_left() -> Result<()> {
        let mut fixture = Fixture::new()?;
        // b is locked; held is inside the locked group g.
        let b = fixture.id("b");
        fixture.edit(vec![Op::UpdateLayer {
            layer: b.into(),
            patch: json!({"locked": true}),
        }])?;
        let names = ["b", "held", "bare", "g", "paint"];
        let layers: Vec<LayerId> = names.iter().map(|name| fixture.id(name)).collect();
        let change = set_skew(&fixture.session.project(), &layers, Some(10.0), None)?;
        assert!(change.edit.is_none());
        // The group and the raster have no text, and count in none.
        assert_eq!(
            change.counts(),
            SkewCounts {
                changed: 0,
                locked: 2,
                untranslated: 1,
            }
        );
        Ok(())
    }

    #[test]
    fn an_axis_left_out_keeps_each_boxs_own() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let first = fixture.text("a")?;
        fixture.skew(&["a"], Some(20.0), Some(5.0))?;
        fixture.skew(&["a"], None, Some(-10.0))?;
        let text = fixture.text("a")?;
        assert_eq!(
            skew::shown(&text, fixture.record("a")?.as_ref()),
            (20.0, -10.0)
        );
        let pivot = skew::pivot(&first.frame);
        let skewed = Affine::from(first.transform) * about(pivot, 20.0, -10.0);
        assert!(close(text.transform, skewed));

        // A shear made elsewhere, as Koharu's edit tool makes one, keeps its
        // lean along the axis left out.
        let b = fixture.text("b")?;
        let sheared = Affine::from(b.transform) * about(skew::pivot(&b.frame), 15.0, 0.0);
        let id = fixture.id("b");
        fixture.edit(vec![Op::UpdateLayer {
            layer: id.into(),
            patch: json!({"translation": {"transform": Transform::from(sheared)}}),
        }])?;
        assert_eq!(fixture.record("b")?, None);
        fixture.skew(&["b"], None, Some(5.0))?;
        let (x, y) = skew::shown(&fixture.text("b")?, fixture.record("b")?.as_ref());
        assert!(
            (x - 15.0).abs() <= 1e-9 && (y - 5.0).abs() <= 1e-9,
            "{x}, {y}"
        );
        Ok(())
    }

    #[test]
    fn zero_and_zero_takes_the_skew_and_its_record_off() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let first = fixture.text("t")?;
        fixture.skew(&["t"], Some(30.0), Some(10.0))?;
        assert!(fixture.record("t")?.is_some());
        let counts = fixture.skew(&["t"], Some(0.0), Some(0.0))?;
        assert_eq!(counts.changed, 1);
        assert!(close(
            fixture.text("t")?.transform,
            Affine::from(first.transform)
        ));
        let layer = fixture.layer("t")?;
        assert!(
            !layer
                .attributes
                .get(attrs::NAMESPACE)
                .is_some_and(|keys| keys.contains_key(Skew::KEY))
        );
        Ok(())
    }

    #[test]
    fn an_angle_out_of_range_refuses_the_call() -> Result<()> {
        let fixture = Fixture::new()?;
        let project = fixture.session.project();
        let a = [fixture.id("a")];
        for angle in [60.5, -61.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(
                set_skew(&project, &a, Some(angle), None).is_err(),
                "{angle}"
            );
            assert!(
                set_skew(&project, &a, Some(0.0), Some(angle)).is_err(),
                "{angle}"
            );
        }
        // The steepest either way is taken.
        assert_eq!(set_skew(&project, &a, Some(60.0), Some(-60.0))?.changed, 1);
        Ok(())
    }

    #[test]
    fn nothing_to_change_gives_no_edit() -> Result<()> {
        let mut fixture = Fixture::new()?;
        fixture.skew(&["a"], Some(20.0), Some(0.0))?;
        let project = fixture.session.project();
        let (a, b) = (fixture.id("a"), fixture.id("b"));
        for (x, y) in [
            (Some(20.0), Some(0.0)),
            (Some(20.0), None),
            (None, Some(0.0)),
            (None, None),
        ] {
            let change = set_skew(&project, &[a], x, y)?;
            assert!(change.edit.is_none(), "{x:?}, {y:?}");
            assert_eq!(change.counts(), SkewCounts::default(), "{x:?}, {y:?}");
        }
        // Nor does a box without a skew asked for none.
        assert!(
            set_skew(&project, &[b], Some(0.0), Some(0.0))?
                .edit
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn a_pages_effects_list_each_text_carrying_any_until_reset() -> Result<()> {
        let mut fixture = Fixture::new()?;
        // As translated, nothing carries an effect.
        assert_eq!(fixture.effects()?, []);
        // By hand: a's border, b's skew, and a gradient on one of t's words,
        // beside letter spacing, which is no effect.
        let (a, b, t) = (fixture.id("a"), fixture.id("b"), fixture.id("t"));
        fixture.edit(vec![
            Op::UpdateLayer {
                layer: a.into(),
                patch: json!({"translation": {"style": {"chars": {
                    "stroke": YELLOW, "stroke_width": 4.0,
                }}}}),
            },
            Op::UpdateLayer {
                layer: t.into(),
                patch: json!({"style": {"chars": {"tracking": 200.0}}}),
            },
            Op::SetRichText {
                layer: t.into(),
                spans: vec![
                    Span {
                        insert: "Hi".to_owned(),
                        attributes: CharStyle {
                            fill_gradient: Some(gradient()),
                            ..CharStyle::default()
                        },
                    },
                    Span {
                        insert: "!".to_owned(),
                        attributes: CharStyle::default(),
                    },
                ],
            },
        ])?;
        fixture.skew(&["b"], Some(12.0), Some(-3.0))?;
        assert_eq!(
            fixture.effects()?,
            [
                TextEffects {
                    layer: a,
                    effects: vec![TextEffect::Border],
                    skew: None,
                },
                TextEffects {
                    layer: b,
                    effects: vec![TextEffect::Skew],
                    skew: Some(SkewAngles { x: 12.0, y: -3.0 }),
                },
                TextEffects {
                    layer: t,
                    effects: vec![TextEffect::Gradient],
                    skew: None,
                },
            ]
        );

        // Reset to lettering default clears every tag.
        let (edit, counts) = crate::styles::reset_lettering(
            &fixture.session.project(),
            &[a, b, t],
            &LetteringSettings::default(),
            Some(FONT),
        )?;
        assert_eq!(counts.changed, 3);
        fixture.session.apply(edit.context("a reset")?)?;
        assert_eq!(fixture.effects()?, []);
        Ok(())
    }
}
