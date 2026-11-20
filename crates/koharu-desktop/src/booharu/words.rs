//! The words edit's commands: styling a run of a text's characters by hand,
//! as the TypeBar and the Story toolbar do, every word style cleared off
//! whole boxes, as the type bar's Clear does with no words chosen, and the
//! marks the emphasis star gives text typed on the canvas, each worked out
//! by `booharu::words`, which the agent's tools share, its emphasis under
//! the saved `[booharu-emphasis]` and `[booharu-lettering]` as they are
//! when called. And the press on the canvas the page is told of, which lets
//! the words chosen in the Story panel go.

use std::{ops::Range, sync::Arc};

use booharu::words::{self, ClearedWords, WordChange};
use booharu_core::{emphasis::EmphasisSettings, settings::LetteringSettings};
use koharu_scene::{CharStyle, Edit, LayerId, Project};
use parking_lot::Mutex;
use tauri::State;
use tauri_runtime_cef::cef::{CefString, Frame, ImplFrame as _};

use super::apply_if;
use crate::{app::App, commands::Result};

/// Styles the characters `start` to `end`, UTF-8 byte offsets, of the text
/// of `layer`, a block's translation or a text layer's, as `change` says,
/// as one undo step labelled `booharu_style_words`, `text` being the plain
/// text the words were chosen in. A change that changes nothing makes no
/// undo step. Refused, with why: a change asking for none or more than one
/// of clear, emphasis, a step and explicit values; a layer that's neither a
/// block nor a text layer, or is locked or inside a locked layer; a block
/// without a translation; a text that changed since ("The text changed.
/// Select the words again."); and a range that holds no character, or is
/// past the text or off a character boundary. Only emphasis reads the saved
/// settings, so a section that can't be read refuses it alone.
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
        style(
            &session.project(),
            layer,
            start..end,
            &text,
            &change,
            settings,
        )
    })
    .await?;
    Ok(())
}

/// The words edit [`booharu_style_words`] applies to `project`, its
/// settings read through `read` only when `change` stresses the words.
fn style(
    project: &Project,
    layer: LayerId,
    range: Range<usize>,
    text: &str,
    change: &WordChange,
    read: impl FnOnce() -> anyhow::Result<(EmphasisSettings, LetteringSettings)>,
) -> anyhow::Result<Option<Edit>> {
    let (emphasis, lettering) = words::settings(change.emphasis, read)?;
    words::style_words(
        project,
        layer,
        range,
        Some(text),
        change,
        &emphasis,
        &lettering,
    )
}

/// Takes every word style off the texts of `layers`, each a block's
/// translation or a text layer's, so their words look as their texts, as one
/// undo step labelled `booharu_clear_word_styles`, none when none of them
/// carries a word style. A box that is, or is inside, a locked layer is left
/// as it is; a block without a translation, and any other layer, are left
/// out. Answers how many texts it cleared, and how many with word styles it
/// left as they are because they're locked.
#[tauri::command]
#[specta::specta]
pub async fn booharu_clear_word_styles(
    app: State<'_, Arc<App>>,
    layers: Vec<LayerId>,
) -> Result<ClearedWords> {
    let answer = Arc::new(Mutex::new(ClearedWords::default()));
    let counted = Arc::clone(&answer);
    apply_if(&app, move |session| {
        let mut cleared = words::clear_all(&session.project(), &layers)?;
        let edit = cleared.edit.take();
        *counted.lock() = cleared;
        Ok(edit)
    })
    .await?;
    Ok(std::mem::take(&mut *answer.lock()))
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
/// press outside them anywhere but the sidebar (`CANVAS_PRESS` in
/// lib/booharu/wordSelection.ts).
const PRESSED: &str = "dispatchEvent(new Event('booharu:canvas-press'))";

/// Tells the page in `frame` that the canvas took a press.
pub(crate) fn pressed(frame: &Frame) {
    frame.execute_java_script(Some(&CefString::from(PRESSED)), None, 0);
}

#[cfg(test)]
mod tests {
    use anyhow::{Context as _, bail};
    use booharu::words::WordChange;
    use booharu_core::{emphasis::EmphasisSettings, settings::LetteringSettings};
    use koharu_scene::{CharStyle, Created, Edit, Session};
    use serde_json::json;

    use super::{PRESSED, style};

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

    #[test]
    fn a_section_that_cant_be_read_refuses_only_the_star() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let mut session = Session::create(dir.path().join("test.khr"))?;
        let frame =
            json!({"box": {"bounds": {"x": 10.0, "y": 20.0, "width": 40.0, "height": 120.0}}});
        let ops = [
            json!({"op": "add_page", "id": "$p", "index": 0, "size": {"width": 400, "height": 400}}),
            json!({"op": "add_layer", "id": "$block", "page": "$p", "index": 0, "layer": {
                "kind": "block",
                "source": {"frame": frame, "content": "こんにちは"},
                "translation": {"frame": frame, "content": "Hello world",
                    "style": {"chars": {"size": 30.0}}},
            }}),
        ];
        let change = session.apply(Edit {
            label: "test".to_owned(),
            ops: ops
                .into_iter()
                .map(serde_json::from_value)
                .collect::<Result<_, _>>()?,
        })?;
        let block = change
            .created
            .into_iter()
            .find_map(|created| match created {
                Created::Layer { id, .. } => Some(id),
                _ => None,
            })
            .context("the block")?;
        let project = session.project();
        // A bad section edited into the file, as `settings` reads it then.
        let unreadable = || -> anyhow::Result<(EmphasisSettings, LetteringSettings)> {
            bail!("Booharu's emphasis can't be read from [booharu-emphasis]")
        };
        let words =
            |change: WordChange| style(&project, block, 6..11, "Hello world", &change, unreadable);

        // B and the type bar's values style the words without them, and
        // Clear goes through, with nothing to take off here.
        let bold = WordChange {
            bold: Some(true),
            ..WordChange::default()
        };
        assert!(words(bold)?.is_some());
        let set = WordChange {
            set: CharStyle {
                size: Some(40.0),
                ..CharStyle::default()
            },
            ..WordChange::default()
        };
        assert!(words(set)?.is_some());
        let clear = WordChange {
            clear: true,
            ..WordChange::default()
        };
        assert!(words(clear)?.is_none());

        // The star needs them, and is refused with why.
        let star = WordChange {
            emphasis: true,
            ..WordChange::default()
        };
        let refused = words(star).expect_err("the star needs the settings");
        assert!(
            format!("{refused:#}").contains("[booharu-emphasis]"),
            "{refused:#}"
        );
        Ok(())
    }
}
