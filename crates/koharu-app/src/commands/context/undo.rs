//! Chapter-reading sidecar changes follow the same scene undo entry as their labels.

use std::collections::BTreeSet;

use anyhow::{Result, ensure};
use booharu_context::ProjectContext;
use koharu_scene::{Commit, Component, EntityId, Revision};
use revision::revisioned;

use super::super::project::Project;

// A read without changed typography still needs a real revision and inverse.
// Scene's open component registry stores this without a container schema change.
#[revisioned(revision = 1)]
#[derive(Clone, Debug)]
struct ChapterRead {
    token: String,
}

impl Component for ChapterRead {
    const KIND: &'static str = "dev.booharu.chapter.read";
}

#[derive(Clone)]
pub(crate) struct ContextChange {
    before: ProjectContext,
    after: ProjectContext,
}

impl ContextChange {
    fn reverse(&self, current: &mut ProjectContext) -> Result<()> {
        // Compare only fields the reader changed. Later flags, personal notes and
        // recipe bookkeeping must survive undo; later hand edits must conflict.
        if self.before.brief != self.after.brief {
            ensure!(
                current.brief == self.after.brief,
                "chapter brief changed after reading"
            );
            current.brief = self.before.brief.clone();
        }
        if self.before.suggestions != self.after.suggestions {
            ensure!(
                current.suggestions == self.after.suggestions,
                "chapter terms changed after reading"
            );
            current.suggestions = self.before.suggestions.clone();
        }
        let ids: BTreeSet<_> = self
            .before
            .lines
            .keys()
            .chain(self.after.lines.keys())
            .copied()
            .collect();
        for id in ids {
            let before = self.before.lines.get(&id);
            let after = self.after.lines.get(&id);
            let old_category = before.and_then(|line| line.category);
            let new_category = after.and_then(|line| line.category);
            let old_note = before.and_then(|line| line.note.as_ref());
            let new_note = after.and_then(|line| line.note.as_ref());
            if old_category != new_category {
                ensure!(
                    current.lines.get(&id).and_then(|line| line.category) == new_category,
                    "lettering category changed after reading"
                );
                current.lines.entry(id).or_default().category = old_category;
            }
            if old_note != new_note {
                ensure!(
                    current.lines.get(&id).and_then(|line| line.note.as_ref()) == new_note,
                    "chapter line note changed after reading"
                );
                current.lines.entry(id).or_default().note = old_note.cloned();
            }
        }
        let key = "chapter_read_model";
        if self.before.other.get(key) != self.after.other.get(key) {
            ensure!(
                current.other.get(key) == self.after.other.get(key),
                "chapter reading model changed"
            );
            match self.before.other.get(key) {
                Some(value) => {
                    current.other.insert(key.into(), value.clone());
                }
                None => {
                    current.other.remove(key);
                }
            }
        }
        Ok(())
    }
}

impl Project {
    pub(crate) async fn commit_chapter_read(
        &mut self,
        before: ProjectContext,
        after: ProjectContext,
        changed: &[EntityId],
    ) -> Result<Commit> {
        ensure!(
            self.load_context()? == before,
            "chapter context changed during reading"
        );
        self.update_context(|context| *context = after.clone())?;
        let result = async {
            if let Some(commit) = super::super::lettering::labels_changed(self, changed).await?
            {
                return Ok(commit);
            }
            let patch = self.snapshot().patch(|edit| {
                edit.set_project(&ChapterRead {
                    token: EntityId::new().to_string(),
                })
            })?;
            self.commit(patch).await
        }
        .await;
        let commit = match result {
            Ok(commit) => commit,
            Err(error) => {
                self.update_context(|context| *context = before)?;
                return Err(error);
            }
        };
        self.context_undo
            .insert(commit.revision, ContextChange { before, after });
        self.record_commit(&commit);
        Ok(commit)
    }

    pub(crate) async fn undo_with_context(&mut self, revisions: &[Revision]) -> Result<Commit> {
        let mut ordered = revisions.to_vec();
        ordered.sort_unstable_by(|left, right| right.cmp(left));
        ordered.dedup();
        let has_context = ordered
            .iter()
            .any(|revision| self.context_undo.contains_key(revision));
        if !has_context {
            return Ok(self.session.undo_many(ordered).await?);
        }
        let before = self.load_context()?;
        let mut after = before.clone();
        for revision in &ordered {
            if let Some(change) = self.context_undo.get(revision) {
                change.reverse(&mut after)?;
            }
        }
        self.update_context(|context| *context = after.clone())?;
        let commit = match self.session.undo_many(ordered).await {
            Ok(commit) => commit,
            Err(error) => {
                self.update_context(|context| *context = before)?;
                return Err(error.into());
            }
        };
        self.context_undo
            .insert(commit.revision, ContextChange { before, after });
        Ok(commit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use booharu_context::{ChapterBrief, LetteringCategory};
    use serde_json::json;

    #[tokio::test]
    async fn brief_only_read_is_revisioned_and_undo_redo_preserve_unrelated_sidecar_fields() {
        let directory = tempfile::tempdir().unwrap();
        let mut project =
            Project::create("chapter".into(), directory.path().join("chapter.khrproj"))
                .await
                .unwrap();
        let before = project.load_context().unwrap();
        let mut after = before.clone();
        after.brief = Some(ChapterBrief {
            summary: "read result".into(),
            read_at: Some("1".into()),
            ..Default::default()
        });
        let layer = EntityId::new();
        after.set_model_category(layer, LetteringCategory::Shout, Some(0.9));
        let base = project.revision();
        project.commit_chapter_read(before, after, &[]).await.unwrap();
        assert_ne!(project.revision(), base);
        assert_eq!(project.undo.len(), 1);
        project
            .update_context(|context| {
                context.other.insert("auto_run_inputs".into(), json!({"a":1}));
                context.lines.entry(layer).or_default().box_note = Some("my note".into());
            })
            .unwrap();
        project.undo().await.unwrap();
        let undone = project.load_context().unwrap();
        assert!(undone.brief.is_none());
        assert!(undone.category(layer).is_none());
        assert_eq!(undone.lines[&layer].box_note.as_deref(), Some("my note"));
        assert!(undone.other.contains_key("auto_run_inputs"));
        project.redo().await.unwrap();
        assert_eq!(
            project.load_context().unwrap().brief.unwrap().summary,
            "read result"
        );
        assert_eq!(
            project.load_context().unwrap().category(layer).unwrap().value,
            LetteringCategory::Shout
        );
    }

    #[tokio::test]
    async fn undo_refuses_to_overwrite_a_later_hand_category() {
        let directory = tempfile::tempdir().unwrap();
        let mut project =
            Project::create("chapter".into(), directory.path().join("chapter.khrproj"))
                .await
                .unwrap();
        let before = project.load_context().unwrap();
        let mut after = before.clone();
        let layer = EntityId::new();
        after.set_model_category(layer, LetteringCategory::Shout, Some(0.9));
        project.commit_chapter_read(before, after, &[]).await.unwrap();
        project
            .update_context(|context| {
                context.set_user_category(layer, Some(LetteringCategory::Whisper))
            })
            .unwrap();
        let revision = project.revision();
        assert!(project.undo().await.is_err());
        assert_eq!(project.revision(), revision);
        assert_eq!(project.undo.len(), 1);
        assert_eq!(
            project.load_context().unwrap().category(layer).unwrap().value,
            LetteringCategory::Whisper
        );
    }
}
