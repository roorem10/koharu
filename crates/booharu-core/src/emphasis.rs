//! Emphasis: how a stressed word looks, `[booharu-emphasis]`, and the words
//! of a translation marked with it.
//!
//! - [`EmphasisSettings`] holds the look: a default and, by lettering
//!   category, the values a category's own replaces. [`patch`] turns one
//!   into the marks a word gets over its text's own character values.
//! - [`find_words`] finds the words a model picked in a translation, as
//!   Booharu 0.0.4 found them, and [`restyled`] changes the marks of a range
//!   of a text's characters, never panicking on a range it can't take.
//! - A run's translation takes off the marks a run left on the one it
//!   replaces ([`stripped`]), where they still hold, and marks the words its
//!   model picked ([`marked`]), recording each in the block's hand-edit
//!   record ([`AutoMark`]), so `styles::edited` never counts them as a hand
//!   edit.

use std::{collections::BTreeMap, ops::Range};

use anyhow::Context as _;
use koharu_config::Section;
use koharu_scene::{CharStyle, Rgba, RichText, Span};
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::{
    attrs::{Auto, AutoMark},
    lettering::LetteringCategory,
    sizes::SIZE,
};

/// `[booharu-emphasis]`: how a stressed word looks, the Emphasis button's
/// and a run's alike, and whether a run's translation stresses the words its
/// model picks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct EmphasisSettings {
    /// Whether a run's translation marks the words its model picked for
    /// emphasis. Off, it marks none, though it still takes off the marks a
    /// run left on the translation it replaces.
    pub ai: bool,
    /// How every category's stressed words look, but for the values a
    /// category's own sets: bold at 115% of the text's size to start with.
    /// It sets at least one value.
    #[serde(deserialize_with = "default_style")]
    #[specta(type = EmphasisStyle)]
    pub default: EmphasisStyle,
    /// The categories whose stressed words look different, by id: each
    /// entry sets at least one value, which replaces the default's. A null
    /// entry, which only a JSON caller can send, as the settings tool does,
    /// removes it.
    #[serde(deserialize_with = "categories")]
    #[specta(type = BTreeMap<LetteringCategory, EmphasisStyle>)]
    pub categories: BTreeMap<LetteringCategory, EmphasisStyle>,
}

/// The weight the default emphasis sets: bold.
const BOLD: u16 = 700;

impl Default for EmphasisSettings {
    fn default() -> Self {
        Self {
            ai: true,
            default: EmphasisStyle {
                weight: Some(BOLD),
                size_factor: Some(1.15),
                ..EmphasisStyle::default()
            },
            categories: BTreeMap::new(),
        }
    }
}

impl Section for EmphasisSettings {
    const NAME: &'static str = "booharu-emphasis";
}

impl EmphasisSettings {
    /// The section as the file holds it now: a stored value that doesn't read
    /// reads as its default, and only a file that can't be read, such as one
    /// that isn't TOML, fails, naming the section.
    pub fn read() -> anyhow::Result<Self> {
        koharu_config::get()
            .with_context(|| format!("Booharu's emphasis can't be read from [{}]", Self::NAME))
    }

    /// How `category`'s stressed words look: its own values over the
    /// default's, or the default's for none, as a text layer's.
    pub fn style_for(&self, category: Option<LetteringCategory>) -> EmphasisStyle {
        match category.and_then(|category| self.categories.get(&category)) {
            Some(own) => own.or(&self.default),
            None => self.default.clone(),
        }
    }
}

/// The values a stressed word gets in place of its text's own, each none to
/// keep the text's. Its default sets none.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct EmphasisStyle {
    /// The font family, never blank.
    #[serde(deserialize_with = "family")]
    #[specta(type = Option<String>)]
    pub family: Option<String>,
    /// The weight, from 1 to 1000: 400 is regular and 700 bold.
    #[serde(deserialize_with = "weight")]
    #[specta(type = Option<u16>)]
    pub weight: Option<u16>,
    /// Whether it's italic; false sets it upright in an italic text.
    pub italic: Option<bool>,
    /// Its size as a multiple of the text's, from 0.25 to 4.
    #[serde(deserialize_with = "size_factor")]
    #[specta(type = Option<f64>)]
    pub size_factor: Option<f64>,
    /// The text's colour.
    #[serde(deserialize_with = "crate::settings::colour")]
    #[specta(type = Option<Rgba>)]
    pub fill: Option<Rgba>,
    /// The outline's colour.
    #[serde(deserialize_with = "crate::settings::colour")]
    #[specta(type = Option<Rgba>)]
    pub stroke: Option<Rgba>,
    /// The outline's width in pixels, from 0 to 100.
    #[serde(deserialize_with = "width")]
    #[specta(type = Option<f64>)]
    pub stroke_width: Option<f64>,
    /// The second border's colour, drawn outside the first.
    #[serde(deserialize_with = "crate::settings::colour")]
    #[specta(type = Option<Rgba>)]
    pub outer_stroke: Option<Rgba>,
    /// The second border's width in pixels, from 0 to 100.
    #[serde(deserialize_with = "width")]
    #[specta(type = Option<f64>)]
    pub outer_stroke_width: Option<f64>,
}

