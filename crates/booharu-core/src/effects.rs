//! The text effects a box carries beyond its lettering default (S12-j):
//! what the fx tag on a box and the FX window show, worked out from the
//! scene alone, with no model call.
//!
//! - An effect is a border, a second border, the outline method, a gradient
//!   fill or a skew that a text has and that neither its lettering default
//!   at its current size (`resolver`) nor what Booharu's automatic steps
//!   left there (its hand-edit record's style) gives it. So a translation
//!   resized on the type bar, its outline as placed, carries none, and
//!   Reset to lettering default takes every one off.
//! - A font, weight, italic, size, flat colour, alignment, line or letter
//!   spacing, direction, frame, position or rotation is never an effect.
//! - Word marks count, after the marks a run's emphasis gave the words come
//!   off (`emphasis::stripped`), so stressed words carry none.
//!
//! The `booharu` crate's `effects::page_effects` lists a page's.

use anyhow::Result;
use koharu_scene::{CharStyle, Content, Layer, LayerId, Page, Rgba, StrokeMethod, Text};
use serde::{Deserialize, Serialize};

use crate::{
    attrs::{self, Auto, AutoStyle, Skew},
    emphasis,
    resolver::{self, Look},
    settings::LetteringSettings,
    skew,
};

/// A text effect, as the fx tag names it.
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "snake_case")]
pub enum TextEffect {
    /// A border unlike the default's, or none where the default has one.
    Border,
    /// A second border the default doesn't give.
    SecondBorder,
    /// Borders drawn letter by letter (`StrokeMethod::Expansion`).
    OutlineMethod,
    /// A gradient fill, on the box or on any of its words.
    Gradient,
    /// A skew of at least `skew::LEAST` degrees along either axis.
    Skew,
}

/// The skew a text shows, in degrees along each axis (`skew::shown`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct SkewAngles {
    pub x: f64,
    pub y: f64,
}

/// A text layer's or a translated block's effects: `effects` in the
/// order [`TextEffect`] lists them, and the skew it shows, when it shows
/// one, for the FX window's sliders.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct TextEffects {
    pub layer: LayerId,
    pub effects: Vec<TextEffect>,
    pub skew: Option<SkewAngles>,
}

/// How far apart two colours' channels may be and still be one colour:
/// half a step of 255.
const COLOUR: f32 = 1.0 / 510.0;

/// How far apart two border widths may be and still be one, in pixels.
const WIDTH: f64 = 0.01;

/// The effects of `layer`, a text layer or a translated block on `page`,
/// under `settings` and `font`, the default font, with `median` the page's
/// lettering size (`resolver::lettering_size`); none for any other layer,
/// and for one with no effect.
pub fn text_effects(
    page: &Page,
    layer: &Layer,
    settings: &LetteringSettings,
    font: Option<&str>,
    median: Option<f64>,
) -> Result<Option<TextEffects>> {
    let (text, record): (&Text, Auto) = match &layer.content {
        Content::Block(block) => match &block.translation {
            Some(translation) => (translation, Auto::of(layer)),
            None => return Ok(None),
        },
        Content::Text(text) => (&**text, Auto::default()),
        _ => return Ok(None),
    };
    let look = resolver::lettering_default_on(page, layer, settings, font, median)?;
    let shown = skew::shown(text, attrs::get::<Skew>(layer).as_ref());
    let skewed = skew::counts(shown);
    let mut effects = found(text, look.as_ref(), &record);
    if skewed {
        effects.push(TextEffect::Skew);
    }
    if effects.is_empty() {
        return Ok(None);
    }
    Ok(Some(TextEffects {
        layer: layer.id,
        effects,
        skew: skewed.then_some(SkewAngles {
            x: shown.0,
            y: shown.1,
        }),
    }))
}

