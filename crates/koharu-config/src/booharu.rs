//! Booharu's part of koharu-config: how a save of one of Booharu's sections,
//! a `[booharu-<area>]` table, writes a key the file holds a value in that
//! doesn't read.
//!
//! The read drops what doesn't read, the key its error names, so a map whose
//! entries are checked together, as Booharu's category fonts and emphasis
//! are, reads as its default for one bad entry, and so does a table checked
//! whole that's left setting nothing. Koharu's write then edits the file's
//! own map or table key by key against that default, which leaves the bad
//! entry in it, so the change wouldn't read back and the save failed,
//! naming nothing. A Booharu section writes such a key whole instead: the
//! value as read, with the change, over what was edited by hand.

use std::{collections::BTreeSet, path::Path};

use toml_edit::{DocumentMut, Item};

use super::{Result, Section, edit_table, edit_value, implicit_table, section, table};

/// Writes whole, in `document`, each key of the Booharu section `T` that
/// doesn't read back as `after` sets it once Koharu's edit is in; a key set
/// to its default in `defaults` leaves the file. Any other section, Koharu's
/// own, is left as Koharu's edit left it.
pub(super) fn whole<T: Section>(
    path: &Path,
    document: &mut DocumentMut,
    after: &toml::Table,
    defaults: &toml::Table,
) -> Result<()> {
    if !T::NAME.starts_with("booharu-") {
        return Ok(());
    }
    let read = table(&section::<T>(path, &document.to_string())?);
    if read == *after {
        return Ok(());
    }
    let Some(held) = document.get_mut(T::NAME).and_then(Item::as_table_like_mut) else {
        return Ok(());
    };
    let keys: BTreeSet<&String> = after.keys().chain(read.keys()).collect();
    for key in keys {
        let value = after.get(key);
        if read.get(key) == value {
            continue;
        }
        match value {
            Some(value) if defaults.get(key) != Some(value) => {
                held.insert(key, written(value));
            }
            _ => {
                held.remove(key);
            }
        }
    }
    Ok(())
}

