//! Booharu's settings: a `[booharu-<area>]` table per area in the Booharu
//! home's `config.toml`, each a [`Section`] that owns its defaults, as
//! Koharu's own sections do. A value is checked as it is read, as Koharu reads
//! its own: one the file holds that doesn't read reads as its default, and
//! only a file that can't be read, such as one that isn't TOML, fails; one a
//! caller gives that doesn't read is refused, so a save never writes it. A
//! save writes only what differs from the defaults.
//!
//! [`BooharuPreferences`] holds every section, keyed by its table's name as
//! Koharu's `Preferences` keys its own: the desktop's settings commands read
//! and save it whole, and the settings tool changes one section by its name,
//! saving that section alone.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context as _, anyhow, bail};
use koharu_config::Section;
use koharu_scene::Rgba;
use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned, de::Error as _};
use serde_json::{Map, Value, map::Entry};

use crate::{
    emphasis::EmphasisSettings,
    lettering::{CategoryFont, LetteringCategory},
    runs::{RunPolicy, RunsSettings},
};

/// `[booharu-mcp]`: the MCP server outside AI apps reach Booharu through.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct McpSettings {
    /// The port on 127.0.0.1 it listens on, from 1024 to 65535, read at
    /// launch. Booharu has its own, so it runs beside Koharu, which serves
    /// on 7878.
    #[serde(deserialize_with = "port")]
    #[specta(type = u16)]
    pub port: u16,
}

impl Default for McpSettings {
    fn default() -> Self {
        Self { port: 47821 }
    }
}

impl Section for McpSettings {
    const NAME: &'static str = "booharu-mcp";
}

/// A port a client can keep in its configuration: 0 would take another free
/// port at every launch, and the ones below 1024 are the system's.
fn port<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u16, D::Error> {
    let port = u16::deserialize(deserializer)?;
    if port < 1024 {
        return Err(D::Error::custom(format!(
            "port {port} isn't between 1024 and 65535"
        )));
    }
    Ok(port)
}

/// `[booharu-ocr-box]`: the O tool, whose box drawn on a page becomes a text
/// block that is read, and as its mode says translated and cleaned.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct OcrBoxSettings {
    /// What a drawn box goes through when the drawing picks no other mode.
    pub default_mode: OcrBoxMode,
}

impl Section for OcrBoxSettings {
    const NAME: &'static str = "booharu-ocr-box";
}

/// What a box drawn with the O tool goes through, Booharu 0.0.4's three
/// modes: it's always read. Only a box that is cleaned gets a mask, the whole
/// box, so reading it never erases the art inside.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    PartialEq,
    Serialize,
    Deserialize,
    specta::Type,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OcrBoxMode {
    /// Read only.
    Read,
    /// Read, then translated.
    #[default]
    ReadTranslate,
    /// Read, cleaned under the whole box, then translated.
    ReadTranslateClean,
}

/// `[booharu-lettering]`: how new text looks, and the font each lettering
/// category letters its text in. The default font isn't one of these
/// settings: new text gets a copy of the first of Koharu's Preferred fonts,
/// `[renderer] font_families`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct LetteringSettings {
    /// The default text look: the colours and outline new text gets, the T
    /// tool's always, and a new translation's when `detect_colors` is off.
    pub default: DefaultTextStyle,
    /// Whether a new translation keeps the colours Koharu measured from the
    /// original lettering, or, typed into a block with none, copied from its
    /// source. When off, it gets `default`'s instead. Existing translations
    /// keep theirs either way.
    pub detect_colors: bool,
    /// Pick fonts automatically: whether a block's translation is lettered
    /// in its category's font, as it's written, as its category changes, and
    /// by Apply. Off, Booharu writes no category font.
    pub auto: bool,
    /// The categories given their own font, by id: a category with none uses
    /// Normal Dialogue's, and with neither the default font.
    #[serde(deserialize_with = "crate::lettering::categories")]
    #[specta(type = BTreeMap<LetteringCategory, CategoryFont>)]
    pub categories: BTreeMap<LetteringCategory, CategoryFont>,
    /// Text whose glyphs are under this share of the page's height is small
    /// text, above 0 and at most 1: 0.012 is 24 pixels on a page 2000 high.
    #[serde(deserialize_with = "crate::lettering::small_text_share")]
    #[specta(type = f64)]
    pub small_text: f64,
    /// How sure, from 0 to 1, a model must be of a delivery category, such
    /// as a whisper, for its label to win over small text and the balloon's
    /// shape.
    #[serde(
        deserialize_with = "crate::lettering::confidence",
        serialize_with = "crate::lettering::decimal"
    )]
    #[specta(type = f32)]
    pub model_confidence: f32,
    /// Which sound effects a run translates and cleans: one it skips keeps
    /// the original art, its clean-up hidden. A block is a sound effect when
    /// its lettering category, as Booharu reads it, is Sound Effect.
    pub sfx: SfxMode,
    /// How a sound effect's subtitle looks: the small plain translation
    /// written beside an effect kept as art.
    pub subtitle: SubtitleStyle,
}

impl Default for LetteringSettings {
    fn default() -> Self {
        Self {
            default: DefaultTextStyle::default(),
            detect_colors: true,
            auto: false,
            categories: BTreeMap::new(),
            small_text: 0.012,
            model_confidence: 0.6,
            sfx: SfxMode::default(),
            subtitle: SubtitleStyle::default(),
        }
    }
}

impl Section for LetteringSettings {
    const NAME: &'static str = "booharu-lettering";
}

impl LetteringSettings {
    /// The section as the file holds it now, as a run's snapshot reads it: a
    /// stored value that doesn't read reads as its default, and only a file
    /// that can't be read, such as one that isn't TOML, fails.
    pub fn read() -> anyhow::Result<Self> {
        Self::checked(koharu_config::get())
    }

    /// `read`, the section as it was read, or why it couldn't be.
    fn checked(read: koharu_config::Result<Self>) -> anyhow::Result<Self> {
        read.with_context(|| {
            format!(
                "Booharu's default text style can't be read from [{}]",
                Self::NAME
            )
        })
    }
}

/// The default text look's colours and outline, as Booharu 0.0.4's
/// Settings card set them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct DefaultTextStyle {
    /// The text's colour; none draws Koharu's black.
    #[serde(deserialize_with = "colour")]
    #[specta(type = Option<Rgba>)]
    pub fill: Option<Rgba>,
    /// The outline's colour; none for no outline.
    #[serde(deserialize_with = "colour")]
    #[specta(type = Option<Rgba>)]
    pub stroke: Option<Rgba>,
    /// The outline's width in pixels, from 0 to 50, when it has a colour.
    #[serde(deserialize_with = "stroke_width")]
    #[specta(type = f64)]
    pub stroke_width: f64,
}

impl Default for DefaultTextStyle {
    fn default() -> Self {
        Self {
            fill: None,
            stroke: None,
            stroke_width: 1.5,
        }
    }
}

/// A colour of four channels, each from 0 to 1, as the scene holds one.
pub(crate) fn colour<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Rgba>, D::Error> {
    let colour = Option::<Rgba>::deserialize(deserializer)?;
    if let Some(Rgba(channels)) = colour
        && !channels.iter().all(|channel| (0.0..=1.0).contains(channel))
    {
        return Err(D::Error::custom(format!(
            "colour {channels:?} isn't four channels from 0 to 1"
        )));
    }
    Ok(colour)
}

