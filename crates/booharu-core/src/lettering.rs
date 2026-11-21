//! Lettering categories: what kind of text a block holds, such as speech, a
//! shout, narration, or small text, so it can be lettered in that kind's
//! font.
//!
//! - The categories are Booharu 0.0.4's fifteen, with the same ids in the
//!   same order, which is Settings' order, then `handwritten`, `sign`, and
//!   `electronic_voice` ([`LetteringCategory`]).
//! - `[booharu-lettering]` gives each category its own font, family, bold,
//!   and italic ([`CategoryFont`]), holding only the categories given one, and
//!   its `auto` switch, Pick fonts automatically, off by default, decides
//!   whether Booharu letters a block in its category's font at all.
//! - A category's font ([`LetteringSettings::category_font`]) is its own
//!   entry, else Normal Dialogue's, else none, and text with none gets the
//!   default look's font, a copy of the default font, neither bold nor
//!   italic. An entry without a family takes Normal Dialogue's family, else
//!   a copy of the default font, with its own bold and italic, so a category
//!   Settings shows as "Same as Normal Dialogue" follows Normal Dialogue's
//!   family when only its bold or italic is its own. Written on a
//!   translation, a category's font sets its `style.chars` `family` (null
//!   when there's no default font to copy either, for Koharu's font chain),
//!   `weight` (700 when bold, else null), and `italic` (true when italic,
//!   else null): never other keys and never the marks, so word styles
//!   survive a change of category.
//! - A block's category ([`effective`]) is, in this order: a user's label; a
//!   model's label of a presentation category ([`LetteringCategory::presentation`])
//!   at any confidence; `sfx` for a block Koharu detected as a sound effect;
//!   a model's label of a delivery category ([`LetteringCategory::delivery`])
//!   at least `model_confidence` sure, one with no confidence counting as
//!   sure; `small_text` for small text; the shape of its balloon, round for
//!   `normal_dialogue`, rectangular for `narration_box`, spiky for `shout`;
//!   and `floating` without a balloon. A model's label of another category
//!   never wins, as in 0.0.4, and a label that doesn't read counts as none,
//!   since anyone can write one through Koharu's generic edit tool. Only
//!   blocks have a category: the T tool's free text keeps the style set by
//!   hand.
//! - Text is small when its glyphs are under `small_text` of the page's
//!   height (0.012 is 24 pixels on a page 2000 high): the size Koharu's
//!   detection measured on its source, else 0.0.4's estimate from its
//!   source's frame and recognized wording ([`glyph_size`]); text with
//!   neither is never small.

use std::collections::BTreeMap;

use koharu_scene::{Content, Direction, Frame, LayerId, Page, Role, Text};
use kurbo::{Affine, BezPath, Rect, Shape as _};
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::{
    attrs::{self, Lettering, LetteringSource},
    settings::LetteringSettings,
    shape::{self, BubbleShape},
};

/// What kind of text a block holds, which decides the font it's lettered in.
/// The order is Settings' order.
#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Serialize,
    Deserialize,
    specta::Type,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum LetteringCategory {
    /// Speech in a round balloon: the default kind.
    NormalDialogue,
    /// A character's thoughts.
    Thoughts,
    /// Text outside any balloon.
    Floating,
    /// An aside in small letters.
    SmallText,
    /// Narration, in a rectangular box.
    NarrationBox,
    /// A shout, in a spiky balloon.
    Shout,
    /// A weak or panicked shout.
    WeakShout,
    /// A sound effect.
    Sfx,
    /// A moan.
    Moan,
    /// A title.
    Title,
    /// Text on a phone's screen.
    PhoneText,
    /// A whisper.
    Whisper,
    /// Shy or hesitant speech.
    Shy,
    /// Crying or trembling speech.
    Crying,
    /// Menacing or sinister speech.
    Menacing,
    /// Handwriting.
    Handwritten,
    /// A sign or printed text.
    Sign,
    /// An electronic voice, such as a speaker's or a robot's.
    ElectronicVoice,
}

impl LetteringCategory {
    /// Every category, in Settings' order.
    pub const ALL: [Self; 18] = [
        Self::NormalDialogue,
        Self::Thoughts,
        Self::Floating,
        Self::SmallText,
        Self::NarrationBox,
        Self::Shout,
        Self::WeakShout,
        Self::Sfx,
        Self::Moan,
        Self::Title,
        Self::PhoneText,
        Self::Whisper,
        Self::Shy,
        Self::Crying,
        Self::Menacing,
        Self::Handwritten,
        Self::Sign,
        Self::ElectronicVoice,
    ];

