//! WO-06: notes on text boxes.

use anyhow::Context as _;
use koharu_desktop::Desktop;
use koharu_scene::{Commit, EntityId};
use schemars::JsonSchema;
use serde::Deserialize;
use tauri::State;

use super::{
    ChannelExt as _, Error,
    canvas::CanvasChannel,
    project::{CurrentProject, Project},
};

#[derive(Deserialize, JsonSchema)]
pub(crate) struct SetBoxNote {
    pub layer: String,
    pub note: Option<String>,
}

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
    pub(crate) fn box_note(&self, layer: EntityId) -> anyhow::Result<Option<String>> {
        self.snapshot().text_layer(layer)?;
        Ok(self
            .load_context()?
            .lines
            .get(&layer)
            .and_then(|line| line.box_note.clone()))
    }

    pub(crate) async fn set_box_note(
        &mut self,
        layer: EntityId,
        note: Option<String>,
    ) -> anyhow::Result<Option<Commit>> {
        self.snapshot().text_layer(layer)?;
        let before = self.load_context()?;
        let mut after = before.clone();
        after.lines.entry(layer).or_default().box_note =
            note.filter(|note| !note.trim().is_empty());
        self.commit_context(before, after).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use koharu_scene::{At, Origin, PageDraft, Session, TextLayout, TextLayoutKind};

    #[tokio::test]
    async fn box_notes_are_revisioned_and_undo_without_changing_reader_notes() {
        let directory = tempfile::tempdir().unwrap();
        let mut session = Session::memory().await.unwrap();
        let mut layer = EntityId::new();
        let patch = session
            .snapshot()
            .patch(|edit| {
                let page = edit.add_page(PageDraft::new("page", 100.0, 100.0), At::End)?;
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
        session.commit(patch).await.unwrap();
        let mut project = Project::new(session, "notes".into());
        project.directory = Some(directory.path().to_owned());
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
        let commit = project
            .set_box_note(layer, Some("Check this pun".into()))
            .await
            .unwrap()
            .unwrap();
        project.record_commit(&commit);
        assert_eq!(
            project.box_note(layer).unwrap().as_deref(),
            Some("Check this pun")
        );
        let saved = booharu_context::ProjectContext::load(directory.path()).unwrap();
        assert_eq!(
            saved.lines[&layer].box_note.as_deref(),
            Some("Check this pun")
        );
        assert_ne!(project.revision(), revision);
        assert_eq!(project.undo.len(), 1);
        assert!(
            project
                .set_box_note(layer, Some("Check this pun".into()))
                .await
                .unwrap()
                .is_none()
        );
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
        let line = project.load_context().unwrap().lines[&layer].clone();
        assert_eq!(line.box_note, None);
        assert_eq!(line.note.as_deref(), Some("Chapter reader note"));
        assert_eq!(line.qc_ok, ["checked"]);
        assert_eq!(line.other["future"], true);
        assert_eq!(project.undo.len(), 2);
        assert!(
            project
                .set_box_note(EntityId::new(), Some("stale".into()))
                .await
                .is_err()
        );
    }
}
