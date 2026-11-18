//! The default look of new text, and the hand-edit record that tells what
//! Booharu's automatic steps wrote on a block from what was changed by hand.
//!
//! - New text gets a copy of the default font, the first of Koharu's
//!   Preferred fonts, when it has no family of its own (D8), so changing the
//!   setting later leaves finished pages alone. Its colours follow
//!   `[booharu-lettering]`: with `detect_colors` on, Booharu writes none, and
//!   a new translation keeps the colours Koharu measured; with it off, the
//!   default look's replace them. An existing translation is restyled only by
//!   Apply, in the `booharu` crate.
//! - Each automatic write records itself in the block's [`Auto`], in the
//!   same edit: the source wording the Recognition stage wrote, the
//!   translation the Translation stage wrote, and the style values a step
//!   left. [`edited`] compares a block with its record.

use std::collections::BTreeSet;

use anyhow::{Result, bail, ensure};
use koharu_scene::{CharStyle, Color, Content, Hash, Layer, Op, Rgba, Role};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::{
    attrs::{self, Auto, AutoStyle, Origin},
    settings::{DefaultTextStyle, LetteringSettings},
};

/// The `c2pa` namespace Koharu's stages record what they made in, by step.
const C2PA: &str = "c2pa";

/// The default font for new text: the first of Koharu's Preferred fonts,
/// `families` as koharu-renderer's `RendererConfig` reads `[renderer]
/// font_families`, its defaults included. None when the list names none, so
/// new text follows Koharu's font chain.
pub fn first_font(families: &[String]) -> Option<&str> {
    families
        .iter()
        .map(|family| family.trim())
        .find(|family| !family.is_empty())
}

