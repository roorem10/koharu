use std::collections::BTreeSet;

use anyhow::{Context as _, Result, ensure};
use booharu_context::{LineContext, ProjectContext};
use koharu_pipeline::TranslationContextRevision;
use koharu_scene::{Commit, EntityId, Revision};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::super::project::Project;

const JOURNAL: &str = "booharu_context_transaction";

#[derive(Deserialize, Serialize)]
struct PendingContext {
    before: ProjectContext,
    expected: Option<TranslationContextRevision>,
}

#[derive(Clone)]
struct FieldChange {
    path: Vec<String>,
    before: Option<Value>,
    after: Option<Value>,
}

pub(crate) struct ContextChanges {
    fields: Vec<FieldChange>,
    layers: BTreeSet<EntityId>,
    previous_marker: Option<TranslationContextRevision>,
}

impl ContextChanges {
    fn between(
        before: &ProjectContext,
        after: &ProjectContext,
        previous_marker: Option<TranslationContextRevision>,
    ) -> Result<Self> {
        let layers = before
            .lines
            .keys()
            .chain(after.lines.keys())
            .copied()
            .collect();
        let before = normalized(before, &layers)?;
        let after = normalized(after, &layers)?;
        let mut fields = Vec::new();
        diff(&mut Vec::new(), Some(&before), Some(&after), &mut fields);
        Ok(Self {
            fields,
            layers,
            previous_marker,
        })
    }

    fn reversed(&self, context: &ProjectContext) -> Result<ProjectContext> {
        let mut value = normalized(context, &self.layers)?;
        // Validate the entire inverse before touching the sidecar or scene.
        for field in &self.fields {
            let current = field
                .path
                .iter()
                .try_fold(&value, |value, key| value.get(key));
            ensure!(
                current == field.after.as_ref(),
                "context changed since this edit"
            );
        }
        for field in &self.fields {
            let (key, parents) = field.path.split_last().context("invalid context change")?;
            let mut parent = &mut value;
            for key in parents {
                parent = parent
                    .get_mut(key)
                    .context("context field no longer exists")?;
            }
            let object = parent
                .as_object_mut()
                .context("context field is not an object")?;
            match &field.before {
                Some(before) => {
                    object.insert(key.clone(), before.clone());
                }
                None => {
                    object.remove(key);
                }
            }
        }
        let mut result: ProjectContext = serde_json::from_value(value)?;
        result.lines.retain(|_, line| *line != LineContext::default());
        Ok(result)
    }
}

fn normalized(context: &ProjectContext, layers: &BTreeSet<EntityId>) -> Result<Value> {
    let mut context = context.clone();
    // Adding one field to a previously absent line must not make undo erase its
    // later box note, category or QC result.
    for layer in layers {
        context.lines.entry(*layer).or_default();
    }
    Ok(serde_json::to_value(context)?)
}

fn diff(
    path: &mut Vec<String>,
    before: Option<&Value>,
    after: Option<&Value>,
    fields: &mut Vec<FieldChange>,
) {
    if before == after {
        return;
    }
    if let (Some(Value::Object(before)), Some(Value::Object(after))) = (before, after) {
        let keys: BTreeSet<_> = before.keys().chain(after.keys()).collect();
        for key in keys {
            path.push(key.clone());
            diff(path, before.get(key), after.get(key), fields);
            path.pop();
        }
    } else {
        fields.push(FieldChange {
            path: path.clone(),
            before: before.cloned(),
            after: after.cloned(),
        });
    }
}

/// A staged sidecar is accepted only if its scene marker was published. This
/// also recovers interruption between the sidecar write and the scene commit.
pub(crate) fn load(project: &Project) -> Result<ProjectContext> {
    let Some(directory) = project.directory() else {
        return Ok(ProjectContext::default());
    };
    let marker = project
        .snapshot()
        .project_component::<TranslationContextRevision>()?;
    ProjectContext::update(directory, |stored| -> Result<ProjectContext> {
        let Some(pending) = stored.other.get(JOURNAL) else {
            return Ok(stored.clone());
        };
        let pending: PendingContext = serde_json::from_value(pending.clone())
            .context("invalid context transaction journal")?;
        if marker == pending.expected {
            stored.other.remove(JOURNAL);
        } else {
            *stored = pending.before;
        }
        Ok(stored.clone())
    })?
}

