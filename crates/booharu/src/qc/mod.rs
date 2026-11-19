//! The quality check (F11): what on a page needs a look, from one rule
//! engine the QC tab, the ? tag, Box details and the agent's tools share.
//!
//! [`check_page`] walks a page's story, the blocks and text layers a reader
//! sees in reading order, leaving out each locked one and each inside a
//! locked layer, which count as finished, and answers their [`Finding`]s in
//! story order, then in the checks' order: overflow, too small, a small
//! word, a missing translation, an empty source, leftover source text, an
//! odd font, glossary slips, and the agent's flags. A text layer gets only
//! the size checks and flags. Every page is checked: none waits for a run.
//!
//! - Sizes and overflow are measured as the caller's renderer draws the
//!   text ([`measure`]); the rules are pure ([`rules`]).
//! - What a check reads beside the page, the settings and the chapter's
//!   glossary and majority fonts, is gathered once into [`Inputs`].
//! - Findings are never stored here. Mark OK is the layer's `qc_ok`
//!   attribute, whose entries name a check and the layer's fingerprint
//!   (`booharu_core::qc`), so a mark hides its finding only until the layer
//!   changes, and the agent's flags are its `flags`, which [`edits`]
//!   writes. A flag is never hidden: it goes when dismissed.

mod edits;
mod measure;
mod rules;

use std::{collections::BTreeMap, path::Path};

use anyhow::Result;
use booharu_core::{
    attrs::{self, Auto, Flags, Origin, QcOk},
    context::{Chapter, ChapterContext, Series, SeriesStore, slips},
    lettering::{self, LetteringCategory},
    qc::{self as marks, GLOSSARY},
    settings::{LetteringSettings, QcSettings},
    status::{self, DETECTION, OCR, TRANSLATION},
    styles::recorded,
};
use koharu_renderer::Renderer;
use koharu_scene::{Content, LanguageIdentifier, Layer, LayerId, Page, PageId, Project, Text};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use self::rules::{Facts, Judge};
pub use self::{
    edits::{
        CLEAR_QC_OK, DISMISS_FLAG, FLAG_LINE, Flagged, MARK_QC_OK, MAX_FLAG_MESSAGE, dismiss_flag,
        flag_line, mark_qc_ok,
    },
    measure::{Measured, Sized, measure},
};

/// The check of text running past its box.
pub const OVERFLOW: &str = "overflow";
/// The check of text drawn under the smallest readable size.
pub const TOO_SMALL: &str = "too_small";
/// The check of one styled word drawn under the smallest readable size.
pub const SMALL_WORD: &str = "small_word";
/// The check of a line Koharu's Translation stage left untranslated.
pub const MISSING_TRANSLATION: &str = "missing_translation";
/// The check of a block Koharu's stages found and read nothing in.
pub const EMPTY_SOURCE: &str = "empty_source";
/// The check of source text a translation keeps, or a line given back
/// unchanged shows.
pub const LEFTOVER_SOURCE: &str = "leftover_source";
/// The check of a font off its lettering category's majority.
pub const ODD_FONT: &str = "odd_font";
/// The agent's flags' check, which Mark OK doesn't take.
pub const FLAG: &str = "flag";

/// Every check Mark OK takes by its id alone; a glossary slip's is
/// `glossary:` and its term's source.
pub const CHECKS: [&str; 7] = [
    OVERFLOW,
    TOO_SMALL,
    SMALL_WORD,
    MISSING_TRANSLATION,
    EMPTY_SOURCE,
    LEFTOVER_SOURCE,
    ODD_FONT,
];

/// One thing on a page that needs a look.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type, JsonSchema)]
pub struct Finding {
    pub page: PageId,
    /// The block or text layer it's on.
    pub layer: LayerId,
    /// Its check's id, which Mark OK takes: one of [`CHECKS`],
    /// `glossary:<term source>`, or [`FLAG`].
    pub check: String,
    pub problem: Problem,
    pub source: Source,
    /// The line it's on: its translation's wording, or its source's when it
    /// has none, or a text layer's text, trimmed.
    pub text: String,
    /// Whether a person marked it OK while the layer was as it is now. A
    /// flag never is.
    pub hidden: bool,
}