/// The record's hash of `text`, trimmed: the first 8 bytes of its BLAKE3
/// hash as 16 hex digits, the same in every build and on every machine.
pub fn text_hash(text: &str) -> String {
    Hash::of(text.trim().as_bytes()).as_bytes()[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The default look as character values, for text with nothing measured to
/// go by, such as the T tool's: the default font, the default colour, and
/// the default outline, its width only with a colour.
pub fn default_chars(settings: &LetteringSettings, font: Option<&str>) -> CharStyle {
    let look = &settings.default;
    CharStyle {
        family: font.map(str::to_owned),
        fill: look.fill,
        stroke: look.stroke,
        stroke_width: look.stroke.map(|_| look.stroke_width),
        ..CharStyle::default()
    }
}

/// The patch on a new translation's `style.chars` giving it the default
/// look, whoever writes the translation: `placed` is its character values as
/// Koharu leaves them, placed or copied from its source, and `own` what its
/// style set before, which is kept, as placement keeps it.
///
/// It gets the default `font` when it has no family. With `detect_colors`
/// on, Koharu's colours stay. With it off, the default fill and outline
/// replace Koharu's, null where the look sets none, except that a sound
/// effect the look gives no outline gets Koharu's legibility stroke against
/// its new fill (T13).
pub fn new_translation(
    settings: &LetteringSettings,
    font: Option<&str>,
    sfx: bool,
    own: &CharStyle,
    placed: &CharStyle,
) -> Map<String, Value> {
    let mut patch = Map::new();
    if placed.family.is_none()
        && let Some(font) = font
    {
        patch.insert("family".to_owned(), json!(font));
    }
    if settings.detect_colors {
        return patch;
    }
    let look = &settings.default;
    if own.fill.is_none() {
        patch.insert("fill".to_owned(), json!(look.fill));
    }
    if own.stroke.is_none() {
        let (stroke, width) = outline(look, sfx, own.fill.or(look.fill), placed.size);
        patch.insert("stroke".to_owned(), json!(stroke));
        patch.insert("stroke_width".to_owned(), json!(width));
    }
    patch
}

/// The patch on a translation's `style.chars` giving it the default look
/// again, as Apply does: as a new translation would get it with nothing of
/// its own, the default font in place of its family, and with
/// `detect_colors` off the default colours in place of its own. With it on,
/// its colours stay, and with no default font, its family.
pub fn restyle(
    settings: &LetteringSettings,
    font: Option<&str>,
    sfx: bool,
    current: &CharStyle,
) -> Map<String, Value> {
    let placed = CharStyle {
        family: None,
        ..current.clone()
    };
    new_translation(settings, font, sfx, &CharStyle::default(), &placed)
}

/// The outline the default look gives text of `size` filled with `fill`:
/// its own, or for a sound effect Koharu's legibility stroke, as placement
/// gives one: a sixteenth of the size, black around a light fill and white
/// around any other, the renderer's black included. None without a size to
/// scale it by.
fn outline(
    look: &DefaultTextStyle,
    sfx: bool,
    fill: Option<Rgba>,
    size: Option<f64>,
) -> (Option<Rgba>, Option<f64>) {
    match (look.stroke, size) {
        (Some(stroke), _) => (Some(stroke), Some(look.stroke_width)),
        (None, Some(size)) if sfx => {
            let light = fill
                .is_some_and(|fill| Color::from(fill).discard_alpha().relative_luminance() > 0.5);
            let stroke = if light {
                [0.0, 0.0, 0.0, 1.0]
            } else {
                [1.0; 4]
            };
            (Some(Rgba(stroke)), Some(size / 16.0))
        }
        _ => (None, None),
    }
}

/// `chars` with `patch` merged in, as a merge patch on a text's
/// `style.chars` merges: a key set to null clears its value.
pub fn patched(chars: &CharStyle, patch: &Map<String, Value>) -> Result<CharStyle> {
    let Value::Object(mut merged) = serde_json::to_value(chars)? else {
        bail!("character values aren't an object");
    };
    for (key, value) in patch {
        match value {
            Value::Null => merged.remove(key),
            value => merged.insert(key.clone(), value.clone()),
        };
    }
    Ok(serde_json::from_value(Value::Object(merged))?)
}

/// What the Translation stage adds to its ops for `layer`, a block it gives
/// the translation `text` placed by `placed`, placement's op, both read
/// before the edit, as placement reads the project: after placement's op in
/// the same edit, the default look as a patch on the translation's style
/// when it writes anything, then the hand-edit record, written on every
/// translation, styled or not, so a block the run translated never counts as
/// typed by hand.
pub fn translated(
    layer: &Layer,
    text: &str,
    placed: &Op,
    settings: &LetteringSettings,
    font: Option<&str>,
) -> Result<Vec<Op>> {
    let looked = looked(layer, placed, settings, font)?;
    let mut record = Auto::of(layer);
    // A new translation's style owes nothing to one the block had before.
    if !looked.existed {
        record.style = None;
    }
    record.text = Some(text_hash(text));
    let record = record.restyled(&looked.after, &looked.written);
    let mut ops = Vec::from_iter(looked.op);
    ops.push(attrs::set(layer.id, &record)?);
    Ok(ops)
}

/// The op giving a translation typed by hand into `layer`, a block with
/// none, in the Story panel or through the agent, the default look, as
/// [`new_translation`] gives one, after `placed`, placement's op for it,
/// both read before the edit; none when it writes nothing. It gets no
/// record: it's hand-made.
pub fn typed(
    layer: &Layer,
    placed: &Op,
    settings: &LetteringSettings,
    font: Option<&str>,
) -> Result<Option<Op>> {
    Ok(looked(layer, placed, settings, font)?.op)
}

/// The op recording `text` as the source wording the Recognition stage
/// wrote on `layer`, a block read before the edit, in the stage's edit; the
/// record's other parts are kept.
pub fn recognized(layer: &Layer, text: &str) -> Result<Op> {
    let record = Auto {
        source: Some(text_hash(text)),
        ..Auto::of(layer)
    };
    attrs::set(layer.id, &record)
}

/// A new translation's look, as [`translated`] and [`typed`] write it.
struct Looked {
    /// Whether the block had a translation before.
    existed: bool,
    /// The op writing the look, when it writes anything.
    op: Option<Op>,
    /// The translation's character values once placed and given the look.
    after: CharStyle,
    /// The keys placement and the look wrote.
    written: BTreeSet<String>,
}

fn looked(
    layer: &Layer,
    placed: &Op,
    settings: &LetteringSettings,
    font: Option<&str>,
) -> Result<Looked> {
    let Content::Block(block) = &layer.content else {
        bail!("layer {} isn't a block", layer.id);
    };
    let placement = match placed {
        Op::UpdateLayer {
            layer: target,
            patch,
        } => {
            ensure!(
                target.uuid() == Some(layer.id.0),
                "placement's op is for layer {target}, not {}",
                layer.id
            );
            patch
                .pointer("/translation/style/chars")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default()
        }
        other => bail!("placement's op is {other:?}, not a patch"),
    };
    let own = block
        .translation
        .as_ref()
        .map(|translation| translation.style.chars.clone())
        .unwrap_or_default();
    let placed = patched(&own, &placement)?;
    let sfx = block.role == Role::Sfx;
    let look = new_translation(settings, font, sfx, &own, &placed);
    let after = patched(&placed, &look)?;
    let written = placement.keys().chain(look.keys()).cloned().collect();
    let op = (!look.is_empty()).then(|| Op::UpdateLayer {
        layer: layer.id.into(),
        patch: json!({"translation": {"style": {"chars": look}}}),
    });
    Ok(Looked {
        existed: block.translation.is_some(),
        op,
        after,
        written,
    })
}

impl Auto {
    /// `layer`'s record, or an empty one, which reads as having none.
    pub fn of(layer: &Layer) -> Self {
        attrs::get(layer).unwrap_or_default()
    }

    /// This record once an automatic step left the translation the
    /// character values `after`, writing the keys `written`: those take
    /// `after`'s values, and the others keep the ones recorded, so a value
    /// changed by hand stays a hand edit. With no style recorded yet, every
    /// key takes `after`'s.
    #[must_use]
    pub fn restyled(self, after: &CharStyle, written: &BTreeSet<String>) -> Self {
        let now = AutoStyle::of(after);
        let style = match self.style {
            None => now,
            Some(kept) => {
                let wrote = |key: &str| written.contains(key);
                AutoStyle {
                    family: if wrote("family") {
                        now.family
                    } else {
                        kept.family
                    },
                    weight: if wrote("weight") {
                        now.weight
                    } else {
                        kept.weight
                    },
                    italic: if wrote("italic") {
                        now.italic
                    } else {
                        kept.italic
                    },
                    fill: if wrote("fill") { now.fill } else { kept.fill },
                    stroke: if wrote("stroke") {
                        now.stroke
                    } else {
                        kept.stroke
                    },
                    stroke_width: if wrote("stroke_width") {
                        now.stroke_width
                    } else {
                        kept.stroke_width
                    },
                    outer_stroke: if wrote("outer_stroke") {
                        now.outer_stroke
                    } else {
                        kept.outer_stroke
                    },
                    outer_stroke_width: if wrote("outer_stroke_width") {
                        now.outer_stroke_width
                    } else {
                        kept.outer_stroke_width
                    },
                }
            }
        };
        Self {
            style: Some(style),
            ..self
        }
    }
}

impl AutoStyle {
    /// The recorded keys' values in `chars`.
    pub fn of(chars: &CharStyle) -> Self {
        Self {
            family: chars.family.clone(),
            weight: chars.weight,
            italic: chars.italic,
            fill: chars.fill,
            stroke: chars.stroke,
            stroke_width: chars.stroke_width,
            outer_stroke: chars.outer_stroke,
            outer_stroke_width: chars.outer_stroke_width,
        }
    }
}

/// Which parts of a block were changed by hand since Booharu's automatic
/// steps wrote them, as its [`Auto`] record and Koharu's `c2pa` records
/// tell. A part with nothing recorded counts as automatic only when a
/// Koharu stage recorded writing it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Edited {
    /// Its source wording isn't what the Recognition stage wrote, or no
    /// stage read the wording it has.
    pub source: bool,
    /// Its translation isn't what the Translation stage wrote, or no stage
    /// wrote it, as one typed by hand.
    pub text: bool,
    /// A style value of its translation isn't the one recorded, a span of
    /// it is marked with one, or the agent chose its style on purpose; with
    /// nothing recorded, no stage wrote the translation.
    pub style: bool,
    /// It was drawn by hand: no detection made it, as the O tool's boxes.
    pub drawn: bool,
}

impl Edited {
    /// Whether any part was changed by hand, so automatic steps leave the
    /// block alone.
    pub fn hand_edited(&self) -> bool {
        self.source || self.text || self.style || self.drawn
    }
}

/// Which parts of `layer` were changed by hand. A text layer was made by
/// hand, as the T tool makes one, and any other layer has no text to change.
pub fn edited(layer: &Layer) -> Edited {
    let block = match &layer.content {
        Content::Block(block) => block,
        Content::Text(_) => {
            return Edited {
                drawn: true,
                ..Edited::default()
            };
        }
        _ => return Edited::default(),
    };
    let recorded = |step: &str| {
        layer
            .attributes
            .get(C2PA)
            .is_some_and(|steps| steps.contains_key(step))
    };
    let record = Auto::of(layer);
    let wording = block.source.content.text();
    let source = match &record.source {
        Some(hash) => *hash != text_hash(&wording),
        None => !wording.trim().is_empty() && !recorded("ocr"),
    };
    let (text, style) = match &block.translation {
        None => (false, false),
        Some(translation) => {
            let text = match &record.text {
                Some(hash) => *hash != text_hash(&translation.content.text()),
                None => !recorded("translation"),
            };
            let marked = translation
                .content
                .0
                .iter()
                .any(|span| AutoStyle::of(&span.attributes) != AutoStyle::default());
            let differs = match &record.style {
                Some(style) => *style != AutoStyle::of(&translation.style.chars),
                None => !recorded("translation"),
            };
            (text, record.origin == Origin::Agent || marked || differs)
        }
    };
    Edited {
        source,
        text,
        style,
        drawn: !recorded("detection"),
    }
}

#[cfg(test)]
mod tests {
    use koharu_scene::{Created, Edit, LayerId, Session};

    use super::*;

    const RED: Rgba = Rgba([1.0, 0.0, 0.0, 1.0]);
    const BLUE: Rgba = Rgba([0.0, 0.0, 1.0, 1.0]);
    const BLACK: Rgba = Rgba([0.0, 0.0, 0.0, 1.0]);
    const WHITE: Rgba = Rgba([1.0; 4]);
    const YELLOW: Rgba = Rgba([1.0, 1.0, 0.0, 1.0]);

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
        }
    }

    /// Character values as Koharu's placement leaves a dialogue
    /// translation: the measured ink and outline, at the page's size.
    fn measured() -> CharStyle {
        CharStyle {
            size: Some(32.0),
            fill: Some(BLUE),
            stroke: Some(WHITE),
            stroke_width: Some(3.0),
            ..CharStyle::default()
        }
    }

    fn keys(patch: &Map<String, Value>) -> Vec<&str> {
        patch.keys().map(String::as_str).collect()
    }

    #[test]
    fn the_text_hash_is_stable_and_trimmed() {
        // BLAKE3's hash of nothing begins af1349b9f5f9a1a6.
        assert_eq!(text_hash(""), "af1349b9f5f9a1a6");
        assert_eq!(text_hash(" \n "), "af1349b9f5f9a1a6");
        assert_eq!(text_hash("Why?"), text_hash("  Why?\n"));
        assert_ne!(text_hash("Why?"), text_hash("Why!"));
        assert_eq!(text_hash("Why?"), "ee88547874fcd194");
    }

    #[test]
    fn the_first_preferred_font_is_the_default_font() {
        let families = [
            "  ".to_owned(),
            "CCWildWords ".to_owned(),
            "Arial".to_owned(),
        ];
        assert_eq!(first_font(&families), Some("CCWildWords"));
        assert_eq!(first_font(&[]), None);
    }

    #[test]
    fn a_new_translation_gets_the_font_only_without_one_of_its_own() {
        let detected = settings(true, Some(RED), None);
        let patch = new_translation(
            &detected,
            Some("CCWildWords"),
            false,
            &CharStyle::default(),
            &measured(),
        );
        assert_eq!(
            patch,
            Map::from_iter([("family".to_owned(), json!("CCWildWords"))])
        );
        // Its own family is kept, and with no Preferred font nothing is
        // written, so Koharu's font chain applies.
        let own = CharStyle {
            family: Some("Comic Neue".to_owned()),
            ..CharStyle::default()
        };
        let placed = patched(&own, &Map::new()).expect("values");
        assert!(new_translation(&detected, Some("CCWildWords"), false, &own, &placed).is_empty());
        assert!(
            new_translation(&detected, None, false, &CharStyle::default(), &measured()).is_empty()
        );
    }

    #[test]
    fn detection_on_keeps_the_measured_colours_and_off_replaces_them() {
        let none = CharStyle::default();
        // On: nothing but the font, whatever the default look holds.
        let on = settings(true, Some(RED), Some(BLACK));
        assert!(new_translation(&on, None, false, &none, &measured()).is_empty());

        // Off: the default colours and outline replace the measured ones.
        let off = settings(false, Some(RED), Some(BLACK));
        let patch = new_translation(&off, None, false, &none, &measured());
        let after = patched(&measured(), &patch).expect("values");
        assert_eq!(
            (after.fill, after.stroke, after.stroke_width, after.size),
            (Some(RED), Some(BLACK), Some(2.0), Some(32.0))
        );

        // Null where the look sets none: Koharu's black, and no outline.
        let bare = settings(false, None, None);
        let patch = new_translation(&bare, None, false, &none, &measured());
        assert_eq!(keys(&patch), ["fill", "stroke", "stroke_width"]);
        let after = patched(&measured(), &patch).expect("values");
        assert_eq!(
            (after.fill, after.stroke, after.stroke_width),
            (None, None, None)
        );

        // What the translation's own style set is kept.
        let own = CharStyle {
            fill: Some(YELLOW),
            ..CharStyle::default()
        };
        let placed = CharStyle {
            fill: Some(YELLOW),
            ..measured()
        };
        let patch = new_translation(&off, None, false, &own, &placed);
        assert_eq!(keys(&patch), ["stroke", "stroke_width"]);
    }

    #[test]
    fn a_sound_effect_keeps_a_legibility_outline_without_a_default_one() {
        let none = CharStyle::default();
        // A light default fill gets a black stroke a sixteenth of its size.
        let light = settings(false, Some(YELLOW), None);
        let after = patched(
            &measured(),
            &new_translation(&light, None, true, &none, &measured()),
        )
        .expect("values");
        assert_eq!(
            (after.fill, after.stroke, after.stroke_width),
            (Some(YELLOW), Some(BLACK), Some(2.0))
        );
        // The renderer's black, with no default fill, gets a white one.
        let unset = settings(false, None, None);
        let after = patched(
            &measured(),
            &new_translation(&unset, None, true, &none, &measured()),
        )
        .expect("values");
        assert_eq!(
            (after.fill, after.stroke, after.stroke_width),
            (None, Some(WHITE), Some(2.0))
        );
        // A default outline is the effect's outline too.
        let outlined = settings(false, Some(YELLOW), Some(RED));
        let after = patched(
            &measured(),
            &new_translation(&outlined, None, true, &none, &measured()),
        )
        .expect("values");
        assert_eq!((after.stroke, after.stroke_width), (Some(RED), Some(2.0)));
        // Dialogue gets no legibility outline.
        let after = patched(
            &measured(),
            &new_translation(&light, None, false, &none, &measured()),
        )
        .expect("values");
        assert_eq!((after.stroke, after.stroke_width), (None, None));
    }

    #[test]
    fn t_tool_text_gets_the_whole_default_look() {
        let look = settings(true, Some(RED), None);
        assert_eq!(
            default_chars(&look, Some("CCWildWords")),
            CharStyle {
                family: Some("CCWildWords".to_owned()),
                fill: Some(RED),
                ..CharStyle::default()
            }
        );
        let outlined = settings(true, None, Some(WHITE));
        let chars = default_chars(&outlined, None);
        assert_eq!(
            (chars.family, chars.fill, chars.stroke, chars.stroke_width),
            (None, None, Some(WHITE), Some(2.0))
        );
    }

    #[test]
    fn apply_restyles_over_what_the_translation_has() {
        let current = CharStyle {
            family: Some("Comic Neue".to_owned()),
            ..measured()
        };
        // On: only the font, in place of its own.
        let on = settings(true, Some(RED), None);
        let patch = restyle(&on, Some("CCWildWords"), false, &current);
        assert_eq!(keys(&patch), ["family"]);
        assert!(restyle(&on, None, false, &current).is_empty());
        // Off: the default colours too.
        let off = settings(false, Some(RED), None);
        let after = patched(
            &current,
            &restyle(&off, Some("CCWildWords"), false, &current),
        )
        .expect("values");
        assert_eq!(
            (
                after.family.as_deref(),
                after.fill,
                after.stroke,
                after.stroke_width
            ),
            (Some("CCWildWords"), Some(RED), None, None)
        );
    }

    /// A page holding one block whose source reads `source`, with the
    /// `c2pa` records `steps` of the Koharu stages that made it.
    fn page(source: &str, steps: &[&str]) -> (tempfile::TempDir, Session, LayerId) {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        let c2pa: Map<String, Value> = steps
            .iter()
            .map(|step| ((*step).to_owned(), json!({"action": "c2pa.created"})))
            .collect();
        let mut block = json!({"kind": "block", "source": {
            "frame": {"box": {"bounds": {"x": 10.0, "y": 10.0, "width": 40.0, "height": 120.0}}},
            "content": source,
        }});
        if !c2pa.is_empty() {
            block["attributes"] = json!({C2PA: c2pa});
        }
        let ops = [
            json!({"op": "add_page", "id": "$p", "index": 0, "size": {"width": 200, "height": 200}}),
            json!({"op": "add_layer", "id": "$b", "page": "$p", "index": 0, "layer": block}),
        ];
        let change = apply(
            &mut session,
            ops.into_iter()
                .map(|op| serde_json::from_value(op).expect("an op"))
                .collect(),
        );
        let id = change.created.iter().find_map(|created| match created {
            Created::Layer {
                id,
                label: Some(label),
            } if label == "b" => Some(*id),
            _ => None,
        });
        (dir, session, id.expect("the block"))
    }

    fn apply(session: &mut Session, ops: Vec<Op>) -> koharu_scene::Change {
        session
            .apply(Edit {
                label: "test".to_owned(),
                ops,
            })
            .expect("the edit")
    }

    fn layer(session: &Session, id: LayerId) -> Layer {
        session.project().layer(id).expect("the block").clone()
    }

    /// Placement's op for `block`'s translation, as Koharu's places a
    /// dialogue one: the measured colours at the page's size, where its
    /// style sets none.
    fn placement(session: &Session, block: LayerId) -> Op {
        let own = match &layer(session, block).content {
            Content::Block(found) => found
                .translation
                .as_ref()
                .map(|translation| translation.style.chars.clone())
                .unwrap_or_default(),
            _ => panic!("not a block"),
        };
        let mut chars = Map::new();
        if own.size.is_none() {
            chars.insert("size".to_owned(), json!(32.0));
        }
        if own.fill.is_none() {
            chars.insert("fill".to_owned(), json!(BLUE));
        }
        if own.stroke.is_none() {
            chars.insert("stroke".to_owned(), json!(WHITE));
            chars.insert("stroke_width".to_owned(), json!(3.0));
        }
        Op::UpdateLayer {
            layer: block.into(),
            patch: json!({"translation": {"style": {"chars": chars}}}),
        }
    }

    /// Translates `block` as the Translation stage does, with its Booharu
    /// hook, under `look` and `font`.
    fn translate(
        session: &mut Session,
        block: LayerId,
        text: &str,
        look: &LetteringSettings,
        font: Option<&str>,
    ) {
        let before = layer(session, block);
        let placed = placement(session, block);
        let mut ops = vec![
            Op::SetText {
                layer: block.into(),
                text: text.to_owned(),
            },
            placed.clone(),
        ];
        ops.extend(translated(&before, text, &placed, look, font).expect("the hook's ops"));
        ops.push(Op::SetAttribute {
            layer: block.into(),
            namespace: C2PA.to_owned(),
            key: "translation".to_owned(),
            value: Some(json!({"action": "c2pa.translated"})),
        });
        apply(session, ops);
    }

    fn translation(session: &Session, block: LayerId) -> koharu_scene::Text {
        match &layer(session, block).content {
            Content::Block(found) => found.translation.clone().expect("a translation"),
            _ => panic!("not a block"),
        }
    }

    #[test]
    fn a_translated_block_is_automatic_until_changed_by_hand() {
        let (_dir, mut session, block) = page("どうして", &["detection", "ocr"]);
        let look = settings(false, Some(RED), None);
        translate(&mut session, block, "Why?", &look, Some("CCWildWords"));

        let written = translation(&session, block);
        assert_eq!(written.style.chars.family.as_deref(), Some("CCWildWords"));
        assert_eq!(
            (written.style.chars.fill, written.style.chars.stroke),
            (Some(RED), None)
        );
        let found = layer(&session, block);
        assert_eq!(edited(&found), Edited::default());
        let record = Auto::of(&found);
        assert_eq!(record.text.as_deref(), Some(text_hash("Why?").as_str()));
        assert_eq!(
            record.style.as_ref().and_then(|style| style.fill),
            Some(RED)
        );

        // Retyped: the text was changed by hand.
        apply(
            &mut session,
            vec![Op::SetText {
                layer: block.into(),
                text: "Why...?".to_owned(),
            }],
        );
        let retyped = edited(&layer(&session, block));
        assert_eq!(
            (retyped.text, retyped.style, retyped.hand_edited()),
            (true, false, true)
        );
    }

    #[test]
    fn a_style_key_or_a_mark_changed_by_hand_is_a_style_edit() {
        let (_dir, mut session, block) = page("どうして", &["detection", "ocr"]);
        let look = settings(true, None, None);
        translate(&mut session, block, "Why?", &look, Some("CCWildWords"));
        assert!(!edited(&layer(&session, block)).hand_edited());

        // A style key set by hand.
        apply(
            &mut session,
            vec![Op::UpdateLayer {
                layer: block.into(),
                patch: json!({"translation": {"style": {"chars": {"weight": 700}}}}),
            }],
        );
        let restyled = edited(&layer(&session, block));
        assert_eq!((restyled.text, restyled.style), (false, true));

        // Set back, then a word marked with a colour.
        apply(
            &mut session,
            vec![Op::UpdateLayer {
                layer: block.into(),
                patch: json!({"translation": {"style": {"chars": {"weight": null}}}}),
            }],
        );
        assert!(!edited(&layer(&session, block)).style);
        let spans = vec![
            koharu_scene::Span {
                insert: "Why".to_owned(),
                attributes: CharStyle {
                    fill: Some(RED),
                    ..CharStyle::default()
                },
            },
            koharu_scene::Span {
                insert: "?".to_owned(),
                attributes: CharStyle::default(),
            },
        ];
        apply(
            &mut session,
            vec![Op::SetRichText {
                layer: block.into(),
                spans,
            }],
        );
        let marked = edited(&layer(&session, block));
        assert_eq!((marked.text, marked.style), (false, true));
    }

    #[test]
    fn without_a_record_koharus_c2pa_records_tell_automatic_from_typed() {
        // Read and translated by Koharu's stages with no Booharu record, as
        // before the hooks: automatic.
        let (_dir, mut session, block) = page("どうして", &["detection", "ocr", "translation"]);
        apply(
            &mut session,
            vec![Op::SetText {
                layer: block.into(),
                text: "Why?".to_owned(),
            }],
        );
        assert_eq!(edited(&layer(&session, block)), Edited::default());

        // Read by Koharu, but the translation typed by hand: no stage wrote
        // it, so its text and style count as hand-made.
        let (_dir, mut session, block) = page("どうして", &["detection", "ocr"]);
        apply(
            &mut session,
            vec![Op::SetText {
                layer: block.into(),
                text: "Why?".to_owned(),
            }],
        );
        let typed = edited(&layer(&session, block));
        assert_eq!(
            (typed.source, typed.text, typed.style, typed.drawn),
            (false, true, true, false)
        );

        // A source typed into a box drawn by hand, which no stage read.
        let (_dir, session, block) = page("どうして", &[]);
        let drawn = edited(&layer(&session, block));
        assert_eq!(
            (drawn.source, drawn.text, drawn.style, drawn.drawn),
            (true, false, false, true)
        );
        // A box drawn by hand and not yet read has no wording to protect,
        // but it's still the user's.
        let (_dir, session, block) = page("", &[]);
        let empty = edited(&layer(&session, block));
        assert_eq!((empty.source, empty.drawn), (false, true));
    }

    #[test]
    fn the_recognition_record_tells_a_corrected_source() {
        let (_dir, mut session, block) = page("どうして", &["detection", "ocr"]);
        let read = recognized(&layer(&session, block), "どうして").expect("the record");
        apply(&mut session, vec![read]);
        assert!(!edited(&layer(&session, block)).source);
        apply(
            &mut session,
            vec![Op::UpdateLayer {
                layer: block.into(),
                patch: json!({"source": {"content": "どうしてよ"}}),
            }],
        );
        assert!(edited(&layer(&session, block)).source);

        // Reading again records the new wording and keeps the rest.
        let look = settings(true, None, None);
        translate(&mut session, block, "Why?", &look, None);
        let read = recognized(&layer(&session, block), "どうしてよ").expect("the record");
        apply(&mut session, vec![read]);
        let found = layer(&session, block);
        assert_eq!(edited(&found), Edited::default());
        assert_eq!(
            Auto::of(&found).text.as_deref(),
            Some(text_hash("Why?").as_str())
        );
    }

    #[test]
    fn translating_again_keeps_a_colour_changed_by_hand_a_hand_edit() {
        let (_dir, mut session, block) = page("どうして", &["detection", "ocr"]);
        let look = settings(false, Some(RED), None);
        translate(&mut session, block, "Why?", &look, Some("CCWildWords"));
        apply(
            &mut session,
            vec![Op::UpdateLayer {
                layer: block.into(),
                patch: json!({"translation": {"style": {"chars": {"fill": YELLOW}}}}),
            }],
        );
        assert!(edited(&layer(&session, block)).style);

        // Placement keeps the yellow, and so does the look, and the record
        // still holds the red it wrote, so the yellow stays the user's.
        translate(&mut session, block, "Why!?", &look, Some("CCWildWords"));
        let found = layer(&session, block);
        assert_eq!(translation(&session, block).style.chars.fill, Some(YELLOW));
        let after = edited(&found);
        assert_eq!((after.text, after.style), (false, true));
        assert_eq!(
            Auto::of(&found).style.and_then(|style| style.fill),
            Some(RED)
        );
    }

    #[test]
    fn a_style_the_agent_chose_counts_as_a_hand_edit() {
        let (_dir, mut session, block) = page("どうして", &["detection", "ocr"]);
        translate(
            &mut session,
            block,
            "Why?",
            &settings(true, None, None),
            None,
        );
        let found = layer(&session, block);
        let chose = Auto {
            origin: Origin::Agent,
            ..Auto::of(&found)
        };
        apply(
            &mut session,
            vec![attrs::set(block, &chose).expect("the record")],
        );
        let agent = edited(&layer(&session, block));
        assert_eq!((agent.text, agent.style), (false, true));
    }

    #[test]
    fn a_text_layer_is_made_by_hand() {
        let (_dir, session, block) = page("", &["detection"]);
        let text = Layer {
            content: Content::Text(Box::default()),
            ..layer(&session, block)
        };
        assert_eq!(
            edited(&text),
            Edited {
                drawn: true,
                ..Edited::default()
            }
        );
    }

    #[test]
    fn a_typed_translation_gets_the_look_and_no_record() {
        let (_dir, session, block) = page("どうして", &["detection", "ocr"]);
        let found = layer(&session, block);
        let look = settings(false, Some(RED), None);
        let op = typed(
            &found,
            &placement(&session, block),
            &look,
            Some("CCWildWords"),
        )
        .expect("the look")
        .expect("an op");
        let Op::UpdateLayer { patch, .. } = op else {
            panic!("{op:?}");
        };
        assert_eq!(
            patch,
            json!({"translation": {"style": {"chars": {
                "family": "CCWildWords", "fill": RED, "stroke": null, "stroke_width": null,
            }}}})
        );
        let on = settings(true, None, None);
        assert!(
            typed(&found, &placement(&session, block), &on, None)
                .expect("the look")
                .is_none()
        );
        // Placement's op for another layer is refused.
        let other = Op::UpdateLayer {
            layer: LayerId(koharu_scene::Uuid::now_v7()).into(),
            patch: json!({}),
        };
        assert!(typed(&found, &other, &look, None).is_err());
    }
}
