//! Booharu's part of the canvas: what a box drawn with Koharu's K tool, the
//! text block tool, asks of the host beside the edit adding its block, to be
//! read at once ([`drawn`], `Effect::Block`), as the O tool's box was before
//! the O tool folded into the K tool, and such a box outlasting a change it
//! didn't make ([`outlasts`]); and the default look of new text, which the T
//! tool's text and a translation typed into a block with none get. Such a
//! translation also gets the marker of one typed by hand in its block's
//! record, in the same step, so it stays a hand edit once cleared, and,
//! typed into a sound effect kept as art, shows its clean-up again, the
//! record carrying no kept mark ([`hand_typed`]). The Booharu settings the
//! canvas needs are one `BooharuSettings`, which the host pushes at startup
//! and whenever the settings they come from are saved, and whenever the
//! brush's feel changes (`BrushFeel`): each brush tool's hardness and Draw's
//! opacity, which its strokes take, soft below a hardness of 1 (`brush`).
//! What the type bar takes off the characters being typed is an `Unmark`,
//! which typing's own Booharu part, `typing/booharu.rs`, does, with the size
//! steps and the selected text the type bar quotes. A right-click on a text
//! or a block with the Select, Direct Select or Text tool, while nothing is
//! typed, selects it unless it's selected already and answers `Effect::Menu`,
//! for the host to open the box's menu there.
//!
//! A box styled before it's translated holds a translation that is only a
//! placeholder ([`status::placeholder`]): the type bar wrote a style set on
//! a block with no translation into the translation the scene starts as its
//! source's blank (Koharu's #1494). The canvas reads such a block as one
//! with none ([`placeholder`]): it shows its source's control, typing into
//! it starts from its source's blank with that style laid over
//! ([`styled_source`]) and makes a first translation in its source's place
//! that keeps the style ([`first_translation`]), and a resize, a reshape, a
//! move or a turn changes its source ([`lettering()`], [`moved()`]), so the
//! placeholder stays one.
//!
//! Show original draws a levelled page's Original scan, the scan as it came
//! in, which its background no longer is ([`original_scan`]).

use booharu_core::{lettering, settings::LetteringSettings, sfx, status, styles};
use koharu_renderer::Drawn;
use koharu_scene::{
    Block, Content, Fit, Frame, Layer, LayerId, Op, Page, PageId, Role, Text, Transform,
};
use kurbo::{Affine, Point};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use ui_events::keyboard::Modifiers;

use crate::{
    Tool,
    control::Control,
    gesture::{self, Context, Press},
    snapshot::Snapshot,
    tools::transform::Lettering,
    view::Sample,
};

pub(crate) mod brush;

/// The Booharu settings the canvas reads, as the host pushed them last. Their
/// default is Koharu's own behaviour, which holds until the host pushes any.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BooharuSettings {
    /// The look of new text.
    pub text: TextDefaults,
    /// How the brush tools' strokes feel, which the interface keeps and the
    /// host pushes as it changes, beside the settings saved in the file,
    /// which hold none of it.
    pub brush: BrushFeel,
}

/// How the brush tools' strokes feel beyond Koharu's brush, its size and
/// colour: each tool's hardness, from 0, soft from its centre line out, to 1,
/// Koharu's hard edge, and Draw's opacity, from just above 0 to 1, at which
/// a stroke paints as one pass, never building up within itself. Its
/// default, every value 1, is Koharu's hard, opaque brush.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct BrushFeel {
    /// The Draw tool's hardness.
    pub draw_hardness: f64,
    /// The Eraser's hardness.
    pub eraser_hardness: f64,
    /// The Remove tool's hardness.
    pub remove_hardness: f64,
    /// The Draw tool's opacity, which its colour's alpha is multiplied by.
    pub draw_opacity: f64,
}

impl Default for BrushFeel {
    fn default() -> Self {
        Self {
            draw_hardness: 1.0,
            eraser_hardness: 1.0,
            remove_hardness: 1.0,
            draw_opacity: 1.0,
        }
    }
}