/// The effects but the skew `text` carries over `look`, its lettering
/// default, and `record`, its hand-edit record, the box's own values and
/// each word's, the run's emphasis taken off.
fn found(text: &Text, look: Option<&Look>, record: &Auto) -> Vec<TextEffect> {
    let own = &text.style.chars;
    let words = emphasis::stripped(&text.content, record);
    let mut each = vec![own.clone()];
    each.extend(words.0.iter().map(|span| span.attributes.or(own)));
    let default = look.map(|look| &look.chars);
    let recorded = record.style.as_ref();
    let mut effects = Vec::new();
    let first = |chars: &CharStyle| border(chars.stroke, chars.stroke_width);
    let first_kept = |style: &AutoStyle| border(style.stroke, style.stroke_width);
    if each.iter().any(|chars| {
        differs(
            first(chars),
            default.and_then(first),
            recorded.map(first_kept),
        )
    }) {
        effects.push(TextEffect::Border);
    }
    let second = |chars: &CharStyle| border(chars.outer_stroke, chars.outer_stroke_width);
    let second_kept = |style: &AutoStyle| border(style.outer_stroke, style.outer_stroke_width);
    if each.iter().any(|chars| {
        second(chars).is_some()
            && differs(
                second(chars),
                default.and_then(second),
                recorded.map(second_kept),
            )
    }) {
        effects.push(TextEffect::SecondBorder);
    }
    let method = |method: Option<StrokeMethod>| method.unwrap_or_default();
    if method(text.style.stroke_method) == StrokeMethod::Expansion
        && method(look.and_then(|look| look.stroke_method)) != StrokeMethod::Expansion
    {
        effects.push(TextEffect::OutlineMethod);
    }
    if own.fill_gradient.is_some()
        || words
            .0
            .iter()
            .any(|span| span.attributes.fill_gradient.is_some())
    {
        effects.push(TextEffect::Gradient);
    }
    effects
}

/// A border of `colour` and `width`, as the renderer draws one: none
/// without a colour, without a width above 0, or in a colour that can't be
/// seen.
fn border(colour: Option<Rgba>, width: Option<f64>) -> Option<(Rgba, f64)> {
    let (colour, width) = (colour?, width?);
    (width > 0.0 && colour.0[3] > COLOUR).then_some((colour, width))
}

/// Whether `own` differs from both `default`, the lettering default's
/// border, and `recorded`, the one the hand-edit record holds, when it
/// holds a style.
fn differs(
    own: Option<(Rgba, f64)>,
    default: Option<(Rgba, f64)>,
    recorded: Option<Option<(Rgba, f64)>>,
) -> bool {
    !same(own, default) && recorded.is_none_or(|recorded| !same(own, recorded))
}

