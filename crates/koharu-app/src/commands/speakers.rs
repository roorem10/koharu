use anyhow::{Context as _, ensure};
use booharu_context::{LabelSource, LineSpeaker};
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

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_box_speaker(
    layer: EntityId,
    project: State<'_, CurrentProject>,
) -> Result<Option<LineSpeaker>, Error> {
    let projects = project.project.lock().await;
    let project = projects.as_ref().context("no project is open")?;
    Ok(project.box_speaker(layer)?)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn set_box_speaker(
    layer: EntityId,
    speaker: Option<String>,
    desktop: State<'_, Desktop>,
    project: State<'_, CurrentProject>,
    canvas_channel: State<'_, CanvasChannel>,
) -> Result<Option<LineSpeaker>, Error> {
    let (speaker, commit, page) = {
        let mut projects = project.project.lock().await;
        let project = projects.as_mut().context("no project is open")?;
        let commit = project
            .set_box_speaker(layer, speaker, LabelSource::User)
            .await?;
        if let Some(commit) = &commit {
            project.record_commit(commit);
        }
        (project.box_speaker(layer)?, commit, project.active_page())
    };
    if let Some(commit) = commit {
        desktop.synchronize(&commit.snapshot, page, &commit).await?;
        canvas_channel.channel.publish(desktop.canvas_state());
    }
    Ok(speaker)
}

impl Project {
    pub(crate) fn box_speaker(&self, layer: EntityId) -> anyhow::Result<Option<LineSpeaker>> {
        self.snapshot().text_layer(layer)?;
        Ok(self.load_context()?.speaker(layer))
    }

    pub(crate) async fn set_box_speaker(
        &mut self,
        layer: EntityId,
        speaker: Option<String>,
        source: LabelSource,
    ) -> anyhow::Result<Option<Commit>> {
        self.snapshot().text_layer(layer)?;
        let before = self.load_context()?;
        if let Some(id) = &speaker {
            let series = super::context::linked_series(&before)?;
            ensure!(
                series.as_ref().is_some_and(|series| {
                    series.characters.iter().any(|character| &character.id == id)
                }),
                "speaker is not in the linked series character list"
            );
        }
        let mut after = before.clone();
        match source {
            LabelSource::User => after.set_user_speaker(layer, speaker),
            LabelSource::Model => after.set_model_speaker(layer, speaker),
        };
        self.commit_context(before, after).await
    }
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct GetBoxSpeaker {
    pub layer: String,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct SetBoxSpeaker {
    pub layer: String,
    /// Character ID from the linked series, or null for no known speaker.
    pub speaker: Option<String>,
}

