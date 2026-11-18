//! Booharu's settings: a `[booharu-<area>]` table per area in the Booharu
//! home's `config.toml`, each a [`Section`] that owns its defaults, as
//! Koharu's own sections do. A value is checked as it is read, so a bad one
//! fails whether the file or a caller holds it, and a save writes only what
//! differs from the defaults.
//!
//! [`BooharuPreferences`] holds every section, keyed by its table's name as
//! Koharu's `Preferences` keys its own: the desktop's settings commands read
//! and save it whole, and the settings tool reaches one section by its name.

use anyhow::{anyhow, bail};
use koharu_config::Section;
use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned, de::Error as _};
use serde_json::{Map, Value};

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
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, specta::Type)]
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

/// Every Booharu settings section, each under its table's name. Koharu's
/// own stay in its `Preferences`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct BooharuPreferences {
    #[serde(rename = "booharu-mcp")]
    pub mcp: McpSettings,
    #[serde(rename = "booharu-ocr-box")]
    pub ocr_box: OcrBoxSettings,
}

impl BooharuPreferences {
    /// Every section as the file holds it now. A bad stored value is an
    /// error, never its default.
    pub fn get() -> anyhow::Result<Self> {
        Ok(Self {
            mcp: koharu_config::get()?,
            ocr_box: koharu_config::get()?,
        })
    }

    /// Saves every section, writing only what differs from its defaults, and
    /// answers them as the file holds them now.
    pub fn save(self) -> anyhow::Result<Self> {
        // Every section is read and checked before any is written, so a bad
        // value, stored or given, writes nothing.
        Self::get()?;
        self.check()?;
        let Self { mcp, ocr_box } = self;
        koharu_config::update(|section: &mut McpSettings| *section = mcp)?;
        koharu_config::update(|section: &mut OcrBoxSettings| *section = ocr_box)?;
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

    /// Sets each key `values` names in the section named `name` to its whole
    /// new value. An unknown section or key, or a bad value, fails and
    /// changes nothing.
    pub fn set_section(&mut self, name: &str, values: Map<String, Value>) -> anyhow::Result<()> {
        *self = merged(self, name, values)?;
        Ok(())
    }

    /// Fails on a value its section doesn't take, as reading it from the
    /// file would.
    fn check(&self) -> anyhow::Result<()> {
        serde_json::from_value::<Self>(serde_json::to_value(self)?)?;
        Ok(())
    }
}

/// `sections` with each key `values` names in the section `name` set to its
/// value, read again as the file would read it, so a bad value fails. So
/// does a key the section doesn't have: the file would ignore it, but a
/// caller naming one made a mistake. A key the section's JSON leaves out, as
/// `skip_serializing_if` does, counts as one it doesn't have.
fn merged<T: Serialize + DeserializeOwned>(
    sections: &T,
    name: &str,
    values: Map<String, Value>,
) -> anyhow::Result<T> {
    let mut tables = tables(sections);
    let Some(Value::Object(table)) = tables.get_mut(name) else {
        return Err(unknown(name));
    };
    for (key, value) in values {
        let Some(slot) = table.get_mut(&key) else {
            bail!("[{name}] has no key `{key}`");
        };
        *slot = value;
    }
    serde_json::from_value(Value::Object(tables)).map_err(|error| anyhow!("[{name}]: {error}"))
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
        assert_eq!(
            BooharuPreferences::names(),
            [McpSettings::NAME, OcrBoxSettings::NAME]
        );
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
        let changed = merged(&sections, "booharu-a", values(json!({"number": 3})));
        assert_eq!(
            changed.ok(),
            Some(Sections {
                a: pair(3, "kept"),
                b: pair(2, "also kept"),
            })
        );
    }
}