impl EmphasisStyle {
    /// These values, with `base`'s where these have none.
    #[must_use]
    pub fn or(&self, base: &Self) -> Self {
        Self {
            family: self.family.clone().or_else(|| base.family.clone()),
            weight: self.weight.or(base.weight),
            italic: self.italic.or(base.italic),
            size_factor: self.size_factor.or(base.size_factor),
            fill: self.fill.or(base.fill),
            stroke: self.stroke.or(base.stroke),
            stroke_width: self.stroke_width.or(base.stroke_width),
            outer_stroke: self.outer_stroke.or(base.outer_stroke),
            outer_stroke_width: self.outer_stroke_width.or(base.outer_stroke_width),
        }
    }
}

/// The weight the renderer draws a text with none at: regular.
const REGULAR: u16 = 400;

/// The marks `style` gives a word over `base`, its text's own character
/// values, as the renderer draws the characters with no marks: each value
/// the style sets, its size `size_factor` times the text's, or 24 pixels'
/// without one, to a hundredth of a pixel, but for a value the text already
/// draws, regular and upright counting as no weight and no italic. A text
/// with a gradient fill draws the gradient, not its own fill, so there the
/// style's fill is kept even when it's the text's own: a fill beside no
/// gradient hides the gradient for its word, as a fill and a gradient are
/// one paint.
pub fn patch(style: &EmphasisStyle, base: &CharStyle) -> CharStyle {
    fn new<T: PartialEq + Clone>(value: &Option<T>, base: &Option<T>) -> Option<T> {
        value.clone().filter(|value| base.as_ref() != Some(value))
    }
    let size = base.size.unwrap_or(SIZE);
    CharStyle {
        family: new(&style.family, &base.family),
        weight: style
            .weight
            .filter(|weight| *weight != base.weight.unwrap_or(REGULAR)),
        italic: style
            .italic
            .filter(|italic| *italic != base.italic.unwrap_or(false)),
        size: style
            .size_factor
            .map(|factor| (size * factor * 100.0).round() / 100.0)
            .filter(|scaled| *scaled != size),
        fill: match base.fill_gradient {
            Some(_) => style.fill,
            None => new(&style.fill, &base.fill),
        },
        stroke: new(&style.stroke, &base.stroke),
        stroke_width: new(&style.stroke_width, &base.stroke_width),
        outer_stroke: new(&style.outer_stroke, &base.outer_stroke),
        outer_stroke_width: new(&style.outer_stroke_width, &base.outer_stroke_width),
        fill_gradient: None,
        tracking: None,
    }
}

/// Where each of `words`, a model's picks, is in `text`, as UTF-8 byte
/// ranges in order, as Booharu 0.0.4 found them: each pick claims its first
/// exact occurrence, case and all, that no earlier pick claimed and that
/// overlaps none claimed, so a word picked twice stresses two occurrences. A
/// pick is found only as whole words, with no letter, digit, `_` or
/// combining mark joined to either end, but for one holding a CJK character,
/// which is found anywhere, as those scripts don't space their words. A
/// blank pick, and one with no free occurrence, finds nothing.
pub fn find_words(text: &str, words: &[String]) -> Vec<Range<usize>> {
    let mut found: Vec<Range<usize>> = Vec::new();
    for word in words {
        if word.trim().is_empty() {
            continue;
        }
        let cjk = word.chars().any(is_cjk);
        let joins_before = word.chars().next().is_some_and(is_word);
        let joins_after = word.chars().next_back().is_some_and(is_word);
        for (start, _) in text.char_indices() {
            if !text[start..].starts_with(word.as_str()) {
                continue;
            }
            let end = start + word.len();
            let joined = (joins_before && text[..start].chars().next_back().is_some_and(is_word))
                || (joins_after && text[end..].chars().next().is_some_and(is_word));
            if !cjk && joined {
                continue;
            }
            if found
                .iter()
                .any(|claimed| start < claimed.end && claimed.start < end)
            {
                continue;
            }
            found.push(start..end);
            break;
        }
    }
    found.sort_unstable_by_key(|range| range.start);
    found
}