/// A colour that must be set, of four channels each from 0 to 1.
fn solid<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Rgba, D::Error> {
    colour(deserializer)?.ok_or_else(|| D::Error::custom("a colour can't be left unset here"))
}

/// The widest outline the default look takes, in pixels.
const STROKE_WIDTH: f64 = 50.0;

fn stroke_width<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    let width = f64::deserialize(deserializer)?;
    if !(0.0..=STROKE_WIDTH).contains(&width) {
        return Err(D::Error::custom(format!(
            "an outline {width} pixels wide isn't between 0 and {STROKE_WIDTH}"
        )));
    }
    Ok(width)
}

/// Which sound effects a run translates, `[booharu-lettering] sfx`. A
/// sound effect a run skips keeps the original art: no translation, its
/// clean-up hidden. One with a subtitle is skipped whatever the mode.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    PartialEq,
    Serialize,
    Deserialize,
    specta::Type,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SfxMode {
    /// None: every sound effect stays as art.
    #[default]
    Skip,
    /// Only those inside a speech balloon, which are lettered like speech.
    InsideBubbles,
    /// Every one, as Koharu translates them.
    TranslateAll,
}

/// How a sound effect's subtitle looks, `[booharu-lettering] subtitle`: a
/// small plain line beside the effect, outlined `size / 9` wide.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct SubtitleStyle {
    /// Its font; none for the default font. Never blank.
    #[serde(deserialize_with = "subtitle_family")]
    #[specta(type = Option<String>)]
    pub family: Option<String>,
    /// Its drawn size, as a share of the page's shorter side, above 0 and at
    /// most 0.1: 0.015 is 18 pixels on a page 1,200 wide.
    #[serde(deserialize_with = "subtitle_size")]
    #[specta(type = f64)]
    pub size: f64,
    /// The text's colour.
    #[serde(deserialize_with = "solid")]
    #[specta(type = Rgba)]
    pub fill: Rgba,
    /// The outline's colour.
    #[serde(deserialize_with = "solid")]
    #[specta(type = Rgba)]
    pub stroke: Rgba,
}

impl Default for SubtitleStyle {
    fn default() -> Self {
        Self {
            family: None,
            size: 0.015,
            fill: Rgba([0.0, 0.0, 0.0, 1.0]),
            stroke: Rgba([1.0; 4]),
        }
    }
}

/// The largest subtitle, a tenth of the page's shorter side.
const MAX_SUBTITLE_SIZE: f64 = 0.1;

fn subtitle_family<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    let family = Option::<String>::deserialize(deserializer)?;
    if family
        .as_deref()
        .is_some_and(|family| family.trim().is_empty())
    {
        return Err(D::Error::custom(
            "a subtitle's font can't be blank; leave it out for the default font",
        ));
    }
    Ok(family)
}

fn subtitle_size<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    let size = f64::deserialize(deserializer)?;
    if !(size > 0.0 && size <= MAX_SUBTITLE_SIZE) {
        return Err(D::Error::custom(format!(
            "a subtitle {size} of the page's shorter side isn't above 0 and at most \
             {MAX_SUBTITLE_SIZE}"
        )));
    }
    Ok(size)
}

/// `[booharu-fonts]`: the font picker's favourites.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct FontSettings {
    /// The families starred in the font picker, by name, in name order.
    #[serde(deserialize_with = "families")]
    #[specta(type = Vec<String>)]
    pub favorites: BTreeSet<String>,
}

impl Section for FontSettings {
    const NAME: &'static str = "booharu-fonts";
}

/// `[booharu-start]`: what Booharu showed once as it started.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct StartSettings {
    /// Whether the note on a first start, saying Booharu 0.0.4's series
    /// weren't copied and where to import them from, was dismissed.
    pub fresh_start_note_seen: bool,
}

impl Section for StartSettings {
    const NAME: &'static str = "booharu-start";
}

/// `[booharu-qc]`: the quality check, which looks over each page after a run
/// and after each edit, and lists the boxes that need a look.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct QcSettings {
    /// The smallest drawn text that reads, as a share of the page's shorter
    /// side, above 0 and at most 0.1: 0.01 is 12 pixels on a page 1,200
    /// wide. Text drawn smaller is too small. A share rather than pixels, as
    /// scans come at very different resolutions, and the shorter side, as a
    /// long strip's height says nothing of its lettering's size.
    #[serde(
        deserialize_with = "min_size",
        serialize_with = "crate::lettering::decimal"
    )]
    #[specta(type = f32)]
    pub min_size: f32,
}

impl Default for QcSettings {
    fn default() -> Self {
        Self { min_size: 0.01 }
    }
}

impl Section for QcSettings {
    const NAME: &'static str = "booharu-qc";
}

/// The largest `[booharu-qc] min_size`, a tenth of the page's shorter side.
const MAX_MIN_SIZE: f32 = 0.1;

fn min_size<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f32, D::Error> {
    let share = f32::deserialize(deserializer)?;
    if !(share > 0.0 && share <= MAX_MIN_SIZE) {
        return Err(D::Error::custom(format!(
            "the smallest readable text, {share} of the page's shorter side, isn't above 0 and \
             at most {MAX_MIN_SIZE}"
        )));
    }
    Ok(share)
}

/// The most pages one Read chapter request sends.
pub const MAX_PAGES_PER_REQUEST: u32 = 40;

/// `[booharu-reading]`: Read chapter, which reads the whole chapter with a
/// model that sees its pages before it is translated, and Explain this line,
/// which asks the same model first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct ReadingSettings {
    /// The model that reads, as Koharu's settings name a model (a reference
    /// such as `openai/gpt-5.4`), never blank; none for the translation model
    /// when it can read.
    #[serde(deserialize_with = "reading_model")]
    #[specta(type = Option<String>)]
    pub model: Option<String>,
    /// Whether the Context tab asks before reading a chapter.
    pub ask: bool,
    /// How many pages one request sends at most, from 1 to 40,
    /// `MAX_PAGES_PER_REQUEST`; fewer go when the model's context window
    /// can't hold them.
    #[serde(deserialize_with = "pages_per_request")]
    #[specta(type = u32)]
    pub pages_per_request: u32,
}

impl Default for ReadingSettings {
    fn default() -> Self {
        Self {
            model: None,
            ask: true,
            pages_per_request: 20,
        }
    }
}

impl Section for ReadingSettings {
    const NAME: &'static str = "booharu-reading";
}

/// A model reference, or none; a blank one names no model.
fn reading_model<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    let model = Option::<String>::deserialize(deserializer)?;
    if model
        .as_deref()
        .is_some_and(|model| model.trim().is_empty())
    {
        return Err(D::Error::custom(
            "the Read chapter model can't be blank; leave it out for the translation model",
        ));
    }
    Ok(model)
}

fn pages_per_request<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
    let pages = u32::deserialize(deserializer)?;
    if !(1..=MAX_PAGES_PER_REQUEST).contains(&pages) {
        return Err(D::Error::custom(format!(
            "{pages} pages per Read chapter request aren't between 1 and {MAX_PAGES_PER_REQUEST}"
        )));
    }
    Ok(pages)
}

/// Family names, none of them blank.
fn families<'de, D: Deserializer<'de>>(deserializer: D) -> Result<BTreeSet<String>, D::Error> {
    let families = BTreeSet::<String>::deserialize(deserializer)?;
    if families.iter().any(|family| family.trim().is_empty()) {
        return Err(D::Error::custom("a font family's name can't be blank"));
    }
    Ok(families)
}