/// What a finding found, with what the interface tells it with. Sizes are
/// page pixels, rounded to one decimal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Problem {
    /// The text runs past its box.
    Overflow,
    /// The text is drawn at `size` at its smallest, under `limit`, the
    /// smallest readable size on its page, as its box sets it.
    TooSmall { size: f32, limit: f32 },
    /// One styled word, `word`, is drawn at `size`, under `limit`, while its
    /// box sets the rest larger.
    SmallWord { word: String, size: f32, limit: f32 },
    /// The line has source text but no translation, on a page Koharu's
    /// Translation stage ran on.
    MissingTranslation,
    /// Koharu's stages found the block and read nothing in it.
    EmptySource,
    /// The line shows source text in a script its language doesn't write:
    /// in its translation, or, `unchanged`, in the page's own lettering of
    /// a line the model gave back unchanged.
    LeftoverSource { unchanged: bool },
    /// The block's font isn't `expected`, the font most of its lettering
    /// category's blocks in the chapter are set in, but `font`; none for
    /// the default font.
    OddFont {
        expected: Option<String>,
        font: Option<String>,
    },
    /// The translation leaves out the linked series' glossary term with the
    /// source `source`, which it should write as `target`.
    Glossary { source: String, target: String },
    /// The agent flagged the line for a person to review.
    Flag { message: String },
}

impl Problem {
    /// The id of the check that found it.
    pub fn check(&self) -> String {
        match self {
            Self::Overflow => OVERFLOW.to_owned(),
            Self::TooSmall { .. } => TOO_SMALL.to_owned(),
            Self::SmallWord { .. } => SMALL_WORD.to_owned(),
            Self::MissingTranslation => MISSING_TRANSLATION.to_owned(),
            Self::EmptySource => EMPTY_SOURCE.to_owned(),
            Self::LeftoverSource { .. } => LEFTOVER_SOURCE.to_owned(),
            Self::OddFont { .. } => ODD_FONT.to_owned(),
            Self::Glossary { source, .. } => format!("{GLOSSARY}{source}"),
            Self::Flag { .. } => FLAG.to_owned(),
        }
    }

    /// Who found it.
    pub fn source(&self) -> Source {
        match self {
            Self::Flag { .. } => Source::Agent,
            _ => Source::Auto,
        }
    }
}

/// Who found a finding: Booharu's automatic checks, or the agent's review.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, specta::Type, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Auto,
    Agent,
}

/// Each lettering category's majority font across a chapter: the family
/// more than half of the category's translated blocks are set in, none for
/// the default font, for a category with at least 3 of them. A category
/// without one is left out.
pub type Majorities = BTreeMap<LetteringCategory, Option<String>>;

/// What a check reads beside a page.
#[derive(Clone, Debug, PartialEq)]
pub struct Inputs {
    /// `[booharu-qc] min_size`: the smallest readable text, as a share of
    /// a page's shorter side.
    pub min_size: f32,
    /// `[pipeline] translator.target`: the language a translation without a
    /// tag of its own is in, and a line given back unchanged was to be.
    pub target: LanguageIdentifier,
    /// `[booharu-lettering]`, by whose categories the odd font check groups
    /// blocks.
    pub lettering: LetteringSettings,
    /// The series the chapter links, whose glossary the slips are checked
    /// against; none when it links none, or it couldn't be read.
    pub series: Option<Series>,
    /// Each lettering category's majority font in the chapter.
    pub majorities: Majorities,
}

