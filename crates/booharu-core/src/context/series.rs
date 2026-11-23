//! A series: its glossary, its characters, and its notes, one JSON file each
//! in the series folder, `<id>.json`, in Booharu 0.0.4's shape, so 0.0.4's
//! files and exports load and save back with the same keys.
//!
//! Every write is atomic, a temporary file renamed over the file, under one
//! lock for the process, so a crash or a second writer never leaves half a
//! file. What a write takes is checked first and refused when bad: a blank
//! name, a term without a source or a target, or an id that isn't a safe
//! file name, so no id from the interface, the agent, or an imported file
//! names a path outside the folder.

use std::{
    io::{ErrorKind, Write as _},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, PoisonError},
};

use anyhow::{Context as _, Result, bail, ensure};
use atomicwrites::{AtomicFile, OverwriteBehavior};
use koharu_scene::Uuid;
use serde::{Deserialize, Serialize};

use super::same_source;

/// Holds every write of a series file in this process.
static WRITE: Mutex<()> = Mutex::new(());

fn write_lock() -> MutexGuard<'static, ()> {
    WRITE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The longest series id, in characters.
const MAX_ID: usize = 80;

/// A series' glossary, characters, and notes, as 0.0.4 kept them. It's
/// exported to TypeScript as `BooharuSeries`, because Koharu's Gallery exports
/// its own `Series`; a container rename changes no field, so the JSON and the
/// saved series files don't change.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default, rename = "BooharuSeries")]
pub struct Series {
    /// Its file's name without `.json`: 1 to 80 of `a`–`z`, `0`–`9`, and
    /// `-`, a slug of its first name and 12 random hex digits.
    pub id: String,
    /// Never blank.
    pub name: String,
    /// What every chapter's translation reads as background, never
    /// translating it.
    pub notes: String,
    /// Its terms, in the order they were added. One source may come twice,
    /// as 0.0.4's files could hold it: the first locked one is used, else the
    /// first.
    pub glossary: Vec<GlossaryEntry>,
    pub characters: Vec<Character>,
}

/// A glossary term: its source as the pages write it, the translation every
/// page keeps to, and a note on it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct GlossaryEntry {
    /// Random, unique in its series.
    pub id: String,
    pub source: String,
    pub target: String,
    pub note: String,
    pub kind: TermKind,
    /// Whether the term is fixed: a locked term goes before unlocked ones
    /// and wins over one with the same source, and Booharu's automatic steps
    /// never replace it.
    pub locked: bool,
}

/// What kind of term a glossary term is.
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
pub enum TermKind {
    Name,
    Place,
    Term,
    Honorific,
    #[default]
    Other,
}

/// A character of the series: their name as translated, as the pages write
/// it, and how they speak.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct Character {
    pub id: String,
    pub name: String,
    pub original: String,
    /// Their voice: how they speak, the pronouns they use.
    pub notes: String,
}

impl Series {
    /// A new series named `name`, trimmed, with nothing in it and a new id:
    /// a slug of the name's ASCII letters and digits, at most 48 of them, then
    /// 12 random hex digits, as 0.0.4 made one. A blank name fails.
    pub fn new(name: &str) -> Result<Self> {
        let name = name.trim();
        ensure!(!name.is_empty(), "a series needs a name");
        Ok(Self {
            id: new_id(name),
            name: name.to_owned(),
            ..Self::default()
        })
    }

    /// The term `source` names, the first locked one when it's there twice,
    /// else the first.
    pub fn term(&self, source: &str) -> Option<&GlossaryEntry> {
        let mut terms = self
            .glossary
            .iter()
            .filter(|term| same_source(&term.source, source));
        let first = terms.next()?;
        Some(match first.locked {
            true => first,
            false => terms.find(|term| term.locked).unwrap_or(first),
        })
    }

    /// Puts `entry` in the glossary: in place of the term with the same
    /// source, keeping its place and its id, and dropping any other with
    /// that source, so the series ends with one term for it; else after the
    /// others.
    pub fn put(&mut self, entry: GlossaryEntry) {
        let Some(at) = self
            .glossary
            .iter()
            .position(|term| same_source(&term.source, &entry.source))
        else {
            self.glossary.push(entry);
            return;
        };
        let source = entry.source.clone();
        let held = std::mem::take(&mut self.glossary[at].id);
        let id = if held.is_empty() {
            entry.id.clone()
        } else {
            held
        };
        self.glossary[at] = GlossaryEntry { id, ..entry };
        let mut index = 0;
        self.glossary.retain(|term| {
            let kept = index <= at || !same_source(&term.source, &source);
            index += 1;
            kept
        });
    }