impl BrushFeel {
    /// The hardness `tool`'s strokes take: Draw's, the Eraser's or Remove's
    /// own, and Koharu's hard edge, 1, for any other tool.
    pub fn hardness(self, tool: Tool) -> f64 {
        match tool {
            Tool::Draw => self.draw_hardness,
            Tool::Eraser => self.eraser_hardness,
            Tool::Remove => self.remove_hardness,
            _ => 1.0,
        }
    }
}

/// The look new text gets on the canvas, as Booharu's Translation stage hook
/// gives a new translation its look.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextDefaults {
    /// `[booharu-lettering]`: the default colours and outline, which the T
    /// tool's text always gets, having nothing measured to go by; the
    /// colour detection switch, which decides whether a translation typed
    /// into a block with none keeps its source's colours or gets those; and
    /// the lettering categories' fonts, which such a translation gets its
    /// block's in place of the default font while Pick fonts automatically
    /// is on. The T tool's text has no category.
    pub lettering: LetteringSettings,
    /// The default font, the first of Koharu's Preferred fonts, which new
    /// text without a family gets a copy of; none to write none, so Koharu's
    /// font chain applies.
    pub family: Option<String>,
    /// How a box drawn with the T tool meets its text at first: Koharu's
    /// shrink unless the host says otherwise.
    pub fit: Fit,
}

/// `text`, the T tool's new text where its drag went, with the default look
/// of `settings`: its font, colours, and outline, as text with nothing
/// measured to go by gets them, and, drawn as a box, the starting fit. Point
/// text keeps its frame.
pub(crate) fn drafted(mut text: Text, settings: &BooharuSettings) -> Text {
    let defaults = &settings.text;
    let look = styles::default_chars(&defaults.lettering, defaults.family.as_deref());
    text.style.chars = text.style.chars.or(&look);
    if let Frame::Box { fit, .. } = &mut text.frame {
        *fit = defaults.fit;
    }
    text
}

/// `base`, the text typing took for `layer`, a block on `page` with no
/// translation of its own (the blank its source starts a translation as, or
/// [`styled_source`]), standing in for the translation typed into it, with
/// the default look of `settings` a new translation gets, as Booharu's
/// Translation stage hook gives it: the default font when it has no family,
/// or the block's category's while Pick fonts automatically is on, and with
/// colour detection off the default colours in place of its source's, a
/// sound effect keeping a legibility outline when the default sets none.
/// With detection on, its source's colours stay. A block whose translation
/// is only a placeholder has a style of its own, set by hand before it was
/// translated, whose values the look keeps, as placement keeps a
/// translation's: with detection off, a colour set there stays.
pub(crate) fn untranslated(
    base: &mut Text,
    page: &Page,
    layer: LayerId,
    settings: &BooharuSettings,
) {
    let patch = look(base, page, layer, settings);
    // The patch holds only character values, so the merge can't fail; were
    // it to, the translation would keep its source's look.
    if let Ok(chars) = styles::patched(&base.style.chars, &patch) {
        base.style.chars = chars;
    }
}

/// The patch on `base`'s character values giving it the look
/// [`untranslated`] gives it, null where a value goes.
fn look(
    base: &Text,
    page: &Page,
    layer: LayerId,
    settings: &BooharuSettings,
) -> Map<String, Value> {
    let defaults = &settings.text;
    let sfx = role(&page.layers, layer) == Some(Role::Sfx);
    // Not those the placeholder has from its source's blank (Koharu's #1494).
    let own = find(&page.layers, layer)
        .and_then(|found| styled(found).map(|block| status::own_chars(found, block)))
        .unwrap_or_default();
    let category = lettering::auto_category(page, layer, &defaults.lettering);
    styles::new_translation(
        &defaults.lettering,
        defaults.family.as_deref(),
        sfx,
        category,
        &own,
        &base.style.chars,
    )
}