    /// Its id, as JSON, the settings, and the interface's strings name it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NormalDialogue => "normal_dialogue",
            Self::Thoughts => "thoughts",
            Self::Floating => "floating",
            Self::SmallText => "small_text",
            Self::NarrationBox => "narration_box",
            Self::Shout => "shout",
            Self::WeakShout => "weak_shout",
            Self::Sfx => "sfx",
            Self::Moan => "moan",
            Self::Title => "title",
            Self::PhoneText => "phone_text",
            Self::Whisper => "whisper",
            Self::Shy => "shy",
            Self::Crying => "crying",
            Self::Menacing => "menacing",
            Self::Handwritten => "handwritten",
            Self::Sign => "sign",
            Self::ElectronicVoice => "electronic_voice",
        }
    }

    /// Its English name, as the agent's tools describe it; the interface
    /// translates its own.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::NormalDialogue => "Normal Dialogue",
            Self::Thoughts => "Thoughts",
            Self::Floating => "Floating",
            Self::SmallText => "Small Text",
            Self::NarrationBox => "Narration Box",
            Self::Shout => "Shout",
            Self::WeakShout => "Weak/Panic Shout",
            Self::Sfx => "SFX",
            Self::Moan => "Moan",
            Self::Title => "Title",
            Self::PhoneText => "Phone Text",
            Self::Whisper => "Whisper",
            Self::Shy => "Shy / Hesitant",
            Self::Crying => "Crying / Trembling",
            Self::Menacing => "Menacing / Sinister",
            Self::Handwritten => "Handwritten",
            Self::Sign => "Sign or printed text",
            Self::ElectronicVoice => "Electronic voice",
        }
    }

    /// The category whose id is `id`, ignoring case and the space around it.
    #[must_use]
    pub fn parse(id: &str) -> Option<Self> {
        let id = id.trim();
        Self::ALL
            .into_iter()
            .find(|category| category.as_str().eq_ignore_ascii_case(id))
    }

    /// Whether it's a presentation category, how the text is shown rather
    /// than spoken, which a model's label gives at any confidence: `title`,
    /// `phone_text`, `sfx`, `handwritten`, `sign`, and `electronic_voice`.
    #[must_use]
    pub const fn presentation(self) -> bool {
        matches!(
            self,
            Self::Title
                | Self::PhoneText
                | Self::Sfx
                | Self::Handwritten
                | Self::Sign
                | Self::ElectronicVoice
        )
    }

    /// Whether it's a delivery category, how the line is spoken, which a
    /// model's label gives only at least `model_confidence` sure:
    /// `thoughts`, `weak_shout`, `moan`, `whisper`, `shy`, `crying`,
    /// `menacing`, and `shout`.
    #[must_use]
    pub const fn delivery(self) -> bool {
        matches!(
            self,
            Self::Thoughts
                | Self::WeakShout
                | Self::Moan
                | Self::Whisper
                | Self::Shy
                | Self::Crying
                | Self::Menacing
                | Self::Shout
        )
    }
}

/// A category's own font in `[booharu-lettering] categories`. An entry
/// holds at least one of its three, as a category with none has no entry.
#[derive(
    Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, specta::Type, schemars::JsonSchema,
)]
#[serde(default)]
pub struct CategoryFont {
    /// The font family, never blank; none for Normal Dialogue's family, or
    /// in Normal Dialogue's own entry, for a copy of the default font.
    pub family: Option<String>,
    /// Whether it's lettered bold, at weight 700.
    pub bold: bool,
    /// Whether it's lettered italic.
    pub italic: bool,
}

/// The weight a bold category font writes.
const BOLD: u16 = 700;

impl CategoryFont {
    /// Its part of a translation's `style.chars`, as resolved: its family,
    /// null without one, so the family a translation had goes and Koharu's
    /// font chain applies, its weight, 700 or null, and its italic, true or
    /// null.
    #[must_use]
    pub fn chars(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut chars = serde_json::Map::new();
        chars.insert("family".to_owned(), self.family.clone().into());
        chars.insert("weight".to_owned(), self.bold.then_some(BOLD).into());
        chars.insert("italic".to_owned(), self.italic.then_some(true).into());
        chars
    }
}

impl LetteringSettings {
    /// The font `category` is lettered in: its own entry, else Normal
    /// Dialogue's, an entry without a family taking Normal Dialogue's, else
    /// a copy of `font`, the default font; none with neither entry, so the
    /// text keeps the default look's font. A family is written without the
    /// space around it.
    #[must_use]
    pub fn category_font(
        &self,
        category: LetteringCategory,
        font: Option<&str>,
    ) -> Option<CategoryFont> {
        let normal = self.categories.get(&LetteringCategory::NormalDialogue);
        let entry = self.categories.get(&category).or(normal)?;
        let family = entry
            .family
            .as_deref()
            .or_else(|| normal.and_then(|normal| normal.family.as_deref()))
            .map(str::trim)
            .or(font);
        Some(CategoryFont {
            family: family.map(str::to_owned),
            bold: entry.bold,
            italic: entry.italic,
        })
    }
}

/// Why a block has its category.
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, specta::Type, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CategoryReason {
    /// A user chose it.
    User,
    /// A model chose it.
    Model,
    /// Koharu detected the block as a sound effect.
    Sfx,
    /// Its text is small.
    SmallText,
    /// Its balloon's shape.
    Shape,
    /// It's in no balloon.
    Floating,
}

/// A block's category, and why it has it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct EffectiveCategory {
    pub category: LetteringCategory,
    pub reason: CategoryReason,
}

/// The category of the block `block` on `page`, by its label; none for a
/// layer that isn't a block there.
pub fn effective(
    page: &Page,
    block: LayerId,
    settings: &LetteringSettings,
) -> Option<EffectiveCategory> {
    let (layer, _) = shape::find(page, block)?;
    effective_with(
        page,
        block,
        attrs::get::<Lettering>(layer).as_ref(),
        settings,
    )
}

/// The category the block `block` on `page` would have with the label
/// `label` in place of its own, as one being changed gets.
pub fn effective_with(
    page: &Page,
    block: LayerId,
    label: Option<&Lettering>,
    settings: &LetteringSettings,
) -> Option<EffectiveCategory> {
    let (layer, _) = shape::find(page, block)?;
    let Content::Block(found) = &layer.content else {
        return None;
    };
    effective_as(page, block, label, found.role == Role::Sfx, settings)
}

