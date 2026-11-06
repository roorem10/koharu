use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Mutex,
};

use anyhow::{Context as _, Result};
use koharu_scene::EntityId;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use specta::Type;

use crate::{ChapterBrief, Flag, GlossaryEntry, LetteringCategory};

/// File name of the sidecar inside a project directory.
pub const CONTEXT_FILE: &str = "booharu-context.json";
/// Written into new sidecars. Readers accept any version and keep unknown fields.
pub const CONTEXT_VERSION: u32 = 1;

/// Serializes every read-modify-write of a sidecar in this process.
static UPDATE: Mutex<()> = Mutex::new(());

/// Everything Booharu keeps about one project outside its save file.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, Type)]
#[serde(default)]
pub struct ProjectContext {
    pub version: u32,
    pub series: Option<String>,
    pub brief: Option<ChapterBrief>,
    pub suggestions: Vec<GlossaryEntry>,
    /// Keyed by text layer id, the id the Inspector selects.
    pub lines: BTreeMap<EntityId, LineContext>,
    /// Fields this build doesn't know, kept so saving never erases them.
    #[serde(flatten)]
    #[specta(skip)]
    pub other: Map<String, Value>,
}

impl Default for ProjectContext {
    fn default() -> Self {
        Self {
            version: CONTEXT_VERSION,
            series: None,
            brief: None,
            suggestions: Vec::new(),
            lines: BTreeMap::new(),
            other: Map::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, Type)]
#[serde(default)]
pub struct LineContext {
    pub category: Option<LineCategory>,
    pub note: Option<String>,
    pub flags: Vec<Flag>,
    pub dismissed: Vec<String>,
    /// QC checks (WO-12) the user marked OK for this line, by check id.
    pub qc_ok: Vec<String>,
    /// The user's own note on the box (WO-06). `note` is the chapter reader's.
    pub box_note: Option<String>,
    #[serde(flatten)]
    #[specta(skip)]
    pub other: Map<String, Value>,
}

impl LineContext {
    fn is_empty(&self) -> bool {
        self.category.is_none()
            && self.note.is_none()
            && self.flags.is_empty()
            && self.dismissed.is_empty()
            && self.qc_ok.is_empty()
            && self.box_note.is_none()
            && self.other.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize, Type)]
pub struct LineCategory {
    pub value: LetteringCategory,
    pub source: LabelSource,
    /// 0 to 1 when the model reported it; `None` for hand-set labels.
    pub confidence: Option<f32>,
}

/// Who set a label. A `User` label is never replaced by a model label.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LabelSource {
    Model,
    User,
}

