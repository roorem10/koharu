//! WO-06: notes on text boxes.

pub(crate) mod agent;

use anyhow::Context as _;
use koharu_desktop::Desktop;
use koharu_scene::{BoxNote, Commit, EntityId, Origin};
use tauri::State;

use super::{
    ChannelExt as _, Error,
    canvas::CanvasChannel,
    project::{CurrentProject, Project},
};

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_box_note(
    layer: EntityId,
    project: State<'_, CurrentProject>,
) -> Result<Option<String>, Error> {
    let projects = project.project.lock().await;
    let project = projects.as_ref().context("no project is open")?;
    Ok(project.box_note(layer)?)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn set_box_note(
    layer: EntityId,
    note: Option<String>,
    desktop: State<'_, Desktop>,
    project: State<'_, CurrentProject>,
    canvas_channel: State<'_, CanvasChannel>,
) -> Result<(), Error> {
    let (commit, page) = {
        let mut projects = project.project.lock().await;
        let project = projects.as_mut().context("no project is open")?;
        let commit = project.set_box_note(layer, note).await?;
        if let Some(commit) = &commit {
            project.record_commit(commit);
        }
        (commit, project.active_page())
    };
    if let Some(commit) = commit {
        desktop.synchronize(&commit.snapshot, page, &commit).await?;
        canvas_channel.channel.publish(desktop.canvas_state());
    }
    Ok(())
}

impl Project {
    fn box_note(&self, layer: EntityId) -> anyhow::Result<Option<String>> {
        let snapshot = self.snapshot();
        snapshot.text_layer(layer)?;
        Ok(snapshot.component::<BoxNote>(layer)?.map(|note| note.text))
    }

    async fn set_box_note(
        &mut self,
        layer: EntityId,
        note: Option<String>,
    ) -> anyhow::Result<Option<Commit>> {
        let snapshot = self.snapshot();
        snapshot.text_layer(layer)?;
        let note = note.filter(|note| !note.trim().is_empty());
        if snapshot.component::<BoxNote>(layer)?.map(|note| note.text) == note {
            return Ok(None);
        }
        let patch = snapshot.patch(|edit| {
            edit.promote_entity_to_user(layer)?;
            match note {
                Some(text) => edit.set(
                    layer,
                    &BoxNote {
                        origin: Origin::User,
                        text,
                    },
                ),
                None => edit.remove::<BoxNote>(layer),
            }
        })?;
        Ok(Some(self.commit(patch).await?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use koharu_scene::{At, PageDraft, TextLayout, TextLayoutKind};

    #[tokio::test]
    async fn box_notes_persist_and_undo_without_changing_chapter_notes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("notes.khrproj");
        let mut project = Project::create("notes".into(), path.clone()).await.unwrap();
        let mut layer = EntityId::new();
        let mut page = EntityId::new();
        let patch = project
            .snapshot()
            .patch(|edit| {
                page = edit.add_page(PageDraft::new("page", 100.0, 100.0), At::End)?;
                let content = edit.add_text_content(page, At::End)?;
                layer = edit.add_text_layer(
                    page,
                    At::End,
                    content,
                    &TextLayout {
                        origin: Origin::User,
                        kind: TextLayoutKind::Paragraph,
                        angle_degrees: None,
                        warp: None,
                    },
                )?;
                Ok(())
            })
            .unwrap();
        project.commit(patch).await.unwrap();
        let revision = project.revision();
        project
            .update_context(|context| {
                let line = context.lines.entry(layer).or_default();
                line.note = Some("Chapter reader note".into());
                line.qc_ok.push("checked".into());
                line.other.insert("future".into(), serde_json::json!(true));
            })
            .unwrap();
        assert_eq!(project.box_note(layer).unwrap(), None);
        let context = project.load_context().unwrap();
        let commit = project
            .set_box_note(layer, Some("Check this pun".into()))
            .await
            .unwrap()
            .unwrap();
        project.record_commit(&commit);
        drop(commit);
        assert_ne!(project.revision(), revision);
        assert_eq!(project.undo.len(), 1);
        assert_eq!(
            project.box_note(layer).unwrap().as_deref(),
            Some("Check this pun")
        );
        assert!(
            project
                .set_box_note(layer, Some("Check this pun".into()))
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(project.undo.len(), 1);
        project.undo().await.unwrap();
        assert_eq!(project.box_note(layer).unwrap(), None);
        project.redo().await.unwrap();
        assert_eq!(
            project.box_note(layer).unwrap().as_deref(),
            Some("Check this pun")
        );
        let commit = project
            .set_box_note(layer, Some("  \n  ".into()))
            .await
            .unwrap()
            .unwrap();
        project.record_commit(&commit);
        drop(commit);
        assert_eq!(project.undo.len(), 2);
        assert_eq!(project.box_note(layer).unwrap(), None);
        project.undo().await.unwrap();
        assert_eq!(
            project.box_note(layer).unwrap().as_deref(),
            Some("Check this pun")
        );
        assert_eq!(project.load_context().unwrap(), context);

        let revision = project.revision();
        assert!(
            project
                .set_box_note(EntityId::new(), Some("stale".into()))
                .await
                .is_err()
        );
        assert!(
            project
                .set_box_note(layer, Some("bad\0note".into()))
                .await
                .is_err()
        );
        assert!(
            project
                .snapshot()
                .patch(|edit| edit.set(
                    page,
                    &BoxNote {
                        origin: Origin::User,
                        text: "not a text layer".into(),
                    },
                ))
                .is_err()
        );
        assert_eq!(project.revision(), revision);
        drop(project);
        let project = Project::open("notes".into(), path).await.unwrap();
        assert_eq!(
            project.box_note(layer).unwrap().as_deref(),
            Some("Check this pun")
        );
        assert_eq!(project.load_context().unwrap(), context);
    }
}