/// Whether two borders are one: both none, or colours within half a step
/// of 255 on every channel and widths within a hundredth of a pixel.
fn same(a: Option<(Rgba, f64)>, b: Option<(Rgba, f64)>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some((a, width)), Some((b, other))) => {
            a.0.iter().zip(b.0).all(|(a, b)| (a - b).abs() <= COLOUR)
                && (width - other).abs() <= WIDTH
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use koharu_scene::{Created, Edit, Gradient, GradientKind, GradientStop, Op, Session};
    use serde_json::{Value, json};

    use super::*;
    use crate::{
        emphasis::{EmphasisSettings, EmphasisStyle},
        settings::DefaultTextStyle,
        styles,
    };

    const BLUE: Rgba = Rgba([0.0, 0.0, 1.0, 1.0]);
    const WHITE: Rgba = Rgba([1.0; 4]);
    const RED: Rgba = Rgba([1.0, 0.0, 0.0, 1.0]);
    const YELLOW: Rgba = Rgba([1.0, 1.0, 0.0, 1.0]);

    /// A page holding a block read as blue lettering outlined 3 pixels wide
    /// in white at size 32, translated as the Translation stage translates
    /// one with colour detection on, and a text layer the T tool made.
    struct Fixture {
        _dir: tempfile::TempDir,
        session: Session,
        block: LayerId,
        typed: LayerId,
        settings: LetteringSettings,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("a folder");
            let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
            let settings = LetteringSettings {
                default: DefaultTextStyle {
                    fill: None,
                    stroke: None,
                    stroke_width: 2.0,
                },
                detect_colors: true,
                ..LetteringSettings::default()
            };
            let typed = json!({"kind": "text", "content": "Hi",
                "style": {"chars": styles::default_chars(&settings, Some("CCWildWords"))}});
            let ops = [
                json!({"op": "add_page", "id": "$p", "index": 0, "size": {"width": 200, "height": 200}}),
                json!({"op": "add_layer", "id": "$b", "page": "$p", "index": 0, "layer": {
                "kind": "block", "source": {
                    "frame": {"box": {"bounds": {"x": 10.0, "y": 10.0, "width": 40.0, "height": 120.0}}},
                    "content": "どうして",
                    "style": {"chars": {"size": 32.0, "fill": BLUE, "stroke": WHITE, "stroke_width": 3.0}},
                }}}),
                json!({"op": "add_layer", "id": "$t", "page": "$p", "index": 1, "layer": typed}),
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
            let id = |name: &str| {
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
            let mut fixture = Self {
                _dir: dir,
                block: id("b"),
                typed: id("t"),
                session,
                settings,
            };
            fixture.translate(&emphasis::Marking::default());
            fixture
        }

        /// Translates the block "Why do you care?", as the Translation
        /// stage does with its Booharu hook, its model picking `marking`.
        fn translate(&mut self, marking: &emphasis::Marking<'_>) {
            let project = self.session.project();
            let placed = Op::UpdateLayer {
                layer: self.block.into(),
                patch: json!({"translation": {"style": {"chars": {
                    "size": 32.0, "fill": BLUE, "stroke": WHITE, "stroke_width": 3.0}}}}),
            };
            let text = "Why do you care?";
            let hook = styles::translated(
                &project,
                self.block,
                text,
                &placed,
                &self.settings,
                Some("CCWildWords"),
                marking,
            )
            .expect("the hook's ops");
            let mut ops = vec![
                Op::SetText {
                    layer: self.block.into(),
                    text: text.to_owned(),
                },
                placed,
            ];
            ops.extend(hook);
            ops.push(Op::SetAttribute {
                layer: self.block.into(),
                namespace: "c2pa".to_owned(),
                key: "translation".to_owned(),
                value: Some(json!({"action": "c2pa.translated"})),
            });
            self.apply(ops);
        }

        fn apply(&mut self, ops: Vec<Op>) {
            self.session
                .apply(Edit {
                    label: "test".to_owned(),
                    ops,
                })
                .expect("the edit");
        }

        fn patch(&mut self, id: LayerId, patch: Value) {
            self.apply(vec![Op::UpdateLayer {
                layer: id.into(),
                patch,
            }]);
        }

        fn effects(&self, id: LayerId) -> Option<TextEffects> {
            let project = self.session.project();
            let page = project.pages().next().expect("a page");
            let layer = project.layer(id).expect("the layer");
            text_effects(
                page,
                layer,
                &self.settings,
                Some("CCWildWords"),
                resolver::lettering_size(&page.layers),
            )
            .expect("the effects")
        }

        /// The block's translation.
        fn translation(&self) -> Text {
            match &self
                .session
                .project()
                .layer(self.block)
                .expect("the block")
                .content
            {
                Content::Block(block) => block.translation.clone().expect("a translation"),
                _ => panic!("not a block"),
            }
        }

        fn tags(&self, id: LayerId) -> Vec<TextEffect> {
            self.effects(id)
                .map(|found| found.effects)
                .unwrap_or_default()
        }
    }

    fn gradient() -> Gradient {
        Gradient {
            kind: GradientKind::Linear,
            angle: 0.0,
            stops: vec![
                GradientStop {
                    offset: 0.0,
                    color: RED,
                },
                GradientStop {
                    offset: 1.0,
                    color: YELLOW,
                },
            ],
        }
    }

    #[test]
    fn a_fresh_translation_and_text_carry_none() {
        let fixture = Fixture::new();
        assert_eq!(fixture.effects(fixture.block), None);
        assert_eq!(fixture.effects(fixture.typed), None);
    }

    #[test]
    fn a_size_letter_spacing_or_colour_change_is_no_effect() {
        let mut fixture = Fixture::new();
        // The type bar's size: the default's outline grows with the size,
        // the translation's stays as placed, as the record holds it.
        fixture.patch(
            fixture.block,
            json!({"translation": {"style": {"chars": {"size": 24.0}}}}),
        );
        assert_eq!(fixture.tags(fixture.block), []);
        // Letter spacing, a flat colour, a font, line height and alignment.
        fixture.patch(
            fixture.block,
            json!({"translation": {"style": {"chars": {"tracking": 200.0, "fill": RED,
                "family": "Arial", "weight": 700, "italic": true},
                "line_height": 1.5, "align": "start"}}}),
        );
        assert_eq!(fixture.tags(fixture.block), []);
    }

    #[test]
    fn a_hand_border_second_border_and_method_each_give_theirs() {
        let mut fixture = Fixture::new();
        fixture.patch(
            fixture.block,
            json!({"translation": {"style": {"chars": {"stroke": RED, "stroke_width": 5.0}}}}),
        );
        assert_eq!(fixture.tags(fixture.block), [TextEffect::Border]);
        // Within half a step of 255 and a hundredth of a pixel is the same.
        fixture.patch(
            fixture.block,
            json!({"translation": {"style": {"chars": {"stroke": Rgba([1.0, 1.0, 0.999, 1.0]),
                "stroke_width": 3.005}}}}),
        );
        assert_eq!(fixture.tags(fixture.block), []);
        // A second border, on the text layer.
        fixture.patch(
            fixture.typed,
            json!({"style": {"chars": {"outer_stroke": YELLOW, "outer_stroke_width": 2.0}}}),
        );
        assert_eq!(fixture.tags(fixture.typed), [TextEffect::SecondBorder]);
        // Borders letter by letter.
        fixture.patch(
            fixture.block,
            json!({"translation": {"style": {"stroke_method": "expansion"}}}),
        );
        assert_eq!(fixture.tags(fixture.block), [TextEffect::OutlineMethod]);
        fixture.patch(
            fixture.block,
            json!({"translation": {"style": {"stroke_method": "contour"}}}),
        );
        assert_eq!(fixture.tags(fixture.block), []);
    }

    #[test]
    fn a_gradient_on_the_box_or_a_word_and_a_skew_give_theirs() {
        let mut fixture = Fixture::new();
        fixture.patch(
            fixture.block,
            json!({"translation": {"style": {"chars": {"fill_gradient": gradient()}}}}),
        );
        assert_eq!(fixture.tags(fixture.block), [TextEffect::Gradient]);
        fixture.patch(
            fixture.block,
            json!({"translation": {"style": {"chars": {"fill_gradient": null}}}}),
        );
        // On one word.
        fixture.apply(vec![Op::SetRichText {
            layer: fixture.typed.into(),
            spans: vec![
                koharu_scene::Span {
                    insert: "H".to_owned(),
                    attributes: CharStyle {
                        fill_gradient: Some(gradient()),
                        ..CharStyle::default()
                    },
                },
                koharu_scene::Span {
                    insert: "i".to_owned(),
                    attributes: CharStyle::default(),
                },
            ],
        }]);
        assert_eq!(fixture.tags(fixture.typed), [TextEffect::Gradient]);
        // A skew, with its angles.
        let translation = fixture.translation();
        let pivot = skew::pivot(&translation.frame);
        let transform = skew::reskewed(translation.transform, None, (12.0, -3.0), pivot);
        let record = Skew {
            x: 12.0,
            y: -3.0,
            pivot: [pivot.x, pivot.y],
        };
        fixture.apply(vec![
            Op::UpdateLayer {
                layer: fixture.block.into(),
                patch: json!({"translation": {"transform": transform}}),
            },
            attrs::set(fixture.block, &record).expect("the record"),
        ]);
        let found = fixture.effects(fixture.block).expect("effects");
        assert_eq!(found.effects, [TextEffect::Skew]);
        assert_eq!(found.skew, Some(SkewAngles { x: 12.0, y: -3.0 }));
    }

    #[test]
    fn a_runs_emphasis_outline_is_no_effect_but_a_hand_one_is() {
        let mut fixture = Fixture::new();
        // The run stresses "care" in an outline of its own.
        let emphasis = EmphasisSettings {
            ai: true,
            default: EmphasisStyle {
                stroke: Some(RED),
                stroke_width: Some(6.0),
                ..EmphasisStyle::default()
            },
            categories: Default::default(),
        };
        let picks = ["care".to_owned()];
        let marking = emphasis::Marking {
            settings: Some(&emphasis),
            words: &picks,
        };
        fixture.translate(&marking);
        let content = fixture.translation().content;
        assert!(
            content
                .0
                .iter()
                .any(|span| span.attributes.stroke == Some(RED)),
            "the run's mark"
        );
        assert_eq!(fixture.tags(fixture.block), []);
        // The same outline on a word by hand is one.
        let mut spans = content.0.clone();
        spans.push(koharu_scene::Span {
            insert: "!".to_owned(),
            attributes: CharStyle {
                stroke: Some(YELLOW),
                stroke_width: Some(4.0),
                ..CharStyle::default()
            },
        });
        fixture.apply(vec![Op::SetRichText {
            layer: fixture.block.into(),
            spans,
        }]);
        assert_eq!(fixture.tags(fixture.block), [TextEffect::Border]);
    }
}