/// Whether `ch` is part of a word: a letter, a digit, `_`, or a combining
/// mark.
fn is_word(ch: char) -> bool {
    ch.is_alphanumeric()
        || ch == '_'
        || matches!(
            ch as u32,
            0x0300..=0x036f | 0x1ab0..=0x1aff | 0x1dc0..=0x1dff | 0x20d0..=0x20ff | 0xfe20..=0xfe2f
        )
}

/// Whether `ch` is a CJK character: Han, kana, or hangul.
fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x2e80..=0x9fff | 0xac00..=0xd7af | 0xf900..=0xfaff | 0xff66..=0xff9f | 0x20000..=0x323af
    )
}

/// `content` with `change` giving each character in `range`, UTF-8 byte
/// offsets, its marks from the ones it has, the others keeping theirs, and
/// equal neighbours joined; none for a range past the text, backwards, or
/// off a character boundary.
pub fn restyled(
    content: &RichText,
    range: Range<usize>,
    change: impl Fn(&CharStyle) -> CharStyle,
) -> Option<RichText> {
    let text = content.text();
    if range.start > range.end
        || !text.is_char_boundary(range.start)
        || !text.is_char_boundary(range.end)
    {
        return None;
    }
    let mut spans = Vec::new();
    let mut start = 0;
    for span in &content.0 {
        let end = start + span.insert.len();
        let from = range.start.clamp(start, end) - start;
        let to = range.end.clamp(start, end) - start;
        for (piece, inside) in [(0..from, false), (from..to, true), (to..end - start, false)] {
            // `change` sees only characters in the range.
            if piece.is_empty() {
                continue;
            }
            let insert = span.insert.get(piece)?;
            let attributes = match inside {
                true => change(&span.attributes),
                false => span.attributes.clone(),
            };
            spans.push(Span {
                insert: insert.to_owned(),
                attributes,
            });
        }
        start = end;
    }
    Some(joined(spans))
}

/// Whether no character of `content` in `range`, UTF-8 byte offsets, has a
/// mark.
pub fn unmarked(content: &RichText, range: Range<usize>) -> bool {
    let mut start = 0;
    content.0.iter().all(|span| {
        let end = start + span.insert.len();
        let overlaps = start < range.end && range.start < end;
        start = end;
        !overlaps || span.attributes == CharStyle::default()
    })
}

/// What a run's translation marks for emphasis: how a stressed word looks,
/// and the words its model picked. The default marks none, though the
/// translation still takes off the marks a run left on the one it replaces.
#[derive(Clone, Copy, Debug, Default)]
pub struct Marking<'a> {
    /// `[booharu-emphasis]`, as the run's snapshot holds it; none, or one
    /// with `ai` off, marks no word.
    pub settings: Option<&'a EmphasisSettings>,
    /// The words the model picked, as [`find_words`] takes them.
    pub words: &'a [String],
}

impl<'a> Marking<'a> {
    /// The settings the words are marked with: none while no word was
    /// picked, or the settings mark none.
    pub fn active(&self) -> Option<&'a EmphasisSettings> {
        self.settings
            .filter(|settings| settings.ai && !self.words.is_empty())
    }
}

/// `content`, a block's translation, without the values `record`'s trusted
/// marks ([`Auto::trusted_marks`]) set, on the characters each covers and
/// only where they still have them: what's left was marked, or changed, by
/// hand. A mark that starts or ends the text the stage wrote covers too the
/// whitespace added before or after it, which the scene gives the marks of
/// the character beside it ([`RichText::replaced`]), so none of the run's
/// values stays behind there to ride onto the next translation.
pub fn stripped(content: &RichText, record: &Auto) -> RichText {
    let marks = record.trusted_marks(content);
    if marks.is_empty() {
        return content.clone();
    }
    let text = content.text();
    let (first, last) = (text.len() - text.trim_start().len(), text.trim_end().len());
    let mut out = content.clone();
    for mark in marks {
        let start = if mark.start == first { 0 } else { mark.start };
        let end = if mark.end == last {
            text.len()
        } else {
            mark.end
        };
        if let Some(next) = restyled(&out, start..end, |marks| without(marks, &mark.style)) {
            out = next;
        }
    }
    out
}

/// `marks` without each value `set` holds too.
fn without(marks: &CharStyle, set: &CharStyle) -> CharStyle {
    fn kept<T: PartialEq + Clone>(value: &Option<T>, set: &Option<T>) -> Option<T> {
        value.clone().filter(|_| value != set)
    }
    CharStyle {
        family: kept(&marks.family, &set.family),
        weight: kept(&marks.weight, &set.weight),
        italic: kept(&marks.italic, &set.italic),
        size: kept(&marks.size, &set.size),
        fill: kept(&marks.fill, &set.fill),
        stroke: kept(&marks.stroke, &set.stroke),
        stroke_width: kept(&marks.stroke_width, &set.stroke_width),
        outer_stroke: kept(&marks.outer_stroke, &set.outer_stroke),
        outer_stroke_width: kept(&marks.outer_stroke_width, &set.outer_stroke_width),
        fill_gradient: kept(&marks.fill_gradient, &set.fill_gradient),
        tracking: kept(&marks.tracking, &set.tracking),
    }
}

