//! The lettering default: the look Reset to lettering default gives a box
//! back, worked out from the scene alone, with no model call (D13).
//!
//! A translated block's default is the look its first translation got at
//! its current size: Koharu's placement's colours and outline, measured from
//! its source ([`koharu_chars`]), then Booharu's default look over them, as
//! `styles::new_translation` gives a new translation, with the block's
//! category font while Pick fonts automatically is on. A subtitle's is the
//! subtitle look of `[booharu-lettering] subtitle`, and any other text
//! layer's the default look the T tool gives (`styles::default_chars`).
//! An untranslated block, and any other layer, has none.
//!
//! Translating again keeps a box's look, so this is the only way back to
//! the default once it was changed by hand.

use anyhow::Result;
use koharu_scene::{
    Block, CharStyle, Color, Content, Layer, LayerId, Page, Project, Rgba, Role, StrokeMethod,
};
use serde_json::{Value, json};

use crate::{
    attrs::{self, SfxSubtitle},
    lettering,
    settings::LetteringSettings,
    styles,
};

/// The character keys a lettering default sets, every one of them, null
/// where it sets none: the hand-edit record's style keys (`AutoStyle`'s).
pub const LOOK: [&str; 8] = [
    "family",
    "weight",
    "italic",
    "fill",
    "stroke",
    "stroke_width",
    "outer_stroke",
    "outer_stroke_width",
];

/// A lettering default: the character values of the [`LOOK`] keys, beside
/// which a size may stand, which Reset never writes, and how the borders
/// are drawn, which no default sets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Look {
    pub chars: CharStyle,
    pub stroke_method: Option<StrokeMethod>,
}

/// The character values Koharu's placement gives the translation of
/// `block` on `page` as if the translation set none, at `size`, or at the
/// page's lettering size without one: the source's ink as its fill, and its
/// outline as its stroke, the width scaled from the source's size to `size`,
/// or for a sound effect with none a stroke a sixteenth of its size, black
/// around a light fill and white around any other.
///
/// A copy of the fill, stroke and size rules of koharu-pipeline's
/// `stages/translate/placement/mod.rs` (`placement` and its `lettering`),
/// which this crate can't call: keep it in step with them. A parity test in
/// koharu-pipeline's Booharu tests compares the two.
pub fn koharu_chars(page: &Page, block: &Block, size: Option<f64>) -> CharStyle {
    let size = size.or_else(|| lettering_size(&page.layers));
    let source = &block.source.style.chars;
    let outline = match (source.stroke, source.stroke_width) {
        (Some(stroke), Some(width)) => {
            let scale = size.zip(source.size).map_or(1.0, |(to, from)| to / from);
            Some((stroke, width * scale))
        }
        _ if block.role == Role::Sfx => size.map(|size| (contrasting(source.fill), size / 16.0)),
        _ => None,
    };
    CharStyle {
        size,
        fill: source.fill,
        stroke: outline.map(|(stroke, _)| stroke),
        stroke_width: outline.map(|(_, width)| width),
        ..CharStyle::default()
    }
}

/// The outline colour that keeps text filled with `fill` legible over art,
/// as Koharu's placement picks a sound effect's: black around a light fill,
/// and white around any other, the renderer's black included.
pub fn contrasting(fill: Option<Rgba>) -> Rgba {
    let light =
        fill.is_some_and(|fill| Color::from(fill).discard_alpha().relative_luminance() > 0.5);
    if light {
        Rgba([0.0, 0.0, 0.0, 1.0])
    } else {
        Rgba([1.0; 4])
    }
}

/// The page's lettering size, as placement's `lettering` finds it: the
/// lower median size of the sources of its text blocks, inside other layers
/// included, if any has one.
fn lettering_size(layers: &[Layer]) -> Option<f64> {
    fn sizes(layers: &[Layer], found: &mut Vec<f64>) {
        for layer in layers {
            if let Content::Block(block) = &layer.content
                && block.role == Role::Text
                && let Some(size) = block.source.style.chars.size
            {
                found.push(size);
            }
            sizes(&layer.children, found);
        }
    }
    let mut found = Vec::new();
    sizes(layers, &mut found);
    found.sort_unstable_by(f64::total_cmp);
    found.get(found.len().checked_sub(1)? / 2).copied()
}