/// `value` as the file holds it: a table with a header of its own, as
/// Koharu's edit writes a table new to the file, or a value.
fn written(value: &toml::Value) -> Item {
    match value {
        toml::Value::Table(value) => {
            let mut item = implicit_table();
            if let Some(table) = item.as_table_like_mut() {
                edit_table(table, &toml::Table::new(), value, None);
            }
            item
        }
        value => Item::Value(edit_value(value)),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use indoc::indoc;
    use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

    use super::*;
    use crate::{Error, read, write};

    /// A section shaped as Booharu's lettering: a map checked as one, keyed
    /// by a variant, so one bad entry drops it whole, and a value beside it.
    /// This one is Koharu's, by its name.
    #[derive(Debug, Default, PartialEq, Deserialize, Serialize)]
    #[serde(default)]
    struct Shaped {
        auto: bool,
        #[serde(deserialize_with = "fonts")]
        fonts: BTreeMap<Kind, String>,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
    #[serde(rename_all = "snake_case")]
    enum Kind {
        Shout,
        Whisper,
        Thoughts,
    }

    fn fonts<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<Kind, String>, D::Error> {
        let fonts = BTreeMap::<Kind, String>::deserialize(deserializer)?;
        match fonts.values().any(|family| family.trim().is_empty()) {
            true => Err(D::Error::custom("a blank family")),
            false => Ok(fonts),
        }
    }

    impl Section for Shaped {
        const NAME: &'static str = "shaped";
    }

    /// The same section as Booharu's: a `[booharu-<area>]` table.
    #[derive(Debug, Default, PartialEq, Deserialize, Serialize)]
    #[serde(default)]
    struct BooharuShaped {
        auto: bool,
        #[serde(deserialize_with = "fonts")]
        fonts: BTreeMap<Kind, String>,
    }

    impl Section for BooharuShaped {
        const NAME: &'static str = "booharu-shaped";
    }

    const STORED: &str = indoc! {r#"
        [booharu-shaped]
        auto = true

        [booharu-shaped.fonts]
        shout = "Anime Ace" # chosen
        whisper = ""

        [shaped]
        auto = true

        [shaped.fonts]
        shout = "Anime Ace"
        whisper = ""
    "#};

    #[test]
    fn a_change_to_a_booharu_map_holding_a_bad_entry_writes_it_whole() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        fs_err::write(&path, STORED).unwrap();
        // One bad entry drops the map, so it reads as its default, none.
        let stored: BooharuShaped = read(&path).unwrap();
        assert!(stored.auto && stored.fonts.is_empty());

        // A change elsewhere leaves the map as the file holds it, its
        // comment and its bad entry with it.
        write(&path, |shaped: &mut BooharuShaped| shaped.auto = false).unwrap();
        let text = fs_err::read_to_string(&path).unwrap();
        assert!(
            text.contains(indoc! {r#"
                [booharu-shaped.fonts]
                shout = "Anime Ace" # chosen
                whisper = ""
            "#}),
            "{text}"
        );

        // A change to the map writes it as read, with the change.
        write(&path, |shaped: &mut BooharuShaped| {
            shaped.fonts.insert(Kind::Thoughts, "Komika".into());
        })
        .unwrap();
        let text = fs_err::read_to_string(&path).unwrap();
        let saved: BooharuShaped = read(&path).unwrap();
        assert_eq!(
            saved.fonts,
            BTreeMap::from([(Kind::Thoughts, "Komika".to_owned())])
        );
        assert!(!text.contains("Anime Ace\" # chosen"), "{text}");
        // Koharu's section beside it is left alone.
        assert!(
            text.contains(indoc! {r#"
                [shaped]
                auto = true

                [shaped.fonts]
                shout = "Anime Ace"
                whisper = ""
            "#}),
            "{text}"
        );

        // Set back to its default, the map leaves the file.
        write(&path, |shaped: &mut BooharuShaped| shaped.fonts.clear()).unwrap();
        assert!(!fs_err::read_to_string(&path).unwrap().contains("Komika"));
    }

    #[test]
    fn a_hand_written_inline_or_dotted_booharu_map_is_written_whole_too() {
        // The shapes a hand-edited file often has, each holding the bad
        // entry: an inline section, dotted keys at the root, and dotted keys
        // in the section's table.
        let shapes = [
            "booharu-shaped = { auto = true, fonts = { shout = \"Anime Ace\", whisper = \"\" } }\n",
            indoc! {r#"
                booharu-shaped.auto = true
                booharu-shaped.fonts.shout = "Anime Ace"
                booharu-shaped.fonts.whisper = ""
            "#},
            indoc! {r#"
                [booharu-shaped]
                auto = true
                fonts.shout = "Anime Ace"
                fonts.whisper = ""
            "#},
        ];
        for stored in shapes {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("config.toml");
            fs_err::write(&path, stored).unwrap();

            write(&path, |shaped: &mut BooharuShaped| {
                shaped.fonts.insert(Kind::Thoughts, "Komika".into());
            })
            .unwrap_or_else(|error| panic!("{error}, saving over:\n{stored}"));
            let text = fs_err::read_to_string(&path).unwrap();
            assert!(text.parse::<toml::Table>().is_ok(), "{text}");
            assert_eq!(
                read::<BooharuShaped>(&path).unwrap(),
                BooharuShaped {
                    auto: true,
                    fonts: BTreeMap::from([(Kind::Thoughts, "Komika".to_owned())]),
                },
                "{text}"
            );
            assert!(!text.contains("Anime Ace"), "{text}");
        }
    }

    #[test]
    fn koharus_own_section_still_refuses_such_a_change() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        fs_err::write(&path, STORED).unwrap();

        let refused = write(&path, |shaped: &mut Shaped| {
            shaped.fonts.insert(Kind::Thoughts, "Komika".into());
        });
        assert!(matches!(refused, Err(Error::Invalid { .. })), "{refused:?}");
        assert_eq!(fs_err::read_to_string(&path).unwrap(), STORED);
    }
}
