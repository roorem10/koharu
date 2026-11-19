//! Add subtitle's command: a sound effect's subtitle, worked out by
//! `booharu::sfx`, which the agent's tool shares, under the saved
//! `[booharu-lettering]`, the first of Koharu's Preferred fonts, and the
//! saved pipeline settings as they are when called.

use std::{collections::BTreeSet, sync::Arc};

use anyhow::Context as _;
use booharu::sfx::{self, SfxSubtitleAnswer, Subtitle};
use koharu_pipeline::{PipelineConfig, Stage, Target};
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
/// which adds the subtitle as it writes, answered as the run. Refuses a
/// layer that isn't a sound effect, and one that is, or is in, a locked
/// layer.
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
    let (target, stages, config) = subtitle_run(block, config);
    let run = app.process(target, stages, Some(config))?;
    Ok(SfxSubtitleAnswer {
        layer: None,
        run: Some(run),
    })
}

/// The Translate run in subtitle mode on the sound effect `block`, with
/// `config`, the saved pipeline settings.
fn subtitle_run(
    block: LayerId,
    mut config: PipelineConfig,
) -> (Target, BTreeSet<Stage>, PipelineConfig) {
    config.booharu.subtitle_run = true;
    (
        Target::Blocks(vec![block]),
        BTreeSet::from([Stage::Translate]),
        config,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_effect_without_words_is_translated_alone_in_subtitle_mode() {
        let block: LayerId = "00000000-0000-0000-0000-000000000001"
            .parse()
            .expect("an id");
        let (target, stages, config) = subtitle_run(block, PipelineConfig::default());
        assert_eq!(target, Target::Blocks(vec![block]));
        assert_eq!(stages, BTreeSet::from([Stage::Translate]));
        assert!(config.booharu.subtitle_run);
        assert!(!config.booharu.redo);
    }
}