/// Booharu's settings as one read takes them, for what keeps one set of
/// values throughout: a pipeline run, from when it's queued, whichever way it
/// came; the canvas, until the next save; a translation typed by hand. Its
/// default, which only a caller that read nothing holds, is Koharu's own
/// behaviour: no font written, the colours Koharu gives kept, and the first
/// failure ending a run.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    /// `[booharu-lettering]`: how new text looks.
    pub lettering: LetteringSettings,
    /// The default font new text gets a copy of, the first of Koharu's
    /// Preferred fonts; none to write none.
    pub font: Option<String>,
    /// `[booharu-runs]`'s run policy, which `Pipeline::run` reads into the
    /// snapshot of each run it queues, and nothing else reads; none, as
    /// [`read`](Self::read) leaves it, runs as Koharu does.
    pub runs: Option<RunPolicy>,
    /// Whether this run is Redo boxes taking hand-fixed boxes too: a box
    /// read or translated by hand is read and translated again. A lock or a
    /// sound effect's skip still leaves a box out. False as
    /// [`read`](Self::read) leaves it; only Booharu's own Rust callers set
    /// it, on the config of the run they queue, which `Pipeline::run` keeps.
    pub redo: bool,
    /// Whether this run translates sound effects for their subtitles: the
    /// blocks its target names are translated though skipped or fixed by
    /// hand. False as [`read`](Self::read) leaves it; only Booharu's own
    /// Rust callers set it, as `redo`.
    pub subtitle_run: bool,
    /// `[booharu-emphasis]`, for a run that translates: how a stressed word
    /// looks, and whether the words its model picks are marked. None, as
    /// [`read`](Self::read) leaves it, marks no word, though a translation
    /// still takes off the marks a run left on the one it replaces.
    pub emphasis: Option<EmphasisSettings>,
    /// Whether this run's translation asks its model which words to stress.
    /// False as [`read`](Self::read) leaves it.
    pub asks_emphasis: bool,
}

impl Snapshot {
    /// The settings as the file holds them now, with `families`, Koharu's
    /// Preferred fonts as `[renderer] font_families` reads them now, and no
    /// run policy, which only a queued run reads. A stored value that doesn't
    /// read reads as its default, and only a file that can't be read, such as
    /// one that isn't TOML, fails, so nothing is lettered while the look the
    /// file holds is unknown.
    pub fn read(families: koharu_config::Result<Vec<String>>) -> anyhow::Result<Self> {
        Self::of(koharu_config::get(), families)
    }

    /// The snapshot of `lettering` and `families` as they were read, or the
    /// error of the first that couldn't be.
    fn of(
        lettering: koharu_config::Result<LetteringSettings>,
        families: koharu_config::Result<Vec<String>>,
    ) -> anyhow::Result<Self> {
        let lettering = LetteringSettings::checked(lettering)?;
        let families = families.context("Koharu's Preferred fonts can't be read")?;
        Ok(Self {
            lettering,
            font: crate::styles::first_font(&families).map(str::to_owned),
            runs: None,
            redo: false,
            subtitle_run: false,
            emphasis: None,
            asks_emphasis: false,
        })
    }
}

/// Every Booharu settings section, each under its table's name. Koharu's
/// own stay in its `Preferences`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct BooharuPreferences {
    #[serde(rename = "booharu-mcp")]
    pub mcp: McpSettings,
    #[serde(rename = "booharu-ocr-box")]
    pub ocr_box: OcrBoxSettings,
    #[serde(rename = "booharu-lettering")]
    pub lettering: LetteringSettings,
    #[serde(rename = "booharu-fonts")]
    pub fonts: FontSettings,
    #[serde(rename = "booharu-runs")]
    pub runs: RunsSettings,
    #[serde(rename = "booharu-reading")]
    pub reading: ReadingSettings,
    #[serde(rename = "booharu-qc")]
    pub qc: QcSettings,
    #[serde(rename = "booharu-start")]
    pub start: StartSettings,
    #[serde(rename = "booharu-emphasis")]
    pub emphasis: EmphasisSettings,
}

impl BooharuPreferences {
    /// Every section as the file holds it now: a stored value that doesn't
    /// read reads as its default, and only a file that can't be read, such as
    /// one that isn't TOML, fails.
    pub fn get() -> anyhow::Result<Self> {
        Ok(Self {
            mcp: koharu_config::get()?,
            ocr_box: koharu_config::get()?,
            lettering: koharu_config::get()?,
            fonts: koharu_config::get()?,
            runs: koharu_config::get()?,
            reading: koharu_config::get()?,
            qc: koharu_config::get()?,
            start: koharu_config::get()?,
            emphasis: koharu_config::get()?,
        })
    }

    /// Saves every section, writing only what differs from its defaults, and
    /// answers them as the file holds them now. A stored value that doesn't
    /// read, which reads as its default, stays in the file until a save
    /// changes it, as koharu-config writes only the keys that change; a
    /// change to a list, or to a map or table that reads as its default for
    /// one, writes it whole, as it read, with the change.
    pub fn save(self) -> anyhow::Result<Self> {
        // Every section is read and every given value checked before any is
        // written, so a bad value given, or a file that can't be read, writes
        // nothing.
        Self::get()?;
        self.check()?;
        let Self {
            mcp,
            ocr_box,
            lettering,
            fonts,
            runs,
            reading,
            qc,
            start,
            emphasis,
        } = self;
        koharu_config::update(|section: &mut McpSettings| *section = mcp)?;
        koharu_config::update(|section: &mut OcrBoxSettings| *section = ocr_box)?;
        koharu_config::update(|section: &mut LetteringSettings| *section = lettering)?;
        koharu_config::update(|section: &mut FontSettings| *section = fonts)?;
        koharu_config::update(|section: &mut RunsSettings| *section = runs)?;
        koharu_config::update(|section: &mut ReadingSettings| *section = reading)?;
        koharu_config::update(|section: &mut QcSettings| *section = qc)?;
        koharu_config::update(|section: &mut StartSettings| *section = start)?;
        koharu_config::update(|section: &mut EmphasisSettings| *section = emphasis)?;
        Self::get()
    }

    /// Every section's table name.
    pub fn names() -> Vec<String> {
        tables(&Self::default()).keys().cloned().collect()
    }

    /// Whether Booharu reads the section named `name` only as it starts, so a
    /// change to it applies once Booharu restarts.
    pub fn read_at_launch(name: &str) -> bool {
        name == McpSettings::NAME
    }

    /// The section named `name`, as JSON.
    pub fn section(&self, name: &str) -> anyhow::Result<Value> {
        tables(self).remove(name).ok_or_else(|| unknown(name))
    }

    /// Sets each key `values` names in the section named `name` to its new
    /// value, a table among them only in the keys it names, so its others,
    /// such as `default`'s `stroke` beside a new `fill`, keep theirs. An
    /// unknown section or key, a key inside one of its tables included, or a
    /// bad value, fails and changes nothing.
    pub fn set_section(&mut self, name: &str, values: Map<String, Value>) -> anyhow::Result<()> {
        *self = merged(tables(self), name, values)?;
        Ok(())
    }