/// `typed`, the ops Koharu's typing commits for what was typed into `layer`
/// on `page`, and when that's the block's first translation, typed into a
/// block with none or with only a placeholder ([`placeholder`]), the look
/// [`untranslated`] gives it ahead of them and [`hand_typed`]'s ops after
/// them, all in the commit's one step, so Undo takes the marker of a
/// translation typed by hand back with the text. Koharu's scene adds a
/// missing translation as its source's blank (#1494), which the look merges
/// into, so a block with none gets only the look and the marker; a
/// placeholder gets `base`, the text typing took ([`styled_source`]),
/// written whole with the look, so its first translation goes in its
/// source's place in the style set on it. Any other text, as it is.
pub(crate) fn first_translation(
    mut base: Text,
    page: &Page,
    layer: LayerId,
    settings: &BooharuSettings,
    typed: Vec<Op>,
) -> Vec<Op> {
    let Some(found) = find(&page.layers, layer) else {
        return typed;
    };
    let styled = placeholder(found);
    if !matches!(&found.content, Content::Block(block) if block.translation.is_none() || styled) {
        return typed;
    }
    let mut ops = Vec::with_capacity(typed.len() + 3);
    if styled {
        untranslated(&mut base, page, layer, settings);
        // The translation takes the base's place and style; its characters
        // come only through the text ops.
        let mut placed = serde_json::to_value(&base).expect("a text serializes");
        if let Some(fields) = placed.as_object_mut() {
            fields.remove("content");
        }
        ops.push(Op::UpdateLayer {
            layer: layer.into(),
            patch: json!({ "translation": placed }),
        });
    } else {
        let patch = look(&base, page, layer, settings);
        if !patch.is_empty() {
            ops.push(Op::UpdateLayer {
                layer: layer.into(),
                patch: json!({ "translation": {"style": {"chars": patch}} }),
            });
        }
    }
    ops.extend(typed);
    ops.extend(hand_typed(page, layer));
    ops
}

/// The ops a first translation typed into `layer`, a block on `page` with
/// none, adds to its step, as the Story panel's typing does: for a sound
/// effect Booharu kept as art, its clean-up shown again
/// ([`sfx::shown_again`]), and the block's record of a translation typed by
/// hand ([`styles::hand_typed`]), the marker that keeps the translation a
/// hand edit once cleared, never read as a placeholder, with no kept mark.
/// A clean-up hidden by hand stays hidden, as Koharu leaves it.
pub(crate) fn hand_typed(page: &Page, layer: LayerId) -> Vec<Op> {
    let Some(found) = find(&page.layers, layer) else {
        return Vec::new();
    };
    let mut ops = sfx::shown_again(found);
    // A record read as valid stays valid with the marker, so this never
    // fails; were it to, the translation would read as Koharu's typing
    // alone leaves it.
    ops.extend(styles::hand_typed(found).ok());
    ops
}

/// Whether `layer` is a block whose translation is only a placeholder
/// ([`status::placeholder`]), a style set on it before it was translated,
/// which the canvas reads as a block with none.
pub(crate) fn placeholder(layer: &Layer) -> bool {
    styled(layer).is_some()
}

/// The block `layer` holds, when its translation is only a placeholder.
fn styled(layer: &Layer) -> Option<&Block> {
    match &layer.content {
        Content::Block(block) if status::placeholder(layer, block) => Some(block),
        _ => None,
    }
}

/// What typing into `block`, whose translation is only a placeholder, takes
/// as its text, as typing into a block with none takes the blank its source
/// starts a translation as: that blank with the placeholder's style laid
/// over, booharu-core's rule, which a first translation the Story panel or
/// the agent writes follows too.
pub(crate) use booharu_core::status::styled_source;

/// Where `layer`'s drawn text lives for a resize or a reshape, as
/// [`Lettering::of`] reads it, but a block whose translation is only a
/// placeholder letters by its source, as one with none does: its source's
/// frame changes, so the placeholder stays unplaced, and so stays one.
pub(crate) fn lettering(layer: &Layer) -> Option<(Lettering, &Text)> {
    match styled(layer) {
        Some(block) => Some((Lettering::Source, &block.source)),
        None => Lettering::of(&layer.content),
    }
}

/// `op`, the move or turn of `control`'s layer, which `transform_layer`
/// puts on a block's translation whenever it has one; for a block whose
/// translation is only a placeholder, an update of its source's transform
/// instead, as a block with none moves, composed as `transform_layer`
/// composes it, the move after what the source had. So its control, its
/// source's, goes where it was dragged, and the placeholder stays as it
/// was. Any other op or layer, as it is.
pub(crate) fn moved(snapshot: &Snapshot, control: &Control, op: Op) -> Op {
    let source = snapshot
        .layer(control.layer)
        .and_then(styled)
        .map(|block| &block.source);
    match (op, source) {
        (Op::TransformLayer { layer, transform }, Some(source)) => {
            let moved = Affine::from(transform) * Affine::from(source.transform);
            Op::UpdateLayer {
                layer,
                patch: json!({"source": {"transform": Transform::from(moved)}}),
            }
        }
        (op, _) => op,
    }
}