/// The lettering default of the layer `id` in `project`, under `settings`
/// and `font`, the default font: for a translated block, the look its first
/// translation got, at its current size, with its live role and, while Pick
/// fonts automatically is on, its category's font; for a subtitle's text
/// layer, the subtitle look, its outline a ninth of its current size; for
/// any other text layer, the default look; none for an untranslated block
/// or any other layer.
pub fn lettering_default(
    project: &Project,
    id: LayerId,
    settings: &LetteringSettings,
    font: Option<&str>,
) -> Result<Option<Look>> {
    let layer = project.layer(id)?;
    let chars = match &layer.content {
        Content::Block(block) => {
            let Some(translation) = &block.translation else {
                return Ok(None);
            };
            let (page, _) = project.parent(id)?;
            let base = koharu_chars(page, block, translation.style.chars.size);
            let look = styles::new_translation(
                settings,
                font,
                block.role == Role::Sfx,
                lettering::auto_category(page, id, settings),
                &CharStyle::default(),
                &base,
            );
            styles::patched(&base, &look)?
        }
        Content::Text(text) if attrs::get::<SfxSubtitle>(layer).is_some() => {
            let style = &settings.subtitle;
            let size = text.style.chars.size;
            CharStyle {
                family: style.family.clone().or_else(|| font.map(str::to_owned)),
                size,
                fill: Some(style.fill),
                stroke: Some(style.stroke),
                stroke_width: size.map(|size| size / 9.0),
                ..CharStyle::default()
            }
        }
        Content::Text(text) => CharStyle {
            size: text.style.chars.size,
            ..styles::default_chars(settings, font)
        },
        _ => return Ok(None),
    };
    Ok(Some(Look {
        chars,
        stroke_method: None,
    }))
}

