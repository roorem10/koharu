//! WO-06: notes on text boxes.

use anyhow::Context as _;
use koharu_scene::EntityId;
use tauri::State;

use super::{
    Error,
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
    project: State<'_, CurrentProject>,
) -> Result<(), Error> {
    let projects = project.project.lock().await;
    let project = projects.as_ref().context("no project is open")?;
    project.set_box_note(layer, note)?;
    Ok(())
}

impl Project {
    fn box_note(&self, layer: EntityId) -> anyhow::Result<Option<String>> {
        self.snapshot().text_layer(layer)?;
        Ok(self
            .load_context()?
            .lines
            .get(&layer)
            .and_then(|line| line.box_note.clone()))
    }

    fn set_box_note(&self, layer: EntityId, note: Option<String>) -> anyhow::Result<()> {
        self.snapshot().text_layer(layer)?;
        self.update_context(|context| {
            context.lines.entry(layer).or_default().box_note =
                note.filter(|note| !note.trim().is_empty());
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use koharu_scene::{At, Origin, PageDraft, Session, TextLayout, TextLayoutKind};

    #[tokio::test]
    async fn box_notes_persist_without_changing_chapter_notes_or_undo() {
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
        project
            .set_box_note(layer, Some("Check this pun".into()))
            .unwrap();
        assert_eq!(
            project.box_note(layer).unwrap().as_deref(),
            Some("Check this pun")
        );
        let saved = booharu_context::ProjectContext::load(directory.path()).unwrap();
        assert_eq!(
            saved.lines[&layer].box_note.as_deref(),
            Some("Check this pun")
        );
        project.set_box_note(layer, Some("  \n  ".into())).unwrap();
        let line = project.load_context().unwrap().lines[&layer].clone();
        assert_eq!(line.box_note, None);
        assert_eq!(line.note.as_deref(), Some("Chapter reader note"));
        assert_eq!(line.qc_ok, ["checked"]);
        assert_eq!(line.other["future"], true);
        assert_eq!(project.revision(), revision);
        assert!(project.undo.is_empty());
        assert!(
            project
                .set_box_note(EntityId::new(), Some("stale".into()))
                .is_err()
        );
    }
}