/// What Show original draws of a levelled page: its Original scan, the scan
/// as it came in, which its background no longer is (D37), shown though
/// it's hidden, as Koharu shows a hidden background; `None` for a page with
/// none, whose background is its scan.
pub(crate) fn original_scan(snapshot: &Snapshot) -> Option<Drawn<'_>> {
    let (layer, _) = booharu_core::cleanup::original_scan(snapshot.page())?;
    snapshot.frame().layer(layer)
}

/// The layer `id` among `layers` and their children.
fn find(layers: &[Layer], id: LayerId) -> Option<&Layer> {
    layers.iter().find_map(|layer| match layer.id == id {
        true => Some(layer),
        false => find(&layer.children, id),
    })
}

/// The role of the block `id` among `layers` and their children.
fn role(layers: &[Layer], id: LayerId) -> Option<Role> {
    match &find(layers, id)?.content {
        Content::Block(block) => Some(block.role),
        _ => None,
    }
}

/// What the type bar takes off the characters being typed, for real, as
/// the words edit takes marks off Story's words, so they agree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Unmark {
    /// Clear: every mark, so the characters look as the text.
    All,
    /// B off: the weight mark, or regular, 400, where the text itself is
    /// bold (600 or more).
    Bold,
    /// I off: the italic mark, or upright where the text itself is italic.
    Italic,
}

/// What one of Booharu's gestures or presses did, for the host to act on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    /// A text block the K tool drew, `layer` of `page`, which the edit
    /// before this effect adds and selects as Koharu's tool does, for the
    /// host to read at once, and translate and clean as the modifiers held
    /// as the pointer lifted pick, as Booharu's O tool did before it folded
    /// into the K tool.
    Block {
        page: PageId,
        layer: LayerId,
        modifiers: BoxModifiers,
    },
    /// A right-click on `layer`, a text or a block of `page`, for the host
    /// to open the box's menu `at` the pointer, in the window's CSS pixels.
    /// The selection holds the layer by then.
    Menu {
        page: PageId,
        layer: LayerId,
        at: Point,
    },
}

/// The modifiers held as a text block's box's pointer lifted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BoxModifiers {
    pub alt: bool,
    pub shift: bool,
}

/// What the K tool's box, `layer` of `page`, asks of the host beside
/// Koharu's edit adding it, with `modifiers`, those the drag held as it
/// lifted: to be read ([`Effect::Block`]).
pub(crate) fn drawn(page: PageId, layer: LayerId, modifiers: Modifiers) -> crate::Effect {
    crate::Effect::Booharu(Effect::Block {
        page,
        layer,
        modifiers: BoxModifiers {
            alt: modifiers.alt(),
            shift: modifiers.shift(),
        },
    })
}

/// Whether `gesture`, the one in progress, outlasts a change it didn't make:
/// a new revision of the page it's on, or another selection. Only a text
/// block's box does, as the O tool's did, as it reads neither until it lifts
/// and then adds its block to the page as it is by then, so the next box
/// drawn while the last one's block is read isn't dropped.
pub(crate) fn outlasts(gesture: Option<&gesture::Gesture>) -> bool {
    matches!(gesture, Some(gesture::Gesture::Block(_)))
}

