//! Apply's commands: how many blocks of a scope the default text look would
//! change, and giving it to them, each worked out by `booharu::styles`,
//! which the agent's tool shares, under the saved `[booharu-lettering]` and
//! the first of Koharu's Preferred fonts as they are when called.

use std::sync::Arc;

use booharu::styles::{self, StyleCounts, StyleScope};
use booharu_core::{attrs::Origin, settings::LetteringSettings, styles::first_font};
use koharu_renderer::RendererConfig;
use tauri::State;

use crate::{app::App, commands::Result};

/// How many blocks of `scope` applying the default text look would change,
/// would keep because they were changed by hand, unless
/// `include_hand_edited`, or are locked, and found untranslated: what
/// `booharu_apply_text_styles` would answer now, changing nothing.
#[tauri::command]
#[specta::specta]
pub async fn booharu_count_text_style_targets(
    app: State<'_, Arc<App>>,
    scope: StyleScope,
    include_hand_edited: bool,
) -> Result<StyleCounts> {
    let project = app.session()?.lock().project();
    Ok(tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let (settings, font) = look()?;
        styles::count(
            &project,
            &scope,
            include_hand_edited,
            None,
            &settings,
            font.as_deref(),
        )
    })
    .await??)
}

/// Gives the translations of `scope` the default text look again, as one
/// undo step labelled `booharu_apply_text_style`: the first Preferred font in
/// place of each family, and with colour detection off the default colours
/// and outline in place of its own. Blocks changed by hand are kept unless
/// `include_hand_edited`, locked blocks always are, and untranslated blocks
/// are left alone. Answers how many it changed, kept, and found
/// untranslated; with nothing to change, it makes no undo step.
#[tauri::command]
#[specta::specta]
pub async fn booharu_apply_text_styles(
    app: State<'_, Arc<App>>,
    scope: StyleScope,
    include_hand_edited: bool,
) -> Result<StyleCounts> {
    let app = Arc::clone(&app);
    Ok(tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let (settings, font) = look()?;
        let session = app.session()?;
        let mut session = session.lock();
        let (edit, counts) = styles::apply(
            &session.project(),
            &scope,
            include_hand_edited,
            None,
            &settings,
            font.as_deref(),
            Origin::Automatic,
        )?;
        if let Some(edit) = edit {
            tracing::info!(label = %edit.label, ops = edit.ops.len(), "edit");
            session.apply(edit)?;
        }
        Ok(counts)
    })
    .await??)
}

/// The saved default look, and the default font: the first of Koharu's
/// Preferred fonts, none when it lists none.
pub(super) fn look() -> anyhow::Result<(LetteringSettings, Option<String>)> {
    let settings = koharu_config::get::<LetteringSettings>()?;
    let families = koharu_config::get::<RendererConfig>()?.font_families;
    Ok((settings, first_font(&families).map(str::to_owned)))
}