    /// Removes every term with the same source as `source`, and answers how
    /// many it removed.
    pub fn remove_term(&mut self, source: &str) -> usize {
        let before = self.glossary.len();
        self.glossary
            .retain(|term| !same_source(&term.source, source));
        before - self.glossary.len()
    }

    /// Fails on what a series file can't hold: an id that isn't a safe file
    /// name, a blank name, or a term without a source or a target.
    pub fn check(&self) -> Result<()> {
        ensure!(
            valid_id(&self.id),
            "\"{}\" isn't a series id: it must be 1 to {MAX_ID} of a-z, 0-9, and -",
            self.id
        );
        ensure!(
            !self.name.trim().is_empty(),
            "the series \"{}\" needs a name",
            self.id
        );
        if let Some(term) = self
            .glossary
            .iter()
            .find(|term| term.source.trim().is_empty() || term.target.trim().is_empty())
        {
            bail!(
                "every term of \"{}\" needs a source and a target; \"{}\" → \"{}\" lacks one",
                self.name.trim(),
                term.source,
                term.target
            );
        }
        Ok(())
    }

    /// The series a file holds, as a 0.0.4 or 0.0.5 export or a series file
    /// writes it, once checked as a save would check it.
    pub fn read_file(path: &Path) -> Result<Self> {
        let text = fs_err::read(path)?;
        let series: Self = serde_json::from_slice(&text)
            .with_context(|| format!("{} isn't a series file", path.display()))?;
        series
            .check()
            .with_context(|| format!("{} can't be imported", path.display()))?;
        Ok(series)
    }

    /// Writes the series to `path` as an export, in 0.0.4's shape, replacing
    /// what's there atomically.
    pub fn write_file(&self, path: &Path) -> Result<()> {
        self.check()?;
        write(path, self)
    }
}

impl GlossaryEntry {
    /// A term of `source` and `target`, trimmed, with a new random id,
    /// unlocked, of no particular kind, with no note.
    pub fn new(source: &str, target: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            source: source.trim().to_owned(),
            target: target.trim().to_owned(),
            ..Self::default()
        }
    }
}

/// Whether `id` can name a series file: 1 to 80 of `a`–`z`, `0`–`9`, and
/// `-`, which every id Booharu 0.0.4 and 0.0.5 made is.
pub fn valid_id(id: &str) -> bool {
    (1..=MAX_ID).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// A new series id for `name`: its ASCII letters and digits in lower case,
/// every other character a `-`, trimmed of `-`, at most 48 characters, or
/// `series` when nothing is left, then `-` and 12 random hex digits.
fn new_id(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|character| match character.is_ascii_alphanumeric() {
            true => character.to_ascii_lowercase(),
            false => '-',
        })
        .collect();
    let slug: String = slug.trim_matches('-').chars().take(48).collect();
    let random = Uuid::new_v4().simple().to_string();
    let slug = if slug.is_empty() { "series" } else { &slug };
    format!("{slug}-{}", &random[random.len() - 12..])
}

/// The series files in one folder: the Booharu home's `series`, or another
/// in tests.
#[derive(Clone, Debug)]
pub struct SeriesStore {
    folder: PathBuf,
}

impl SeriesStore {
    /// The store in the Booharu home's `series` folder, which its first
    /// save makes.
    pub fn open() -> Result<Self> {
        Ok(Self::new(koharu_config::booharu_home()?.join("series")))
    }