/// A run's translation `text` over `content`, the block's translation
/// before it, whose hand-edit record is `record`, and the marks to record:
/// `content` without the run's own marks ([`stripped`]), carried over to
/// `text` as Koharu carries a text's marks ([`RichText::replaced`]), then
/// each of `words` [`find_words`] finds marked with `patch` where none of
/// its characters has a mark yet, with one [`AutoMark`] holding exactly
/// `patch` for each. With no words, or no patch, or one setting nothing, it
/// only strips.
pub fn marked(
    content: &RichText,
    record: &Auto,
    text: &str,
    words: &[String],
    patch: Option<&CharStyle>,
) -> (RichText, Vec<AutoMark>) {
    let mut out = stripped(content, record).replaced(text);
    let mut marks = Vec::new();
    let Some(patch) = patch.filter(|patch| **patch != CharStyle::default()) else {
        return (out, marks);
    };
    for range in find_words(text, words) {
        if !unmarked(&out, range.clone()) {
            continue;
        }
        if let Some(next) = restyled(&out, range.clone(), |marks| patch.or(marks)) {
            out = next;
            marks.push(AutoMark {
                start: range.start,
                end: range.end,
                style: patch.clone(),
            });
        }
    }
    (out, marks)
}

/// `spans` with empty ones dropped and equal neighbours joined, as the scene
/// keeps a text's.
fn joined(spans: impl IntoIterator<Item = Span>) -> RichText {
    let mut out: Vec<Span> = Vec::new();
    for span in spans {
        match out.last_mut() {
            _ if span.insert.is_empty() => {}
            Some(last) if last.attributes == span.attributes => last.insert.push_str(&span.insert),
            _ => out.push(span),
        }
    }
    RichText(out)
}

/// The heaviest weight a style takes.
const MAX_WEIGHT: u16 = 1000;

/// The smallest and largest size factors a style takes.
const SIZE_FACTORS: std::ops::RangeInclusive<f64> = 0.25..=4.0;

/// The widest border a style takes, in pixels.
const MAX_WIDTH: f64 = 100.0;

fn family<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    let family = Option::<String>::deserialize(deserializer)?;
    if family
        .as_deref()
        .is_some_and(|family| family.trim().is_empty())
    {
        return Err(D::Error::custom(
            "an emphasis font family can't be blank: leave it out to keep the text's own",
        ));
    }
    Ok(family)
}

fn weight<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u16>, D::Error> {
    let weight = Option::<u16>::deserialize(deserializer)?;
    if let Some(weight) = weight
        && !(1..=MAX_WEIGHT).contains(&weight)
    {
        return Err(D::Error::custom(format!(
            "an emphasis weight of {weight} isn't from 1 to {MAX_WEIGHT}"
        )));
    }
    Ok(weight)
}

fn size_factor<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<f64>, D::Error> {
    let factor = Option::<f64>::deserialize(deserializer)?;
    if let Some(factor) = factor
        && !SIZE_FACTORS.contains(&factor)
    {
        return Err(D::Error::custom(format!(
            "an emphasis {factor} times the text's size isn't from {} to {} times",
            SIZE_FACTORS.start(),
            SIZE_FACTORS.end()
        )));
    }
    Ok(factor)
}

fn width<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<f64>, D::Error> {
    let width = Option::<f64>::deserialize(deserializer)?;
    if let Some(width) = width
        && !(0.0..=MAX_WIDTH).contains(&width)
    {
        return Err(D::Error::custom(format!(
            "an emphasis border {width} pixels wide isn't from 0 to {MAX_WIDTH}"
        )));
    }
    Ok(width)
}

/// `[booharu-emphasis] default`: a style setting at least one value, as the
/// file can't hold one setting none.
fn default_style<'de, D: Deserializer<'de>>(deserializer: D) -> Result<EmphasisStyle, D::Error> {
    let style = EmphasisStyle::deserialize(deserializer)?;
    if style == EmphasisStyle::default() {
        return Err(D::Error::custom(
            "the default emphasis sets nothing: give it at least one value",
        ));
    }
    Ok(style)
}

