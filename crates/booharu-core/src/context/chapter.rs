//! A chapter's own data, in a side file beside its project: `Ch.1.khr` keeps
//! it in `Ch.1.booharu.json`, as a project has no place of its own for it.
//! It holds what belongs to the chapter as a whole: the series it links, the
//! brief Read chapter writes, and the terms suggested for it. Per-line facts
//! are layer attributes inside the project, never kept here.
//!
//! A missing file reads as an empty chapter. Every write goes through
//! [`ChapterContext::update`], one read and write under a lock for the
//! process, atomic, so two writers can't lose each other's change; a file
//! that doesn't read is an error, never replaced. Keys this Booharu doesn't
//! know are kept, so a later Booharu's survive its writes.

use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{
    GlossaryEntry, same_source,
    series::{Series, SeriesStore, write},
};

/// The version a new side file is written with. A reader takes any, and
/// keeps it.
pub const CHAPTER_VERSION: u32 = 1;

/// Holds every read and write of a side file in this process.
static UPDATE: Mutex<()> = Mutex::new(());

/// What a chapter keeps beside its project.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct ChapterContext {
    pub version: u32,
    /// The id of the series the chapter follows. A series whose file is gone
    /// reads as none.
    pub series: Option<String>,
    /// What Read chapter found the chapter to be about.
    pub brief: Option<ChapterBrief>,
    /// Terms suggested for the chapter, by Read chapter or the agent, which
    /// its translation follows as the series' terms until accepted into the
    /// series or dismissed.
    pub suggestions: Vec<GlossaryEntry>,
    /// The keys this Booharu doesn't know, kept as they were.
    #[serde(flatten)]
    #[specta(skip)]
    pub other: Map<String, Value>,
}

impl Default for ChapterContext {
    fn default() -> Self {
        Self {
            version: CHAPTER_VERSION,
            series: None,
            brief: None,
            suggestions: Vec::new(),
            other: Map::new(),
        }
    }
}

/// The chapter in short, as Read chapter writes it: what happens, and who
/// is in it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct ChapterBrief {
    pub summary: String,
    pub characters: Vec<ChapterCharacter>,
    /// Whether the user wrote the summary, which reading the chapter again
    /// keeps.
    pub edited: bool,
    /// When the chapter was read, as an RFC 3339 time.
    pub read_at: Option<String>,
}

/// A character in the chapter, and the pages they're on, counted from 1.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct ChapterCharacter {
    pub name: String,
    pub original: String,
    pub pages: Vec<u32>,
}

/// The side file of the project file `project`: `Ch.1.khr` gives
/// `Ch.1.booharu.json`.
pub fn side_file(project: &Path) -> PathBuf {
    project.with_extension("booharu.json")
}

/// The side file the project's conversation with the agent is kept in:
/// `Ch.1.khr` gives `Ch.1.booharu-agent.json`.
pub fn agent_file(project: &Path) -> PathBuf {
    project.with_extension("booharu-agent.json")
}

impl ChapterContext {
    /// The chapter of the project file `project`, as its side file holds it
    /// now; empty when it has none. A file that doesn't read fails, naming
    /// it.
    pub fn load(project: &Path) -> Result<Self> {
        let path = side_file(project);
        let text = match fs_err::read(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(error.into()),
        };
        serde_json::from_slice(&text)
            .with_context(|| format!("Booharu's chapter file {} can't be read", path.display()))
    }

    /// Runs `change` on the chapter of the project file `project` as its side
    /// file holds it now and writes what it changed, in one read and write
    /// under the lock, atomically; a change that changes nothing writes
    /// nothing. A file that doesn't read fails, and stays as it is.
    pub fn update<R>(project: &Path, change: impl FnOnce(&mut Self) -> R) -> Result<R> {
        let _held = UPDATE.lock().unwrap_or_else(PoisonError::into_inner);
        let mut chapter = Self::load(project)?;
        let before = chapter.clone();
        let changed = change(&mut chapter);
        if chapter != before {
            write(&side_file(project), &chapter)?;
        }
        Ok(changed)
    }