impl ProjectContext {
    /// Reads the sidecar in `project_dir`. A missing file is an empty context;
    /// a damaged one is an error, so it is never silently overwritten.
    pub fn load(project_dir: &Path) -> Result<Self> {
        let path = project_dir.join(CONTEXT_FILE);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => {
                return Err(error).with_context(|| format!("failed to read {}", path.display()));
            }
        };
        serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
    }

    /// Writes the sidecar through a temporary file, so a crash keeps the old one.
    pub fn save(&self, project_dir: &Path) -> Result<()> {
        let path = project_dir.join(CONTEXT_FILE);
        let temporary = project_dir.join(format!("{CONTEXT_FILE}.tmp"));
        let text = serde_json::to_string_pretty(self).context("failed to encode the context")?;
        std::fs::write(&temporary, text)
            .with_context(|| format!("failed to write {}", temporary.display()))?;
        std::fs::rename(&temporary, &path)
            .with_context(|| format!("failed to replace {}", path.display()))
    }

    /// Loads, changes and saves the sidecar as one step. Use this for every
    /// write, so two features editing the same file can't lose each other's change.
    pub fn update<R>(project_dir: &Path, change: impl FnOnce(&mut Self) -> R) -> Result<R> {
        let _guard = UPDATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut context = Self::load(project_dir)?;
        let before = context.clone();
        let result = change(&mut context);
        if context != before {
            context.save(project_dir)?;
        }
        Ok(result)
    }

    #[must_use]
    pub fn category(&self, layer: EntityId) -> Option<LineCategory> {
        self.lines.get(&layer).and_then(|line| line.category)
    }

    /// Sets a model label unless the user already chose one. Returns whether it changed.
    pub fn set_model_category(
        &mut self,
        layer: EntityId,
        value: LetteringCategory,
        confidence: Option<f32>,
    ) -> bool {
        let line = self.lines.entry(layer).or_default();
        if line
            .category
            .is_some_and(|current| current.source == LabelSource::User)
        {
            return false;
        }
        let next = Some(LineCategory {
            value,
            source: LabelSource::Model,
            confidence: confidence.map(|value| value.clamp(0.0, 1.0)),
        });
        let changed = line.category != next;
        line.category = next;
        changed
    }

    /// Sets or clears a hand-picked label. Clearing hands the line back to the
    /// automatic rules. Returns whether it changed.
    pub fn set_user_category(&mut self, layer: EntityId, value: Option<LetteringCategory>) -> bool {
        let next = value.map(|value| LineCategory {
            value,
            source: LabelSource::User,
            confidence: None,
        });
        let line = self.lines.entry(layer).or_default();
        let changed = line.category != next;
        line.category = next;
        if line.is_empty() {
            self.lines.remove(&layer);
        }
        changed
    }

    /// Drops entries for layers that no longer exist, and empty entries.
    pub fn retain_layers(&mut self, layers: &BTreeSet<EntityId>) {
        self.lines
            .retain(|layer, line| layers.contains(layer) && !line.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_empty_and_saves_roundtrip() {
        let directory = tempfile::tempdir().unwrap();
        let context = ProjectContext::load(directory.path()).unwrap();
        assert_eq!(context, ProjectContext::default());

        let layer = EntityId::new();
        let mut context = context;
        context.set_model_category(layer, LetteringCategory::Whisper, Some(0.8));
        context.save(directory.path()).unwrap();
        let restored = ProjectContext::load(directory.path()).unwrap();
        assert_eq!(restored, context);
        assert!(
            !directory
                .path()
                .join(format!("{CONTEXT_FILE}.tmp"))
                .exists()
        );
    }

    #[test]
    fn damaged_file_is_an_error() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join(CONTEXT_FILE), "{ not json").unwrap();
        assert!(ProjectContext::load(directory.path()).is_err());
    }

    #[test]
    fn user_labels_win_over_model_labels() {
        let layer = EntityId::new();
        let mut context = ProjectContext::default();
        assert!(context.set_model_category(layer, LetteringCategory::Shout, Some(2.0)));
        assert_eq!(context.category(layer).unwrap().confidence, Some(1.0));
        assert!(context.set_user_category(layer, Some(LetteringCategory::Thoughts)));
        assert!(!context.set_model_category(layer, LetteringCategory::Shout, None));
        assert_eq!(
            context.category(layer).unwrap(),
            LineCategory {
                value: LetteringCategory::Thoughts,
                source: LabelSource::User,
                confidence: None,
            }
        );
        assert!(context.set_user_category(layer, None));
        assert!(context.lines.is_empty());
        assert!(context.set_model_category(layer, LetteringCategory::Shout, None));
    }

    #[test]
    fn unknown_fields_survive_a_save() {
        let directory = tempfile::tempdir().unwrap();
        let layer = EntityId::new();
        std::fs::write(
            directory.path().join(CONTEXT_FILE),
            format!(
                r#"{{"version":7,"brief":{{"summary":"kept"}},"lines":{{"{layer}":{{"note":"pun","category":null}}}}}}"#
            ),
        )
        .unwrap();
        ProjectContext::update(directory.path(), |context| {
            context.set_model_category(layer, LetteringCategory::Title, None);
        })
        .unwrap();
        let value: Value = serde_json::from_str(
            &std::fs::read_to_string(directory.path().join(CONTEXT_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(value["version"], 7);
        assert_eq!(value["brief"]["summary"], "kept");
        assert_eq!(value["lines"][layer.to_string()]["note"], "pun");
        assert_eq!(
            value["lines"][layer.to_string()]["category"]["value"],
            "title"
        );
    }

    #[test]
    fn retain_layers_drops_deleted_and_empty_entries() {
        let kept = EntityId::new();
        let deleted = EntityId::new();
        let mut context = ProjectContext::default();
        context.set_model_category(kept, LetteringCategory::Sfx, None);
        context.set_model_category(deleted, LetteringCategory::Sfx, None);
        context
            .lines
            .insert(EntityId::new(), LineContext::default());
        context.retain_layers(&BTreeSet::from([kept]));
        assert_eq!(context.lines.keys().copied().collect::<Vec<_>>(), [kept]);
    }

    #[test]
    fn update_skips_the_write_when_nothing_changed() {
        let directory = tempfile::tempdir().unwrap();
        ProjectContext::update(directory.path(), |_| ()).unwrap();
        assert!(!directory.path().join(CONTEXT_FILE).exists());
    }
}