/// The category the block `block` on `page` would have with the label
/// `label` and Koharu's role `sfx`, a sound effect when true, in place of
/// its own, as one whose label and role are both being changed gets; none
/// for a layer that isn't a block there.
pub fn effective_as(
    page: &Page,
    block: LayerId,
    label: Option<&Lettering>,
    sfx: bool,
    settings: &LetteringSettings,
) -> Option<EffectiveCategory> {
    let (layer, _) = shape::find(page, block)?;
    let Content::Block(found) = &layer.content else {
        return None;
    };
    Some(decide(
        label,
        sfx,
        || small_text(page, &found.source, settings),
        || shape::block_shape(page, block).map(|shape| shape.shape),
        settings,
    ))
}

/// The category the block `block` on `page` is lettered in while Pick fonts
/// automatically is on; none while it's off, or for a layer that isn't a
/// block there.
pub fn auto_category(
    page: &Page,
    block: LayerId,
    settings: &LetteringSettings,
) -> Option<LetteringCategory> {
    if !settings.auto {
        return None;
    }
    effective(page, block, settings).map(|effective| effective.category)
}

/// The category a balloon of `shape` letters its text in: round for Normal
/// Dialogue, rectangular for Narration Box, spiky for Shout, and none for
/// Floating.
#[must_use]
pub const fn shape_category(shape: Option<BubbleShape>) -> LetteringCategory {
    match shape {
        Some(BubbleShape::Round) => LetteringCategory::NormalDialogue,
        Some(BubbleShape::Rectangle) => LetteringCategory::NarrationBox,
        Some(BubbleShape::Spiky) => LetteringCategory::Shout,
        None => LetteringCategory::Floating,
    }
}

/// The effective category's order, for a block labelled `label`, detected
/// as a sound effect when `sfx`, whose text is `small`, in a balloon of
/// `shape`: the two last are worked out only when nothing before them wins.
fn decide(
    label: Option<&Lettering>,
    sfx: bool,
    small: impl FnOnce() -> bool,
    shape: impl FnOnce() -> Option<BubbleShape>,
    settings: &LetteringSettings,
) -> EffectiveCategory {
    let chosen = |category, reason| EffectiveCategory { category, reason };
    let model = label.filter(|label| label.source == LetteringSource::Model);
    if let Some(label) = label.filter(|label| label.source == LetteringSource::User) {
        return chosen(label.category, CategoryReason::User);
    }
    if let Some(label) = model.filter(|label| label.category.presentation()) {
        return chosen(label.category, CategoryReason::Model);
    }
    if sfx {
        return chosen(LetteringCategory::Sfx, CategoryReason::Sfx);
    }
    let sure = |label: &&Lettering| {
        label.category.delivery() && label.confidence.unwrap_or(1.0) >= settings.model_confidence
    };
    if let Some(label) = model.filter(sure) {
        return chosen(label.category, CategoryReason::Model);
    }
    if small() {
        return chosen(LetteringCategory::SmallText, CategoryReason::SmallText);
    }
    match shape() {
        Some(shape) => chosen(shape_category(Some(shape)), CategoryReason::Shape),
        None => chosen(LetteringCategory::Floating, CategoryReason::Floating),
    }
}

/// Whether `source`, a block's source on `page`, is small text: its glyphs
/// under `small_text` of the page's height. Text with no glyph size is never
/// small.
pub fn small_text(page: &Page, source: &Text, settings: &LetteringSettings) -> bool {
    let height = f64::from(page.size.height);
    height > 0.0 && glyph_size(source).is_some_and(|glyph| glyph / height < settings.small_text)
}

/// The size of the glyphs of `source`, a block's source, in page pixels: the
/// size Koharu's detection measured, else Booharu 0.0.4's estimate from its
/// frame and recognized wording, the side of the square each letter or digit
/// would fill in the frame, at most as wide as the frame across its lines.
/// Punctuation isn't counted. None for point text, for a frame enclosing
/// nothing, and for wording with no letter or digit.
pub fn glyph_size(source: &Text) -> Option<f64> {
    let measured = source.style.chars.size;
    if let Some(size) = measured.filter(|size| size.is_finite() && *size > 0.0) {
        return Some(size);
    }
    let letters = source
        .content
        .text()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .count();
    if letters == 0 {
        return None;
    }
    let (area, frame) = match &source.frame {
        Frame::Point => return None,
        Frame::Box { bounds, .. } => {
            let rect = Rect::from(*bounds);
            (rect.area().abs(), rect)
        }
        Frame::Path { path, .. } => {
            let path = BezPath::from(path);
            (path.area().abs(), path.bounding_box())
        }
    };
    // In page pixels: the frame lies in the text's own coordinates.
    let transform = Affine::from(source.transform);
    let [a, b, c, d, _, _] = transform.as_coeffs();
    let area = area * transform.determinant().abs();
    let across = match source.direction {
        Direction::Vertical => frame.width().abs() * a.hypot(b),
        Direction::Horizontal => frame.height().abs() * c.hypot(d),
    };
    let glyph = (area / letters as f64).sqrt().min(across);
    (glyph.is_finite() && glyph > 0.0).then_some(glyph)
}