    /// The suggestion `source` names, the first when it's there twice.
    pub fn suggestion(&self, source: &str) -> Option<&GlossaryEntry> {
        self.suggestions
            .iter()
            .find(|term| same_source(&term.source, source))
    }
}

/// The side files of a project being deleted, which go after it: made just
/// before the project's file is removed, it removes them as it's dropped,
/// once that file is gone, and leaves them while it's still there, as when
/// its removal failed, so a project that stays keeps its chapter's data. A
/// side file it can't remove then is logged, as the project is gone.
#[must_use = "the side files go as it's dropped"]
pub struct SideFiles(PathBuf);

impl SideFiles {
    /// The side files of the project file `project`, which go once it's
    /// gone.
    pub fn of(project: &Path) -> Self {
        Self(project.to_owned())
    }
}

impl Drop for SideFiles {
    fn drop(&mut self) {
        // Only once the project is surely gone: a file that can't be looked
        // at may still be there.
        if !matches!(self.0.try_exists(), Ok(false)) {
            return;
        }
        if let Err(error) = remove_side_files(&self.0) {
            tracing::warn!(
                "the side files of {}, deleted, stay: {error:#}",
                self.0.display()
            );
        }
    }
}

/// Removes the side files of the project file `project`, the chapter's and
/// the agent's, as the project goes; one it doesn't have is no error.
pub fn remove_side_files(project: &Path) -> Result<()> {
    let _held = UPDATE.lock().unwrap_or_else(PoisonError::into_inner);
    for path in [side_file(project), agent_file(project)] {
        match fs_err::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

/// A chapter as a translation reads it: its side file, and the series it
/// links.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chapter {
    pub context: ChapterContext,
    /// The series the chapter links; none when it links none, or the series'
    /// file is gone.
    pub series: Option<Series>,
    /// Whether the chapter links a series whose file is gone.
    pub missing: bool,
}

impl Chapter {
    /// The chapter of the project file `project`, with the series it links
    /// from the store `store` opens, which only a linked series needs. A side
    /// file or a series file that doesn't read fails, naming the file; a
    /// series file that's gone reads as no series.
    pub fn read(project: &Path, store: impl FnOnce() -> Result<SeriesStore>) -> Result<Self> {
        let context = ChapterContext::load(project)?;
        let Some(id) = context.series.clone() else {
            return Ok(Self {
                context,
                series: None,
                missing: false,
            });
        };
        let series = store()?.find(&id).with_context(|| {
            format!(
                "Booharu's chapter file {} links the series \"{id}\", which can't be read",
                side_file(project).display()
            )
        })?;
        Ok(Self {
            missing: series.is_none(),
            context,
            series,
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn project() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("a folder");
        let project = dir.path().join("Ch.1.khr");
        (dir, project)
    }

    #[test]
    fn the_side_files_sit_beside_the_project() {
        let project = Path::new("projects").join("Ch.1.khr");
        assert_eq!(
            side_file(&project),
            Path::new("projects").join("Ch.1.booharu.json")
        );
        assert_eq!(
            agent_file(&project),
            Path::new("projects").join("Ch.1.booharu-agent.json")
        );
    }

    #[test]
    fn a_missing_side_file_is_empty_and_a_broken_one_an_error() -> Result<()> {
        let (_dir, project) = project();
        assert_eq!(ChapterContext::load(&project)?, ChapterContext::default());
        assert_eq!(ChapterContext::load(&project)?.version, CHAPTER_VERSION);
        // A change that changes nothing writes nothing.
        ChapterContext::update(&project, |_| ())?;
        assert!(!side_file(&project).exists());

        fs_err::write(side_file(&project), "{ not json")?;
        let error = ChapterContext::load(&project).expect_err("a broken file");
        assert!(
            format!("{error:#}").contains("Ch.1.booharu.json"),
            "{error:#}"
        );
        let refused = ChapterContext::update(&project, |chapter| {
            chapter.series = Some("blue-archive-000000000000".to_owned());
        });
        assert!(refused.is_err());
        assert_eq!(
            fs_err::read_to_string(side_file(&project))?,
            "{ not json",
            "never replaced"
        );
        Ok(())
    }

    #[test]
    fn a_side_file_keeps_unknown_keys_through_an_update() -> Result<()> {
        let (_dir, project) = project();
        fs_err::write(
            side_file(&project),
            json!({
                "version": 3,
                "brief": {"summary": "Hina naps.", "characters": [{"name": "Hina", "pages": [2]}]},
                "read_model": "openai/gpt-5",
                "lines": {"kept": {"note": "a later Booharu's"}},
            })
            .to_string(),
        )?;
        let changed = ChapterContext::update(&project, |chapter| {
            chapter.series = Some("blue-archive-000000000000".to_owned());
            chapter.suggestions.push(GlossaryEntry::new("ヒナ", "Hina"));
            chapter.brief.as_ref().map(|brief| brief.summary.clone())
        })?;
        assert_eq!(changed.as_deref(), Some("Hina naps."));
        let written: Value = serde_json::from_str(&fs_err::read_to_string(side_file(&project))?)?;
        assert_eq!(written["version"], 3);
        assert_eq!(written["series"], "blue-archive-000000000000");
        assert_eq!(written["read_model"], "openai/gpt-5");
        assert_eq!(written["lines"]["kept"]["note"], "a later Booharu's");
        assert_eq!(written["brief"]["characters"][0]["pages"], json!([2]));
        assert_eq!(written["suggestions"][0]["target"], "Hina");
        let read = ChapterContext::load(&project)?;
        assert_eq!(
            read.suggestion(" ヒ ナ").map(|term| &*term.target),
            Some("Hina")
        );
        Ok(())
    }

    #[test]
    fn a_linked_series_that_is_gone_reads_as_none() -> Result<()> {
        let (dir, project) = project();
        let store = SeriesStore::new(dir.path().join("series"));
        let series = store.create("Blue Archive")?;
        let opened = || Ok(store.clone());
        assert_eq!(Chapter::read(&project, opened)?, Chapter::default());

        ChapterContext::update(&project, |chapter| chapter.series = Some(series.id.clone()))?;
        let read = Chapter::read(&project, opened)?;
        assert_eq!((read.series.as_ref(), read.missing), (Some(&series), false));

        store.delete(&series.id)?;
        let read = Chapter::read(&project, opened)?;
        assert_eq!((read.series, read.missing), (None, true));

        ChapterContext::update(&project, |chapter| chapter.series = Some("../x".to_owned()))?;
        let error = Chapter::read(&project, opened).expect_err("an unsafe id");
        assert!(
            format!("{error:#}").contains("Ch.1.booharu.json"),
            "{error:#}"
        );
        // A chapter linking nothing never opens the store.
        ChapterContext::update(&project, |chapter| chapter.series = None)?;
        let read = Chapter::read(&project, || anyhow::bail!("no store"))?;
        assert_eq!(read.series, None);
        Ok(())
    }

    #[test]
    fn the_side_files_go_only_once_the_project_is_gone() -> Result<()> {
        let (_dir, project) = project();
        fs_err::write(&project, "a project")?;
        fs_err::write(side_file(&project), "{}")?;
        fs_err::write(agent_file(&project), "{}")?;
        // The project stays, as when its removal failed: so do they.
        drop(SideFiles::of(&project));
        assert!(side_file(&project).exists() && agent_file(&project).exists());
        // Gone, they go after it.
        let sides = SideFiles::of(&project);
        fs_err::remove_file(&project)?;
        drop(sides);
        assert!(!side_file(&project).exists() && !agent_file(&project).exists());
        Ok(())
    }

    #[test]
    fn deleting_a_project_removes_both_side_files() -> Result<()> {
        let (_dir, project) = project();
        remove_side_files(&project)?;
        fs_err::write(side_file(&project), "{}")?;
        fs_err::write(agent_file(&project), "{}")?;
        remove_side_files(&project)?;
        assert!(!side_file(&project).exists() && !agent_file(&project).exists());
        Ok(())
    }
}
