//! The words edit's commands: styling a run of a text's characters by hand,
//! as the TypeBar and the Story toolbar do, and the marks the emphasis star
//! gives text typed on the canvas, each worked out by `booharu::words`,
//! which the agent's tool shares, its emphasis under the saved
//! `[booharu-emphasis]` and `[booharu-lettering]` as they are when called.

use std::sync::Arc;

use booharu::words::{self, WordChange};
use booharu_core::{emphasis::EmphasisSettings, settings::LetteringSettings};
use koharu_scene::{CharStyle, LayerId};
use tauri::State;

use super::apply_if;
use crate::{app::App, commands::Result};

/// Styles the characters `start` to `end`, UTF-8 byte offsets, of the text
/// of `layer`, a block's translation or a text layer's, as `change` says,
/// as one undo step labelled `booharu_style_words`, `text` being the plain
/// text the words were chosen in. A change that changes nothing makes no
/// undo step. Refused, with why: a change asking for none or more than one
/// of clear, emphasis and explicit values; a layer that's neither a block
/// nor a text layer, or is locked or inside a locked layer; a block without
/// a translation; a text that changed since ("The text changed. Select the
/// words again."); and a range that holds no character, or is past the text
/// or off a character boundary. Only emphasis reads the saved settings, so
/// a section that can't be read refuses it alone.
#[tauri::command]
#[specta::specta]
pub async fn booharu_style_words(
    app: State<'_, Arc<App>>,
    layer: LayerId,
    start: usize,
    end: usize,
    text: String,
    change: WordChange,
) -> Result<()> {
    apply_if(&app, move |session| {
        let (emphasis, lettering) = words::settings(change.emphasis, settings)?;
        words::style_words(
            &session.project(),
            layer,
            start..end,
            Some(&text),
            &change,
            &emphasis,
            &lettering,
        )
    })
    .await?;
    Ok(())
}

/// The marks the emphasis star gives the text typed into `layer`, a block
/// or a text layer: its lettering category's emphasis, a text layer's the
/// default, over its text's own values, so only what changes is set.
/// Refused for a layer that's neither, and for a locked one.
#[tauri::command]
#[specta::specta]
pub async fn booharu_emphasis_patch(app: State<'_, Arc<App>>, layer: LayerId) -> Result<CharStyle> {
    let project = app.session()?.lock().project();
    Ok(tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let (emphasis, lettering) = settings()?;
        words::emphasis_patch(&project, layer, &emphasis, &lettering)
    })
    .await??)
}

/// The saved `[booharu-emphasis]` and `[booharu-lettering]`, a bad stored
/// value failing, never replaced by its default.
fn settings() -> anyhow::Result<(EmphasisSettings, LetteringSettings)> {
    Ok((
        EmphasisSettings::read()?,
        koharu_config::get::<LetteringSettings>()?,
    ))
}