/// `[booharu-lettering] categories`: each entry with a family that isn't
/// blank, or bold or italic, by a category's id. A null entry, which only a
/// JSON caller can send, as the settings tool does, removes it.
pub(crate) fn categories<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<LetteringCategory, CategoryFont>, D::Error> {
    let entries = BTreeMap::<LetteringCategory, Option<CategoryFont>>::deserialize(deserializer)?;
    let mut categories = BTreeMap::new();
    for (category, font) in entries {
        let Some(font) = font else {
            continue;
        };
        let id = category.as_str();
        if font
            .family
            .as_deref()
            .is_some_and(|family| family.trim().is_empty())
        {
            return Err(D::Error::custom(format!(
                "{id}'s font family can't be blank: leave it out for the default font"
            )));
        }
        if font == CategoryFont::default() {
            return Err(D::Error::custom(format!(
                "{id}'s font sets nothing: leave the category out instead"
            )));
        }
        categories.insert(category, font);
    }
    Ok(categories)
}

/// `[booharu-lettering] small_text`: a share of the page's height above 0
/// and at most 1.
pub(crate) fn small_text_share<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<f64, D::Error> {
    let share = f64::deserialize(deserializer)?;
    if !(share > 0.0 && share <= 1.0) {
        return Err(D::Error::custom(format!(
            "small text under {share} of the page's height isn't above 0 and at most 1"
        )));
    }
    Ok(share)
}

/// `[booharu-lettering] model_confidence`: from 0 to 1.
pub(crate) fn confidence<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f32, D::Error> {
    let confidence = f32::deserialize(deserializer)?;
    if !(0.0..=1.0).contains(&confidence) {
        return Err(D::Error::custom(format!(
            "a confidence of {confidence} isn't from 0 to 1"
        )));
    }
    Ok(confidence)
}

/// `[booharu-lettering] model_confidence` as the shortest decimal that
/// reads back as it, so the file holds `0.7` rather than the `f32`'s
/// `0.699999988079071`.
pub(crate) fn decimal<S: serde::Serializer>(
    confidence: &f32,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let shortest = confidence.to_string().parse::<f64>();
    serializer.serialize_f64(shortest.unwrap_or(f64::from(*confidence)))
}

#[cfg(test)]
mod tests {
    use koharu_scene::{Created, Edit, Op, Session};
    use serde_json::{Value, json};

    use super::*;
    use crate::{
        settings::BooharuPreferences,
        testing::{saved, stored, unreadable},
    };

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
    fn ids_round_trip_through_serde_and_parse_in_settings_order() {
        for category in LetteringCategory::ALL {
            let json = serde_json::to_string(&category).expect("an id");
            assert_eq!(json, format!("\"{}\"", category.as_str()));
            assert_eq!(
                serde_json::from_str::<LetteringCategory>(&json).ok(),
                Some(category)
            );
            assert_eq!(LetteringCategory::parse(category.as_str()), Some(category));
        }
        assert_eq!(
            LetteringCategory::parse(" Electronic_Voice "),
            Some(LetteringCategory::ElectronicVoice)
        );
        assert_eq!(LetteringCategory::parse("tone"), None);
        assert_eq!(LetteringCategory::parse("automatic"), None);
        // 0.0.4's fifteen first, in their order, then the three new ones,
        // which is also how the map of fonts sorts them.
        let ids = LetteringCategory::ALL.map(LetteringCategory::as_str);
        assert_eq!(
            ids,
            [
                "normal_dialogue",
                "thoughts",
                "floating",
                "small_text",
                "narration_box",
                "shout",
                "weak_shout",
                "sfx",
                "moan",
                "title",
                "phone_text",
                "whisper",
                "shy",
                "crying",
                "menacing",
                "handwritten",
                "sign",
                "electronic_voice",
            ]
        );
        let mut sorted = LetteringCategory::ALL;
        sorted.sort();
        assert_eq!(sorted, LetteringCategory::ALL);
        // Each category is in at most one set, and the four the rules give
        // are in neither.
        for category in LetteringCategory::ALL {
            assert!(
                !(category.presentation() && category.delivery()),
                "{category:?}"
            );
        }
        let neither = LetteringCategory::ALL
            .into_iter()
            .filter(|category| !category.presentation() && !category.delivery())
            .collect::<Vec<_>>();
        assert_eq!(
            neither,
            [
                LetteringCategory::NormalDialogue,
                LetteringCategory::Floating,
                LetteringCategory::SmallText,
                LetteringCategory::NarrationBox,
            ]
        );
    }

    #[test]
    fn the_settings_start_with_no_category_fonts_and_the_switch_off() {
        let lettering: LetteringSettings = toml::from_str("").expect("the defaults");
        assert_eq!(lettering, LetteringSettings::default());
        assert!(!lettering.auto);
        assert!(lettering.categories.is_empty());
        assert_eq!(lettering.small_text, 0.012);
        assert_eq!(lettering.model_confidence, 0.6);
    }

    #[test]
    fn the_category_fonts_read_back_as_they_were_written() {
        let lettering = LetteringSettings {
            auto: true,
            categories: BTreeMap::from([
                (
                    LetteringCategory::Shout,
                    CategoryFont {
                        family: Some("Badaboom BB".to_owned()),
                        bold: true,
                        italic: false,
                    },
                ),
                (
                    LetteringCategory::Thoughts,
                    CategoryFont {
                        family: None,
                        bold: false,
                        italic: true,
                    },
                ),
            ]),
            small_text: 0.02,
            model_confidence: 0.7,
            ..LetteringSettings::default()
        };
        let text = toml::to_string(&lettering).expect("a table");
        assert!(text.contains("[categories.shout]"), "{text}");
        // The confidence as it was typed, and no family where there's none.
        assert!(text.contains("model_confidence = 0.7\n"), "{text}");
        assert_eq!(text.matches("family").count(), 1, "{text}");
        assert_eq!(
            toml::from_str::<LetteringSettings>(&text).ok(),
            Some(lettering.clone())
        );
        let json = serde_json::to_value(&lettering).expect("JSON");
        assert_eq!(json["model_confidence"], json!(0.7));
        assert_eq!(
            json["categories"]["thoughts"],
            json!({"family": null, "bold": false, "italic": true})
        );
        assert_eq!(
            serde_json::from_value::<LetteringSettings>(json).ok(),
            Some(lettering)
        );
    }

