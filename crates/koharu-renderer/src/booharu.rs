//! Booharu's part of the renderer: the names font families go by in other
//! languages, which Booharu's font picker shows muted after a family's name
//! and searches, so typing 游ゴシック finds Yu Gothic. Koharu's `FontFamily`
//! stays as it is.

use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::LazyLock,
};

use anyhow::Result;
use fontique::{Collection, CollectionOptions, SourceCache};
use parking_lot::Mutex;
use skrifa::{FontRef, MetadataProvider, string::StringId};

/// Each family with names in other languages, by the name the font list
/// shows, with those names: the installed families', and the bundled
/// catalog's whose faces are on disk already, as a bundled face downloads
/// only when a text asks for it, and never for this. A family with none is
/// left out. It's worked out on a blocking thread: an installed family's
/// names are read once, as Koharu lists the installed families once, and a
/// bundled face's once it's on disk.
pub async fn local_names() -> Result<BTreeMap<String, Vec<String>>> {
    Ok(tokio::task::spawn_blocking(|| {
        let mut found = BTreeMap::new();
        for (family, names) in INSTALLED.iter() {
            merge(&mut found, family, names);
        }
        for (family, path) in crate::fonts::stored_faces() {
            let names = BUNDLED
                .lock()
                .entry(path)
                .or_insert_with_key(|path| match fs_err::read(path) {
                    Ok(data) => local_family_names(&data, 0, &family),
                    Err(error) => {
                        tracing::warn!("couldn't read a bundled font's names: {error:#}");
                        Vec::new()
                    }
                })
                .clone();
            merge(&mut found, &family, &names);
        }
        found
            .into_values()
            .filter(|(_, names)| !names.is_empty())
            .collect()
    })
    .await?)
}

/// The names the font at `index` of the file `data` gives its family in
/// languages other than English, its typographic family names first, then
/// its family names, each once, leaving out its English names and `family`,
/// the name Koharu lists it by. None when the font can't be read.
pub fn local_family_names(data: &[u8], index: u32, family: &str) -> Vec<String> {
    let Ok(font) = FontRef::from_index(data, index) else {
        return Vec::new();
    };
    let ids = [StringId::TYPOGRAPHIC_FAMILY_NAME, StringId::FAMILY_NAME];
    let mut known: Vec<String> = vec![normalize(family)];
    for id in ids {
        known.extend(
            font.localized_strings(id)
                .filter(|string| string.language().is_none_or(english))
                .map(|string| normalize(&string.to_string())),
        );
    }
    let mut names = Vec::new();
    for id in ids {
        for string in font.localized_strings(id) {
            let name = string.to_string();
            let name = name.trim();
            if name.is_empty() || known.contains(&normalize(name)) {
                continue;
            }
            known.push(normalize(name));
            names.push(name.to_owned());
        }
    }
    names
}

/// Each installed family by its name, the system's aliases included, with
/// its names in other languages, as the system had them when first asked.
static INSTALLED: LazyLock<Vec<(String, Vec<String>)>> = LazyLock::new(|| {
    let mut collection = Collection::new(CollectionOptions {
        shared: false,
        system_fonts: true,
    });
    let mut sources = SourceCache::default();
    let families: Vec<String> = collection.family_names().map(str::to_owned).collect();
    families
        .into_iter()
        .filter_map(|family| {
            let info = collection.family_by_name(&family)?;
            let font = info.default_font()?;
            let data = font.load(Some(&mut sources))?;
            let names = local_family_names(data.as_ref(), font.index(), &family);
            Some((family, names))
        })
        .collect()
});

/// A bundled face's names in other languages, by its file, read once: a
/// file of the catalog's pinned snapshot never changes.
static BUNDLED: LazyLock<Mutex<HashMap<PathBuf, Vec<String>>>> = LazyLock::new(Mutex::default);

/// Whether `language`, a BCP 47 tag, is English.
fn english(language: &str) -> bool {
    language == "en" || language.starts_with("en-")
}

/// Adds `names`, `family`'s names in other languages, to `found`, under the
/// family's name as Koharu matches it, spelt as it was first found, each
/// once: Koharu lists an installed family and a bundled one of the same name
/// as one, by the installed one's spelling.
fn merge(found: &mut BTreeMap<String, (String, Vec<String>)>, family: &str, names: &[String]) {
    let (_, known) = found
        .entry(normalize(family))
        .or_insert_with(|| (family.to_owned(), Vec::new()));
    for name in names {
        if !known
            .iter()
            .any(|known| normalize(known) == normalize(name))
        {
            known.push(name.clone());
        }
    }
}