    /// Sets each key `values` names in the section named `name`, as
    /// [`set_section`](Self::set_section) does, in the section as the file
    /// holds it, and saves that section alone, in one read and write of it
    /// under the settings file's lock: what another writer saved meanwhile,
    /// such as a font's star, stays. Answers every section as the file holds
    /// them then. An unknown section or key, a bad value given, or a file
    /// that can't be read writes nothing; a stored value that doesn't read
    /// reads as its default, and stays in the file unless `values` changes
    /// it, as [`save`](Self::save) writes it.
    pub fn save_section(name: &str, values: Map<String, Value>) -> anyhow::Result<Self> {
        let save = saver(name).ok_or_else(|| unknown(name))?;
        // The file is read first, as `save` reads it, so one that can't be
        // read writes nothing.
        Self::get()?;
        save(values)?;
        Self::get()
    }

    /// Fails on a value its section doesn't take, which reading the file
    /// would take for its default.
    fn check(&self) -> anyhow::Result<()> {
        serde_json::from_value::<Self>(serde_json::to_value(self)?)?;
        Ok(())
    }
}

/// The save of each section alone, by its table's name, as
/// [`BooharuPreferences::save_section`] makes it.
type Saver = fn(Map<String, Value>) -> anyhow::Result<()>;

/// How the section named `name` is saved alone; none for a name Booharu has
/// no section of.
fn saver(name: &str) -> Option<Saver> {
    let savers: [(&str, Saver); 9] = [
        (McpSettings::NAME, saved::<McpSettings>),
        (OcrBoxSettings::NAME, saved::<OcrBoxSettings>),
        (LetteringSettings::NAME, saved::<LetteringSettings>),
        (FontSettings::NAME, saved::<FontSettings>),
        (RunsSettings::NAME, saved::<RunsSettings>),
        (ReadingSettings::NAME, saved::<ReadingSettings>),
        (QcSettings::NAME, saved::<QcSettings>),
        (StartSettings::NAME, saved::<StartSettings>),
        (EmphasisSettings::NAME, saved::<EmphasisSettings>),
    ];
    savers
        .into_iter()
        .find_map(|(section, save)| (section == name).then_some(save))
}

/// Sets each key `values` names in the section `S` as the file holds it, as
/// [`merged`] sets them, and writes it, in one read and write under the
/// settings file's lock. A bad value given writes nothing.
fn saved<S: Section>(values: Map<String, Value>) -> anyhow::Result<()> {
    let mut outcome = Ok(());
    koharu_config::update(|section: &mut S| {
        let held = tables(&BTreeMap::from([(S::NAME, &*section)]));
        match merged::<BTreeMap<String, S>>(held, S::NAME, values) {
            Ok(mut merged) => {
                if let Some(merged) = merged.remove(S::NAME) {
                    *section = merged;
                }
            }
            Err(error) => outcome = Err(error),
        }
    })?;
    outcome
}

/// `tables`, sections as JSON keyed by their tables' names, with each key
/// `values` names in the section `name` set to its value, read as `T`, so a bad
/// value fails, where the file's would read as its default. A table among the
/// values sets only the keys it names, so its others keep theirs; anything else
/// is set whole. A key the section doesn't have fails too, as does one a table
/// in it doesn't have: the file would drop it, but a caller naming one made a
/// mistake. A key the section's JSON leaves out, as `skip_serializing_if` does,
/// counts as one it doesn't have, unless it held the key and the value is null,
/// which removes it, as it removes a category's font.
fn merged<T: Serialize + DeserializeOwned>(
    mut tables: Map<String, Value>,
    name: &str,
    values: Map<String, Value>,
) -> anyhow::Result<T> {
    let Some(Value::Object(table)) = tables.get_mut(name) else {
        return Err(unknown(name));
    };
    let held = Value::Object(table.clone());
    for (key, value) in &values {
        let Some(slot) = table.get_mut(key) else {
            bail!("[{name}] has no key `{key}`");
        };
        patch(slot, value.clone());
    }
    let merged: T = serde_json::from_value(Value::Object(tables))
        .map_err(|error| anyhow!("[{name}]: {error}"))?;
    let read = self::tables(&merged).remove(name).unwrap_or(Value::Null);
    if let Some(key) = missing(&values, &held, &read) {
        bail!("[{name}] has no key `{key}`");
    }
    Ok(merged)
}

/// `slot` with `value` merged in: a table key by key, so a key `value`
/// leaves out keeps its own, and anything else whole.
fn patch(slot: &mut Value, value: Value) {
    match (slot, value) {
        (Value::Object(table), Value::Object(keys)) => {
            for (key, value) in keys {
                match table.entry(key) {
                    Entry::Occupied(mut slot) => patch(slot.get_mut(), value),
                    Entry::Vacant(slot) => {
                        slot.insert(value);
                    }
                }
            }
        }
        (slot, value) => *slot = value,
    }
}

/// The first key `values` names, as a dotted path through its tables, that
/// `read` doesn't hold, but for one `held` held that's set to null, which
/// removes it.
fn missing(values: &Map<String, Value>, held: &Value, read: &Value) -> Option<String> {
    values
        .iter()
        .find_map(|(key, value)| match (read.get(key), value) {
            (None, Value::Null) if held.get(key).is_some() => None,
            (None, _) => Some(key.clone()),
            (Some(found), Value::Object(keys)) => {
                let held = held.get(key).unwrap_or(&Value::Null);
                missing(keys, held, found).map(|path| format!("{key}.{path}"))
            }
            _ => None,
        })
}

/// `sections` as JSON, keyed by their tables' names. A section that isn't a
/// table is a bug in its type, so this panics, as koharu-config does on one.
fn tables(sections: &impl Serialize) -> Map<String, Value> {
    match serde_json::to_value(sections) {
        Ok(Value::Object(tables)) => tables,
        other => panic!("Booharu's preferences aren't a table of tables: {other:?}"),
    }
}