    #[test]
    fn a_bad_category_font_or_share_isnt_taken_and_reads_as_its_default() {
        let refused = [
            // A blank family, an empty entry, an unknown category.
            "[categories.shout]\nfamily = \"  \"",
            "[categories.shout]\nfamily = \"\"\nbold = true",
            "[categories.shout]\nbold = false",
            "[categories.shout]\n",
            "[categories.tone]\nfamily = \"Arial\"",
            "[categories.Shout]\nfamily = \"Arial\"",
            "categories = []",
            // Out of range.
            "small_text = 0",
            "small_text = -0.01",
            "small_text = 1.5",
            "small_text = nan",
            "small_text = inf",
            "model_confidence = -0.1",
            "model_confidence = 1.01",
            "model_confidence = nan",
            "auto = 1",
        ];
        for text in refused {
            assert!(
                toml::from_str::<LetteringSettings>(text).is_err(),
                "took {text}"
            );
            // Edited into the file, it reads as its default.
            assert_eq!(
                stored::<LetteringSettings>(text).ok(),
                Some(LetteringSettings::default()),
                "{text}"
            );
        }
        // The section's other values stay; a bad category font takes the
        // categories' default, none, with the good ones beside it. A file
        // that isn't TOML fails.
        let kept: LetteringSettings = stored(
            "auto = true\nsmall_text = 2\n\
             [categories.shout]\nfamily = \"  \"\n[categories.sign]\nitalic = true",
        )
        .expect("the section");
        assert!(kept.auto);
        assert_eq!(kept.small_text, LetteringSettings::default().small_text);
        assert!(kept.categories.is_empty());
        assert!(unreadable::<LetteringSettings>().is_err());
        let edges: LetteringSettings = toml::from_str(
            "small_text = 1\nmodel_confidence = 0\n[categories.sign]\nitalic = true",
        )
        .expect("the edges");
        assert_eq!((edges.small_text, edges.model_confidence), (1.0, 0.0));
        assert_eq!(
            toml::from_str::<LetteringSettings>("model_confidence = 1")
                .ok()
                .map(|lettering| lettering.model_confidence),
            Some(1.0)
        );
        assert_eq!(edges.categories.len(), 1);
    }

    #[test]
    fn a_change_to_categories_holding_a_bad_one_writes_them_whole() {
        // Shout's font is good and Whisper's blank family isn't, so they
        // read as none.
        let stored = "[categories.shout]\nfamily = \"Anime Ace\"\nbold = true\n\
                      [categories.whisper]\nfamily = \"\"";
        let thoughts = CategoryFont {
            italic: true,
            ..CategoryFont::default()
        };
        // A change to them saves them as read with it, over the file's.
        let (outcome, read, text) = saved::<LetteringSettings>(stored, |lettering| {
            assert!(lettering.categories.is_empty());
            lettering
                .categories
                .insert(LetteringCategory::Thoughts, thoughts.clone());
        });
        outcome.expect("saved");
        assert_eq!(
            read.categories,
            BTreeMap::from([(LetteringCategory::Thoughts, thoughts)])
        );
        assert!(!text.contains("Anime Ace"), "{text}");
        // Shout's font picked again is saved alone.
        let shout = CategoryFont {
            family: Some("Anime Ace".into()),
            bold: true,
            italic: false,
        };
        let (outcome, read, _) = saved::<LetteringSettings>(stored, |lettering| {
            lettering
                .categories
                .insert(LetteringCategory::Shout, shout.clone());
        });
        outcome.expect("saved");
        assert_eq!(
            read.categories,
            BTreeMap::from([(LetteringCategory::Shout, shout)])
        );
        // A change elsewhere leaves them in the file as they were.
        let (outcome, read, text) = saved::<LetteringSettings>(stored, |lettering| {
            lettering.auto = true;
        });
        outcome.expect("saved");
        assert!(read.auto && read.categories.is_empty());
        assert!(text.contains("Anime Ace"), "{text}");
    }

    fn values(values: Value) -> serde_json::Map<String, Value> {
        match values {
            Value::Object(values) => values,
            other => panic!("{other} isn't an object"),
        }
    }

