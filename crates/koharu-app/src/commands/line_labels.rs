//! Shared commands for a line's lettering category. Frozen: neither WO-18 nor
//! WO-19 changes this file; both call [`labels_changed`].

use anyhow::Context as _;
use booharu_context::{LetteringCategory, LineCategory};
use koharu_desktop::Desktop;
use koharu_scene::EntityId;
use tauri::State;

use super::{ChannelExt as _, Error, canvas::CanvasChannel, project::CurrentProject};

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_line_category(
    layer: EntityId,
    project: State<'_, CurrentProject>,
) -> Result<Option<LineCategory>, Error> {
    let project = project.project.lock().await;
    let project = project.as_ref().context("no project is open")?;
    Ok(project.load_context()?.category(layer))
}

/// Sets a hand-picked category, or clears it with `None` so the automatic
/// rules decide again, then restyles the line.
#[tauri::command]
#[specta::specta]
pub(crate) async fn set_line_category(
    layer: EntityId,
    category: Option<LetteringCategory>,
    desktop: State<'_, Desktop>,
    project: State<'_, CurrentProject>,
    canvas_channel: State<'_, CanvasChannel>,
) -> Result<Option<LineCategory>, Error> {
    let (label, commit, page) = {
        let mut project = project.project.lock().await;
        let project = project.as_mut().context("no project is open")?;
        project.snapshot().text_layer(layer)?;
        let (changed, label) = project.update_context(|context| {
            let changed = context.set_user_category(layer, category);
            (changed, context.category(layer))
        })?;
        let commit = if changed {
            super::lettering::labels_changed(project, &[layer]).await?
        } else {
            None
        };
        if let Some(commit) = &commit {
            project.record_commit(commit);
        }
        (label, commit, project.active_page())
    };
    if let Some(commit) = commit {
        desktop.synchronize(&commit.snapshot, page, &commit).await?;
        canvas_channel.channel.publish(desktop.canvas_state());
    }
    Ok(label)
}

#[cfg(test)]
mod tests {
    use booharu_context::{CONTEXT_FILE, LabelSource, LetteringCategory};
    use koharu_scene::EntityId;

    use super::super::project::Project;

    #[tokio::test]
    async fn the_sidecar_lives_in_the_project_folder() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("chapter.khrproj");
        let project = Project::create("chapter".into(), path.clone())
            .await
            .unwrap();
        let layer = EntityId::new();
        project
            .update_context(|context| {
                context.set_user_category(layer, Some(LetteringCategory::Whisper))
            })
            .unwrap();
        assert!(path.join(CONTEXT_FILE).is_file());
        let label = project.load_context().unwrap().category(layer).unwrap();
        assert_eq!(label.value, LetteringCategory::Whisper);
        assert_eq!(label.source, LabelSource::User);

        let memory = Project::new(
            koharu_scene::Session::memory().await.unwrap(),
            "memory".into(),
        );
        assert!(memory.load_context().unwrap().lines.is_empty());
        assert!(memory.update_context(|_| ()).is_err());
    }
}
