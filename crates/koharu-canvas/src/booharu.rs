//! Booharu's part of the canvas: the O tool, whose box dragged out over the
//! page the canvas answers as `Effect::Booharu`, for the host to make a text
//! block of, and the default look of new text, which the T tool's text and a
//! translation typed into a block with none get. Such a translation typed
//! into a sound effect kept as art also shows its clean-up again, without
//! the kept mark, in the same step. The Booharu settings the canvas needs
//! are one `BooharuSettings`, which the host pushes at startup and whenever
//! the settings they come from are saved. What the type bar takes off the
//! characters being typed is an `Unmark`, which typing's own Booharu part,
//! `typing/booharu.rs`, does, with the size steps and the selected text the
//! type bar quotes.

use booharu_core::{lettering, settings::LetteringSettings, sfx, styles};
use koharu_scene::{CharStyle, Content, Fit, Frame, Layer, LayerId, Op, Page, PageId, Role, Text};
use kurbo::{Point, Rect, Size};
use serde::{Deserialize, Serialize};
use ui_events::keyboard::Modifiers;

use crate::{
    draw::overlay::Overlay,
    gesture::{self, Context, GestureLook, Press},
    view::Sample,
};

/// The Booharu settings the canvas reads, as the host pushed them last. Their
/// default is Koharu's own behaviour, which holds until the host pushes any.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BooharuSettings {
    /// The look of new text.
    pub text: TextDefaults,
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

/// `base`, the source of `layer`, a block on `page` with no translation,
/// standing in for the translation typed into it, with the default look of
/// `settings` a new translation gets, as Booharu's Translation stage hook
/// gives it: the default font when it has no family, or the block's
/// category's while Pick fonts automatically is on, and with colour
/// detection off the default colours in place of its source's, a sound
/// effect keeping a legibility outline when the default sets none. With
/// detection on, its source's colours stay.
pub(crate) fn untranslated(
    base: &mut Text,
    page: &Page,
    layer: LayerId,
    settings: &BooharuSettings,
) {
    let defaults = &settings.text;
    let sfx = role(&page.layers, layer) == Some(Role::Sfx);
    let own = CharStyle::default();
    let category = lettering::auto_category(page, layer, &defaults.lettering);
    let patch = styles::new_translation(
        &defaults.lettering,
        defaults.family.as_deref(),
        sfx,
        category,
        &own,
        &base.style.chars,
    );
    // The patch holds only character values, so the merge can't fail; were
    // it to, the translation would keep its source's look.
    if let Ok(chars) = styles::patched(&base.style.chars, &patch) {
        base.style.chars = chars;
    }
}

/// The ops a first translation typed into `layer`, a block on `page` with
/// none, adds to its step: for a sound effect Booharu kept as art, its
/// clean-up shown again and the kept mark taken off, as the Story panel's
/// typing does ([`sfx::shown_again`]); none for any other block, so a
/// clean-up hidden by hand stays hidden, as Koharu leaves it.
pub(crate) fn shown_again(page: &Page, layer: LayerId) -> Vec<Op> {
    // A record read as valid stays valid without the mark, so this never
    // fails; were it to, the clean-up would stay hidden and marked, as
    // Koharu's typing alone leaves it.
    find(&page.layers, layer)
        .and_then(|found| sfx::shown_again(found).ok())
        .unwrap_or_default()
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

/// What one of Booharu's gestures did, for the host to act on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    /// A box drawn with the O tool over `rect` of `page`, in page pixels, cut
    /// to the page, with the modifiers held as the pointer lifted, by which
    /// the host picks what the box goes through.
    Box {
        page: PageId,
        rect: Rect,
        modifiers: BoxModifiers,
    },
}

/// The modifiers held as an O tool box's pointer lifted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BoxModifiers {
    pub alt: bool,
    pub shift: bool,
}

/// A box shorter than this across or down on screen once cut to the page, in
/// device-independent pixels, is no box, as Booharu 0.0.4's O tool ignored
/// one.
const SMALLEST: f64 = 4.0;

/// Whether `gesture`, the one in progress, outlasts a change it didn't make:
/// a new revision of the page it's on, or another selection. Only an O tool
/// box does, as it reads neither and writes nothing until it lifts, so the
/// next box drawn while the last one's block is added, selected and read
/// isn't dropped.
pub(crate) fn outlasts(gesture: Option<&gesture::Gesture>) -> bool {
    matches!(gesture, Some(gesture::Gesture::Booharu(_)))
}

/// A press with the O tool starts a box.
pub(crate) fn press(sample: Sample) -> Press {
    Press::starts(gesture::Gesture::Booharu(OcrBox {
        start: sample.page,
        current: sample.page,
        modifiers: sample.modifiers,
    }))
}

/// The O tool's box being dragged out, in page points, and the modifiers the
/// pointer last held. As every gesture, a cancel or another tool drops it
/// unmade.
pub(crate) struct OcrBox {
    start: Point,
    current: Point,
    modifiers: Modifiers,
}

impl OcrBox {
    pub fn moved(&mut self, samples: &[Sample]) {
        if let Some(last) = samples.last() {
            self.current = last.page;
            self.modifiers = last.modifiers;
        }
    }

    /// The box on a page of `size`, cut to it.
    fn cut(&self, size: Size) -> Rect {
        Rect::from_points(self.start, self.current).intersect(size.to_rect())
    }

    /// The box made as the pointer lifts, which the canvas moves it to first,
    /// with the modifiers held then; none when it's shorter than `SMALLEST`
    /// across or down.
    pub fn finish(self, cx: &Context) -> Vec<crate::Effect> {
        let rect = self.cut(cx.snapshot.size());
        let smallest = cx.view.page_length(SMALLEST);
        if rect.width() < smallest || rect.height() < smallest {
            return Vec::new();
        }
        vec![crate::Effect::Booharu(Effect::Box {
            page: cx.snapshot.page().id,
            rect,
            modifiers: BoxModifiers {
                alt: self.modifiers.alt(),
                shift: self.modifiers.shift(),
            },
        })]
    }

    /// The box as it would be made, cut to the page, dashed in the
    /// selection's colour as the Type tool's frame being dragged out is.
    pub fn preview(&self, cx: &Context, overlay: &mut Overlay) {
        let rect = self.cut(cx.snapshot.size());
        if rect.area() > 0.0 {
            overlay.dashed(rect);
        }
    }

    pub fn look(&self) -> GestureLook {
        GestureLook::Rect(Rect::from_points(self.start, self.current))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use booharu_core::{
        lettering::{CategoryFont, LetteringCategory},
        settings::DefaultTextStyle,
    };
    use koharu_scene::{Bounds, Created, Edit, Op, Rgba, Session};
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

    /// The source of `layer` on `page`, standing in for a translation typed
    /// into it, with the look `untranslated` gives it under `settings`.
    fn typed(page: &Page, layer: LayerId, settings: &BooharuSettings) -> CharStyle {
        let Some(Content::Block(block)) = find(&page.layers, layer).map(|found| &found.content)
        else {
            panic!("no block {layer}");
        };
        let mut base = block.source.clone();
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
}