    #[test]
    fn the_settings_tool_adds_changes_and_removes_a_category_font() {
        let mut preferences = BooharuPreferences::default();
        preferences
            .set_section(
                "booharu-lettering",
                values(json!({
                    "auto": true,
                    "categories": {
                        "shout": {"family": "Badaboom BB", "bold": true},
                        "whisper": {"italic": true},
                    },
                })),
            )
            .expect("two fonts");
        assert!(preferences.lettering.auto);
        assert_eq!(preferences.lettering.categories.len(), 2);
        // An entry changes only in the keys named, and the others stay.
        preferences
            .set_section(
                "booharu-lettering",
                values(json!({"categories": {"shout": {"italic": true}}})),
            )
            .expect("italic too");
        assert_eq!(
            preferences.lettering.categories[&LetteringCategory::Shout],
            CategoryFont {
                family: Some("Badaboom BB".to_owned()),
                bold: true,
                italic: true,
            }
        );
        // Null removes one, as Same as Normal Dialogue does, and the switch
        // turns off.
        preferences
            .set_section(
                "booharu-lettering",
                values(json!({"auto": false, "categories": {"whisper": null}})),
            )
            .expect("one font");
        assert!(!preferences.lettering.auto);
        assert_eq!(
            preferences.lettering.categories.keys().collect::<Vec<_>>(),
            [&LetteringCategory::Shout]
        );
        // A category it doesn't have can't be removed, nor one Booharu
        // doesn't know set; a blank family or an emptied entry fails.
        for change in [
            json!({"categories": {"whisper": null}}),
            json!({"categories": {"tone": {"bold": true}}}),
            json!({"categories": {"shout": {"family": " "}}}),
            json!({"categories": {"shout": {"family": null, "bold": false, "italic": false}}}),
            json!({"categories": {"shout": {"weight": 700}}}),
            json!({"small_text": 0}),
            json!({"model_confidence": 2}),
        ] {
            let before = preferences.clone();
            assert!(
                preferences
                    .set_section("booharu-lettering", values(change.clone()))
                    .is_err(),
                "took {change}"
            );
            assert_eq!(preferences, before);
        }
    }

    #[test]
    fn a_category_without_a_font_or_family_falls_back_to_normal_dialogues_then_the_default_font() {
        let font = |family: Option<&str>, bold, italic| CategoryFont {
            family: family.map(str::to_owned),
            bold,
            italic,
        };
        let mut settings = LetteringSettings::default();
        // No entries: no category font, so the default look's font.
        assert_eq!(
            settings.category_font(LetteringCategory::Shout, Some("CCWildWords")),
            None
        );
        // Normal Dialogue's entry for any category without its own.
        settings.categories.insert(
            LetteringCategory::NormalDialogue,
            font(Some(" Comic Neue "), false, false),
        );
        assert_eq!(
            settings.category_font(LetteringCategory::Shout, Some("CCWildWords")),
            Some(font(Some("Comic Neue"), false, false))
        );
        // Its own entry first; one without a family takes Normal Dialogue's,
        // with its own bold and italic.
        settings
            .categories
            .insert(LetteringCategory::Shout, font(None, true, false));
        assert_eq!(
            settings.category_font(LetteringCategory::Shout, Some("CCWildWords")),
            Some(font(Some("Comic Neue"), true, false))
        );
        // Without Normal Dialogue's family, the default font, or with no
        // default font no family; Normal Dialogue's own entry without a
        // family takes the default font too.
        settings
            .categories
            .insert(LetteringCategory::NormalDialogue, font(None, false, true));
        assert_eq!(
            settings.category_font(LetteringCategory::Shout, Some("CCWildWords")),
            Some(font(Some("CCWildWords"), true, false))
        );
        assert_eq!(
            settings.category_font(LetteringCategory::NormalDialogue, Some("CCWildWords")),
            Some(font(Some("CCWildWords"), false, true))
        );
        assert_eq!(
            settings.category_font(LetteringCategory::Whisper, Some("CCWildWords")),
            Some(font(Some("CCWildWords"), false, true))
        );
        settings
            .categories
            .remove(&LetteringCategory::NormalDialogue);
        assert_eq!(
            settings.category_font(LetteringCategory::Shout, Some("CCWildWords")),
            Some(font(Some("CCWildWords"), true, false))
        );
        assert_eq!(
            settings.category_font(LetteringCategory::Shout, None),
            Some(font(None, true, false))
        );
        // A font's part of a translation's look: all three always, null when
        // off, a family null without one, so the one before goes.
        assert_eq!(
            Value::Object(font(Some("Arial"), true, false).chars()),
            json!({"family": "Arial", "weight": 700, "italic": null})
        );
        assert_eq!(
            Value::Object(font(None, false, true).chars()),
            json!({"family": null, "weight": null, "italic": true})
        );
    }

    /// The order on its own, with nothing worked out unless asked.
    fn order(
        label: Option<&Lettering>,
        sfx: bool,
        small: bool,
        shape: Option<BubbleShape>,
    ) -> EffectiveCategory {
        decide(
            label,
            sfx,
            || small,
            || shape,
            &LetteringSettings::default(),
        )
    }

