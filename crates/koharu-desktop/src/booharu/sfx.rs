//! Add subtitle's command: a sound effect's subtitle, worked out by
//! `booharu::sfx`, which the agent's tool shares, under the saved
//! `[booharu-lettering]`, the first of Koharu's Preferred fonts, and the
//! saved pipeline settings as they are when called.

use std::sync::Arc;

use anyhow::Context as _;
use booharu::sfx::{self, SfxSubtitleAnswer, Subtitle};
use koharu_pipeline::PipelineConfig;
use koharu_scene::LayerId;
use tauri::State;

use crate::{app::App, commands::Result};

/// Gives the sound effect `block` a subtitle, a small text layer beside it
/// while the effect keeps its art: from `text` when given, 1 to 500
/// characters once trimmed, else from the effect's translation, as one undo
/// step labelled `booharu_add_sfx_subtitle`, answering the subtitle's
/// layer. An effect with a subtitle answers that one and changes nothing.
/// An effect with no words to subtitle it with is translated first: one
/// Translate run on it, with the saved pipeline settings, in subtitle mode,
/// which adds the subtitle as it writes, answered as the run; one with no
/// words read either is refused, as the run would have nothing to send, so
/// it's read again first or given the words. Refuses a layer that isn't a
/// sound effect, one that is, or is in, a locked layer, and one whose source
/// has no box to put the subtitle beside.
#[tauri::command]
#[specta::specta]
pub async fn booharu_add_sfx_subtitle(
    app: State<'_, Arc<App>>,
    block: LayerId,
    text: Option<String>,
) -> Result<SfxSubtitleAnswer> {
    let config = koharu_config::get::<PipelineConfig>()?;
    let target = config.translator.target.clone();
    let editing = Arc::clone(&app);
    let layer = tokio::task::spawn_blocking(move || -> anyhow::Result<Option<LayerId>> {
        let (settings, font) = super::styles::look()?;
        let session = editing.session()?;
        let mut session = session.lock();
        let project = session.project();
        let subtitle =
            sfx::add_sfx_subtitle(&project, block, text, &settings, font.as_deref(), &target)?;
        match subtitle {
            Subtitle::Existing(layer) => Ok(Some(layer)),
            Subtitle::Edit(edit) => {
                tracing::info!(label = %edit.label, ops = edit.ops.len(), "edit");
                let change = session.apply(edit)?;
                Ok(Some(
                    sfx::created(&change).context("the subtitle added has no layer")?,
                ))
            }
            Subtitle::NeedsTranslation => Ok(None),
        }
    })
    .await??;
    if let Some(layer) = layer {
        return Ok(SfxSubtitleAnswer {
            layer: Some(layer),
            run: None,
        });
    }
    let request = sfx::subtitle_run(block, config);
    let run = app.process(request.target, request.stages, request.config)?;
    Ok(SfxSubtitleAnswer {
        layer: None,
        run: Some(run),
    })
}
