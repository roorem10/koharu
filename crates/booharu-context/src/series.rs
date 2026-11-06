use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    sync::Mutex,
};

use anyhow::{Context as _, Result, ensure};
use fs4::FileExt;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{Character, GlossaryEntry};

static WRITE: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, Type)]
#[serde(default)]
pub struct Series {
    pub id: String,
    pub name: String,
    pub notes: String,
    pub glossary: Vec<GlossaryEntry>,
    pub characters: Vec<Character>,
}

impl Series {
    pub fn new(name: String) -> Result<Self> {
        ensure!(!name.trim().is_empty(), "series name is empty");
        let slug: String = name
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        let slug = slug.trim_matches('-');
        let slug: String = slug.chars().take(48).collect();
        let random = koharu_scene::EntityId::new().to_string();
        Ok(Self {
            id: format!(
                "{}-{}",
                if slug.is_empty() { "series" } else { &slug },
                &random[random.len() - 12..]
            ),
            name: name.trim().into(),
            ..Default::default()
        })
    }
}

pub struct SeriesStore {
    directory: PathBuf,
}

impl SeriesStore {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    fn path(&self, id: &str) -> Result<PathBuf> {
        ensure!(
            !id.is_empty()
                && id.len() <= 96
                && id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
            "invalid series id"
        );
        Ok(self.directory.join(format!("{id}.json")))
    }

    pub fn list(&self) -> Result<Vec<Series>> {
        if !self.directory.exists() {
            return Ok(Vec::new());
        }
        let mut series = Vec::new();
        for entry in std::fs::read_dir(&self.directory)? {
            let path = entry?.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                let id = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .context("invalid series filename")?;
                series.push(self.get(id)?);
            }
        }
        series.sort_by_key(|series| series.name.to_lowercase());
        Ok(series)
    }

    pub fn get(&self, id: &str) -> Result<Series> {
        let series: Series = serde_json::from_slice(&std::fs::read(self.path(id)?)?)?;
        ensure!(series.id == id, "series id does not match its file");
        Ok(series)
    }

    pub fn save(&self, series: &Series) -> Result<()> {
        let _guard = WRITE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _file = self.lock()?;
        self.save_unlocked(series)
    }

    fn save_unlocked(&self, series: &Series) -> Result<()> {
        let path = self.path(&series.id)?;
        ensure!(!series.name.trim().is_empty(), "series name is empty");
        ensure!(
            series
                .glossary
                .iter()
                .all(|term| !term.source.trim().is_empty() && !term.target.trim().is_empty()),
            "glossary terms need source and target"
        );
        atomic_json(&path, series)
    }

    fn lock(&self) -> Result<File> {
        std::fs::create_dir_all(&self.directory)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.directory.join(".series.lock"))?;
        FileExt::lock(&file)?;
        Ok(file)
    }

    /// The lock covers the read as well as the atomic publication, including
    /// other app processes opening a different chapter of the same series.
    pub fn update<R>(
        &self,
        id: &str,
        change: impl FnOnce(&mut Series) -> Result<R>,
    ) -> Result<R> {
        let _guard = WRITE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _file = self.lock()?;
        let mut series = self.get(id)?;
        let before = series.clone();
        let result = change(&mut series)?;
        ensure!(series.id == id, "series id cannot change");
        if series != before {
            self.save_unlocked(&series)?;
        }
        Ok(result)
    }

    /// Replays only the fields an edit changed. Unrelated chapter additions
    /// survive undo; a later change to an observed entry rejects stale replay.
    pub fn apply_delta(&self, expected: &Series, next: &Series) -> Result<Series> {
        ensure!(expected.id == next.id, "series id cannot change");
        self.update(&expected.id, |current| {
            if expected.name != next.name {
                ensure!(current.name == expected.name, "series name changed");
                current.name = next.name.clone();
            }
            if expected.notes != next.notes {
                ensure!(current.notes == expected.notes, "series notes changed");
                current.notes = next.notes.clone();
            }
            apply_entries(
                &mut current.glossary,
                &expected.glossary,
                &next.glossary,
                |entry| &entry.id,
                |left, right| left.source.trim() == right.source.trim(),
            )?;
            apply_entries(
                &mut current.characters,
                &expected.characters,
                &next.characters,
                |entry| &entry.id,
                |left, right| {
                    (!left.original.trim().is_empty()
                        && left.original.trim() == right.original.trim())
                        || left.name.trim().to_lowercase() == right.name.trim().to_lowercase()
                },
            )?;
            Ok(current.clone())
        })
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let _guard = WRITE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _file = self.lock()?;
        std::fs::remove_file(self.path(id)?)?;
        Ok(())
    }

    pub fn import(&self, text: &str, replace: bool) -> Result<Series> {
        let mut series: Series = serde_json::from_str(text)?;
        let _guard = WRITE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _file = self.lock()?;
        if self.path(&series.id)?.exists() && !replace {
            series.id = Series::new(series.name.clone())?.id;
        }
        self.save_unlocked(&series)?;
        Ok(series)
    }
}