    #[test]
    fn the_effective_category_follows_the_order() {
        use LetteringCategory as C;
        use LetteringSource::{Model, User};
        let is = |category, reason| EffectiveCategory { category, reason };
        let round = Some(BubbleShape::Round);
        // A user's label beats everything, even one the rules never give.
        for category in [C::Whisper, C::Floating, C::NormalDialogue] {
            let user = label(category, User, Some(0.0));
            assert_eq!(
                order(Some(&user), true, true, Some(BubbleShape::Spiky)),
                is(category, CategoryReason::User)
            );
        }
        // A model's presentation label at 0.1 beats a sound effect.
        let title = label(C::Title, Model, Some(0.1));
        assert_eq!(
            order(Some(&title), true, true, round),
            is(C::Title, CategoryReason::Model)
        );
        // A sound effect beats a model's delivery label, however sure.
        let shout = label(C::Shout, Model, Some(1.0));
        assert_eq!(
            order(Some(&shout), true, true, round),
            is(C::Sfx, CategoryReason::Sfx)
        );
        // A sure delivery label beats small text and the shape.
        assert_eq!(
            order(Some(&shout), false, true, round),
            is(C::Shout, CategoryReason::Model)
        );
        // One under model_confidence loses to small text, and to the shape.
        let unsure = label(C::Whisper, Model, Some(0.59));
        assert_eq!(
            order(Some(&unsure), false, true, round),
            is(C::SmallText, CategoryReason::SmallText)
        );
        assert_eq!(
            order(Some(&unsure), false, false, Some(BubbleShape::Rectangle)),
            is(C::NarrationBox, CategoryReason::Shape)
        );
        // Exactly model_confidence is sure enough, and none counts as 1.0.
        let at = label(C::Whisper, Model, Some(0.6));
        assert_eq!(order(Some(&at), false, true, round).category, C::Whisper);
        let unrated = label(C::Moan, Model, None);
        assert_eq!(
            order(Some(&unrated), false, true, round),
            is(C::Moan, CategoryReason::Model)
        );
        // A model's label of a category the rules give never wins.
        let normal = label(C::NormalDialogue, Model, Some(1.0));
        assert_eq!(
            order(Some(&normal), false, false, None),
            is(C::Floating, CategoryReason::Floating)
        );
        // The shapes, and no balloon.
        assert_eq!(
            order(None, false, false, round),
            is(C::NormalDialogue, CategoryReason::Shape)
        );
        assert_eq!(
            order(None, false, false, Some(BubbleShape::Spiky)),
            is(C::Shout, CategoryReason::Shape)
        );
        assert_eq!(
            order(None, false, false, None),
            is(C::Floating, CategoryReason::Floating)
        );
        // The shape isn't worked out once small text wins.
        let settings = LetteringSettings::default();
        let small = decide(None, false, || true, || panic!("the shape"), &settings);
        assert_eq!(small.category, C::SmallText);
    }

    /// A page 2000 high holding `layers`, as add_layer ops' JSON, and the ids
    /// of the layers labelled.
    fn page(layers: &[Value]) -> (Page, impl Fn(&str) -> LayerId + use<>) {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        let mut ops = vec![
            json!({"op": "add_page", "id": "$p", "index": 0, "size": {"width": 1400, "height": 2000}}),
        ];
        ops.extend(layers.iter().cloned());
        let change = session
            .apply(Edit {
                label: "test".to_owned(),
                ops: ops
                    .into_iter()
                    .map(|op| serde_json::from_value::<Op>(op).expect("an op"))
                    .collect(),
            })
            .expect("a page");
        let ids: Vec<(String, LayerId)> = change
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
        let page = session
            .project()
            .page(change.pages.added[0])
            .expect("the page")
            .clone();
        let id = move |label: &str| {
            ids.iter()
                .find_map(|(found, id)| (found == label).then_some(*id))
                .expect("a layer")
        };
        (page, id)
    }

    /// A block labelled `label` whose source reads `text` in `frame`.
    fn block(label: &str, text: &str, frame: Value, extra: Value) -> Value {
        let mut layer = json!({"kind": "block", "source": {"content": text, "frame": frame}});
        if let (Value::Object(layer), Value::Object(extra)) = (&mut layer, extra) {
            layer.extend(extra);
        }
        json!({"op": "add_layer", "id": format!("${label}"), "page": "$p", "index": 0, "layer": layer})
    }

    fn frame(width: f64, height: f64) -> Value {
        json!({"box": {"bounds": {"x": 100.0, "y": 100.0, "width": width, "height": height}}})
    }

    #[test]
    fn small_text_goes_by_the_measured_size_then_the_estimate() {
        let source = |text: &str, frame: Value, size: Option<f64>, direction: &str| {
            let mut source = json!({"content": text, "frame": frame, "direction": direction});
            if let Some(size) = size {
                source["style"] = json!({"chars": {"size": size}});
            }
            serde_json::from_value::<Text>(source).expect("a text")
        };
        let settings = LetteringSettings::default();
        let (page, _) = page(&[]);
        // Measured: 20 pixels on a page 2000 high is under 0.012, 30 isn't,
        // whatever the frame and wording.
        assert!(small_text(
            &page,
            &source("x", frame(400.0, 400.0), Some(20.0), "horizontal"),
            &settings
        ));
        assert!(!small_text(
            &page,
            &source("", json!("point"), Some(30.0), "horizontal"),
            &settings
        ));
        // Estimated, as 0.0.4 did: speech in a tall vertical box isn't small,
        // an aside in a thin strip is, punctuation uncounted, the same across
        // a vertical strip.
        let speech = source(
            "これは普通のセリフです",
            frame(100.0, 240.0),
            None,
            "vertical",
        );
        assert!(glyph_size(&speech).is_some_and(|glyph| glyph / 2000.0 > 0.012));
        assert!(!small_text(&page, &speech, &settings));
        let aside = source("tiny aside!", frame(160.0, 14.0), None, "horizontal");
        assert_eq!(glyph_size(&aside), Some(14.0));
        assert!(small_text(&page, &aside, &settings));
        let across = source("tiny ... aside?!", frame(14.0, 160.0), None, "vertical");
        assert_eq!(glyph_size(&across), Some(14.0));
        // Twice as big on a page twice as high is the same share.
        let doubled = source("tiny aside!", frame(320.0, 28.0), None, "horizontal");
        assert_eq!(glyph_size(&doubled), Some(28.0));
        // Scaled by its transform, in page pixels.
        let mut scaled = aside.clone();
        scaled.transform = koharu_scene::Transform([[2.0, 0.0, 0.0], [0.0, 2.0, 0.0]]);
        assert_eq!(glyph_size(&scaled), Some(28.0));
        // Never small without either: no letters, no frame, nothing enclosed.
        for nothing in [
            source("… ?! ", frame(160.0, 14.0), None, "horizontal"),
            source("", frame(160.0, 14.0), None, "horizontal"),
            source("tiny aside", json!("point"), None, "horizontal"),
            source("text", frame(0.0, 10.0), None, "horizontal"),
        ] {
            assert_eq!(glyph_size(&nothing), None, "{nothing:?}");
            assert!(!small_text(&page, &nothing, &settings));
        }
        // The share is the setting's.
        let looser = LetteringSettings {
            small_text: 0.02,
            ..LetteringSettings::default()
        };
        assert!(small_text(
            &page,
            &source("x", frame(400.0, 400.0), Some(30.0), "horizontal"),
            &looser
        ));
    }