/// The merge patch on a text's `style` giving it `look`: each [`LOOK`] key
/// of its characters, null where the look sets none, and how its borders
/// are drawn. Never its size, its alignment, or its line height.
pub fn reset_patch(look: &Look) -> Value {
    let chars = &look.chars;
    json!({
        "chars": {
            "family": chars.family,
            "weight": chars.weight,
            "italic": chars.italic,
            "fill": chars.fill,
            "stroke": chars.stroke,
            "stroke_width": chars.stroke_width,
            "outer_stroke": chars.outer_stroke,
            "outer_stroke_width": chars.outer_stroke_width,
        },
        "stroke_method": look.stroke_method,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use koharu_scene::{Created, Edit, Op, Session};

    use super::*;
    use crate::{
        attrs::{Lettering, LetteringSource},
        lettering::{CategoryFont, LetteringCategory},
        settings::{DefaultTextStyle, SubtitleStyle},
    };

    const RED: Rgba = Rgba([1.0, 0.0, 0.0, 1.0]);
    const BLUE: Rgba = Rgba([0.0, 0.0, 1.0, 1.0]);
    const YELLOW: Rgba = Rgba([1.0, 1.0, 0.0, 1.0]);
    const WHITE: Rgba = Rgba([1.0; 4]);
    const BLACK: Rgba = Rgba([0.0, 0.0, 0.0, 1.0]);

    /// A page 400 pixels square holding each `(name, block)`, a block's JSON
    /// as `add_layer` takes it, at the page's top level in that order.
    struct Fixture {
        _dir: tempfile::TempDir,
        session: Session,
        ids: BTreeMap<String, LayerId>,
    }

    impl Fixture {
        fn new(layers: &[(&str, Value)]) -> Self {
            let dir = tempfile::tempdir().expect("a folder");
            let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
            let mut ops = vec![json!({"op": "add_page", "id": "$page", "index": 0,
                "size": {"width": 400, "height": 400}})];
            for (index, (name, layer)) in layers.iter().enumerate() {
                ops.push(
                    json!({"op": "add_layer", "id": format!("${name}"), "page": "$page",
                    "index": index, "layer": layer}),
                );
            }
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
            Self {
                _dir: dir,
                session,
                ids,
            }
        }

        fn id(&self, name: &str) -> LayerId {
            self.ids[name]
        }

        fn page(&self) -> Page {
            let project = self.session.project();
            project.pages().next().expect("a page").clone()
        }

        fn block(&self, name: &str) -> Block {
            match &self
                .session
                .project()
                .layer(self.id(name))
                .expect("a layer")
                .content
            {
                Content::Block(block) => (**block).clone(),
                other => panic!("{other:?} isn't a block"),
            }
        }

        fn default(
            &self,
            name: &str,
            settings: &LetteringSettings,
            font: Option<&str>,
        ) -> Option<Look> {
            lettering_default(&self.session.project(), self.id(name), settings, font)
                .expect("a default")
        }

        fn apply(&mut self, ops: Vec<Op>) {
            self.session
                .apply(Edit {
                    label: "test".to_owned(),
                    ops,
                })
                .expect("the edit");
        }
    }

    /// A block of `role` whose source, at `x`, is lettered in `chars`, and,
    /// when `translation` is some, translated with those character values.
    fn block(role: &str, x: f64, chars: Value, translation: Option<Value>) -> Value {
        let mut block = json!({"kind": "block", "role": role, "source": {
            "content": "どうして",
            "frame": {"box": {"bounds": {"x": x, "y": 20.0, "width": 40.0, "height": 120.0}}},
            "style": {"chars": chars},
        }});
        if let Some(chars) = translation {
            block["translation"] = json!({"content": "Why?", "style": {"chars": chars}});
        }
        block
    }

    fn settings(
        detect_colors: bool,
        fill: Option<Rgba>,
        stroke: Option<Rgba>,
    ) -> LetteringSettings {
        LetteringSettings {
            default: DefaultTextStyle {
                fill,
                stroke,
                stroke_width: 2.0,
            },
            detect_colors,
            ..LetteringSettings::default()
        }
    }

    #[test]
    fn koharus_outline_is_the_sources_rescaled_to_the_size() {
        let measured = json!({"size": 32.0, "fill": BLUE, "stroke": WHITE, "stroke_width": 3.0});
        let fixture = Fixture::new(&[("a", block("text", 10.0, measured, None))]);
        let page = fixture.page();
        let found = fixture.block("a");
        // At the page's lettering size, the source's own.
        assert_eq!(
            koharu_chars(&page, &found, None),
            CharStyle {
                size: Some(32.0),
                fill: Some(BLUE),
                stroke: Some(WHITE),
                stroke_width: Some(3.0),
                ..CharStyle::default()
            }
        );
        // At half the size, half as wide.
        let half = koharu_chars(&page, &found, Some(16.0));
        assert_eq!((half.size, half.stroke_width), (Some(16.0), Some(1.5)));
    }

    #[test]
    fn a_sound_effect_without_an_outline_gets_one_unlike_its_fill() {
        let fixture = Fixture::new(&[
            ("text", block("text", 10.0, json!({"size": 32.0}), None)),
            (
                "light",
                block("sfx", 60.0, json!({"size": 48.0, "fill": YELLOW}), None),
            ),
            (
                "dark",
                block("sfx", 110.0, json!({"size": 48.0, "fill": BLUE}), None),
            ),
            ("unfilled", block("sfx", 160.0, json!({}), None)),
            ("plain", block("text", 210.0, json!({"fill": RED}), None)),
        ]);
        let page = fixture.page();
        let chars = |name: &str| koharu_chars(&page, &fixture.block(name), None);
        // A sound effect's own size never counts toward the page's.
        assert_eq!(chars("light").size, Some(32.0));
        assert_eq!(
            (chars("light").stroke, chars("light").stroke_width),
            (Some(BLACK), Some(2.0))
        );
        assert_eq!(
            (chars("dark").stroke, chars("dark").stroke_width),
            (Some(WHITE), Some(2.0))
        );
        assert_eq!(
            chars("unfilled").stroke,
            Some(WHITE),
            "the renderer's black"
        );
        // Text without an outline gets none.
        assert_eq!(
            (chars("plain").stroke, chars("plain").stroke_width),
            (None, None)
        );
        assert_eq!(chars("plain").fill, Some(RED));
        assert_eq!(contrasting(Some(WHITE)), BLACK);
        assert_eq!(contrasting(None), WHITE);
    }

    #[test]
    fn a_translated_blocks_default_is_its_first_translations_look() {
        let measured = json!({"size": 32.0, "fill": BLUE, "stroke": WHITE, "stroke_width": 3.0});
        // Restyled by hand at a new size: a font, colours and borders.
        let restyled = json!({"size": 24.0, "family": "Comic Sans", "weight": 700,
            "fill": RED, "stroke": BLACK, "stroke_width": 9.0,
            "outer_stroke": YELLOW, "outer_stroke_width": 2.0});
        let fixture = Fixture::new(&[
            ("a", block("text", 10.0, measured.clone(), Some(restyled))),
            ("untranslated", block("text", 60.0, measured, None)),
        ]);
        // Detection on: Koharu's colours at its size, in the default font.
        let on = settings(true, None, None);
        let look = fixture
            .default("a", &on, Some("CCWildWords"))
            .expect("a look");
        assert_eq!(
            look,
            Look {
                chars: CharStyle {
                    family: Some("CCWildWords".to_owned()),
                    size: Some(24.0),
                    fill: Some(BLUE),
                    stroke: Some(WHITE),
                    stroke_width: Some(2.25),
                    ..CharStyle::default()
                },
                stroke_method: None,
            }
        );
        // Detection off: the default look's colours, null where it sets none.
        let off = settings(false, Some(RED), None);
        let look = fixture
            .default("a", &off, Some("CCWildWords"))
            .expect("a look");
        assert_eq!(
            (look.chars.fill, look.chars.stroke, look.chars.stroke_width),
            (Some(RED), None, None)
        );
        let outlined = settings(false, None, Some(YELLOW));
        let look = fixture.default("a", &outlined, None).expect("a look");
        assert_eq!(
            (look.chars.fill, look.chars.stroke, look.chars.stroke_width),
            (None, Some(YELLOW), Some(2.0))
        );
        // No Preferred font: no family at all.
        assert_eq!(look.chars.family, None);
        // An untranslated block has none.
        assert_eq!(
            fixture.default("untranslated", &on, Some("CCWildWords")),
            None
        );
    }

    #[test]
    fn pick_fonts_automatically_gives_the_category_font_by_the_live_role() {
        let measured = json!({"size": 32.0, "fill": YELLOW});
        let mut fixture = Fixture::new(&[(
            "boom",
            block(
                "sfx",
                10.0,
                measured,
                Some(json!({"size": 32.0, "family": "Arial"})),
            ),
        )]);
        let mut lettered = settings(false, None, None);
        lettered.categories = BTreeMap::from([(
            LetteringCategory::Sfx,
            CategoryFont {
                family: Some("Badaboom BB".to_owned()),
                bold: true,
                italic: false,
            },
        )]);
        // Off: the default font, with the sound effect's legibility outline.
        let look = fixture
            .default("boom", &lettered, Some("CCWildWords"))
            .expect("a look");
        assert_eq!(look.chars.family.as_deref(), Some("CCWildWords"));
        assert_eq!(look.chars.weight, None);
        assert_eq!(
            (look.chars.stroke, look.chars.stroke_width),
            (Some(WHITE), Some(2.0))
        );
        // On: the category's font.
        lettered.auto = true;
        let look = fixture
            .default("boom", &lettered, Some("CCWildWords"))
            .expect("a look");
        assert_eq!(
            (look.chars.family.as_deref(), look.chars.weight),
            (Some("Badaboom BB"), Some(700))
        );
        // A user's category wins over the role, and text gets no
        // legibility outline: the role read is the one the block has now.
        let label = Lettering {
            category: LetteringCategory::Shout,
            source: LetteringSource::User,
            confidence: None,
            role: None,
        };
        let id = fixture.id("boom");
        fixture.apply(vec![
            attrs::set(id, &label).expect("a label"),
            Op::UpdateLayer {
                layer: id.into(),
                patch: json!({"role": "text"}),
            },
        ]);
        let look = fixture
            .default("boom", &lettered, Some("CCWildWords"))
            .expect("a look");
        assert_eq!(
            look.chars.family.as_deref(),
            Some("CCWildWords"),
            "Shout has no font"
        );
        assert_eq!((look.chars.stroke, look.chars.stroke_width), (None, None));
    }

    #[test]
    fn a_text_layers_default_is_the_t_tools_and_a_subtitles_its_own() {
        let fixture = Fixture::new(&[
            (
                "typed",
                json!({"kind": "text", "content": "Hi",
                "style": {"chars": {"size": 20.0, "family": "Arial", "fill": BLUE}}}),
            ),
            (
                "subtitle",
                json!({"kind": "text", "content": "Boom",
                "style": {"chars": {"size": 18.0, "fill": BLUE, "stroke": RED, "stroke_width": 5.0}},
                "attributes": {"booharu": {"sfx_subtitle": {"block": LayerId(koharu_scene::Uuid::now_v7())}}}}),
            ),
            (
                "balloon",
                json!({"kind": "balloon", "outline": "M0 0 L10 0 L10 10 Z"}),
            ),
        ]);
        let look = settings(true, Some(RED), Some(WHITE));
        let typed = fixture
            .default("typed", &look, Some("CCWildWords"))
            .expect("a look");
        assert_eq!(
            typed.chars,
            CharStyle {
                size: Some(20.0),
                ..styles::default_chars(&look, Some("CCWildWords"))
            }
        );
        let subtitled = LetteringSettings {
            subtitle: SubtitleStyle {
                family: None,
                size: 0.015,
                fill: BLACK,
                stroke: WHITE,
            },
            ..look.clone()
        };
        let subtitle = fixture
            .default("subtitle", &subtitled, Some("CCWildWords"))
            .expect("a look");
        assert_eq!(
            subtitle.chars,
            CharStyle {
                family: Some("CCWildWords".to_owned()),
                size: Some(18.0),
                fill: Some(BLACK),
                stroke: Some(WHITE),
                stroke_width: Some(2.0),
                ..CharStyle::default()
            }
        );
        assert_eq!(fixture.default("balloon", &look, None), None);
    }

    #[test]
    fn the_patch_sets_every_look_key_and_the_border_method() {
        let look = Look {
            chars: CharStyle {
                family: Some("CCWildWords".to_owned()),
                size: Some(30.0),
                fill: Some(BLUE),
                ..CharStyle::default()
            },
            stroke_method: None,
        };
        let patch = reset_patch(&look);
        let keys: Vec<&str> = patch["chars"]
            .as_object()
            .expect("chars")
            .keys()
            .map(String::as_str)
            .collect();
        let mut expected = LOOK.to_vec();
        expected.sort_unstable();
        assert_eq!(keys, expected, "never the size");
        assert_eq!(patch["chars"]["weight"], Value::Null);
        assert_eq!(patch["chars"]["fill"], json!(BLUE));
        assert_eq!(patch["stroke_method"], Value::Null);
        assert_eq!(patch.as_object().map(|patch| patch.len()), Some(2));
    }
}