fn apply_entries<T: Clone + PartialEq>(
    current: &mut Vec<T>,
    expected: &[T],
    next: &[T],
    id: impl Fn(&T) -> &str,
    same_entry: impl Fn(&T, &T) -> bool,
) -> Result<()> {
    for before in expected {
        let after = next.iter().find(|entry| id(entry) == id(before));
        if after == Some(before) {
            continue;
        }
        let index = current
            .iter()
            .position(|entry| id(entry) == id(before))
            .context("series entry was removed")?;
        ensure!(current[index] == *before, "series entry changed");
        if let Some(after) = after {
            ensure!(
                !current
                    .iter()
                    .any(|entry| id(entry) != id(before) && same_entry(entry, after)),
                "series entry conflicts with another entry"
            );
            current[index] = after.clone();
        } else {
            current.remove(index);
        }
    }
    for after in next {
        if expected.iter().any(|entry| id(entry) == id(after)) {
            continue;
        }
        ensure!(
            !current
                .iter()
                .any(|entry| id(entry) == id(after) || same_entry(entry, after)),
            "series entry already exists"
        );
        current.push(after.clone());
    }
    Ok(())
}

pub fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    use std::io::Write as _;
    let directory = path.parent().context("missing parent directory")?;
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    file.write_all(&serde_json::to_vec_pretty(value)?)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn term(id: &str) -> GlossaryEntry {
        GlossaryEntry {
            id: id.into(),
            source: id.into(),
            target: id.into(),
            ..Default::default()
        }
    }

    #[test]
    fn concurrent_chapters_merge_against_the_latest_series() {
        let directory = tempfile::tempdir().unwrap();
        let store = SeriesStore::new(directory.path().into());
        let series = Series::new("Shared".into()).unwrap();
        store.save(&series).unwrap();
        let ready = std::sync::Arc::new(std::sync::Barrier::new(2));
        let threads: Vec<_> = ["chapter-one", "chapter-two"].into_iter().map(|id| {
            let directory = directory.path().to_owned();
            let series = series.id.clone();
            let ready = ready.clone();
            std::thread::spawn(move || {
                let store = SeriesStore::new(directory);
                ready.wait();
                store.update(&series, |series| {
                    series.glossary.push(term(id));
                    Ok(())
                }).unwrap();
            })
        }).collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(store.get(&series.id).unwrap().glossary.len(), 2);
    }

    #[test]
    fn undo_preserves_other_chapter_entries_and_rejects_stale_edits() {
        let directory = tempfile::tempdir().unwrap();
        let store = SeriesStore::new(directory.path().into());
        let before = Series::new("Shared".into()).unwrap();
        let mut after = before.clone();
        after.glossary.push(term("one"));
        store.save(&after).unwrap();
        store.update(&before.id, |series| {
            series.glossary.push(term("two"));
            Ok(())
        }).unwrap();
        let undone = store.apply_delta(&after, &before).unwrap();
        assert_eq!(undone.glossary, vec![term("two")]);
        store.apply_delta(&before, &after).unwrap();
        store.update(&before.id, |series| {
            series.glossary.iter_mut().find(|entry| entry.id == "one").unwrap().target = "Edited".into();
            Ok(())
        }).unwrap();
        let edited = store.get(&before.id).unwrap();
        assert!(store.apply_delta(&after, &before).is_err());
        assert_eq!(store.get(&before.id).unwrap(), edited);
    }

    #[test]
    fn stale_addition_cannot_duplicate_a_source_with_another_id() {
        let directory = tempfile::tempdir().unwrap();
        let store = SeriesStore::new(directory.path().into());
        let before = Series::new("Shared".into()).unwrap();
        let mut next = before.clone();
        next.glossary.push(term("one"));
        let mut current = next.clone();
        current.glossary[0].id = "other-id".into();
        store.save(&current).unwrap();
        assert!(store.apply_delta(&before, &next).is_err());
        assert_eq!(store.get(&before.id).unwrap(), current);
    }

    #[test]
    fn save_load_import_and_delete_are_separate_from_project_storage() {
        let directory = tempfile::tempdir().unwrap();
        let store = SeriesStore::new(directory.path().join("series"));
        let mut series = Series::new("A / Series".into()).unwrap();
        series.glossary.push(GlossaryEntry {
            id: "one".into(),
            source: "ヒナ".into(),
            target: "Hina".into(),
            locked: true,
            ..Default::default()
        });
        store.save(&series).unwrap();
        assert_eq!(store.get(&series.id).unwrap(), series);
        let text = serde_json::to_string(&series).unwrap();
        let copy = store.import(&text, false).unwrap();
        assert_ne!(copy.id, series.id);
        assert_eq!(copy.glossary, series.glossary);
        assert_eq!(store.import(&text, true).unwrap(), series);
        store.delete(&series.id).unwrap();
        assert_eq!(store.list().unwrap(), vec![copy]);
        assert!(store.get("../unsafe").is_err());
    }
}