    #[test]
    fn a_blocks_category_comes_from_its_label_role_size_and_balloon() {
        use LetteringCategory as C;
        let spiky = {
            let mut star = String::new();
            for i in 0..24 {
                let angle = f64::from(i) * std::f64::consts::TAU / 24.0;
                let radius = if i % 2 == 0 { 300.0 } else { 180.0 };
                let (x, y) = (700.0 + radius * angle.cos(), 1000.0 + radius * angle.sin());
                star.push_str(&format!("{}{x} {y} ", if i == 0 { "M" } else { "L" }));
            }
            star + "Z"
        };
        let (page, id) = page(&[
            json!({"op": "add_layer", "id": "$round", "page": "$p", "index": 0,
                "layer": {"kind": "balloon", "outline": "M100 100 C100 -50 500 -50 500 100 C500 250 100 250 100 100 Z"}}),
            json!({"op": "add_layer", "id": "$spiky", "page": "$p", "index": 0,
                "layer": {"kind": "balloon", "outline": spiky}}),
            // In the round balloon, by its centre.
            block(
                "speech",
                "Why?",
                json!({"box": {"bounds": {"x": 250.0, "y": 60.0, "width": 100.0, "height": 80.0}}}),
                json!({}),
            ),
            // In the spiky one, but labelled by hand.
            block(
                "labelled",
                "No!",
                json!({"box": {"bounds": {"x": 650.0, "y": 950.0, "width": 100.0, "height": 100.0}}}),
                json!({"attributes": {"booharu": {"lettering": {"category": "whisper", "source": "user"}}}}),
            ),
            // In the spiky one, with a label that doesn't read.
            block(
                "unread",
                "No!",
                json!({"box": {"bounds": {"x": 650.0, "y": 950.0, "width": 100.0, "height": 100.0}}}),
                json!({"attributes": {"booharu": {"lettering": {"category": "tone", "source": "model"}}}}),
            ),
            // Koharu's sound effect, outside any balloon.
            block(
                "effect",
                "ドン",
                frame(200.0, 200.0),
                json!({"role": "sfx"}),
            ),
            // Small, measured, outside any balloon.
            block(
                "aside",
                "tiny",
                json!({"box": {"bounds": {"x": 1200.0, "y": 1800.0, "width": 100.0, "height": 20.0}}}),
                json!({"source": {"content": "tiny", "frame": {"box": {"bounds": {"x": 1200.0, "y": 1800.0, "width": 100.0, "height": 20.0}}},
                    "style": {"chars": {"size": 12.0}}}}),
            ),
            // Neither, and outside any balloon.
            block(
                "floating",
                "Hey",
                json!({"box": {"bounds": {"x": 1200.0, "y": 1500.0, "width": 100.0, "height": 100.0}}}),
                json!({}),
            ),
        ]);
        let settings = LetteringSettings::default();
        let effective = |label: &str| effective(&page, id(label), &settings);
        let is = |category, reason| Some(EffectiveCategory { category, reason });
        assert_eq!(
            effective("speech"),
            is(C::NormalDialogue, CategoryReason::Shape)
        );
        assert_eq!(effective("labelled"), is(C::Whisper, CategoryReason::User));
        assert_eq!(effective("unread"), is(C::Shout, CategoryReason::Shape));
        assert_eq!(effective("effect"), is(C::Sfx, CategoryReason::Sfx));
        assert_eq!(
            effective("aside"),
            is(C::SmallText, CategoryReason::SmallText)
        );
        assert_eq!(
            effective("floating"),
            is(C::Floating, CategoryReason::Floating)
        );
        // Only blocks have one.
        assert_eq!(effective("round"), None);
        // With another label in place of its own, as a change gets it.
        let model = label(C::Shout, LetteringSource::Model, Some(0.9));
        assert_eq!(
            effective_with(&page, id("labelled"), Some(&model), &settings),
            is(C::Shout, CategoryReason::Model)
        );
        assert_eq!(
            effective_with(&page, id("labelled"), None, &settings),
            is(C::Shout, CategoryReason::Shape)
        );
        // Lettered in it only while the switch is on.
        assert_eq!(auto_category(&page, id("speech"), &settings), None);
        let on = LetteringSettings {
            auto: true,
            ..LetteringSettings::default()
        };
        assert_eq!(
            auto_category(&page, id("speech"), &on),
            Some(C::NormalDialogue)
        );
        assert_eq!(auto_category(&page, id("round"), &on), None);
    }
}
