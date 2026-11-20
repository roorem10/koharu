//! The words edit's commands: styling a run of a text's characters by hand,
//! as the TypeBar and the Story toolbar do, and the marks the emphasis star
//! gives text typed on the canvas, each worked out by `booharu::words`,
//! which the agent's tool shares, its emphasis under the saved
//! `[booharu-emphasis]` and `[booharu-lettering]` as they are when called.
//! And the press on the canvas the page is told of, which lets the words
//! chosen in the Story panel go.

use std::sync::Arc;

use booharu::words::{self, WordChange};
use booharu_core::{emphasis::EmphasisSettings, settings::LetteringSettings};
use koharu_scene::{CharStyle, LayerId};
use tauri::State;
use tauri_runtime_cef::cef::{CefString, Frame, ImplFrame as _};

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

/// What the page hears when the canvas takes a press, which the window gives
/// the canvas alone: the words chosen in the Story panel go then, as with a
/// press anywhere else outside them (`CANVAS_PRESS` in
/// lib/booharu/wordSelection.ts).
const PRESSED: &str = "dispatchEvent(new Event('booharu:canvas-press'))";

/// Tells the page in `frame` that the canvas took a press.
pub(crate) fn pressed(frame: &Frame) {
    frame.execute_java_script(Some(&CefString::from(PRESSED)), None, 0);
}

#[cfg(test)]
mod tests {
    use super::PRESSED;

    #[test]
    fn a_canvas_press_sends_the_event_the_story_words_listen_for() {
        let hook = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/koharu/lib/booharu/wordSelection.ts"
        );
        let hook = std::fs::read_to_string(hook).expect("the Story words' hook");
        let name = hook
            .split("export const CANVAS_PRESS = '")
            .nth(1)
            .and_then(|rest| rest.split('\'').next())
            .expect("the event the hook listens for");
        assert_eq!(PRESSED, format!("dispatchEvent(new Event('{name}'))"));
    }
}