/// A family name as Koharu matches it: trimmed, in any case.
fn normalize(name: &str) -> String {
    name.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOWS: u16 = 3;
    const MAC: u16 = 1;
    const ENGLISH: u16 = 0x0409;
    const JAPANESE: u16 = 0x0411;
    const KOREAN: u16 = 0x0412;
    const CHINESE: u16 = 0x0804;
    const FAMILY: u16 = 1;
    const TYPOGRAPHIC: u16 = 16;

    /// A font file holding only a `name` table of `records`, each a platform,
    /// a language, a name id, and the name.
    fn font(records: &[(u16, u16, u16, &str)]) -> Vec<u8> {
        let mut strings = Vec::new();
        let mut table = Vec::new();
        let count = u16::try_from(records.len()).expect("a few records");
        for value in [0, count, 6 + 12 * count] {
            table.extend(value.to_be_bytes());
        }
        for &(platform, language, id, name) in records {
            // Windows names are UTF-16 and the Mac's Roman, ASCII here.
            let (encoding, bytes): (u16, Vec<u8>) = match platform {
                WINDOWS => (1, name.encode_utf16().flat_map(u16::to_be_bytes).collect()),
                _ => (0, name.bytes().collect()),
            };
            let length = u16::try_from(bytes.len()).expect("a short name");
            let offset = u16::try_from(strings.len()).expect("short names");
            for value in [platform, encoding, language, id, length, offset] {
                table.extend(value.to_be_bytes());
            }
            strings.extend(bytes);
        }
        table.extend(strings);
        let mut file = Vec::new();
        file.extend(0x0001_0000_u32.to_be_bytes());
        for value in [1_u16, 16, 0, 0] {
            file.extend(value.to_be_bytes());
        }
        file.extend(*b"name");
        file.extend(0_u32.to_be_bytes());
        file.extend(28_u32.to_be_bytes());
        file.extend(
            u32::try_from(table.len())
                .expect("a small table")
                .to_be_bytes(),
        );
        file.extend(table);
        file
    }

    #[test]
    fn a_family_goes_by_its_names_in_other_languages() {
        let data = font(&[
            (WINDOWS, ENGLISH, FAMILY, "Yu Gothic Medium"),
            (WINDOWS, ENGLISH, TYPOGRAPHIC, "Yu Gothic"),
            (MAC, 0, FAMILY, "Yu Gothic Medium"),
            (WINDOWS, JAPANESE, FAMILY, "游ゴシック Medium"),
            (WINDOWS, JAPANESE, TYPOGRAPHIC, "游ゴシック"),
            // The same name twice, an English name, and the family's own.
            (WINDOWS, CHINESE, TYPOGRAPHIC, "游ゴシック"),
            (WINDOWS, KOREAN, FAMILY, "Yu Gothic Medium"),
            (WINDOWS, KOREAN, TYPOGRAPHIC, "yu gothic "),
        ]);
        assert_eq!(
            local_family_names(&data, 0, "Yu Gothic"),
            ["游ゴシック", "游ゴシック Medium"]
        );
        // A family with only English names has none, nor does a file that
        // isn't a font.
        let english = font(&[(WINDOWS, ENGLISH, FAMILY, "Arial")]);
        assert!(local_family_names(&english, 0, "Arial").is_empty());
        assert!(local_family_names(b"not a font", 0, "Arial").is_empty());
    }

    #[test]
    fn a_name_in_another_language_that_repeats_the_familys_is_left_out() {
        // The repository's Noto Sans CJK, whose Japanese family name is its
        // English one.
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/koharu/app/fonts/NotoSansCJK-VF.otf"
        );
        let data = fs_err::read(path).expect("the repository's CJK font");
        assert!(local_family_names(&data, 0, "Noto Sans CJK JP").is_empty());
    }

    #[test]
    fn a_family_installed_and_bundled_is_one() {
        let mut found = BTreeMap::new();
        merge(&mut found, "Noto Sans JP", &["ノート".to_owned()]);
        merge(
            &mut found,
            "noto sans jp",
            &["ノート".to_owned(), "諾托".to_owned()],
        );
        assert_eq!(
            found.into_values().collect::<Vec<_>>(),
            [(
                "Noto Sans JP".to_owned(),
                vec!["ノート".to_owned(), "諾托".to_owned()]
            )]
        );
    }
}