/// `[booharu-emphasis] categories`: each entry setting at least one value,
/// by a category's id. A null entry, which only a JSON caller can send,
/// removes it.
fn categories<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<LetteringCategory, EmphasisStyle>, D::Error> {
    let entries = BTreeMap::<LetteringCategory, Option<EmphasisStyle>>::deserialize(deserializer)?;
    let mut categories = BTreeMap::new();
    for (category, style) in entries {
        let Some(style) = style else {
            continue;
        };
        if style == EmphasisStyle::default() {
            return Err(D::Error::custom(format!(
                "{}'s emphasis sets nothing: leave the category out instead",
                category.as_str()
            )));
        }
        categories.insert(category, style);
    }
    Ok(categories)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::testing::{saved, stored, unreadable};

    const RED: Rgba = Rgba([1.0, 0.0, 0.0, 1.0]);

    fn words(words: &[&str]) -> Vec<String> {
        words.iter().map(|&word| word.to_owned()).collect()
    }

    #[test]
    fn the_defaults_round_trip_and_a_category_inherits_what_it_leaves_out() {
        let mut settings: EmphasisSettings = toml::from_str("").expect("the defaults");
        assert_eq!(settings, EmphasisSettings::default());
        assert!(settings.ai);
        assert_eq!(settings.default.weight, Some(700));
        assert_eq!(settings.default.size_factor, Some(1.15));
        settings.categories.insert(
            LetteringCategory::Shout,
            EmphasisStyle {
                fill: Some(RED),
                size_factor: Some(1.3),
                ..EmphasisStyle::default()
            },
        );
        let saved = toml::to_string(&settings).expect("a table");
        let restored: EmphasisSettings = toml::from_str(&saved).expect("read back");
        assert_eq!(restored, settings);
        let json = serde_json::to_value(&settings).expect("JSON");
        assert_eq!(
            serde_json::from_value::<EmphasisSettings>(json).ok(),
            Some(settings.clone())
        );
        let shout = restored.style_for(Some(LetteringCategory::Shout));
        assert_eq!(shout.weight, Some(700));
        assert_eq!(shout.size_factor, Some(1.3));
        assert_eq!(shout.fill, Some(RED));
        assert_eq!(
            restored.style_for(Some(LetteringCategory::Thoughts)),
            restored.default
        );
        assert_eq!(restored.style_for(None), restored.default);
    }

    #[test]
    fn a_bad_value_isnt_taken_and_reads_as_its_default() {
        let refused = [
            "[default]\nsize_factor = nan",
            "[default]\nsize_factor = 0",
            "[default]\nsize_factor = 0.2",
            "[default]\nsize_factor = 4.5",
            "[default]\nsize_factor = inf",
            "[default]\nweight = 0",
            "[default]\nweight = 1001",
            "[default]\nweight = -1",
            "[default]\nstroke_width = -1",
            "[default]\nouter_stroke_width = 100.5",
            "[default]\nstroke_width = nan",
            "[default]\nfamily = \"  \"",
            "[default]\nfill = [1.5, 0, 0, 1]",
            "[default]\nouter_stroke = [0, 0, 0]",
            // The default sets nothing, as no file could hold.
            "[default]\nitalic = true\n[categories.shout]\n",
            "[categories.tone]\nweight = 900",
            "ai = \"yes\"",
        ];
        // The default's own italic, beside an empty entry, stays.
        let italic = EmphasisSettings {
            default: EmphasisStyle {
                italic: Some(true),
                ..EmphasisStyle::default()
            },
            ..EmphasisSettings::default()
        };
        for text in refused {
            assert!(
                toml::from_str::<EmphasisSettings>(text).is_err(),
                "took {text}"
            );
            // Edited into the file, it reads as its default.
            let default = if text.contains("italic") {
                italic.clone()
            } else {
                EmphasisSettings::default()
            };
            assert_eq!(
                stored::<EmphasisSettings>(text).ok(),
                Some(default),
                "{text}"
            );
        }
        // The section's other values stay; a file that isn't TOML fails.
        let kept: EmphasisSettings =
            stored("ai = \"yes\"\n[categories.whisper]\nweight = 1").expect("the section");
        assert!(kept.ai);
        assert_eq!(kept.categories[&LetteringCategory::Whisper].weight, Some(1));
        assert!(unreadable::<EmphasisSettings>().is_err());
        let empty = serde_json::from_value::<EmphasisSettings>(json!({"default": {}}));
        assert!(empty.is_err(), "a default setting nothing");
        let taken: EmphasisSettings = toml::from_str(
            "[default]\nweight = 1000\nsize_factor = 4\nouter_stroke_width = 100\n\
             [categories.whisper]\nweight = 1\nsize_factor = 0.25\nstroke_width = 0\n",
        )
        .expect("the bounds");
        assert_eq!(taken.default.weight, Some(1000));
        assert_eq!(
            taken.categories[&LetteringCategory::Whisper].size_factor,
            Some(0.25)
        );
        // A null entry, from a JSON caller, removes it.
        let removed: EmphasisSettings =
            serde_json::from_value(json!({"categories": {"shout": null}})).expect("no entry");
        assert!(removed.categories.is_empty());
    }

    #[test]
    fn a_change_over_a_bad_stored_style_writes_it_whole() {
        // A category whose weight doesn't read gives the categories their
        // default, none; a change to them saves them as read with it.
        let whisper = EmphasisStyle {
            italic: Some(true),
            ..EmphasisStyle::default()
        };
        let (outcome, read, text) = saved::<EmphasisSettings>(
            "[categories.shout]\nsize_factor = 2\n[categories.whisper]\nweight = 5000",
            |emphasis| {
                assert!(emphasis.categories.is_empty());
                emphasis
                    .categories
                    .insert(LetteringCategory::Whisper, whisper.clone());
            },
        );
        outcome.expect("saved");
        assert_eq!(
            read.categories,
            BTreeMap::from([(LetteringCategory::Whisper, whisper)])
        );
        assert!(!text.contains("5000"), "{text}");
        // A default whose only value doesn't read is the default look; a
        // change to it saves that look with the change, a weight of its own
        // or italic beside bold at 115%.
        let stored = "[default]\nweight = 5000";
        for (change, wanted) in [
            (
                EmphasisStyle {
                    weight: Some(600),
                    ..EmphasisStyle::default()
                },
                (Some(600), None),
            ),
            (
                EmphasisStyle {
                    italic: Some(true),
                    ..EmphasisStyle::default()
                },
                (Some(BOLD), Some(true)),
            ),
        ] {
            let (outcome, read, text) = saved::<EmphasisSettings>(stored, |emphasis| {
                assert_eq!(emphasis.default, EmphasisSettings::default().default);
                emphasis.default = change.or(&emphasis.default);
            });
            outcome.expect("saved");
            assert_eq!((read.default.weight, read.default.italic), wanted);
            assert_eq!(read.default.size_factor, Some(1.15));
            assert!(!text.contains("5000"), "{text}");
        }
    }

    #[test]
    fn a_cleared_default_value_stays_cleared() {
        // TOML has no null: a value cleared from the default is left out of
        // its table, which the file holds whole, so the default's own value
        // doesn't come back.
        let settings = EmphasisSettings {
            default: EmphasisStyle {
                weight: Some(700),
                ..EmphasisStyle::default()
            },
            ..EmphasisSettings::default()
        };
        let saved = toml::to_string(&settings).expect("a table");
        let table: toml::Table = toml::from_str(&saved).expect("a table");
        assert!(table["default"].get("size_factor").is_none(), "{saved}");
        let restored: EmphasisSettings = toml::from_str(&saved).expect("read back");
        assert_eq!(restored, settings);
        assert_eq!(restored.default.size_factor, None);
    }

    #[test]
    fn a_patch_sets_what_the_text_doesnt_draw() {
        let base = CharStyle {
            size: Some(30.0),
            fill: Some(RED),
            ..CharStyle::default()
        };
        let style = EmphasisStyle {
            fill: Some(RED),
            stroke_width: Some(2.0),
            ..EmphasisSettings::default().default
        };
        assert_eq!(
            patch(&style, &base),
            CharStyle {
                weight: Some(700),
                size: Some(34.5),
                stroke_width: Some(2.0),
                ..CharStyle::default()
            }
        );
        // Without a size, the renderer's 24 pixels, to a hundredth.
        let scaled = EmphasisStyle {
            size_factor: Some(1.333),
            ..EmphasisStyle::default()
        };
        assert_eq!(
            patch(&scaled, &CharStyle::default()).size,
            Some(31.99),
            "24 × 1.333"
        );
        // Regular and upright are what a text with neither draws, and a size
        // the same as the text's sets nothing.
        let plain = EmphasisStyle {
            weight: Some(400),
            italic: Some(false),
            size_factor: Some(1.0),
            ..EmphasisStyle::default()
        };
        assert_eq!(patch(&plain, &CharStyle::default()), CharStyle::default());
        // Upright and regular in a bold italic text.
        let bold_italic = CharStyle {
            weight: Some(700),
            italic: Some(true),
            ..CharStyle::default()
        };
        assert_eq!(
            patch(&plain, &bold_italic),
            CharStyle {
                weight: Some(400),
                italic: Some(false),
                ..CharStyle::default()
            }
        );
    }

    #[test]
    fn a_gradient_text_keeps_a_fill_the_same_as_its_own() {
        use koharu_scene::{Gradient, GradientKind, GradientStop};

        let gradient = Gradient {
            kind: GradientKind::Linear,
            angle: 90.0,
            stops: vec![
                GradientStop {
                    offset: 0.0,
                    color: RED,
                },
                GradientStop {
                    offset: 1.0,
                    color: Rgba([0.0, 0.0, 1.0, 1.0]),
                },
            ],
        };
        let base = CharStyle {
            fill: Some(RED),
            fill_gradient: Some(gradient),
            ..CharStyle::default()
        };
        let style = EmphasisStyle {
            fill: Some(RED),
            ..EmphasisStyle::default()
        };
        // The text draws its gradient, so the word gets its own fill, which
        // hides the gradient for it.
        let marks = patch(&style, &base);
        assert_eq!(
            (marks.fill, marks.fill_gradient.as_ref()),
            (Some(RED), None)
        );
        assert_eq!(marks.or(&base).fill_gradient, None);
        // A flat text drawing the same fill gets none.
        let flat = CharStyle {
            fill_gradient: None,
            ..base
        };
        assert_eq!(patch(&style, &flat).fill, None);
    }

    #[test]
    fn each_pick_claims_its_first_free_whole_word() {
        // As Booharu 0.0.4's tests, in bytes: the emoji is four.
        assert_eq!(
            find_words("🙂 NO! KNOW NO.", &words(&["NO", "NO", "missing", "NO"])),
            [5..7, 14..16]
        );
        assert_eq!(
            find_words("魔王だ魔王", &words(&["魔王", "魔王"])),
            [0..6, 9..15]
        );
        assert!(find_words("cafe\u{301}", &words(&["cafe"])).is_empty());
        let found = find_words("say NO now", &words(&["NO now", "NO"]));
        assert_eq!((found.len(), found.first()), (1, Some(&(4..10))));
        assert_eq!(
            find_words(
                "NO! NO 魔王だ魔王",
                &words(&["NO", "NO", "missing", "魔王", "魔王"])
            ),
            [0..2, 4..6, 7..13, 16..22]
        );
        // Blank picks and no picks find nothing; case counts.
        assert!(find_words("No no", &words(&["", "  ", "NO"])).is_empty());
        assert!(find_words("No no", &[]).is_empty());
    }

    #[test]
    fn a_range_is_restyled_only_on_character_boundaries() {
        let bold = CharStyle {
            weight: Some(700),
            ..CharStyle::default()
        };
        let content = RichText(vec![
            Span {
                insert: "Why ".to_owned(),
                attributes: CharStyle::default(),
            },
            Span {
                insert: "魔王?".to_owned(),
                attributes: bold.clone(),
            },
        ]);
        let unbold = |_: &CharStyle| CharStyle::default();
        let restyled_text = restyled(&content, 4..7, unbold).expect("on boundaries");
        assert_eq!(
            restyled_text,
            RichText(vec![
                Span {
                    insert: "Why 魔".to_owned(),
                    attributes: CharStyle::default(),
                },
                Span {
                    insert: "王?".to_owned(),
                    attributes: bold.clone(),
                },
            ])
        );
        assert!(unmarked(&restyled_text, 0..7));
        assert!(!unmarked(&restyled_text, 6..8));
        // Off a boundary, past the text, and backwards.
        let backwards = Range { start: 7, end: 4 };
        for range in [4..5, 5..8, 0..20, backwards] {
            assert_eq!(restyled(&content, range.clone(), unbold), None, "{range:?}");
        }
        assert_eq!(restyled(&content, 2..2, unbold), Some(content.clone()));
        // The change sees the marks of the characters in the range only.
        let seen = std::cell::RefCell::new(Vec::new());
        let seeing = |marks: &CharStyle| {
            seen.borrow_mut().push(marks.clone());
            marks.clone()
        };
        assert_eq!(restyled(&content, 0..4, seeing), Some(content.clone()));
        assert_eq!(seen.take(), [CharStyle::default()]);
        assert_eq!(restyled(&content, 2..2, seeing), Some(content.clone()));
        assert!(seen.take().is_empty());
    }

    /// A span of `insert` with the marks `attributes`.
    fn span(insert: &str, attributes: CharStyle) -> Span {
        Span {
            insert: insert.to_owned(),
            attributes,
        }
    }

    /// Marks of weight `weight` and size `size`.
    fn heavy(weight: u16, size: Option<f64>) -> CharStyle {
        CharStyle {
            weight: Some(weight),
            size,
            ..CharStyle::default()
        }
    }

    /// A record of the translation `text` as a run wrote it, with `marks`.
    fn record(text: &str, marks: &[(usize, usize, CharStyle)]) -> Auto {
        Auto {
            text: Some(crate::styles::text_hash(text)),
            marks: marks
                .iter()
                .map(|(start, end, style)| AutoMark {
                    start: *start,
                    end: *end,
                    style: style.clone(),
                })
                .collect(),
            ..Auto::default()
        }
    }

    #[test]
    fn a_runs_own_marks_come_off_only_where_they_still_hold() {
        let run = heavy(700, Some(27.6));
        // "NO" as the run marked it, "now" with its weight changed by hand,
        // and "you" marked by hand.
        let content = RichText(vec![
            span("Say ", CharStyle::default()),
            span("NO", run.clone()),
            span(" ", CharStyle::default()),
            span("now", heavy(900, Some(27.6))),
            span(" to ", CharStyle::default()),
            span("you", heavy(700, None)),
        ]);
        let text = content.text();
        let record = record(&text, &[(4, 6, run.clone()), (7, 10, run.clone())]);
        assert_eq!(
            stripped(&content, &record),
            RichText(vec![
                span("Say NO ", CharStyle::default()),
                span("now", heavy(900, None)),
                span(" to ", CharStyle::default()),
                span("you", heavy(700, None)),
            ])
        );
        // Once the text changed, the marks aren't the run's any more.
        let retyped = RichText(vec![
            span("Say ", CharStyle::default()),
            span("NO!", run.clone()),
        ]);
        assert_eq!(stripped(&retyped, &record), retyped);
    }

    #[test]
    fn a_runs_translation_marks_the_free_words_it_finds() {
        let bold = heavy(700, Some(27.6));
        let picks = words(&["NO", "you", "missing", "now"]);
        let at = |marks: &[AutoMark]| -> Vec<(usize, usize)> {
            marks.iter().map(|mark| (mark.start, mark.end)).collect()
        };
        // New wording: the run's mark on the old one comes off, and each
        // word picked is marked.
        let old_text = "NO, I said NO";
        let old = RichText(vec![
            span("NO", bold.clone()),
            span(", I said NO", CharStyle::default()),
        ]);
        let old_record = record(old_text, &[(0, 2, bold.clone())]);
        let text = "Say NO to you now";
        let (content, marks) = marked(&old, &old_record, text, &picks, Some(&bold));
        assert_eq!(
            content,
            RichText(vec![
                span("Say ", CharStyle::default()),
                span("NO", bold.clone()),
                span(" to ", CharStyle::default()),
                span("you", bold.clone()),
                span(" ", CharStyle::default()),
                span("now", bold.clone()),
            ])
        );
        assert_eq!(at(&marks), [(4, 6), (10, 13), (14, 17)]);
        assert!(marks.iter().all(|mark| mark.style == bold));
        let written = Auto {
            marks,
            ..record(text, &[])
        };
        assert_eq!(written.trusted_marks(&content), written.marks.as_slice());

        // "you" restyled by hand: translated again, it keeps the hand mark,
        // so its pick marks nothing there.
        let mut edited = content.clone();
        edited.0[3].attributes = heavy(900, None);
        let (again, marks) = marked(&edited, &written, text, &picks, Some(&bold));
        assert_eq!(
            again,
            RichText(vec![
                span("Say ", CharStyle::default()),
                span("NO", bold.clone()),
                span(" to ", CharStyle::default()),
                span("you", heavy(900, None)),
                span(" ", CharStyle::default()),
                span("now", bold.clone()),
            ])
        );
        assert_eq!(at(&marks), [(4, 6), (14, 17)]);

        // With no words, or no patch, or one setting nothing, it only
        // strips.
        for (picks, patch) in [
            (Vec::new(), Some(&bold)),
            (words(&["NO"]), None),
            (words(&["NO"]), Some(&CharStyle::default())),
        ] {
            let (content, marks) = marked(&edited, &written, text, &picks, patch);
            assert!(marks.is_empty());
            assert_eq!(
                content,
                RichText(vec![
                    span("Say NO to ", CharStyle::default()),
                    span("you", heavy(900, None)),
                    span(" now", CharStyle::default()),
                ]),
                "{picks:?} {patch:?}"
            );
        }
        // A first translation, with nothing before it, and picks that
        // aren't all there.
        let (content, marks) = marked(
            &RichText::default(),
            &Auto::default(),
            "魔王だ",
            &words(&["魔王", "", "王だ"]),
            Some(&bold),
        );
        assert_eq!(at(&marks), [(0, 6)]);
        assert_eq!(
            content,
            RichText(vec![
                span("魔王", bold.clone()),
                span("だ", CharStyle::default())
            ])
        );
    }
}
