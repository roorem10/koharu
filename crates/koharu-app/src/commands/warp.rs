//! Skew and perspective edits share the ordinary project undo and canvas publication path.

use anyhow::Context as _;
use koharu_desktop::Desktop;
use koharu_scene::{EntityId, Origin, TextLayout, TextWarp};
use tauri::State;

use super::{
    ChannelExt as _, Error,
    canvas::CanvasChannel,
    project::{CurrentProject, Project},
};

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_text_warp(
    layers: Vec<EntityId>,
    project: State<'_, CurrentProject>,
) -> Result<Vec<Option<TextWarp>>, Error> {
    let projects = project.project.lock().await;
    let project = projects.as_ref().context("no project is open")?;
    Ok(project.text_warps(&layers)?)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn set_text_warp(
    layers: Vec<EntityId>,
    warp: Option<TextWarp>,
    desktop: State<'_, Desktop>,
    project: State<'_, CurrentProject>,
    canvas_channel: State<'_, CanvasChannel>,
) -> Result<(), Error> {
    let (commit, page) = {
        let mut projects = project.project.lock().await;
        let project = projects.as_mut().context("no project is open")?;
        let commit = project.set_text_warp(layers, warp).await?;
        project.record_commit(&commit);
        (commit, project.active_page())
    };
    desktop.synchronize(&commit.snapshot, page, &commit).await?;
    canvas_channel.channel.publish(desktop.canvas_state());
    Ok(())
}

impl Project {
    fn text_warps(&self, layers: &[EntityId]) -> anyhow::Result<Vec<Option<TextWarp>>> {
        let snapshot = self.snapshot();
        layers
            .iter()
            .map(|&layer| {
                snapshot.text_layer(layer)?;
                Ok(snapshot
                    .component::<TextLayout>(layer)?
                    .context("text layout is missing")?
                    .warp)
            })
            .collect()
    }

    async fn set_text_warp(
        &mut self,
        mut layers: Vec<EntityId>,
        warp: Option<TextWarp>,
    ) -> anyhow::Result<koharu_scene::Commit> {
        if let Some(warp) = &warp {
            warp.validate()?;
        }
        layers.sort_unstable();
        layers.dedup();
        let snapshot = self.snapshot();
        let updates = layers
            .into_iter()
            .map(|layer| -> anyhow::Result<_> {
                snapshot.text_layer(layer)?;
                let mut layout = snapshot
                    .component::<TextLayout>(layer)?
                    .context("text layout is missing")?;
                layout.warp = warp.clone();
                layout.origin = Origin::User;
                Ok((layer, layout))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let patch = snapshot.patch(|edit| {
            for (layer, layout) in updates {
                edit.promote_entity_to_user(layer)?;
                edit.set(layer, &layout)?;
            }
            Ok(())
        })?;
        self.commit(patch).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use koharu_scene::{At, Generation, PageDraft, ProducerId, Session, TextLayoutKind};

    async fn project() -> (Project, [EntityId; 2]) {
        let mut session = Session::memory().await.unwrap();
        let mut ids = Vec::new();
        let patch = session
            .snapshot()
            .patch(|edit| {
                let page = edit.add_page(PageDraft::new("page", 200.0, 100.0), At::End)?;
                for _ in 0..2 {
                    let content = edit.add_text_content(page, At::End)?;
                    ids.push(edit.add_text_layer(
                        page,
                        At::End,
                        content,
                        &TextLayout {
                            origin: Origin::Generated(Generation::new(
                                ProducerId::new("dev.koharu.test").unwrap(),
                            )),
                            kind: TextLayoutKind::Paragraph,
                            angle_degrees: Some(17.0),
                            warp: None,
                        },
                    )?);
                }
                Ok(())
            })
            .unwrap();
        session.commit(patch).await.unwrap();
        (
            Project::new(session, "warp".into()),
            ids.try_into().unwrap(),
        )
    }

    #[tokio::test]
    async fn several_text_warps_are_one_undo_step_and_reset_keeps_rotation() {
        let (mut project, layers) = project().await;
        let warp = TextWarp::Skew {
            x_degrees: 20.0,
            y_degrees: -5.0,
        };
        let commit = project
            .set_text_warp(vec![layers[0], layers[1], layers[0]], Some(warp.clone()))
            .await
            .unwrap();
        project.record_commit(&commit);
        assert_eq!(project.undo.len(), 1);
        assert_eq!(
            project.text_warps(&layers).unwrap(),
            vec![Some(warp.clone()); 2]
        );
        for layer in layers {
            let layout = project
                .snapshot()
                .component::<TextLayout>(layer)
                .unwrap()
                .unwrap();
            assert_eq!(layout.origin, Origin::User);
            assert_eq!(layout.angle_degrees, Some(17.0));
        }
        project.undo().await.unwrap();
        assert_eq!(project.text_warps(&layers).unwrap(), vec![None; 2]);
        project.redo().await.unwrap();
        assert_eq!(project.text_warps(&layers).unwrap(), vec![Some(warp); 2]);
        project.set_text_warp(layers.to_vec(), None).await.unwrap();
        assert_eq!(project.text_warps(&layers).unwrap(), vec![None; 2]);
        assert_eq!(
            project
                .snapshot()
                .component::<TextLayout>(layers[0])
                .unwrap()
                .unwrap()
                .angle_degrees,
            Some(17.0)
        );
    }

    #[tokio::test]
    async fn bad_warps_and_stale_selections_do_not_partially_edit_the_project() {
        let (mut project, layers) = project().await;
        let revision = project.revision();
        assert!(
            project
                .set_text_warp(
                    layers.to_vec(),
                    Some(TextWarp::Skew {
                        x_degrees: f32::NAN,
                        y_degrees: 0.0
                    })
                )
                .await
                .is_err()
        );
        assert!(
            project
                .set_text_warp(
                    vec![layers[0], EntityId::new()],
                    Some(TextWarp::Skew {
                        x_degrees: 20.0,
                        y_degrees: 0.0
                    })
                )
                .await
                .is_err()
        );
        assert_eq!(project.revision(), revision);
        assert_eq!(project.text_warps(&layers).unwrap(), vec![None; 2]);
    }
}