    /// The store in `folder`.
    pub fn new(folder: PathBuf) -> Self {
        Self { folder }
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// The file of the series `id`, or why `id` can't name one.
    pub fn path(&self, id: &str) -> Result<PathBuf> {
        ensure!(
            valid_id(id),
            "\"{id}\" isn't a series id: it must be 1 to {MAX_ID} of a-z, 0-9, and -"
        );
        Ok(self.folder.join(format!("{id}.json")))
    }

    /// Every series, by name, ignoring case, then by id. A file that can't
    /// be read is left out and logged, so one bad file hides no other; the
    /// series it holds then fails to open on its own.
    pub fn list(&self) -> Result<Vec<Series>> {
        let entries = match fs_err::read_dir(&self.folder) {
            Ok(entries) => entries,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut found = Vec::new();
        for entry in entries {
            let path = entry?.path();
            if !path.is_file() || path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            match self.get(id) {
                Ok(series) => found.push(series),
                Err(error) => tracing::warn!("a series file was left out: {error:#}"),
            }
        }
        found.sort_by(|a, b| (a.name.to_lowercase(), &a.id).cmp(&(b.name.to_lowercase(), &b.id)));
        Ok(found)
    }

    /// The series `id`. One whose file is gone, or doesn't read as a series
    /// of that id, fails, naming the file.
    pub fn get(&self, id: &str) -> Result<Series> {
        self.find(id)?
            .with_context(|| format!("there is no series \"{id}\""))
    }

    /// The series `id`, or none when its file is gone, as a chapter linking
    /// a deleted series reads it. A file that doesn't read as a series of
    /// that id fails, naming the file.
    pub fn find(&self, id: &str) -> Result<Option<Series>> {
        let path = self.path(id)?;
        let text = match fs_err::read(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let series: Series = serde_json::from_slice(&text)
            .with_context(|| format!("the series file {} can't be read", path.display()))?;
        ensure!(
            series.id == id,
            "the series file {} holds the series \"{}\"",
            path.display(),
            series.id
        );
        Ok(Some(series))
    }

    /// Whether the series `id` has a file.
    pub fn exists(&self, id: &str) -> Result<bool> {
        Ok(self.path(id)?.is_file())
    }

    /// Makes a series named `name` and saves it.
    pub fn create(&self, name: &str) -> Result<Series> {
        let series = Series::new(name)?;
        self.save(&series)?;
        Ok(series)
    }

    /// Saves `series` whole, in place of its file or as a new one, once
    /// checked; a bad one writes nothing.
    pub fn save(&self, series: &Series) -> Result<()> {
        let _held = write_lock();
        self.write(series)
    }

    /// Runs `change` on the series `id` as its file holds it now and saves
    /// what it left, in one read and write under the lock, so a write between
    /// them can't be lost; a series `change` left bad writes nothing.
    pub fn update<R>(&self, id: &str, change: impl FnOnce(&mut Series) -> R) -> Result<R> {
        let _held = write_lock();
        let mut series = self.get(id)?;
        let before = series.clone();
        let changed = change(&mut series);
        ensure!(
            series.id == before.id,
            "a series keeps its id \"{}\"",
            before.id
        );
        if series != before {
            self.write(&series)?;
        }
        Ok(changed)
    }

    /// Names the series `id` `name`, trimmed; a blank name fails.
    pub fn rename(&self, id: &str, name: &str) -> Result<Series> {
        let name = name.trim();
        ensure!(!name.is_empty(), "a series needs a name");
        self.update(id, |series| {
            series.name = name.to_owned();
            series.clone()
        })
    }

    /// Deletes the series `id`. A chapter linking it then reads as linking
    /// none.
    pub fn delete(&self, id: &str) -> Result<()> {
        let path = self.path(id)?;
        let _held = write_lock();
        match fs_err::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => {
                bail!("there is no series \"{id}\"")
            }
            Err(error) => Err(error.into()),
        }
    }

    /// Saves `series`, a file's being imported: in place of the series of
    /// its id with `replace`, else, when that id is taken, as a copy with a
    /// new id, as 0.0.4 kept both. Answers the series as saved.
    pub fn import(&self, mut series: Series, replace: bool) -> Result<Series> {
        series.check()?;
        let _held = write_lock();
        if !replace && self.path(&series.id)?.exists() {
            series.id = new_id(&series.name);
        }
        self.write(&series)?;
        Ok(series)
    }

    /// Writes `series` to its file, once checked, under the lock the caller
    /// holds.
    fn write(&self, series: &Series) -> Result<()> {
        series.check()?;
        fs_err::create_dir_all(&self.folder)?;
        write(&self.path(&series.id)?, series)
    }
}

/// Writes `value` to `path` as pretty JSON, as 0.0.4 wrote its files,
/// replacing what's there atomically.
pub(super) fn write(path: &Path, value: &impl Serialize) -> Result<()> {
    let text = serde_json::to_vec_pretty(value)?;
    AtomicFile::new(path, OverwriteBehavior::AllowOverwrite)
        .write(|file| file.write_all(&text))
        .map_err(|error| match error {
            atomicwrites::Error::Internal(error) | atomicwrites::Error::User(error) => error,
        })
        .with_context(|| format!("{} can't be written", path.display()))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn term(source: &str, target: &str, locked: bool) -> GlossaryEntry {
        GlossaryEntry {
            locked,
            ..GlossaryEntry::new(source, target)
        }
    }

    fn store() -> (tempfile::TempDir, SeriesStore) {
        let dir = tempfile::tempdir().expect("a folder");
        let store = SeriesStore::new(dir.path().join("series"));
        (dir, store)
    }

    #[test]
    fn a_series_file_reads_back_as_it_was_saved() -> Result<()> {
        let (_dir, store) = store();
        assert!(store.list()?.is_empty(), "no folder yet, no series");
        let mut series = store.create(" A / Series ")?;
        assert_eq!(series.name, "A / Series");
        assert!(series.id.starts_with("a---series-"), "{}", series.id);
        assert_eq!(series.id.len(), "a---series-".len() + 12);
        series.notes = "A school in a city of guns.".to_owned();
        series.glossary.push(GlossaryEntry {
            kind: TermKind::Name,
            note: "the club".to_owned(),
            ..term("シャーレ", "Schale", true)
        });
        series.characters.push(Character {
            id: "hina".to_owned(),
            name: "Hina".to_owned(),
            original: "ヒナ".to_owned(),
            notes: "Curt; calls herself watashi.".to_owned(),
        });
        store.save(&series)?;
        assert_eq!(store.get(&series.id)?, series);
        assert_eq!(store.find(&series.id)?, Some(series.clone()));
        assert!(store.exists(&series.id)?);

        let renamed = store.rename(&series.id, "  Blue Archive ")?;
        assert_eq!(renamed.name, "Blue Archive");
        assert_eq!(renamed.id, series.id, "a rename keeps the id");
        assert!(store.rename(&series.id, " ").is_err());

        let other = store.create("archive")?;
        let names: Vec<String> = store
            .list()?
            .into_iter()
            .map(|series| series.name)
            .collect();
        assert_eq!(names, ["archive", "Blue Archive"], "by name, ignoring case");

        store.delete(&series.id)?;
        assert_eq!(
            store.find(&series.id)?,
            None,
            "a deleted series reads as none"
        );
        assert!(store.get(&series.id).is_err());
        assert!(store.delete(&series.id).is_err());
        assert_eq!(store.list()?, [other]);
        Ok(())
    }

    #[test]
    fn a_name_without_ascii_letters_gets_a_plain_id() -> Result<()> {
        let series = Series::new("ブルーアーカイブ")?;
        assert!(series.id.starts_with("series-"), "{}", series.id);
        assert!(valid_id(&series.id));
        let long = Series::new(&"Long name ".repeat(10))?;
        assert!(
            long.id.len() <= 48 + 13 && valid_id(&long.id),
            "{}",
            long.id
        );
        assert!(Series::new("  ").is_err());
        Ok(())
    }

    /// A series export as Booharu 0.0.4 wrote it, every key in its order.
    const EXPORTED: &str = r#"{
  "id": "blue-archive-0f3a9c2b7d41",
  "name": "Blue Archive",
  "notes": "Students of Kivotos.\nSensei is the reader.",
  "glossary": [
    {
      "id": "0190c3c4-7b8a-7d2e-9a51-3f2b8c9d4e01",
      "source": "シャーレ",
      "target": "Schale",
      "note": "",
      "kind": "place",
      "locked": true
    },
    {
      "id": "0190c3c4-7b8a-7d2e-9a51-3f2b8c9d4e02",
      "source": "先生",
      "target": "Sensei",
      "note": "always this spelling",
      "kind": "honorific",
      "locked": false
    }
  ],
  "characters": [
    {
      "id": "0190c3c4-7b8a-7d2e-9a51-3f2b8c9d4e03",
      "name": "Hina",
      "original": "ヒナ",
      "notes": "Curt, tired."
    }
  ]
}"#;

    #[test]
    fn a_0_0_4_export_loads_and_saves_back_with_the_same_keys() -> Result<()> {
        let (dir, store) = store();
        let file = dir.path().join("Blue Archive.json");
        fs_err::write(&file, EXPORTED)?;
        let series = Series::read_file(&file)?;
        assert_eq!(series.glossary[0].kind, TermKind::Place);
        assert!(series.glossary[0].locked);

        // Saved in the store and exported again, it's the same file.
        let saved = store.import(series.clone(), false)?;
        assert_eq!(saved, series);
        let written = fs_err::read_to_string(store.path(&series.id)?)?;
        assert_eq!(written, EXPORTED);
        let export = dir.path().join("again.json");
        store.get(&series.id)?.write_file(&export)?;
        assert_eq!(
            serde_json::from_str::<Value>(&fs_err::read_to_string(&export)?)?,
            serde_json::from_str::<Value>(EXPORTED)?
        );
        Ok(())
    }

    #[test]
    fn an_import_replaces_or_keeps_both() -> Result<()> {
        let (_dir, store) = store();
        let mut series = store.create("Blue Archive")?;
        series.glossary.push(term("ヒナ", "Hina", false));
        let copy = store.import(series.clone(), false)?;
        assert_ne!(copy.id, series.id, "a taken id gets a new one");
        assert!(copy.id.starts_with("blue-archive-"));
        assert_eq!(copy.glossary, series.glossary);
        assert_eq!(store.import(series.clone(), true)?, series);
        assert_eq!(store.get(&series.id)?, series, "replaced");
        assert_eq!(store.list()?.len(), 2);
        Ok(())
    }

    #[test]
    fn ids_that_arent_safe_file_names_are_refused() {
        let (_dir, store) = store();
        for id in [
            "",
            "../unsafe",
            "a/b",
            "a\\b",
            "Blue-Archive",
            "blue_archive",
            "série",
            "blue archive",
            &"a".repeat(81),
        ] {
            assert!(!valid_id(id), "took {id:?}");
            assert!(store.path(id).is_err(), "took {id:?}");
            assert!(store.get(id).is_err(), "took {id:?}");
            assert!(store.delete(id).is_err(), "took {id:?}");
            let series = Series {
                id: id.to_owned(),
                name: "Named".to_owned(),
                ..Series::default()
            };
            assert!(store.save(&series).is_err(), "saved {id:?}");
            assert!(store.import(series, true).is_err(), "imported {id:?}");
        }
        assert!(valid_id(&"a".repeat(80)));
        assert!(!store.folder().exists(), "nothing was written");
    }

    #[test]
    fn a_bad_series_writes_nothing() -> Result<()> {
        let (_dir, store) = store();
        let series = store.create("Blue Archive")?;
        let refused = [
            Series {
                name: " ".to_owned(),
                ..series.clone()
            },
            Series {
                glossary: vec![term("ヒナ", " ", false)],
                ..series.clone()
            },
            Series {
                glossary: vec![term("", "Hina", true)],
                ..series.clone()
            },
        ];
        for bad in refused {
            assert!(store.save(&bad).is_err(), "saved {bad:?}");
            assert_eq!(store.get(&series.id)?, series);
        }
        let failed = store.update(&series.id, |series| series.name.clear());
        assert!(failed.is_err());
        assert_eq!(store.get(&series.id)?, series);
        Ok(())
    }

    #[test]
    fn one_bad_file_hides_no_other_series() -> Result<()> {
        let (_dir, store) = store();
        let good = store.create("Good")?;
        fs_err::write(
            store.folder().join("broken-000000000000.json"),
            "{ not json",
        )?;
        fs_err::write(store.folder().join("Not An Id.json"), "{}")?;
        fs_err::write(
            store.folder().join("elsewhere-000000000000.json"),
            json!({"id": "other", "name": "Other"}).to_string(),
        )?;
        assert_eq!(store.list()?, [good]);
        let error = store.get("broken-000000000000").expect_err("a broken file");
        assert!(
            format!("{error:#}").contains("broken-000000000000.json"),
            "{error:#}"
        );
        assert!(
            store.get("elsewhere-000000000000").is_err(),
            "an id not its own"
        );
        Ok(())
    }

    #[test]
    fn a_term_is_put_in_place_of_the_one_with_its_source() {
        let mut series = Series::new("Blue Archive").expect("a series");
        series.glossary = vec![
            term("ヒナ", "Hina", false),
            term("先生", "Teacher", false),
            term(" 先 生", "Sensei", true),
            term("シャーレ", "Schale", false),
        ];
        let first = series.glossary[1].id.clone();
        assert_eq!(
            series.term("先生").map(|term| &*term.target),
            Some("Sensei")
        );
        assert_eq!(series.term("ヒナ").map(|term| &*term.target), Some("Hina"));
        assert_eq!(series.term("リムル"), None);

        series.put(term("先生", "Sensei", false));
        let targets: Vec<&str> = series.glossary.iter().map(|term| &*term.target).collect();
        assert_eq!(targets, ["Hina", "Sensei", "Schale"]);
        assert_eq!(
            series.glossary[1].id, first,
            "it keeps the term's place and id"
        );
        assert!(!series.glossary[1].locked);

        series.put(term("リムル", "Rimuru", true));
        assert_eq!(
            series.glossary.last().map(|term| &*term.target),
            Some("Rimuru")
        );
        assert_eq!(series.remove_term(" りむる"), 0);
        assert_eq!(series.remove_term("リム ル"), 1);
        assert_eq!(series.glossary.len(), 3);
    }
}