impl Inputs {
    /// The inputs a check of `project` reads with these settings and the
    /// chapter's linked series `series`, its majority fonts worked out from
    /// the project.
    pub fn new(
        project: &Project,
        qc: &QcSettings,
        target: LanguageIdentifier,
        lettering: LetteringSettings,
        series: Option<Series>,
    ) -> Self {
        let majorities = majorities(project, &lettering);
        Self {
            min_size: qc.min_size,
            target,
            lettering,
            series,
            majorities,
        }
    }
}

/// What a check of `project`, which the project file `file` holds, reads:
/// the smallest readable size of `qc`, `[booharu-qc]`; `target`,
/// `[pipeline] translator.target`; `lettering`, `[booharu-lettering]`; and
/// the series the chapter links, from the store `store` opens. A chapter
/// or series that can't be read is logged and skips the glossary check
/// alone.
pub fn inputs(
    project: &Project,
    file: &Path,
    store: impl FnOnce() -> Result<SeriesStore>,
    qc: &QcSettings,
    target: LanguageIdentifier,
    lettering: LetteringSettings,
) -> Inputs {
    let series = match Chapter::read(file, store) {
        Ok(chapter) => chapter.series,
        Err(error) => {
            tracing::warn!("the quality check checks no glossary slips: {error:#}");
            None
        }
    };
    Inputs::new(project, qc, target, lettering, series)
}

/// Each lettering category's majority font across `project`, as `lettering`
/// sorts its blocks into categories: of every translated block a reader
/// sees, locked ones and those set by hand among them, the family its
/// translation's style sets.
pub fn majorities(project: &Project, lettering: &LetteringSettings) -> Majorities {
    let mut fonts: BTreeMap<LetteringCategory, Vec<Option<String>>> = BTreeMap::new();
    for page in project.pages() {
        for id in project.story(page.id).unwrap_or_default() {
            let Ok(layer) = project.layer(id) else {
                continue;
            };
            let Some(translation) = written(layer) else {
                continue;
            };
            if let Some(effective) = lettering::effective(page, id, lettering) {
                fonts
                    .entry(effective.category)
                    .or_default()
                    .push(translation.style.chars.family.clone());
            }
        }
    }
    fonts
        .into_iter()
        .filter_map(|(category, fonts)| Some((category, rules::majority(&fonts)?)))
        .collect()
}

/// What on `page` of `project` needs a look, with `inputs`, measured as
/// `renderer` draws it: each block and text layer of its story but the
/// locked ones, in story order, each one's findings in the checks' order.
/// A finding a person marked OK at the layer's fingerprint now is hidden.
///
/// It blocks while it lays text out, as a bundled font a text asks for is
/// downloaded the first time, so it runs on a thread that may block, with
/// the renderer the canvas draws with, never `Renderer::get()`. A text that
/// can't be laid out is logged and gets no size checks.
pub fn check_page(
    renderer: &Renderer,
    project: &Project,
    page: PageId,
    inputs: &Inputs,
) -> Result<Vec<Finding>> {
    let view = project.page(page)?;
    let shorter = view.size.width.min(view.size.height);
    let limit = inputs.min_size * shorter as f32;
    let judge = Judge {
        limit,
        target: &inputs.target,
        majorities: &inputs.majorities,
    };
    let translated = translated(view);
    let mut findings = Vec::new();
    for id in project.story(page)? {
        let layer = project.layer(id)?;
        if crate::styles::locked(project, layer)? {
            continue;
        }
        let Some(mut facts) = facts(view, layer, translated, inputs) else {
            continue;
        };
        facts.measured = measured(renderer, layer);
        let fingerprint = marks::fingerprint(layer)?;
        let ok = attrs::get::<QcOk>(layer);
        let text = shown(layer);
        for problem in rules::problems(&facts, &judge) {
            let check = problem.check();
            let source = problem.source();
            let hidden = source == Source::Auto && marks::marked(ok.as_ref(), &check, &fingerprint);
            findings.push(Finding {
                page,
                layer: id,
                check,
                problem,
                source,
                text: text.clone(),
                hidden,
            });
        }
    }
    Ok(findings)
}

