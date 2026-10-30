use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use anyhow::{Context as _, Result, ensure};
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
        let path = self.path(&series.id)?;
        ensure!(!series.name.trim().is_empty(), "series name is empty");
        ensure!(
            series
                .glossary
                .iter()
                .all(|term| !term.source.trim().is_empty() && !term.target.trim().is_empty()),
            "glossary terms need source and target"
        );
        let _guard = WRITE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::fs::create_dir_all(&self.directory)?;
        atomic_json(&path, series)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let _guard = WRITE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::fs::remove_file(self.path(id)?)?;
        Ok(())
    }

    pub fn import(&self, text: &str, replace: bool) -> Result<Series> {
        let mut series: Series = serde_json::from_str(text)?;
        if self.path(&series.id)?.exists() && !replace {
            series.id = Series::new(series.name.clone())?.id;
        }
        self.save(&series)?;
        Ok(series)
    }
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