impl Project {
    fn stage_context(
        &self,
        before: &ProjectContext,
        after: &ProjectContext,
        expected: Option<TranslationContextRevision>,
    ) -> Result<()> {
        let directory = self.directory().context("project has no context folder")?;
        let mut staged = after.clone();
        staged.other.insert(
            JOURNAL.into(),
            serde_json::to_value(PendingContext {
                before: before.clone(),
                expected,
            })?,
        );
        ProjectContext::update(directory, |stored| -> Result<()> {
            ensure!(&*stored == before, "context changed before this edit");
            *stored = staged;
            Ok(())
        })??;
        Ok(())
    }

    pub(crate) async fn commit_context(
        &mut self,
        before: ProjectContext,
        after: ProjectContext,
    ) -> Result<Option<Commit>> {
        ensure!(
            self.load_context()? == before,
            "context changed before this edit"
        );
        ensure!(
            !after.other.contains_key(JOURNAL),
            "reserved context transaction field"
        );
        let snapshot = self.snapshot();
        let changes = ContextChanges::between(&before, &after, snapshot.project_component()?)?;
        if changes.fields.is_empty() {
            return Ok(None);
        }
        let marker = TranslationContextRevision {
            token: EntityId::new().to_string(),
        };
        let patch = snapshot.patch(|edit| edit.set_project(&marker))?;
        self.stage_context(&before, &after, Some(marker))?;
        let commit = match self.session.commit(patch).await {
            Ok(commit) => commit,
            Err(error) => {
                self.load_context()
                    .context("failed to recover context after a rejected edit")?;
                return Err(error.into());
            }
        };
        self.context_changes.insert(commit.revision, changes);
        if let Err(error) = self.load_context() {
            // The scene accepted this edit. Keep it undoable even if journal
            // cleanup needs to be retried by the next context read.
            tracing::warn!(%error, "context journal cleanup deferred");
        }
        Ok(Some(commit))
    }

