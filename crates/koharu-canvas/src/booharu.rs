//! Booharu's part of the canvas: the O tool, whose box dragged out over the
//! page the canvas answers as `Effect::Booharu`, for the host to make a text
//! block of, and the default look of new text, which the T tool's text and a
//! translation typed into a block with none get. The Booharu settings the
//! canvas needs are one `BooharuSettings`, which the host pushes at startup
//! and whenever the settings they come from are saved.

use booharu_core::{settings::LetteringSettings, styles};
use koharu_scene::{CharStyle, Content, Fit, Frame, Layer, LayerId, Page, PageId, Role, Text};
use kurbo::{Point, Rect, Size};
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
    /// tool's text always gets, having nothing measured to go by, and the
    /// colour detection switch, which decides whether a translation typed
    /// into a block with none keeps its source's colours or gets those.
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
/// gives it: the default font when it has no family, and with colour
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
    let patch = styles::new_translation(
        &defaults.lettering,
        defaults.family.as_deref(),
        sfx,
        &own,
        &base.style.chars,
    );
    // The patch holds only character values, so the merge can't fail; were
    // it to, the translation would keep its source's look.
    if let Ok(chars) = styles::patched(&base.style.chars, &patch) {
        base.style.chars = chars;
    }
}

/// The role of the block `id` among `layers` and their children.
fn role(layers: &[Layer], id: LayerId) -> Option<Role> {
    layers.iter().find_map(|layer| match &layer.content {
        _ if layer.id != id => role(&layer.children, id),
        Content::Block(block) => Some(block.role),
        _ => None,
    })
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
    use booharu_core::settings::DefaultTextStyle;
    use koharu_scene::{Bounds, Created, Edit, Op, Rgba, Session};
    use serde_json::json;

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

    /// A page holding a dialogue block and a sound effect, neither
    /// translated, each lettered in blue outlined 3 pixels wide in white at
    /// size 32; and their ids.
    fn page() -> (Page, LayerId, LayerId) {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        let block = |role: &str| {
            json!({"kind": "block", "role": role, "source": {
                "frame": {"box": {"bounds": {"x": 10.0, "y": 20.0, "width": 40.0, "height": 120.0}}},
                "content": "どうして",
                "style": {"chars": {"size": 32.0, "fill": BLUE, "stroke": WHITE, "stroke_width": 3.0}},
            }})
        };
        let ops = [
            json!({"op": "add_page", "id": "$p", "index": 0, "size": {"width": 400, "height": 400}}),
            json!({"op": "add_layer", "id": "$talk", "page": "$p", "index": 0, "layer": block("text")}),
            json!({"op": "add_layer", "id": "$boom", "page": "$p", "index": 1, "layer": block("sfx")}),
        ];
        let change = session
            .apply(Edit {
                label: "test".to_owned(),
                ops: ops
                    .into_iter()
                    .map(|op| serde_json::from_value::<Op>(op).expect("an op"))
                    .collect(),
            })
            .expect("an edit");
        let id = |wanted: &str| {
            change
                .created
                .iter()
                .find_map(|created| match created {
                    Created::Layer {
                        id,
                        label: Some(label),
                    } if label == wanted => Some(*id),
                    _ => None,
                })
                .expect("the block")
        };
        let project = session.project();
        let page = project.page(change.pages.added[0]).expect("the page");
        (page.clone(), id("talk"), id("boom"))
    }

    /// The source of `layer` on `page`, standing in for a translation typed
    /// into it, with the look `untranslated` gives it under `settings`.
    fn typed(page: &Page, layer: LayerId, settings: &BooharuSettings) -> CharStyle {
        let Some(Content::Block(block)) = page
            .layers
            .iter()
            .find(|found| found.id == layer)
            .map(|found| &found.content)
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
}
