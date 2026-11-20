//! The default look of new text, and the hand-edit record that tells what
//! Booharu's automatic steps wrote on a block from what was changed by hand.
//!
//! - New text gets a copy of the default font, the first of Koharu's
//!   Preferred fonts, when it has no family of its own (D8), so changing the
//!   setting later leaves finished pages alone. Its colours follow
//!   `[booharu-lettering]`: with `detect_colors` on, Booharu writes none, and
//!   a new translation keeps the colours Koharu measured; with it off, the
//!   default look's replace them. An existing translation is restyled only by
//!   Apply and by a change of category, in the `booharu` crate.
//! - While Pick fonts automatically is on, a new translation's font is its
//!   block's lettering category's in place of the default font: its family,
//!   weight, and italic, as the `lettering` module resolves it. Off, nothing
//!   here writes a category font.
//! - Each automatic write records itself in the block's [`Auto`], in the
//!   same edit: the source wording the Recognition stage wrote, the
//!   translation the Translation stage wrote, and the style values a step
//!   left. [`edited`] compares a block with its record.

use std::collections::BTreeSet;

use anyhow::{Result, bail, ensure};
use koharu_scene::{CharStyle, Content, Layer, LayerId, Op, Project, Rgba, Role};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::{
    attrs::{self, Auto, AutoStyle, Origin},
    lettering::{self, CategoryFont, LetteringCategory},
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
    crate::qc::short_hash(text.trim().as_bytes())
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
/// It gets the default `font` when it has no family. With `category`, its
/// block's lettering category while Pick fonts automatically is on, it gets
/// that category's font instead ([`category_chars`]), its weight and italic
/// where it has none of its own; none, as with the switch off, writes no
/// category font. With `detect_colors` on, Koharu's colours stay. With it
/// off, the default fill and outline replace Koharu's, null where the look
/// sets none, except that a sound effect the look gives no outline gets
/// Koharu's legibility stroke against its new fill (T13).
pub fn new_translation(
    settings: &LetteringSettings,
    font: Option<&str>,
    sfx: bool,
    category: Option<LetteringCategory>,
    own: &CharStyle,
    placed: &CharStyle,
) -> Map<String, Value> {
    let mut patch = Map::new();
    // A translation keeping a family of its own keeps its whole font.
    if placed.family.is_none() {
        match category {
            Some(category) => {
                let kept = |key: &str| match key {
                    "weight" => placed.weight.is_some(),
                    "italic" => placed.italic.is_some(),
                    _ => false,
                };
                let chars = category_chars(settings, font, category);
                patch.extend(chars.into_iter().filter(|(key, _)| !kept(key)));
            }
            None => {
                if let Some(font) = font {
                    patch.insert("family".to_owned(), json!(font));
                }
            }
        }
    }
    if settings.detect_colors {
        return patch;
    }
    let look = &settings.default;
    if own.fill.is_none() {
        patch.insert("fill".to_owned(), json!(look.fill));
    }
    if own.stroke.is_none() {
        let (stroke, width) = outline(look, sfx, own.fill.or(look.fill), placed);
        patch.insert("stroke".to_owned(), json!(stroke));
        patch.insert("stroke_width".to_owned(), json!(width));
    }
    patch
}

/// The patch on a translation's `style.chars` giving it the default look
/// again, as Apply does: as a new translation would get it with nothing of
/// its own, the default font in place of its family, or with `category`,
/// its block's lettering category while Pick fonts automatically is on, that
/// category's font in place of its family, weight, and italic, and with
/// `detect_colors` off the default colours in place of its own. With it on,
/// its colours stay, and with no default font and no category, its family.
pub fn restyle(
    settings: &LetteringSettings,
    font: Option<&str>,
    sfx: bool,
    category: Option<LetteringCategory>,
    current: &CharStyle,
) -> Map<String, Value> {
    let placed = CharStyle {
        family: None,
        weight: None,
        italic: None,
        ..current.clone()
    };
    new_translation(
        settings,
        font,
        sfx,
        category,
        &CharStyle::default(),
        &placed,
    )
}

/// The patch on a translation's `style.chars` lettering it in `category`'s
/// font, as the `lettering` module resolves it with `font`, the default
/// font: its `family`, null without one, so a family the translation had
/// goes, its `weight`, 700 or null, and its `italic`, true or null. A
/// category with no font, nor Normal Dialogue's, gets a copy of the default
/// font, neither bold nor italic. Never other keys, and never the marks, so
/// word styles survive a change of category.
pub fn category_chars(
    settings: &LetteringSettings,
    font: Option<&str>,
    category: LetteringCategory,
) -> Map<String, Value> {
    settings
        .category_font(category, font)
        .unwrap_or_else(|| CategoryFont {
            family: font.map(str::to_owned),
            ..CategoryFont::default()
        })
        .chars()
}

/// The outline the default look gives text with the character values
/// `placed`, filled with `fill`: its own, or for a sound effect Koharu's
/// legibility stroke, as placement gives one, black around a light fill and
/// white around any other, the renderer's black included, a sixteenth of its
/// size, or without a size as wide as the outline Koharu gave it. None
/// without either.
fn outline(
    look: &DefaultTextStyle,
    sfx: bool,
    fill: Option<Rgba>,
    placed: &CharStyle,
) -> (Option<Rgba>, Option<f64>) {
    if let Some(stroke) = look.stroke {
        return (Some(stroke), Some(look.stroke_width));
    }
    let width = placed
        .size
        .map(|size| size / 16.0)
        .or(placed.stroke.and(placed.stroke_width));
    match width {
        Some(width) if sfx => (Some(crate::resolver::contrasting(fill)), Some(width)),
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

/// What the Translation stage adds to its ops for `block` in `project`, a
/// block it gives the translation `text` placed by `placed`, placement's op,
/// both read before the edit, as placement reads the project: after
/// placement's op in the same edit, the default look as a patch on the
/// translation's style when it writes anything, its category's font while
/// Pick fonts automatically is on, then the hand-edit record, written on
/// every translation, styled or not, so a translation the run wrote never
/// counts as typed by hand, while the style values one typed by hand had,
/// and the run left, stay hand edits. The values the look writes, a
/// category's family, weight, and italic among them, are recorded as
/// automatic. The record drops the kept mark.
pub fn translated(
    project: &Project,
    block: LayerId,
    text: &str,
    placed: &Op,
    settings: &LetteringSettings,
    font: Option<&str>,
) -> Result<Vec<Op>> {
    let layer = project.layer(block)?;
    let looked = looked(project, block, placed, settings, font)?;
    let mut record = Auto::of(layer);
    // A new translation's style owes nothing to one the block had before,
    // nor to whoever chose that one's. One no stage wrote was made by hand,
    // so with nothing recorded its values stay hand edits, but for those
    // this step writes.
    if !looked.existed {
        record.style = None;
        record.origin = Origin::Automatic;
    } else if record.style.is_none() && !recorded(layer, "translation") {
        record.style = Some(AutoStyle::default());
    }
    record.text = Some(text_hash(text));
    // A translation drops the kept mark: its clean-up shows under it.
    record.kept = false;
    let record = record.restyled(&looked.after, &looked.written);
    let mut ops = Vec::from_iter(looked.op);
    ops.push(attrs::set(layer.id, &record)?);
    Ok(ops)
}

/// The ops giving a translation typed by hand into `block` in `project`, a
/// block with none, in the Story panel or through the agent, the default
/// look, its category's font among it while Pick fonts automatically is on,
/// as [`new_translation`] gives one, after `placed`, placement's op for it,
/// both read before the edit; none when they write nothing. It gets no
/// record of its own: it's hand-made. A block carrying the kept mark has the
/// record written without it, as the translation shows its clean-up: the
/// record's part of [`crate::sfx::shown_again`].
pub fn typed(
    project: &Project,
    block: LayerId,
    placed: &Op,
    settings: &LetteringSettings,
    font: Option<&str>,
) -> Result<Vec<Op>> {
    let layer = project.layer(block)?;
    let mut ops = Vec::from_iter(looked(project, block, placed, settings, font)?.op);
    ops.extend(crate::sfx::unmarked(layer)?);
    Ok(ops)
}

/// The op recording on `layer`, a block read before the edit, that the
/// Translation stage was given its source back unchanged, so it wrote no
/// translation, in the stage's edit: the record's translation is no text,
/// so one typed there later counts as typed by hand, even as the wording
/// the stage was given, and the record keeps no style or origin, which went
/// with any translation the block had, nor the kept mark, as the stage's
/// own keeping replaces Booharu's. Its source part is kept.
pub fn given_back(layer: &Layer) -> Result<Op> {
    let record = Auto {
        origin: Origin::Automatic,
        text: Some(text_hash("")),
        style: None,
        kept: false,
        ..Auto::of(layer)
    };
    attrs::set(layer.id, &record)
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
    project: &Project,
    id: LayerId,
    placed: &Op,
    settings: &LetteringSettings,
    font: Option<&str>,
) -> Result<Looked> {
    let layer = project.layer(id)?;
    let Content::Block(block) = &layer.content else {
        bail!("layer {} isn't a block", layer.id);
    };
    let category = lettering::auto_category(project.parent(id)?.0, id, settings);
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
    let look = new_translation(settings, font, sfx, category, &own, &placed);
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
    /// A split's halves keep the detection record they were copied with, so
    /// they aren't: a split changes neither their wording nor their look,
    /// and only Detect again keeps them, by their record's [`Auto::split`].
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
    let recorded = |step: &str| recorded(layer, step);
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

/// Whether a Koharu stage recorded in `layer`'s `c2pa` attributes making
/// its part `step`: `detection`, `ocr`, `inpainting`, or `translation`.
pub fn recorded(layer: &Layer, step: &str) -> bool {
    layer
        .attributes
        .get(C2PA)
        .is_some_and(|steps| steps.contains_key(step))
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
            ..LetteringSettings::default()
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
            None,
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
        assert!(
            new_translation(&detected, Some("CCWildWords"), false, None, &own, &placed).is_empty()
        );
        assert!(
            new_translation(
                &detected,
                None,
                false,
                None,
                &CharStyle::default(),
                &measured()
            )
            .is_empty()
        );
    }

    #[test]
    fn detection_on_keeps_the_measured_colours_and_off_replaces_them() {
        let none = CharStyle::default();
        // On: nothing but the font, whatever the default look holds.
        let on = settings(true, Some(RED), Some(BLACK));
        assert!(new_translation(&on, None, false, None, &none, &measured()).is_empty());

        // Off: the default colours and outline replace the measured ones.
        let off = settings(false, Some(RED), Some(BLACK));
        let patch = new_translation(&off, None, false, None, &none, &measured());
        let after = patched(&measured(), &patch).expect("values");
        assert_eq!(
            (after.fill, after.stroke, after.stroke_width, after.size),
            (Some(RED), Some(BLACK), Some(2.0), Some(32.0))
        );

        // Null where the look sets none: Koharu's black, and no outline.
        let bare = settings(false, None, None);
        let patch = new_translation(&bare, None, false, None, &none, &measured());
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
        let patch = new_translation(&off, None, false, None, &own, &placed);
        assert_eq!(keys(&patch), ["stroke", "stroke_width"]);
    }

    #[test]
    fn a_sound_effect_keeps_a_legibility_outline_without_a_default_one() {
        let none = CharStyle::default();
        // A light default fill gets a black stroke a sixteenth of its size.
        let light = settings(false, Some(YELLOW), None);
        let after = patched(
            &measured(),
            &new_translation(&light, None, true, None, &none, &measured()),
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
            &new_translation(&unset, None, true, None, &none, &measured()),
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
            &new_translation(&outlined, None, true, None, &none, &measured()),
        )
        .expect("values");
        assert_eq!((after.stroke, after.stroke_width), (Some(RED), Some(2.0)));
        // Dialogue gets no legibility outline.
        let after = patched(
            &measured(),
            &new_translation(&light, None, false, None, &none, &measured()),
        )
        .expect("values");
        assert_eq!((after.stroke, after.stroke_width), (None, None));
        // Without a size, an effect's outline keeps the width Koharu gave
        // it, and with neither it gets none, as placement gives none.
        let sizeless = CharStyle {
            size: None,
            ..measured()
        };
        let after = patched(
            &sizeless,
            &new_translation(&light, None, true, None, &none, &sizeless),
        )
        .expect("values");
        assert_eq!((after.stroke, after.stroke_width), (Some(BLACK), Some(3.0)));
        let bare = CharStyle {
            fill: Some(BLUE),
            ..CharStyle::default()
        };
        let after = patched(
            &bare,
            &new_translation(&light, None, true, None, &none, &bare),
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
        let patch = restyle(&on, Some("CCWildWords"), false, None, &current);
        assert_eq!(keys(&patch), ["family"]);
        assert!(restyle(&on, None, false, None, &current).is_empty());
        // Off: the default colours too.
        let off = settings(false, Some(RED), None);
        let after = patched(
            &current,
            &restyle(&off, Some("CCWildWords"), false, None, &current),
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

    /// A page holding one block whose source reads `source`, lettered in
    /// blue outlined 3 pixels wide in white at size 32, with the `c2pa`
    /// records `steps` of the Koharu stages that made it.
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
            "style": {"chars": {"size": 32.0, "fill": BLUE, "stroke": WHITE, "stroke_width": 3.0}},
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

    /// Placement's op for `block`'s translation's characters, as Koharu's
    /// places one (`resolver::koharu_chars`): the measured colours at the
    /// page's size, where its style sets none.
    fn placement(session: &Session, block: LayerId) -> Op {
        let project = session.project();
        let (page, _) = project.parent(block).expect("the page");
        let Content::Block(found) = &project.layer(block).expect("the block").content else {
            panic!("not a block");
        };
        let own = found
            .translation
            .as_ref()
            .map(|translation| translation.style.chars.clone())
            .unwrap_or_default();
        let koharu = crate::resolver::koharu_chars(page, found, own.size);
        let mut chars = Map::new();
        if own.size.is_none()
            && let Some(size) = koharu.size
        {
            chars.insert("size".to_owned(), json!(size));
        }
        if own.fill.is_none()
            && let Some(fill) = koharu.fill
        {
            chars.insert("fill".to_owned(), json!(fill));
        }
        if own.stroke.is_none()
            && let (Some(stroke), Some(width)) = (koharu.stroke, koharu.stroke_width)
        {
            chars.insert("stroke".to_owned(), json!(stroke));
            chars.insert("stroke_width".to_owned(), json!(width));
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
        let project = session.project();
        let placed = placement(session, block);
        let mut ops = vec![
            Op::SetText {
                layer: block.into(),
                text: text.to_owned(),
            },
            placed.clone(),
        ];
        ops.extend(translated(&project, block, text, &placed, look, font).expect("the hook's ops"));
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
    fn a_split_half_isnt_drawn_by_hand() {
        // A split's half keeps the detection record it was copied with, and
        // its record says it's a half of a block split by hand, which
        // changed neither its wording nor its look.
        let (_dir, mut session, block) = page("", &["detection"]);
        let half = Auto {
            split: true,
            ..Auto::default()
        };
        apply(
            &mut session,
            vec![attrs::set(block, &half).expect("the record")],
        );
        assert_eq!(edited(&layer(&session, block)), Edited::default());

        // Read by the stages afterwards, it keeps the mark, and still
        // nothing about it counts as changed by hand.
        let read = recognized(&layer(&session, block), "").expect("the record");
        apply(&mut session, vec![read]);
        let found = layer(&session, block);
        assert!(Auto::of(&found).split);
        assert_eq!(edited(&found), Edited::default());
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
    fn a_translation_typed_by_hand_keeps_its_own_values_hand_edits_once_translated() {
        let (_dir, mut session, block) = page("どうして", &["detection", "ocr"]);
        apply(
            &mut session,
            vec![
                Op::SetText {
                    layer: block.into(),
                    text: "Hm".to_owned(),
                },
                Op::UpdateLayer {
                    layer: block.into(),
                    patch: json!({"translation": {"style": {"chars": {"family": "Comic Neue"}}}}),
                },
            ],
        );
        let look = settings(false, Some(RED), None);
        translate(&mut session, block, "Why?", &look, Some("CCWildWords"));

        // The record holds the colours this step wrote; the family it left
        // is still the user's.
        let found = layer(&session, block);
        let style = Auto::of(&found).style.expect("a style");
        assert_eq!((style.family, style.fill), (None, Some(RED)));
        let after = edited(&found);
        assert_eq!((after.text, after.style), (false, true));
        assert_eq!(
            translation(&session, block).style.chars.family.as_deref(),
            Some("Comic Neue")
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

        // Its translation given back unchanged and so cleared, the agent's
        // choice goes with it, and the record holds no translation.
        let read = recognized(&layer(&session, block), "どうして").expect("the record");
        apply(&mut session, vec![read]);
        let given = given_back(&layer(&session, block)).expect("the record");
        apply(
            &mut session,
            vec![
                Op::UpdateLayer {
                    layer: block.into(),
                    patch: json!({"translation": null}),
                },
                given,
            ],
        );
        let found = layer(&session, block);
        assert_eq!(
            Auto::of(&found),
            Auto {
                source: Some(text_hash("どうして")),
                text: Some(text_hash("")),
                ..Auto::default()
            }
        );
        assert_eq!(edited(&found), Edited::default());

        // A translation typed there afterwards is the user's, even as the
        // wording the stage was given back.
        apply(
            &mut session,
            vec![Op::SetText {
                layer: block.into(),
                text: "どうして".to_owned(),
            }],
        );
        assert!(edited(&layer(&session, block)).text);
        apply(
            &mut session,
            vec![Op::UpdateLayer {
                layer: block.into(),
                patch: json!({"translation": null}),
            }],
        );

        // A new translation the stage writes is Booharu's again.
        translate(
            &mut session,
            block,
            "Why?",
            &settings(true, None, None),
            None,
        );
        let found = layer(&session, block);
        assert_eq!(Auto::of(&found).origin, Origin::Automatic);
        assert_eq!(edited(&found), Edited::default());
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

    /// Hides `block`'s clean-up and marks it kept, as Booharu keeps a sound
    /// effect's art.
    fn keep(session: &mut Session, block: LayerId) {
        let record = Auto {
            kept: true,
            ..Auto::of(&layer(session, block))
        };
        apply(
            session,
            vec![
                Op::UpdateLayer {
                    layer: block.into(),
                    patch: json!({"cleanup": {"visible": false}}),
                },
                attrs::set(block, &record).expect("the mark"),
            ],
        );
        assert!(Auto::of(&layer(session, block)).kept);
    }

    #[test]
    fn a_translation_or_a_line_given_back_drops_the_kept_mark() {
        let look = settings(true, None, None);
        // Translated by a stage.
        let (_dir, mut session, block) = page("ドン", &["detection", "ocr"]);
        keep(&mut session, block);
        translate(&mut session, block, "BOOM", &look, None);
        assert!(!Auto::of(&layer(&session, block)).kept);

        // Typed by hand: no record of its own, so a record holding nothing
        // but the mark goes, and one holding more keeps the rest.
        let (_dir, mut session, block) = page("ドン", &["detection", "ocr"]);
        keep(&mut session, block);
        let ops = typed(
            &session.project(),
            block,
            &placement(&session, block),
            &look,
            None,
        )
        .expect("the ops");
        assert_eq!(
            json!(ops),
            json!([attrs::clear::<Auto>(block)]),
            "the look writes nothing here, so the mark's removal alone"
        );
        let read = recognized(&layer(&session, block), "ドン").expect("the record");
        apply(&mut session, vec![read]);
        assert!(Auto::of(&layer(&session, block)).kept, "reading keeps it");
        let ops = typed(
            &session.project(),
            block,
            &placement(&session, block),
            &look,
            None,
        )
        .expect("the ops");
        apply(&mut session, ops);
        assert_eq!(
            attrs::get::<Auto>(&layer(&session, block)),
            Some(Auto {
                source: Some(text_hash("ドン")),
                ..Auto::default()
            })
        );
        // With no mark, a typed translation writes no record at all.
        let ops = typed(
            &session.project(),
            block,
            &placement(&session, block),
            &look,
            None,
        )
        .expect("the ops");
        assert!(ops.is_empty(), "{ops:?}");

        // Given back.
        let (_dir, mut session, block) = page("ドン", &["detection", "ocr"]);
        keep(&mut session, block);
        let given = given_back(&layer(&session, block)).expect("the record");
        apply(&mut session, vec![given]);
        let found = layer(&session, block);
        assert!(!Auto::of(&found).kept);
        assert_eq!(Auto::of(&found).text, Some(text_hash("")));
    }

    #[test]
    fn a_typed_translation_gets_the_look_and_no_record() {
        let (_dir, session, block) = page("どうして", &["detection", "ocr"]);
        let look = settings(false, Some(RED), None);
        let mut ops = typed(
            &session.project(),
            block,
            &placement(&session, block),
            &look,
            Some("CCWildWords"),
        )
        .expect("the look");
        assert_eq!(ops.len(), 1, "the look alone, and no record: {ops:?}");
        let op = ops.remove(0);
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
            typed(
                &session.project(),
                block,
                &placement(&session, block),
                &on,
                None
            )
            .expect("the look")
            .is_empty()
        );
        // Placement's op for another layer is refused.
        let other = Op::UpdateLayer {
            layer: LayerId(koharu_scene::Uuid::now_v7()).into(),
            patch: json!({}),
        };
        assert!(typed(&session.project(), block, &other, &look, None).is_err());
    }

    /// `look` with Pick fonts automatically on and the category fonts
    /// `fonts`.
    fn lettered(
        look: LetteringSettings,
        fonts: &[(LetteringCategory, Option<&str>, bool, bool)],
    ) -> LetteringSettings {
        LetteringSettings {
            auto: true,
            categories: fonts
                .iter()
                .map(|(category, family, bold, italic)| {
                    let font = CategoryFont {
                        family: family.map(str::to_owned),
                        bold: *bold,
                        italic: *italic,
                    };
                    (*category, font)
                })
                .collect(),
            ..look
        }
    }

    /// Labels `block` with `category`, chosen by a user.
    fn label(session: &mut Session, block: LayerId, category: LetteringCategory) {
        let label = attrs::Lettering {
            category,
            source: attrs::LetteringSource::User,
            confidence: None,
            role: None,
        };
        apply(session, vec![attrs::set(block, &label).expect("a label")]);
    }

    #[test]
    fn with_the_switch_on_a_new_translation_gets_its_categorys_font_recorded_as_automatic() {
        let (_dir, mut session, block) = page("どうして", &["detection", "ocr"]);
        label(&mut session, block, LetteringCategory::Shout);
        let look = lettered(
            settings(false, Some(RED), None),
            &[(LetteringCategory::Shout, Some("Badaboom BB"), true, false)],
        );
        translate(&mut session, block, "Why?", &look, Some("CCWildWords"));

        // The category's font in place of the default font, beside the
        // default colours.
        let chars = translation(&session, block).style.chars;
        assert_eq!(
            (chars.family.as_deref(), chars.weight, chars.italic),
            (Some("Badaboom BB"), Some(700), None)
        );
        assert_eq!((chars.fill, chars.stroke), (Some(RED), None));
        // Recorded as what the step wrote, so it's no hand edit.
        let found = layer(&session, block);
        let style = Auto::of(&found).style.expect("a style");
        assert_eq!(
            (style.family.as_deref(), style.weight, style.italic),
            (Some("Badaboom BB"), Some(700), None)
        );
        assert_eq!(edited(&found), Edited::default());

        // Translated again, it keeps the font it has, as a translation keeps
        // its family, though the category's font changed since.
        let changed = lettered(
            settings(false, Some(RED), None),
            &[(LetteringCategory::Shout, Some("Another"), false, true)],
        );
        translate(&mut session, block, "Why!?", &changed, Some("CCWildWords"));
        let chars = translation(&session, block).style.chars;
        assert_eq!(
            (chars.family.as_deref(), chars.weight, chars.italic),
            (Some("Badaboom BB"), Some(700), None)
        );
        assert_eq!(edited(&layer(&session, block)), Edited::default());
    }

    #[test]
    fn a_category_without_a_font_takes_normal_dialogues_then_the_default_font() {
        // Not in a balloon: floating, which has no font of its own.
        let (_dir, mut session, block) = page("どうして", &["detection", "ocr"]);
        let normal = lettered(
            settings(true, None, None),
            &[(
                LetteringCategory::NormalDialogue,
                Some("Comic Neue"),
                false,
                true,
            )],
        );
        translate(&mut session, block, "Why?", &normal, Some("CCWildWords"));
        let chars = translation(&session, block).style.chars;
        assert_eq!(
            (chars.family.as_deref(), chars.weight, chars.italic),
            (Some("Comic Neue"), None, Some(true))
        );
        assert_eq!(edited(&layer(&session, block)), Edited::default());

        // With neither, the default font.
        let (_dir, mut session, block) = page("どうして", &["detection", "ocr"]);
        let bare = lettered(settings(true, None, None), &[]);
        translate(&mut session, block, "Why?", &bare, Some("CCWildWords"));
        let chars = translation(&session, block).style.chars;
        assert_eq!(
            (chars.family.as_deref(), chars.weight, chars.italic),
            (Some("CCWildWords"), None, None)
        );
        // An entry without a family takes the default font too, with its
        // bold.
        let floating = lettered(
            settings(true, None, None),
            &[(LetteringCategory::Floating, None, true, false)],
        );
        let none = CharStyle::default();
        let patch = new_translation(
            &floating,
            Some("CCWildWords"),
            false,
            Some(LetteringCategory::Floating),
            &none,
            &measured(),
        );
        assert_eq!(
            Value::Object(patch),
            json!({"family": "CCWildWords", "weight": 700, "italic": null})
        );
    }

    #[test]
    fn with_the_switch_off_no_category_font_is_written() {
        let (_dir, mut session, block) = page("どうして", &["detection", "ocr"]);
        label(&mut session, block, LetteringCategory::Shout);
        let off = LetteringSettings {
            auto: false,
            ..lettered(
                settings(true, None, None),
                &[(LetteringCategory::Shout, Some("Badaboom BB"), true, true)],
            )
        };
        let op = typed(
            &session.project(),
            block,
            &placement(&session, block),
            &off,
            Some("CCWildWords"),
        )
        .expect("the look");
        assert_eq!(
            op.first().map(|op| json!(op)["patch"].clone()),
            Some(json!({"translation": {"style": {"chars": {"family": "CCWildWords"}}}}))
        );
        // A typed translation gets the category's font with it on.
        let on = LetteringSettings { auto: true, ..off };
        let op = typed(
            &session.project(),
            block,
            &placement(&session, block),
            &on,
            Some("CCWildWords"),
        )
        .expect("the look");
        assert_eq!(
            op.first().map(|op| json!(op)["patch"].clone()),
            Some(json!({"translation": {"style": {"chars": {
                "family": "Badaboom BB", "weight": 700, "italic": true,
            }}}}))
        );
    }

    #[test]
    fn apply_gives_the_category_font_in_place_of_the_font_it_has() {
        let current = CharStyle {
            family: Some("Comic Neue".to_owned()),
            weight: Some(700),
            italic: Some(true),
            ..measured()
        };
        let look = lettered(
            settings(true, None, None),
            &[(LetteringCategory::Whisper, None, false, true)],
        );
        let patch = restyle(
            &look,
            Some("CCWildWords"),
            false,
            Some(LetteringCategory::Whisper),
            &current,
        );
        assert_eq!(
            Value::Object(patch.clone()),
            json!({"family": "CCWildWords", "weight": null, "italic": true})
        );
        let after = patched(&current, &patch).expect("values");
        assert_eq!(
            (
                after.family.as_deref(),
                after.weight,
                after.italic,
                after.fill
            ),
            (Some("CCWildWords"), None, Some(true), Some(BLUE))
        );
        // Without a category, stage 3's restyle: the weight stays.
        assert_eq!(
            keys(&restyle(&look, Some("CCWildWords"), false, None, &current)),
            ["family"]
        );
        // With no default font, a category's font without a family takes
        // the family away, for Koharu's font chain, as does a category with
        // no font, nor Normal Dialogue's.
        let patch = restyle(
            &look,
            None,
            false,
            Some(LetteringCategory::Whisper),
            &current,
        );
        assert_eq!(
            Value::Object(patch.clone()),
            json!({"family": null, "weight": null, "italic": true})
        );
        let after = patched(&current, &patch).expect("values");
        assert_eq!(
            (after.family, after.weight, after.italic),
            (None, None, Some(true))
        );
        assert_eq!(
            Value::Object(category_chars(&look, None, LetteringCategory::Shout)),
            json!({"family": null, "weight": null, "italic": null})
        );
    }
}