    pub(crate) async fn undo_context_revisions(&mut self, revisions: &[Revision]) -> Result<Commit> {
        let mut revisions = revisions.to_vec();
        revisions.sort_unstable_by(|left, right| right.cmp(left));
        revisions.dedup();
        if !revisions
            .iter()
            .any(|revision| self.context_changes.contains_key(revision))
        {
            return Ok(self.session.undo_many(revisions).await?);
        }
        let before = self.load_context()?;
        let mut after = before.clone();
        let marker = self
            .snapshot()
            .project_component::<TranslationContextRevision>()?;
        let mut expected = marker.clone();
        for revision in &revisions {
            if let Some(changes) = self.context_changes.get(revision) {
                after = changes.reversed(&after)?;
                expected = changes.previous_marker.clone();
            }
        }
        let inverse = ContextChanges::between(&before, &after, marker)?;
        self.stage_context(&before, &after, expected)?;
        let commit = match self.session.undo_many(revisions).await {
            Ok(commit) => commit,
            Err(error) => {
                self.load_context()
                    .context("failed to recover context after a rejected undo")?;
                return Err(error.into());
            }
        };
        self.context_changes.insert(commit.revision, inverse);
        if let Err(error) = self.load_context() {
            tracing::warn!(%error, "context journal cleanup deferred");
        }
        Ok(commit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn context_edits_are_one_undo_step_and_preserve_other_line_fields() {
        let directory = tempfile::tempdir().unwrap();
        let mut project = Project::create("chapter".into(), directory.path().join("chapter.khrproj"))
            .await
            .unwrap();
        let layer = EntityId::new();
        let before = project.load_context().unwrap();
        let mut after = before.clone();
        after.set_user_speaker(layer, Some("mika".into()));
        after.lines.entry(layer).or_default().box_note = Some("hand note".into());
        let revision = project.revision();
        let commit = project
            .commit_context(before, after)
            .await
            .unwrap()
            .unwrap();
        project.record_commit(&commit);
        assert!(project.revision() > revision);
        assert_eq!(project.undo.len(), 1);
        project
            .update_context(|context| {
                let line = context.lines.entry(layer).or_default();
                line.note = Some("reader note".into());
                line.qc_ok.push("checked".into());
                line.other.insert("future".into(), Value::Bool(true));
            })
            .unwrap();
        project.undo().await.unwrap();
        let undone = project.load_context().unwrap();
        assert_eq!(undone.speaker(layer), None);
        assert_eq!(undone.lines[&layer].box_note, None);
        assert_eq!(undone.lines[&layer].note.as_deref(), Some("reader note"));
        assert_eq!(undone.lines[&layer].qc_ok, ["checked"]);
        assert_eq!(undone.lines[&layer].other["future"], true);
        project.redo().await.unwrap();
        let redone = project.load_context().unwrap();
        assert_eq!(redone.speaker(layer).unwrap().value.as_deref(), Some("mika"));
        assert_eq!(redone.lines[&layer].box_note.as_deref(), Some("hand note"));
        assert_eq!(redone.lines[&layer].note, undone.lines[&layer].note);
        assert_eq!(project.undo.len(), 1);
    }

    #[tokio::test]
    async fn grouped_context_edits_undo_and_redo_in_revision_order() {
        let directory = tempfile::tempdir().unwrap();
        let mut project = Project::create("chapter".into(), directory.path().join("chapter.khrproj"))
            .await
            .unwrap();
        let layer = EntityId::new();
        let mut revisions = Vec::new();
        for speaker in ["mika", "aki"] {
            let before = project.load_context().unwrap();
            let mut after = before.clone();
            after.set_user_speaker(layer, Some(speaker.into()));
            let commit = project
                .commit_context(before, after)
                .await
                .unwrap()
                .unwrap();
            revisions.push(commit.revision);
        }
        project.record(revisions);
        project.undo().await.unwrap();
        assert_eq!(project.load_context().unwrap().speaker(layer), None);
        project.redo().await.unwrap();
        assert_eq!(
            project
                .load_context()
                .unwrap()
                .speaker(layer)
                .unwrap()
                .value
                .as_deref(),
            Some("aki")
        );
    }

    #[tokio::test]
    async fn stale_context_edits_and_stale_undo_fail_without_mutation() {
        let directory = tempfile::tempdir().unwrap();
        let mut project = Project::create("chapter".into(), directory.path().join("chapter.khrproj"))
            .await
            .unwrap();
        let layer = EntityId::new();
        let before = project.load_context().unwrap();
        assert!(
            project
                .commit_context(before.clone(), before.clone())
                .await
                .unwrap()
                .is_none()
        );
        let revision = project.revision();
        project
            .update_context(|context| context.series = Some("series".into()))
            .unwrap();
        let mut after = before.clone();
        after.set_user_speaker(layer, Some("mika".into()));
        assert!(project.commit_context(before, after).await.is_err());
        assert_eq!(project.revision(), revision);

        let before = project.load_context().unwrap();
        let mut after = before.clone();
        after.lines.entry(layer).or_default().box_note = Some("first".into());
        let commit = project
            .commit_context(before, after)
            .await
            .unwrap()
            .unwrap();
        project.record_commit(&commit);
        project
            .update_context(|context| {
                context.lines.entry(layer).or_default().box_note = Some("later".into());
            })
            .unwrap();
        let before_undo = project.load_context().unwrap();
        assert!(project.undo().await.is_err());
        assert_eq!(project.load_context().unwrap(), before_undo);
        assert_eq!(project.revision(), commit.revision);
        assert_eq!(project.undo.len(), 1);
    }

    #[tokio::test]
    async fn recovery_uses_the_published_scene_marker() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("chapter.khrproj");
        let project = Project::create("chapter".into(), path.clone())
            .await
            .unwrap();
        let layer = EntityId::new();
        let before = project.load_context().unwrap();
        let mut after = before.clone();
        after.set_user_speaker(layer, Some("mika".into()));
        let marker = TranslationContextRevision {
            token: EntityId::new().to_string(),
        };
        project
            .stage_context(&before, &after, Some(marker.clone()))
            .unwrap();
        drop(project);
        let mut project = Project::open("chapter".into(), path.clone())
            .await
            .unwrap();
        assert_eq!(project.load_context().unwrap(), before);

        project
            .stage_context(&before, &after, Some(marker.clone()))
            .unwrap();
        let patch = project
            .snapshot()
            .patch(|edit| edit.set_project(&marker))
            .unwrap();
        project.session.commit(patch).await.unwrap();
        drop(project);
        let project = Project::open("chapter".into(), path).await.unwrap();
        assert_eq!(project.load_context().unwrap(), after);
        assert!(
            !ProjectContext::load(project.directory().unwrap())
                .unwrap()
                .other
                .contains_key(JOURNAL)
        );
    }
}