fn unknown(name: &str) -> anyhow::Error {
    anyhow!(
        "Booharu has no settings section [{name}]; its sections are {}",
        BooharuPreferences::names().join(", ")
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{
        runs::RunOrder,
        testing::{stored, unreadable},
    };

    fn values(values: Value) -> Map<String, Value> {
        match values {
            Value::Object(values) => values,
            other => panic!("{other} isn't an object"),
        }
    }

    #[test]
    fn an_empty_table_reads_as_the_defaults() {
        let mcp: McpSettings = toml::from_str("").expect("the defaults");
        assert_eq!(mcp, McpSettings::default());
        assert_eq!(mcp.port, 47821);
        let ocr_box: OcrBoxSettings = toml::from_str("").expect("the defaults");
        assert_eq!(ocr_box.default_mode, OcrBoxMode::ReadTranslate);
        let lettering: LetteringSettings = toml::from_str("").expect("the defaults");
        assert_eq!(lettering.default.fill, None);
        assert_eq!(lettering.default.stroke, None);
        assert_eq!(lettering.default.stroke_width, 1.5);
        assert!(lettering.detect_colors);
        let fonts: FontSettings = toml::from_str("").expect("the defaults");
        assert!(fonts.favorites.is_empty());
        let start: StartSettings = toml::from_str("").expect("the defaults");
        assert!(!start.fresh_start_note_seen);
        let reading: ReadingSettings = toml::from_str("").expect("the defaults");
        assert_eq!(
            reading,
            ReadingSettings {
                model: None,
                ask: true,
                pages_per_request: 20,
            }
        );
        // In whichever order serde_json keeps a map's keys.
        let mut names = BooharuPreferences::names();
        names.sort();
        assert_eq!(
            names,
            [
                EmphasisSettings::NAME,
                FontSettings::NAME,
                LetteringSettings::NAME,
                McpSettings::NAME,
                OcrBoxSettings::NAME,
                QcSettings::NAME,
                ReadingSettings::NAME,
                RunsSettings::NAME,
                StartSettings::NAME,
            ]
        );
        let qc: QcSettings = toml::from_str("").expect("the defaults");
        assert_eq!(qc.min_size, 0.01);
    }

    #[test]
    fn a_bad_reading_model_or_page_count_isnt_taken_and_reads_as_its_default() {
        let refused = [
            r#"model = """#,
            r#"model = "  ""#,
            "model = 1",
            "ask = \"yes\"",
            "pages_per_request = 0",
            "pages_per_request = 41",
            "pages_per_request = -1",
            "pages_per_request = 2.5",
        ];
        for text in refused {
            assert!(
                toml::from_str::<ReadingSettings>(text).is_err(),
                "took {text}"
            );
            // Edited into the file, it reads as its default.
            assert_eq!(
                stored::<ReadingSettings>(text).ok(),
                Some(ReadingSettings::default()),
                "{text}"
            );
        }
        // The section's other values stay; a file that isn't TOML fails.
        let kept: ReadingSettings =
            stored("model = \"\"\nask = false\npages_per_request = 0").expect("the section");
        assert_eq!(
            kept,
            ReadingSettings {
                model: None,
                ask: false,
                pages_per_request: 20,
            }
        );
        assert!(unreadable::<ReadingSettings>().is_err());
        let read: ReadingSettings =
            toml::from_str("model = \"openai/gpt-5.4\"\nask = false\npages_per_request = 40\n")
                .expect("a model, no asking and the most pages");
        assert_eq!(
            read,
            ReadingSettings {
                model: Some("openai/gpt-5.4".to_owned()),
                ask: false,
                pages_per_request: MAX_PAGES_PER_REQUEST,
            }
        );
        let text = toml::to_string(&read).expect("a table");
        assert_eq!(toml::from_str::<ReadingSettings>(&text).ok(), Some(read));
        let fewest: ReadingSettings = toml::from_str("pages_per_request = 1").expect("one page");
        assert_eq!(fewest.pages_per_request, 1);

        // The section changes by its name, a null model going back to the
        // translation model's, and a bad value changing nothing.
        let mut preferences = BooharuPreferences::default();
        preferences
            .set_section(
                ReadingSettings::NAME,
                values(json!({"model": "anthropic/claude", "pages_per_request": 8})),
            )
            .expect("a model and a page count");
        assert_eq!(
            preferences.reading.model.as_deref(),
            Some("anthropic/claude")
        );
        assert_eq!(preferences.reading.pages_per_request, 8);
        assert!(preferences.reading.ask, "a key left out keeps its value");
        let refused = preferences.set_section(
            ReadingSettings::NAME,
            values(json!({"pages_per_request": 0})),
        );
        assert!(refused.is_err());
        assert_eq!(preferences.reading.pages_per_request, 8);
        // 0.0.4's automatic reading is the Run menu's Read chapter now.
        let refused =
            preferences.set_section(ReadingSettings::NAME, values(json!({"automatic": true})));
        assert!(refused.is_err());
        preferences
            .set_section(ReadingSettings::NAME, values(json!({"model": null})))
            .expect("the translation model");
        assert_eq!(preferences.reading.model, None);
    }

    #[test]
    fn a_bad_smallest_readable_size_isnt_taken_and_reads_as_its_default() {
        for text in [
            "min_size = 0",
            "min_size = -0.01",
            "min_size = 0.11",
            "min_size = 1",
            "min_size = nan",
            "min_size = inf",
            "min_size = \"0.01\"",
        ] {
            assert!(toml::from_str::<QcSettings>(text).is_err(), "took {text}");
            // Edited into the file, it reads as its default, the section's
            // only value.
            assert_eq!(
                stored::<QcSettings>(text).ok().map(|qc| qc.min_size),
                Some(0.01),
                "{text}"
            );
        }
        assert!(
            unreadable::<QcSettings>().is_err(),
            "a file that isn't TOML"
        );
        for (text, share) in [("min_size = 0.1", 0.1), ("min_size = 0.0005", 0.0005)] {
            let read: QcSettings = toml::from_str(text).expect("a share");
            assert_eq!(read.min_size, share);
        }
        // Written as the decimal it was given, never the f32's long tail.
        let written = toml::to_string(&QcSettings { min_size: 0.015 }).expect("a table");
        assert_eq!(written.trim(), "min_size = 0.015");

        // The section changes by its name, and a bad value changes nothing.
        let mut preferences = BooharuPreferences::default();
        preferences
            .set_section(QcSettings::NAME, values(json!({"min_size": 0.02})))
            .expect("a share");
        assert_eq!(preferences.qc.min_size, 0.02);
        assert_eq!(
            preferences.section(QcSettings::NAME).ok(),
            Some(json!({"min_size": 0.02}))
        );
        let refused = preferences.set_section(QcSettings::NAME, values(json!({"min_size": 0.5})));
        assert!(refused.is_err());
        assert_eq!(preferences.qc.min_size, 0.02);
        let refused = preferences.set_section(QcSettings::NAME, values(json!({"enabled": false})));
        assert!(refused.is_err(), "QC has no switch");
    }

    #[test]
    fn the_default_look_reads_back_as_it_was_written() {
        let lettering = LetteringSettings {
            default: DefaultTextStyle {
                fill: Some(Rgba([1.0, 0.25, 0.0, 1.0])),
                stroke: None,
                stroke_width: 3.0,
            },
            detect_colors: false,
            ..LetteringSettings::default()
        };
        // TOML has no null: an unset colour is left out.
        let text = toml::to_string(&lettering).expect("a table");
        let table: toml::Table = toml::from_str(&text).expect("a table");
        assert!(table["default"].get("stroke").is_none(), "{text}");
        assert_eq!(
            toml::from_str::<LetteringSettings>(&text).ok(),
            Some(lettering)
        );
        let whole: LetteringSettings = toml::from_str(
            "detect_colors = false\n\n[default]\nstroke = [1, 1, 1, 1]\nstroke_width = 2\n",
        )
        .expect("whole numbers for a colour and a width");
        assert_eq!(whole.default.stroke, Some(Rgba([1.0; 4])));
        assert_eq!(whole.default.stroke_width, 2.0);
    }

    #[test]
    fn a_bad_colour_width_or_favourite_isnt_taken_and_reads_as_its_default() {
        let refused = [
            "[default]\nfill = [1.5, 0, 0, 1]",
            "[default]\nfill = [-0.1, 0, 0, 1]",
            "[default]\nstroke = [0, 0, 0]",
            "[default]\nstroke = [nan, 0, 0, 1]",
            "[default]\nstroke_width = -1",
            "[default]\nstroke_width = 50.5",
            "[default]\nstroke_width = inf",
            "[default]\nstroke_width = nan",
            "detect_colors = 1",
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
        // The section's other values stay; a file that isn't TOML fails.
        let kept: LetteringSettings =
            stored("detect_colors = false\n[default]\nfill = [1.5, 0, 0, 1]\nstroke_width = 3")
                .expect("the section");
        assert!(!kept.detect_colors);
        assert_eq!(kept.default.fill, None);
        assert_eq!(kept.default.stroke_width, 3.0);
        assert!(unreadable::<LetteringSettings>().is_err());
        let widest: LetteringSettings =
            toml::from_str("[default]\nstroke_width = 50").expect("the widest");
        assert_eq!(widest.default.stroke_width, 50.0);
        for text in [r#"favorites = [""]"#, r#"favorites = ["Arial", "  "]"#] {
            assert!(toml::from_str::<FontSettings>(text).is_err(), "took {text}");
            // One bad favourite, edited into the file, takes the list's
            // default: no favourites.
            assert_eq!(
                stored::<FontSettings>(text).ok(),
                Some(FontSettings::default()),
                "{text}"
            );
        }
        assert!(unreadable::<FontSettings>().is_err());
    }

    #[test]
    fn the_look_and_the_favourites_change_by_their_sections() {
        let mut preferences = BooharuPreferences::default();
        preferences
            .set_section(
                "booharu-lettering",
                values(json!({
                    "default": {"fill": [1, 0, 0, 1], "stroke": null, "stroke_width": 2},
                    "detect_colors": false,
                })),
            )
            .expect("a look");
        assert_eq!(
            preferences.lettering,
            LetteringSettings {
                default: DefaultTextStyle {
                    fill: Some(Rgba([1.0, 0.0, 0.0, 1.0])),
                    stroke: None,
                    stroke_width: 2.0,
                },
                detect_colors: false,
                ..LetteringSettings::default()
            }
        );
        let refused = preferences.set_section(
            "booharu-lettering",
            values(json!({"default": {"stroke": [2, 0, 0, 1]}})),
        );
        assert!(refused.is_err());
        assert_eq!(preferences.lettering.default.stroke_width, 2.0);

        preferences
            .set_section(
                "booharu-fonts",
                values(json!({"favorites": ["Arial", "CCWildWords", "Arial"]})),
            )
            .expect("favourites");
        assert_eq!(
            preferences.section("booharu-fonts").ok(),
            Some(json!({"favorites": ["Arial", "CCWildWords"]}))
        );
        let refused = preferences.set_section("booharu-fonts", values(json!({"favorites": [""]})));
        assert!(refused.is_err());
        assert_eq!(preferences.fonts.favorites.len(), 2);
    }

    #[test]
    fn a_table_changes_only_in_the_keys_it_names() {
        let white = Rgba([1.0; 4]);
        let mut preferences = BooharuPreferences::default();
        preferences.lettering.default = DefaultTextStyle {
            fill: None,
            stroke: Some(white),
            stroke_width: 3.0,
        };
        preferences
            .set_section(
                "booharu-lettering",
                values(json!({"default": {"fill": [1, 0, 0, 1]}})),
            )
            .expect("a fill");
        assert_eq!(
            preferences.lettering.default,
            DefaultTextStyle {
                fill: Some(Rgba([1.0, 0.0, 0.0, 1.0])),
                stroke: Some(white),
                stroke_width: 3.0,
            }
        );

        // A key the table doesn't have, such as 0.0.4's name for the
        // outline, or a misspelt one, changes nothing.
        for (change, key) in [
            (
                json!({"default": {"outline": [0, 0, 0, 1]}}),
                "default.outline",
            ),
            (
                json!({"default": {"stroke": null, "stroke_widht": 1}}),
                "default.stroke_widht",
            ),
        ] {
            let error = preferences
                .set_section("booharu-lettering", values(change.clone()))
                .expect_err("an unknown key");
            assert!(
                error.to_string().contains(&format!("no key `{key}`")),
                "{change}: {error}"
            );
            assert_eq!(preferences.lettering.default.stroke, Some(white));
        }
    }

    #[test]
    fn sound_effects_are_skipped_and_subtitled_plainly_by_default() {
        let lettering: LetteringSettings = toml::from_str("").expect("the defaults");
        assert_eq!(lettering.sfx, SfxMode::Skip);
        assert_eq!(
            lettering.subtitle,
            SubtitleStyle {
                family: None,
                size: 0.015,
                fill: Rgba([0.0, 0.0, 0.0, 1.0]),
                stroke: Rgba([1.0; 4]),
            }
        );
        let json = serde_json::to_value(&lettering).expect("JSON");
        assert_eq!(json["sfx"], json!("skip"));
        assert_eq!(
            json["subtitle"],
            json!({"family": null, "size": 0.015, "fill": [0.0, 0.0, 0.0, 1.0],
                "stroke": [1.0, 1.0, 1.0, 1.0]})
        );
    }

    #[test]
    fn the_sound_effect_settings_read_back_as_they_were_written() {
        let lettering = LetteringSettings {
            sfx: SfxMode::InsideBubbles,
            subtitle: SubtitleStyle {
                family: Some("Comic Neue".to_owned()),
                size: 0.02,
                fill: Rgba([1.0, 0.0, 0.0, 1.0]),
                stroke: Rgba([0.0, 0.0, 0.0, 1.0]),
            },
            ..LetteringSettings::default()
        };
        let text = toml::to_string(&lettering).expect("a table");
        assert!(text.contains("sfx = \"inside_bubbles\""), "{text}");
        assert_eq!(
            toml::from_str::<LetteringSettings>(&text).ok(),
            Some(lettering.clone())
        );
        let json = serde_json::to_value(&lettering).expect("JSON");
        assert_eq!(
            serde_json::from_value::<LetteringSettings>(json).ok(),
            Some(lettering)
        );
        for (text, mode) in [
            (r#"sfx = "skip""#, SfxMode::Skip),
            (r#"sfx = "inside_bubbles""#, SfxMode::InsideBubbles),
            (r#"sfx = "translate_all""#, SfxMode::TranslateAll),
        ] {
            let read: LetteringSettings = toml::from_str(text).expect("a mode");
            assert_eq!(read.sfx, mode);
        }
        let largest: LetteringSettings =
            toml::from_str("[subtitle]\nsize = 0.1").expect("the largest subtitle");
        assert_eq!(largest.subtitle.size, 0.1);
    }

    #[test]
    fn a_bad_sound_effect_setting_isnt_taken_and_reads_as_its_default() {
        let refused = [
            r#"sfx = "kept""#,
            r#"sfx = "translateAll""#,
            "sfx = 1",
            "[subtitle]\nfamily = \"\"",
            "[subtitle]\nfamily = \"   \"",
            "[subtitle]\nsize = 0",
            "[subtitle]\nsize = -0.01",
            "[subtitle]\nsize = 0.11",
            "[subtitle]\nsize = nan",
            "[subtitle]\nsize = inf",
            "[subtitle]\nfill = [1.5, 0, 0, 1]",
            "[subtitle]\nfill = [0, 0, 1]",
            "[subtitle]\nstroke = [0, 0, -1, 1]",
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
        // The section's other values stay; a file that isn't TOML fails.
        let kept: LetteringSettings = stored(
            "sfx = \"kept\"\ndetect_colors = false\n\
             [subtitle]\nsize = 0.11\nfamily = \"Comic Neue\"",
        )
        .expect("the section");
        assert_eq!(kept.sfx, SfxMode::Skip);
        assert!(!kept.detect_colors);
        assert_eq!(kept.subtitle.size, SubtitleStyle::default().size);
        assert_eq!(kept.subtitle.family.as_deref(), Some("Comic Neue"));
        assert!(unreadable::<LetteringSettings>().is_err());
        // Through its section too, where a null colour is no colour at all,
        // and a bad value or an unknown key changes nothing.
        let mut preferences = BooharuPreferences::default();
        preferences
            .set_section(
                LetteringSettings::NAME,
                values(json!({"sfx": "translate_all", "subtitle": {"size": 0.03}})),
            )
            .expect("a mode and a size");
        assert_eq!(preferences.lettering.sfx, SfxMode::TranslateAll);
        assert_eq!(preferences.lettering.subtitle.size, 0.03);
        assert_eq!(
            preferences.lettering.subtitle.fill,
            SubtitleStyle::default().fill,
            "a key left out keeps its value"
        );
        for change in [
            json!({"subtitle": {"fill": null}}),
            json!({"subtitle": {"family": " "}}),
            json!({"subtitle": {"size": 0.2}}),
            json!({"sfx": "everything"}),
            json!({"subtitle": {"outline_width": 2}}),
        ] {
            let changed = preferences.set_section(LetteringSettings::NAME, values(change.clone()));
            assert!(changed.is_err(), "took {change}");
            assert_eq!(preferences.lettering.sfx, SfxMode::TranslateAll);
            assert_eq!(preferences.lettering.subtitle.size, 0.03);
        }
        preferences
            .set_section(
                LetteringSettings::NAME,
                values(json!({"subtitle": {"family": "Comic Neue"}})),
            )
            .expect("a family");
        assert_eq!(
            preferences.lettering.subtitle.family.as_deref(),
            Some("Comic Neue")
        );
        preferences
            .set_section(
                LetteringSettings::NAME,
                values(json!({"subtitle": {"family": null}})),
            )
            .expect("the default font again");
        assert_eq!(preferences.lettering.subtitle.family, None);
    }

    #[test]
    fn the_preferences_commands_carry_the_sound_effect_settings_both_ways() {
        let mut preferences = BooharuPreferences::default();
        preferences.lettering.sfx = SfxMode::TranslateAll;
        preferences.lettering.subtitle.size = 0.02;
        preferences.lettering.subtitle.family = Some("Comic Neue".to_owned());
        // As `booharu_get_preferences` answers them and
        // `booharu_save_preferences` takes them, and as `save` checks them.
        let sent = serde_json::to_value(&preferences).expect("JSON");
        assert_eq!(sent["booharu-lettering"]["sfx"], json!("translate_all"));
        assert_eq!(sent["booharu-lettering"]["subtitle"]["size"], json!(0.02));
        let taken: BooharuPreferences = serde_json::from_value(sent).expect("taken back");
        assert_eq!(taken, preferences);
        taken.check().expect("a value the file takes");
        // As the file holds each section, under its table's name.
        let text = toml::to_string(&preferences).expect("tables");
        assert_eq!(
            toml::from_str::<BooharuPreferences>(&text).ok(),
            Some(preferences)
        );
        // A file from before them, whose section `get` reads alone, reads
        // their defaults.
        let older: LetteringSettings =
            toml::from_str("detect_colors = false\n").expect("an older section");
        assert_eq!(older.sfx, SfxMode::Skip);
        assert_eq!(older.subtitle, SubtitleStyle::default());
    }

    #[test]
    fn a_snapshot_reads_a_bad_stored_value_as_its_default_and_fails_on_a_file_that_isnt_toml() {
        let lettering = LetteringSettings {
            detect_colors: false,
            ..LetteringSettings::default()
        };
        let families = || Ok(vec![" ".to_owned(), "CCWildWords".to_owned()]);
        assert_eq!(
            Snapshot::of(Ok(lettering.clone()), families()).ok(),
            Some(Snapshot {
                lettering: lettering.clone(),
                font: Some("CCWildWords".to_owned()),
                runs: None,
                redo: false,
                subtitle_run: false,
                emphasis: None,
                asks_emphasis: false,
            })
        );
        // An outline 60 pixels wide, edited into the file, reads as the
        // default width, and the section's other values stay.
        let edited = stored("detect_colors = false\n[default]\nstroke_width = 60");
        assert_eq!(
            Snapshot::of(edited, families())
                .ok()
                .map(|snapshot| snapshot.lettering),
            Some(lettering.clone())
        );
        // A file that isn't TOML is an error, never Koharu's look or no font.
        let error = Snapshot::of(unreadable(), families()).expect_err("no text style");
        assert!(
            format!("{error:#}").contains("[booharu-lettering]")
                && format!("{error:#}").contains("config.toml is invalid"),
            "{error:#}"
        );
        let fonts = unreadable::<FontSettings>().map(|fonts| fonts.favorites.into_iter().collect());
        let error = Snapshot::of(Ok(lettering), fonts).expect_err("no Preferred fonts");
        assert!(
            format!("{error:#}").contains("Preferred fonts"),
            "{error:#}"
        );
        // Koharu's own, for what read nothing: no font, the colours Koharu
        // gives kept, and no run policy. Nor a redo or a subtitle run, which
        // no read sets, nor emphasis, which only a run's snapshot sets.
        let default = Snapshot::default();
        assert!(default.lettering.detect_colors && default.font.is_none());
        assert_eq!(default.runs, None);
        assert!(!default.redo && !default.subtitle_run);
        assert!(default.emphasis.is_none() && !default.asks_emphasis);
    }

    #[test]
    fn the_preferences_commands_carry_the_emphasis_both_ways() {
        let mut preferences = BooharuPreferences::default();
        assert_eq!(preferences.emphasis, EmphasisSettings::default());
        preferences.emphasis.ai = false;
        preferences.emphasis.categories.insert(
            LetteringCategory::Shout,
            crate::emphasis::EmphasisStyle {
                outer_stroke: Some(Rgba([1.0, 1.0, 1.0, 1.0])),
                outer_stroke_width: Some(4.0),
                ..crate::emphasis::EmphasisStyle::default()
            },
        );
        // As `booharu_get_preferences` answers them and
        // `booharu_save_preferences` takes them, and as `save` checks them.
        let sent = serde_json::to_value(&preferences).expect("JSON");
        assert_eq!(sent["booharu-emphasis"]["ai"], json!(false));
        assert_eq!(
            sent["booharu-emphasis"]["default"],
            json!({
                "family": null, "weight": 700, "italic": null, "size_factor": 1.15,
                "fill": null, "stroke": null, "stroke_width": null,
                "outer_stroke": null, "outer_stroke_width": null,
            })
        );
        let taken: BooharuPreferences = serde_json::from_value(sent).expect("taken back");
        assert_eq!(taken, preferences);
        taken.check().expect("a value the file takes");
        let text = toml::to_string(&preferences).expect("tables");
        assert_eq!(
            toml::from_str::<BooharuPreferences>(&text).ok(),
            Some(preferences.clone())
        );

        // The section changes by its name, a table only in the keys it
        // names, a null value clearing it, and a null entry removing it.
        preferences
            .set_section(
                EmphasisSettings::NAME,
                values(json!({
                    "default": {"size_factor": null, "italic": true},
                    "categories": {"whisper": {"weight": 300}},
                })),
            )
            .expect("an italic default and a whisper's weight");
        let emphasis = &preferences.emphasis;
        assert_eq!(
            (emphasis.default.weight, emphasis.default.italic),
            (Some(700), Some(true))
        );
        assert_eq!(emphasis.default.size_factor, None);
        assert_eq!(
            emphasis.categories[&LetteringCategory::Whisper].weight,
            Some(300)
        );
        assert!(emphasis.categories.contains_key(&LetteringCategory::Shout));
        preferences
            .set_section(
                EmphasisSettings::NAME,
                values(json!({"categories": {"shout": null}})),
            )
            .expect("no shout entry");
        assert!(
            !preferences
                .emphasis
                .categories
                .contains_key(&LetteringCategory::Shout)
        );
        // A bad value, or a default setting nothing, changes nothing.
        let before = preferences.clone();
        for refused in [
            json!({"default": {"weight": 0}}),
            json!({"default": {"weight": null, "italic": null}}),
            json!({"categories": {"tone": {"weight": 900}}}),
            json!({"categories": {"whisper": {"weight": null}}}),
            json!({"oblique": true}),
        ] {
            let error = preferences.set_section(EmphasisSettings::NAME, values(refused.clone()));
            assert!(error.is_err(), "took {refused}");
            assert_eq!(preferences, before, "{refused}");
        }
    }

    #[test]
    fn every_section_is_saved_alone_by_its_name() {
        for name in BooharuPreferences::names() {
            assert!(saver(&name).is_some(), "[{name}] can't be saved alone");
        }
        assert!(saver("booharu-nope").is_none());
    }

    #[test]
    fn a_port_below_1024_isnt_taken_and_reads_as_booharus_own() {
        let error = toml::from_str::<McpSettings>("port = 80").expect_err("a system port");
        assert!(error.to_string().contains("port 80 isn't"), "{error}");
        for port in [0, 1023, 65536] {
            assert!(toml::from_str::<McpSettings>(&format!("port = {port}")).is_err());
        }
        // Edited into the file, it reads as Booharu's own port, the
        // section's only value, never Koharu's 7878.
        for port in [80, 0, 1023, 65536] {
            assert_eq!(
                stored::<McpSettings>(&format!("port = {port}"))
                    .ok()
                    .map(|mcp| mcp.port),
                Some(47821),
                "{port}"
            );
        }
        assert!(
            unreadable::<McpSettings>().is_err(),
            "a file that isn't TOML"
        );
        assert_eq!(
            toml::from_str::<McpSettings>("port = 1024")
                .ok()
                .map(|mcp| mcp.port),
            Some(1024)
        );
        // A save checks what it's given before it writes anything.
        let mcp = McpSettings { port: 80 };
        let preferences = BooharuPreferences {
            mcp,
            ..BooharuPreferences::default()
        };
        assert!(preferences.check().is_err());
    }

    #[test]
    fn an_unknown_ocr_box_mode_isnt_taken_and_reads_as_read_and_translate() {
        let error = toml::from_str::<OcrBoxSettings>(r#"default_mode = "clean""#)
            .expect_err("no such mode");
        assert!(error.to_string().contains("unknown variant"), "{error}");
        // Edited into the file, it reads as the default, the section's only
        // value.
        assert_eq!(
            stored::<OcrBoxSettings>(r#"default_mode = "clean""#)
                .ok()
                .map(|ocr_box| ocr_box.default_mode),
            Some(OcrBoxMode::ReadTranslate)
        );
        assert!(
            unreadable::<OcrBoxSettings>().is_err(),
            "a file that isn't TOML"
        );
        let read: OcrBoxSettings =
            toml::from_str(r#"default_mode = "read_translate_clean""#).expect("a mode");
        assert_eq!(read.default_mode, OcrBoxMode::ReadTranslateClean);
    }

    #[test]
    fn the_ocr_box_mode_changes_by_its_section_alone() {
        let mut preferences = BooharuPreferences::default();
        preferences
            .set_section("booharu-ocr-box", values(json!({"default_mode": "read"})))
            .expect("a mode");
        assert_eq!(preferences.ocr_box.default_mode, OcrBoxMode::Read);
        assert_eq!(preferences.mcp, McpSettings::default());
        assert_eq!(
            preferences.section("booharu-ocr-box").ok(),
            Some(json!({"default_mode": "read"}))
        );

        let refused = preferences.set_section(
            "booharu-ocr-box",
            values(json!({"default_mode": "read_everything"})),
        );
        assert!(refused.is_err());
        assert_eq!(preferences.ocr_box.default_mode, OcrBoxMode::Read);
    }

    #[test]
    fn the_run_settings_change_by_their_section() {
        let mut preferences = BooharuPreferences::default();
        preferences
            .set_section(
                "booharu-runs",
                values(json!({"retries": 4, "order": "stages"})),
            )
            .expect("a policy");
        let runs = &preferences.runs;
        assert_eq!((runs.retries, runs.order), (4, RunOrder::Stages));
        assert_eq!(runs.presets, RunsSettings::default().presets);
        // A list is set whole.
        let presets = json!([{"name": "OCR", "stages": ["recognize", "detect"]}]);
        preferences
            .set_section("booharu-runs", values(json!({"presets": presets})))
            .expect("presets");
        assert_eq!(
            preferences
                .section("booharu-runs")
                .ok()
                .map(|runs| runs["presets"].clone()),
            Some(json!([{"name": "OCR", "stages": ["detect", "recognize"]}]))
        );
        let refused = [
            json!({"retries": 6}),
            json!({"on_page_failure": "skip"}),
            json!({"presets": [{"name": " ", "stages": ["detect"]}]}),
            json!({"presets": [{"name": "A", "stages": []}]}),
            json!({"presets": [{"name": "A", "stages": ["detect"], "colour": 1}]}),
        ];
        for change in refused {
            let changed = preferences.set_section("booharu-runs", values(change.clone()));
            assert!(changed.is_err(), "took {change}");
            assert_eq!(preferences.runs.retries, 4);
            assert_eq!(preferences.runs.presets.len(), 1);
        }
    }

    #[test]
    fn a_section_changes_by_its_name() {
        let mut preferences = BooharuPreferences::default();
        preferences
            .set_section("booharu-mcp", values(json!({"port": 50000})))
            .expect("a port");
        assert_eq!(preferences.mcp.port, 50000);
        assert_eq!(
            preferences.section("booharu-mcp").ok(),
            Some(json!({"port": 50000}))
        );

        // An unknown section or key, or a bad value, changes nothing.
        let refused = [
            ("booharu-nope", json!({"port": 50001})),
            ("booharu-mcp", json!({"nope": 1})),
            ("booharu-mcp", json!({"port": 50001, "nope": 1})),
            ("booharu-mcp", json!({"port": 80})),
            ("booharu-mcp", json!({"port": "50001"})),
        ];
        for (name, change) in refused {
            let changed = preferences.set_section(name, values(change.clone()));
            assert!(changed.is_err(), "[{name}] took {change}");
            assert_eq!(preferences.mcp.port, 50000);
        }
        assert!(preferences.section("booharu-nope").is_err());
    }

    /// Two sections of two keys each, to see a change leave the rest alone.
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Sections {
        #[serde(rename = "booharu-a")]
        a: Pair,
        #[serde(rename = "booharu-b")]
        b: Pair,
    }

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(default)]
    struct Pair {
        number: u32,
        name: String,
    }

    #[test]
    fn a_change_keeps_the_keys_and_sections_it_doesnt_name() {
        let pair = |number, name: &str| Pair {
            number,
            name: name.to_owned(),
        };
        let sections = Sections {
            a: pair(1, "kept"),
            b: pair(2, "also kept"),
        };
        let changed = merged(tables(&sections), "booharu-a", values(json!({"number": 3})));
        assert_eq!(
            changed.ok(),
            Some(Sections {
                a: pair(3, "kept"),
                b: pair(2, "also kept"),
            })
        );
    }
}