/// A right-click, with the Select, Direct Select or Text tool while nothing
/// is typed: on a text or a block a click picks, so one neither locked nor
/// hidden, the box's menu at the pointer, after a selection of the layer
/// unless it's selected already, so a right-click inside a selection of
/// several keeps it; on anything else, nothing. It starts no gesture.
pub(crate) fn menu(cx: &Context, sample: Sample) -> Press {
    let Some(layer) = cx.snapshot.hit(sample.page).map(|control| control.layer) else {
        return Press::default();
    };
    let boxed = cx
        .snapshot
        .layer(layer)
        .is_some_and(|found| matches!(found.content, Content::Text(_) | Content::Block(_)));
    if !boxed {
        return Press::default();
    }
    let mut effects = Vec::new();
    if !cx.selection.contains(layer) {
        effects.push(crate::Effect::Selection(vec![layer]));
    }
    // The viewport's physical pixels to the window's CSS pixels.
    let window = sample.screen.to_vec2() + cx.view.viewport().origin().to_vec2();
    effects.push(crate::Effect::Booharu(Effect::Menu {
        page: cx.snapshot.page().id,
        layer,
        at: (window / cx.view.scale()).to_point(),
    }));
    Press::answers(effects)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use booharu_core::{
        lettering::{CategoryFont, LetteringCategory},
        settings::DefaultTextStyle,
    };
    use koharu_scene::{
        Align, Bounds, CharStyle, Created, Direction, Edit, Op, Rgba, Session, StrokeMethod,
        TextStyle,
    };
    use serde_json::{Value, json};

    use super::*;

    const RED: Rgba = Rgba([1.0, 0.0, 0.0, 1.0]);
    const WHITE: Rgba = Rgba([1.0; 4]);
    const BLUE: Rgba = Rgba([0.0, 0.0, 1.0, 1.0]);
    const FONT: &str = "Booharu Test";

    /// Settings as the host pushes them: the default font, red text outlined
    /// 2 pixels wide in `stroke`, colour detection as given, and boxes drawn
    /// at Best Fit.
    fn pushed(detect_colors: bool, stroke: Option<Rgba>) -> BooharuSettings {
        BooharuSettings {
            text: TextDefaults {
                lettering: LetteringSettings {
                    default: DefaultTextStyle {
                        fill: Some(RED),
                        stroke,
                        stroke_width: 2.0,
                    },
                    detect_colors,
                    ..LetteringSettings::default()
                },
                family: Some(FONT.to_owned()),
                fit: Fit::Best,
            },
            ..BooharuSettings::default()
        }
    }

    /// The T tool's box text over a drag, as `Draft::text` makes it.
    fn boxed() -> Text {
        Text {
            frame: Frame::Box {
                bounds: Bounds {
                    x: 0.0,
                    y: 0.0,
                    width: 100.0,
                    height: 50.0,
                },
                fit: Fit::default(),
                anchor: Default::default(),
            },
            ..Text::default()
        }
    }

    #[test]
    fn with_nothing_pushed_new_text_keeps_koharus_look() {
        let settings = BooharuSettings::default();
        assert_eq!(drafted(boxed(), &settings), boxed());
        assert_eq!(drafted(Text::default(), &settings), Text::default());
    }

    #[test]
    fn the_t_tools_text_gets_the_default_look_and_a_box_the_pushed_fit() {
        // Detection or not, as the T tool's text has nothing measured.
        for detect_colors in [true, false] {
            let settings = pushed(detect_colors, Some(WHITE));
            let look = CharStyle {
                family: Some(FONT.to_owned()),
                fill: Some(RED),
                stroke: Some(WHITE),
                stroke_width: Some(2.0),
                ..CharStyle::default()
            };
            let drawn = drafted(boxed(), &settings);
            assert_eq!(drawn.style.chars, look);
            assert!(matches!(drawn.frame, Frame::Box { fit: Fit::Best, .. }));
            let point = drafted(Text::default(), &settings);
            assert_eq!((point.style.chars, point.frame), (look, Frame::Point));
        }
    }

    /// A page 400 by 400 holding `layers`, as add_layer ops' JSON on the page
    /// `$p`, and the ids of the layers labelled.
    fn page_of(layers: Vec<Value>) -> (Page, impl Fn(&str) -> LayerId) {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        let page = json!({"op": "add_page", "id": "$p", "index": 0, "size": {"width": 400, "height": 400}});
        let change = session
            .apply(Edit {
                label: "test".to_owned(),
                ops: std::iter::once(page)
                    .chain(layers)
                    .map(|op| serde_json::from_value::<Op>(op).expect("an op"))
                    .collect(),
            })
            .expect("an edit");
        let ids: Vec<(String, LayerId)> = change
            .created
            .iter()
            .filter_map(|created| match created {
                Created::Layer {
                    id,
                    label: Some(label),
                } => Some((label.clone(), *id)),
                Created::Layer { .. } | Created::Page { .. } => None,
            })
            .collect();
        let project = session.project();
        let page = project.page(change.pages.added[0]).expect("the page");
        let id = move |wanted: &str| {
            ids.iter()
                .find_map(|(label, id)| (label == wanted).then_some(*id))
                .expect("the layer")
        };
        (page.clone(), id)
    }

    /// The add_layer op of the block `label` of `role`, untranslated,
    /// lettered in blue outlined 3 pixels wide in white at size 32 in a box
    /// 40 by 120 at `x`, 20, with `attributes`.
    fn block(label: &str, role: &str, x: f64, attributes: Value) -> Value {
        json!({"op": "add_layer", "id": format!("${label}"), "page": "$p", "index": 0, "layer": {
            "kind": "block", "role": role, "attributes": attributes, "source": {
                "frame": {"box": {"bounds": {"x": x, "y": 20.0, "width": 40.0, "height": 120.0}}},
                "content": "どうして",
                "style": {"chars": {"size": 32.0, "fill": BLUE, "stroke": WHITE, "stroke_width": 3.0}},
            },
        }})
    }

    /// A page holding a dialogue block and a sound effect, neither
    /// translated, each lettered in blue outlined 3 pixels wide in white at
    /// size 32; and their ids.
    fn page() -> (Page, LayerId, LayerId) {
        let (page, id) = page_of(vec![
            block("talk", "text", 10.0, json!({})),
            block("boom", "sfx", 10.0, json!({})),
        ]);
        (page, id("talk"), id("boom"))
    }

    /// The block `layer` on `page`, with the layer holding it.
    fn block_of(page: &Page, layer: LayerId) -> (&Layer, &Block) {
        match find(&page.layers, layer) {
            Some(
                found @ Layer {
                    content: Content::Block(block),
                    ..
                },
            ) => (found, block),
            _ => panic!("no block {layer}"),
        }
    }

    /// What typing takes for `layer` on `page`, its source, in the style set
    /// on it when it's a box styled before it's translated, standing in for
    /// a translation typed into it, with the look `untranslated` gives it
    /// under `settings`.
    fn typed(page: &Page, layer: LayerId, settings: &BooharuSettings) -> CharStyle {
        let (found, block) = block_of(page, layer);
        let mut base = match placeholder(found) {
            true => styled_source(block),
            false => block.source.clone(),
        };
        untranslated(&mut base, page, layer, settings);
        base.style.chars
    }

    #[test]
    fn a_translation_typed_into_a_block_with_none_gets_a_new_translations_look() {
        let (page, talk, boom) = page();
        let source = CharStyle {
            size: Some(32.0),
            fill: Some(BLUE),
            stroke: Some(WHITE),
            stroke_width: Some(3.0),
            ..CharStyle::default()
        };
        // Nothing pushed changes nothing.
        assert_eq!(typed(&page, talk, &BooharuSettings::default()), source);
        // With detection on, the colours copied from its source stay, and
        // only the default font is added.
        let on = pushed(true, Some(WHITE));
        let font = Some(FONT.to_owned());
        let kept = CharStyle {
            family: font.clone(),
            ..source.clone()
        };
        assert_eq!(typed(&page, talk, &on), kept);
        // With it off, the default colours replace them; a sound effect
        // keeps an outline, white about red, an em's sixteenth wide, when
        // the default has none, and dialogue goes without.
        let off = pushed(false, None);
        assert_eq!(
            typed(&page, talk, &off),
            CharStyle {
                family: font.clone(),
                fill: Some(RED),
                stroke: None,
                stroke_width: None,
                ..source.clone()
            }
        );
        assert_eq!(
            typed(&page, boom, &off),
            CharStyle {
                family: font,
                fill: Some(RED),
                stroke: Some(WHITE),
                stroke_width: Some(2.0),
                ..source
            }
        );
    }

    /// `pushed`'s settings, with detection on, with Pick fonts automatically
    /// `auto`: Shout in a bold Booharu Shout, Whisper italic in the default
    /// font, and SFX in Booharu Boom, and no font for Normal Dialogue, so a
    /// category without one of its own has none.
    fn categories(auto: bool) -> BooharuSettings {
        let font = |family: Option<&str>, bold, italic| CategoryFont {
            family: family.map(str::to_owned),
            bold,
            italic,
        };
        let mut settings = pushed(true, Some(WHITE));
        settings.text.lettering.auto = auto;
        settings.text.lettering.categories = BTreeMap::from([
            (
                LetteringCategory::Shout,
                font(Some("Booharu Shout"), true, false),
            ),
            (LetteringCategory::Whisper, font(None, false, true)),
            (
                LetteringCategory::Sfx,
                font(Some("Booharu Boom"), false, false),
            ),
        ]);
        settings
    }

    #[test]
    fn a_translation_typed_into_a_block_takes_its_categorys_font_while_the_switch_is_on() {
        // A spiky balloon holding a block, and outside it a block the user
        // labelled Whisper, Koharu's sound effect, and a floating block.
        let mut spiky = String::new();
        for i in 0..24 {
            let angle = f64::from(i) * std::f64::consts::TAU / 24.0;
            let radius = if i % 2 == 0 { 120.0 } else { 72.0 };
            let (x, y) = (200.0 + radius * angle.cos(), 270.0 + radius * angle.sin());
            spiky.push_str(&format!("{}{x} {y} ", if i == 0 { "M" } else { "L" }));
        }
        let whisper = json!({"booharu": {"lettering": {"category": "whisper", "source": "user"}}});
        let mut inside = block("inside", "text", 180.0, json!({}));
        inside["parent"] = json!("$spiky");
        inside["layer"]["source"]["frame"]["box"]["bounds"]["y"] = json!(210.0);
        let (page, id) = page_of(vec![
            json!({"op": "add_layer", "id": "$spiky", "page": "$p", "index": 0,
                "layer": {"kind": "balloon", "outline": spiky + "Z"}}),
            inside,
            block("labelled", "text", 10.0, whisper),
            block("boom", "sfx", 60.0, json!({})),
            block("floating", "text", 110.0, json!({})),
        ]);
        let source = CharStyle {
            size: Some(32.0),
            fill: Some(BLUE),
            stroke: Some(WHITE),
            stroke_width: Some(3.0),
            ..CharStyle::default()
        };
        let font = |family: &str, weight: Option<u16>, italic: Option<bool>| CharStyle {
            family: Some(family.to_owned()),
            weight,
            italic,
            ..source.clone()
        };

        // On, resolved from the page as the Story panel's typed translation
        // resolves it: the spiky balloon's block is a shout; the labelled
        // block a whisper, in the default font made italic; the sound effect
        // SFX; and the floating block's category has no font, nor Normal
        // Dialogue, so it keeps the default look's.
        let on = categories(true);
        assert_eq!(
            typed(&page, id("inside"), &on),
            font("Booharu Shout", Some(700), None)
        );
        assert_eq!(
            typed(&page, id("labelled"), &on),
            font(FONT, None, Some(true))
        );
        assert_eq!(
            typed(&page, id("boom"), &on),
            font("Booharu Boom", None, None)
        );
        assert_eq!(typed(&page, id("floating"), &on), font(FONT, None, None));

        // Off, the default look alone, whatever the category.
        let off = categories(false);
        for name in ["inside", "labelled", "boom", "floating"] {
            assert_eq!(
                typed(&page, id(name), &off),
                font(FONT, None, None),
                "{name}"
            );
        }

        // The T tool's free text has no category (T17): the default look.
        assert_eq!(drafted(boxed(), &on), drafted(boxed(), &off));
    }

    /// A colour set by hand, as the type bar's.
    const GREEN: Rgba = Rgba([0.0, 0.5, 0.0, 1.0]);

    /// The op the type bar sends for a style set on the block `label` while
    /// it has no translation: `translation`, which the scene merges into its
    /// source's blank (Koharu's #1494), a placeholder.
    fn styled_by_hand(label: &str, translation: Value) -> Value {
        json!({"op": "update_layer", "layer": format!("${label}"),
            "patch": {"translation": translation}})
    }

    #[test]
    fn a_box_styled_before_its_translated_types_from_its_source_in_that_style() {
        // A block never styled; one given a font, a colour and every
        // paragraph key, set vertical; one whose source is vertical, given
        // an alignment, so its placeholder keeps the scene's horizontal; and
        // one with a translation typed into it.
        let mut upright = block("upright", "text", 110.0, json!({}));
        upright["layer"]["source"]["direction"] = json!("vertical");
        let mut translated = block("translated", "text", 160.0, json!({}));
        translated["layer"]["translation"] = json!({"content": "Why?",
            "frame": {"box": {"bounds": {"x": 160.0, "y": 20.0, "width": 40.0, "height": 120.0}}}});
        let (page, id) = page_of(vec![
            block("plain", "text", 10.0, json!({})),
            block("styled", "text", 60.0, json!({})),
            styled_by_hand(
                "styled",
                json!({"direction": "vertical", "style": {
                    "chars": {"family": "Comic Neue", "fill": GREEN},
                    "align": "center", "line_height": 1.5, "stroke_method": "contour",
                }}),
            ),
            upright,
            styled_by_hand("upright", json!({"style": {"align": "end"}})),
            translated,
        ]);
        let is_placeholder = |name| placeholder(block_of(&page, id(name)).0);
        assert!(!is_placeholder("plain"));
        assert!(is_placeholder("styled"));
        assert!(is_placeholder("upright"));
        assert!(!is_placeholder("translated"));

        // The style laid over the source's, the source's values where it
        // sets none; its frame, place and language the source's.
        let (_, styled) = block_of(&page, id("styled"));
        let typed = styled_source(styled);
        assert_eq!(
            typed.style,
            TextStyle {
                chars: CharStyle {
                    family: Some("Comic Neue".to_owned()),
                    size: Some(32.0),
                    fill: Some(GREEN),
                    stroke: Some(WHITE),
                    stroke_width: Some(3.0),
                    ..CharStyle::default()
                },
                align: Some(Align::Center),
                line_height: Some(1.5),
                stroke_method: Some(StrokeMethod::Contour),
            }
        );
        assert_eq!(typed.direction, Direction::Vertical);
        let source = &styled.source;
        assert_eq!(
            (&typed.frame, typed.transform, &typed.language),
            (&source.frame, source.transform, &source.language)
        );
        // A placeholder's horizontal, the scene's default, never turns a
        // vertical source.
        let (_, upright) = block_of(&page, id("upright"));
        let typed = styled_source(upright);
        assert_eq!(typed.direction, Direction::Vertical);
        assert_eq!(typed.style.align, Some(Align::End));
    }

    #[test]
    fn a_style_set_before_translating_is_kept_by_the_new_translations_look() {
        // Dialogue given a font and a colour, and a sound effect given only
        // a colour, each lettered as `block` letters it.
        let (page, id) = page_of(vec![
            block("talk", "text", 10.0, json!({})),
            styled_by_hand(
                "talk",
                json!({"style": {"chars": {"family": "Comic Neue", "fill": GREEN}}}),
            ),
            block("boom", "sfx", 60.0, json!({})),
            styled_by_hand("boom", json!({"style": {"chars": {"fill": GREEN}}})),
        ]);
        let source = CharStyle {
            size: Some(32.0),
            fill: Some(BLUE),
            stroke: Some(WHITE),
            stroke_width: Some(3.0),
            ..CharStyle::default()
        };
        let hand = CharStyle {
            family: Some("Comic Neue".to_owned()),
            fill: Some(GREEN),
            ..source.clone()
        };
        // With detection off, the colour set by hand stays, the default
        // outline, none for dialogue, replaces the source's, and the font
        // set by hand stays; the sound effect, with no font set, gets the
        // default one and its legibility outline about its own colour.
        let off = pushed(false, None);
        assert_eq!(
            typed(&page, id("talk"), &off),
            CharStyle {
                stroke: None,
                stroke_width: None,
                ..hand.clone()
            }
        );
        assert_eq!(
            typed(&page, id("boom"), &off),
            CharStyle {
                family: Some(FONT.to_owned()),
                fill: Some(GREEN),
                stroke: Some(WHITE),
                stroke_width: Some(2.0),
                ..source.clone()
            }
        );
        // With it on, the source's colours stay where none was set.
        let on = pushed(true, Some(WHITE));
        assert_eq!(typed(&page, id("talk"), &on), hand);
        assert_eq!(
            typed(&page, id("boom"), &on),
            CharStyle {
                family: Some(FONT.to_owned()),
                fill: Some(GREEN),
                ..source
            }
        );
    }
}
