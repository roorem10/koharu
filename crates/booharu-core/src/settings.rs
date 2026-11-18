//! Booharu's settings: a `[booharu-<area>]` table per area in the Booharu
//! home's `config.toml`, each a [`Section`] that owns its defaults, as
//! Koharu's own sections do. A value is checked as it is read, so a bad one
//! fails whether the file or a caller holds it, and a save writes only what
//! differs from the defaults.
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
/// category letters its text in (the `lettering` module). The default font
/// isn't one of these settings: new text gets a copy of the first of
/// Koharu's Preferred fonts, `[renderer] font_families`.
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
        }
    }
}

impl Section for LetteringSettings {
    const NAME: &'static str = "booharu-lettering";
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
fn colour<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Rgba>, D::Error> {
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
}

impl Snapshot {
    /// The settings as the file holds them now, with `families`, Koharu's
    /// Preferred fonts as `[renderer] font_families` reads them now, and no
    /// run policy, which only a queued run reads. A bad stored value fails,
    /// never replaced by its default, so nothing is lettered with a look
    /// nobody chose.
    pub fn read(families: koharu_config::Result<Vec<String>>) -> anyhow::Result<Self> {
        Self::of(koharu_config::get(), families)
    }

    /// The snapshot of `lettering` and `families` as they were read, or the
    /// error of the first that couldn't be.
    fn of(
        lettering: koharu_config::Result<LetteringSettings>,
        families: koharu_config::Result<Vec<String>>,
    ) -> anyhow::Result<Self> {
        let lettering = lettering.with_context(|| {
            format!(
                "Booharu's default text style can't be read from [{}]",
                LetteringSettings::NAME
            )
        })?;
        let families = families.context("Koharu's Preferred fonts can't be read")?;
        Ok(Self {
            lettering,
            font: crate::styles::first_font(&families).map(str::to_owned),
            runs: None,
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
}

impl BooharuPreferences {
    /// Every section as the file holds it now. A bad stored value is an
    /// error, never its default.
    pub fn get() -> anyhow::Result<Self> {
        Ok(Self {
            mcp: koharu_config::get()?,
            ocr_box: koharu_config::get()?,
            lettering: koharu_config::get()?,
            fonts: koharu_config::get()?,
            runs: koharu_config::get()?,
        })
    }

    /// Saves every section, writing only what differs from its defaults, and
    /// answers them as the file holds them now.
    pub fn save(self) -> anyhow::Result<Self> {
        // Every section is read and checked before any is written, so a bad
        // value, stored or given, writes nothing.
        Self::get()?;
        self.check()?;
        let Self {
            mcp,
            ocr_box,
            lettering,
            fonts,
            runs,
        } = self;
        koharu_config::update(|section: &mut McpSettings| *section = mcp)?;
        koharu_config::update(|section: &mut OcrBoxSettings| *section = ocr_box)?;
        koharu_config::update(|section: &mut LetteringSettings| *section = lettering)?;
        koharu_config::update(|section: &mut FontSettings| *section = fonts)?;
        koharu_config::update(|section: &mut RunsSettings| *section = runs)?;
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
    /// them then. An unknown section or key, or a bad value, stored or given,
    /// writes nothing.
    pub fn save_section(name: &str, values: Map<String, Value>) -> anyhow::Result<Self> {
        let save = saver(name).ok_or_else(|| unknown(name))?;
        // Every section is read and checked first, as `save` does.
        Self::get()?;
        save(values)?;
        Self::get()
    }

    /// Fails on a value its section doesn't take, as reading it from the
    /// file would.
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
    let savers: [(&str, Saver); 5] = [
        (McpSettings::NAME, saved::<McpSettings>),
        (OcrBoxSettings::NAME, saved::<OcrBoxSettings>),
        (LetteringSettings::NAME, saved::<LetteringSettings>),
        (FontSettings::NAME, saved::<FontSettings>),
        (RunsSettings::NAME, saved::<RunsSettings>),
    ];
    savers
        .into_iter()
        .find_map(|(section, save)| (section == name).then_some(save))
}

/// Sets each key `values` names in the section `S` as the file holds it, as
/// [`merged`] sets them, and writes it, in one read and write under the
/// settings file's lock. A bad value writes nothing.
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
/// `values` names in the section `name` set to its value, read as `T` as the
/// file would read it, so a bad value fails. A table among the values sets
/// only the keys it names, so its others keep theirs; anything else is set
/// whole. A key the section doesn't have fails too, as does one a table in
/// it doesn't have: the file would drop it, but a caller naming one made a
/// mistake. A key the section's JSON leaves out, as `skip_serializing_if`
/// does, counts as one it doesn't have, unless it held the key and the value
/// is null, which removes it, as it removes a category's font.
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
    use crate::runs::RunOrder;

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
        // In whichever order serde_json keeps a map's keys.
        let mut names = BooharuPreferences::names();
        names.sort();
        assert_eq!(
            names,
            [
                FontSettings::NAME,
                LetteringSettings::NAME,
                McpSettings::NAME,
                OcrBoxSettings::NAME,
                RunsSettings::NAME,
            ]
        );
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
        assert!(!text.contains("stroke ="), "{text}");
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
    fn a_bad_colour_width_or_favourite_fails_closed() {
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
        }
        let widest: LetteringSettings =
            toml::from_str("[default]\nstroke_width = 50").expect("the widest");
        assert_eq!(widest.default.stroke_width, 50.0);
        for text in [r#"favorites = [""]"#, r#"favorites = ["Arial", "  "]"#] {
            assert!(toml::from_str::<FontSettings>(text).is_err(), "took {text}");
        }
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
    fn a_snapshot_fails_on_a_section_it_cant_read() {
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
            })
        );
        // A bad stored value is an error, never Koharu's look or no font.
        fn bad<T>() -> koharu_config::Result<T> {
            Err(koharu_config::Error::Invalid {
                path: "config.toml".into(),
                message: "an outline 60 pixels wide".to_owned(),
            })
        }
        let error = Snapshot::of(bad(), families()).expect_err("a bad text style");
        assert!(
            format!("{error:#}").contains("[booharu-lettering]")
                && format!("{error:#}").contains("60 pixels"),
            "{error:#}"
        );
        let error = Snapshot::of(Ok(lettering), bad()).expect_err("bad Preferred fonts");
        assert!(
            format!("{error:#}").contains("Preferred fonts"),
            "{error:#}"
        );
        // Koharu's own, for what read nothing: no font, the colours Koharu
        // gives kept, and no run policy.
        let default = Snapshot::default();
        assert!(default.lettering.detect_colors && default.font.is_none());
        assert_eq!(default.runs, None);
    }

    #[test]
    fn every_section_is_saved_alone_by_its_name() {
        for name in BooharuPreferences::names() {
            assert!(saver(&name).is_some(), "[{name}] can't be saved alone");
        }
        assert!(saver("booharu-nope").is_none());
    }

    #[test]
    fn a_port_below_1024_fails_closed() {
        let error = toml::from_str::<McpSettings>("port = 80").expect_err("a system port");
        assert!(error.to_string().contains("port 80 isn't"), "{error}");
        for port in [0, 1023, 65536] {
            assert!(toml::from_str::<McpSettings>(&format!("port = {port}")).is_err());
        }
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
    fn an_unknown_ocr_box_mode_fails_closed() {
        let error = toml::from_str::<OcrBoxSettings>(r#"default_mode = "clean""#)
            .expect_err("no such mode");
        assert!(error.to_string().contains("unknown variant"), "{error}");
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