/// Whether Koharu's Translation stage ran on `page`: it wrote one of its
/// blocks, or gave one back unchanged.
fn translated(page: &Page) -> bool {
    status::blocks(page)
        .iter()
        .any(|(layer, _, _)| status::translation_ran(layer))
}

/// What the checks read of `layer`, a block or a text on `page`, but its
/// measurements; none for a layer of another kind. `translated` says
/// whether Koharu's Translation stage ran on the page.
fn facts(page: &Page, layer: &Layer, translated: bool, inputs: &Inputs) -> Option<Facts> {
    let flags = attrs::get::<Flags>(layer)
        .map(|Flags(flags)| flags.into_iter().map(|flag| flag.message).collect())
        .unwrap_or_default();
    match &layer.content {
        Content::Text(_) => Some(Facts {
            flags,
            ..Facts::default()
        }),
        Content::Block(block) => {
            let source = block.source.content.text();
            let translation = block.translation.as_ref();
            let wording = translation.map(|translation| translation.content.text());
            let slips = wording.as_deref().map_or_else(Vec::new, |wording| {
                slips(
                    &source,
                    wording,
                    &ChapterContext::default(),
                    inputs.series.as_ref(),
                )
            });
            Some(Facts {
                block: true,
                language: translation.map(|translation| translation.language.clone()),
                cleanup_visible: block.cleanup.visible,
                detected: recorded(layer, DETECTION),
                read: recorded(layer, OCR),
                given_back: status::given_back(layer, block),
                translated_page: translated,
                category: lettering::effective(page, layer.id, &inputs.lettering)
                    .map(|effective| effective.category),
                font: translation.and_then(|translation| translation.style.chars.family.clone()),
                hand_set: translation.is_some_and(|translation| hand_set(layer, translation)),
                slips,
                flags,
                source,
                translation: wording,
                measured: None,
            })
        }
        _ => None,
    }
}

/// `layer`'s text as drawn, a block's translation's or a text layer's own;
/// none when it's blank, as it measures nothing, or can't be laid out,
/// which is logged.
fn measured(renderer: &Renderer, layer: &Layer) -> Option<Measured> {
    let text = match &layer.content {
        Content::Block(block) => block.translation.as_ref()?,
        Content::Text(text) => &**text,
        _ => return None,
    };
    if text.content.text().trim().is_empty() {
        return None;
    }
    measure(renderer, layer.id, text)
        .inspect_err(|error| {
            tracing::warn!(
                "the quality check can't lay out the text of layer {}: {error:#}",
                layer.id
            );
        })
        .ok()
        .flatten()
}

/// The translation of `layer`, a block, when its wording isn't blank.
fn written(layer: &Layer) -> Option<&Text> {
    let Content::Block(block) = &layer.content else {
        return None;
    };
    block
        .translation
        .as_ref()
        .filter(|translation| !translation.content.text().trim().is_empty())
}

/// Whether the font of `translation`, which `layer` holds, was set by hand
/// or by the agent on purpose (T3): the agent chose its style, its family
/// isn't the one Booharu's automatic steps last wrote, or, with nothing
/// recorded, no Koharu stage wrote the translation, as one typed by hand.
fn hand_set(layer: &Layer, translation: &Text) -> bool {
    let record = Auto::of(layer);
    record.origin == Origin::Agent
        || match &record.style {
            Some(style) => style.family != translation.style.chars.family,
            None => !recorded(layer, TRANSLATION),
        }
}

/// The line `layer` holds, as a finding shows it: a block's translation's
/// wording, or its source's when it has none, or a text layer's text,
/// trimmed.
fn shown(layer: &Layer) -> String {
    let text = match &layer.content {
        Content::Block(block) => block
            .translation
            .as_ref()
            .map(|translation| translation.content.text())
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| block.source.content.text()),
        Content::Text(text) => text.content.text(),
        _ => String::new(),
    };
    text.trim().to_owned()
}

#[cfg(test)]
mod tests;
