//! The commands the React interface calls.

mod agent;
mod canvas;
#[cfg(debug_assertions)]
mod debug;
mod export;
mod fonts;
mod import;
mod pages;
mod preferences;
mod processing;
mod project;
mod scene;

use serde::Serialize;
use specta::{
    Type, Types,
    datatype::{DataType, Function},
};
use tauri::ipc::{Invoke, InvokeResponseBody, IpcResponse};
use tauri_runtime_cef::CefRuntime;

/// A command's failure, as its message with its causes.
#[derive(Debug, Serialize, Type)]
#[serde(transparent)]
pub struct Error(String);

impl<E: Into<anyhow::Error>> From<E> for Error {
    fn from(error: E) -> Self {
        Self(format!("{:#}", error.into()))
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Bytes a command answers as an `ArrayBuffer`, not a JSON array of numbers.
pub struct Bytes(pub Vec<u8>);

impl Type for Bytes {
    fn definition(_: &mut Types) -> DataType {
        DataType::Reference(specta_typescript::define("ArrayBuffer"))
    }
}

impl IpcResponse for Bytes {
    fn body(self) -> tauri::Result<InvokeResponseBody> {
        Ok(InvokeResponseBody::Raw(self.0))
    }
}

/// Every command, each run inside a span continuing the trace its
/// `sentry-trace` header carries: the interface's span of that call. Tauri
/// runs an async command's future inside a span of its own, made under
/// this one.
pub fn handler() -> impl Fn(Invoke<CefRuntime>) -> bool + Send + Sync + 'static {
    let commands: fn(Invoke<CefRuntime>) -> bool = tauri::generate_handler![
        project::list_projects,
        project::create,
        project::open,
        project::delete_project,
        project::close,
        project::get_open_project,
        pages::get_shown_page,
        pages::show,
        scene::query,
        scene::edit,
        scene::undo,
        scene::redo,
        scene::go_to,
        pages::get_page_thumbnail,
        processing::run_pipeline,
        processing::stop_pipeline,
        processing::unload,
        processing::get_pipeline_events,
        agent::run_agent,
        agent::stop_agent,
        agent::reset_agent,
        preferences::get_preferences,
        preferences::save_preferences,
        preferences::list_models,
        preferences::list_vendors,
        preferences::set_api_key,
        preferences::sign_in,
        preferences::stop_sign_in,
        preferences::sign_out,
        fonts::list_fonts,
        fonts::get_font_preview,
        import::import,
        export::export,
        canvas::set_viewport,
        canvas::set_tool,
        canvas::set_brush,
        canvas::set_colors,
        canvas::set_story,
        canvas::style_typing,
        canvas::fit,
        canvas::set_zoom,
        canvas::select,
        processing::fill,
        canvas::deselect,
        #[cfg(debug_assertions)]
        debug::capture,
        #[cfg(debug_assertions)]
        debug::debug_pointer,
        // Booharu: Booharu's commands, the same as in `bindings`.
        crate::booharu::settings::booharu_get_preferences,
        crate::booharu::settings::booharu_save_preferences,
        crate::booharu::translation::booharu_set_translation,
        crate::booharu::boxes::booharu_find_split_balloons,
        crate::booharu::boxes::booharu_join_balloon,
        crate::booharu::boxes::booharu_join_page,
        crate::booharu::boxes::booharu_split_block,
        crate::booharu::notes::booharu_set_review_note,
        crate::booharu::styles::booharu_count_text_style_targets,
        crate::booharu::styles::booharu_apply_text_styles,
        crate::booharu::fonts::booharu_get_font_favorites,
        crate::booharu::fonts::booharu_set_font_favorite,
        crate::booharu::fonts::booharu_get_font_local_names,
        crate::booharu::runs::booharu_run,
        crate::booharu::lettering::booharu_set_lettering_category,
        crate::booharu::lettering::booharu_get_lettering,
        crate::booharu::context::booharu_list_series,
        crate::booharu::context::booharu_create_series,
        crate::booharu::context::booharu_get_series,
        crate::booharu::context::booharu_save_series,
        crate::booharu::context::booharu_rename_series,
        crate::booharu::context::booharu_delete_series,
        crate::booharu::context::booharu_link_series,
        crate::booharu::context::booharu_get_chapter_context,
        crate::booharu::context::booharu_accept_suggestion,
        crate::booharu::context::booharu_dismiss_suggestion,
        crate::booharu::context::booharu_glossary_usage,
        crate::booharu::context::booharu_fix_term_everywhere,
        crate::booharu::context::booharu_preview_instruction_glossary,
        crate::booharu::context::booharu_import_instruction_glossary,
        crate::booharu::context::booharu_export_series,
        crate::booharu::context::booharu_preview_series_import,
        crate::booharu::context::booharu_import_series,
        crate::booharu::context::booharu_get_first_start_note,
        crate::booharu::context::booharu_dismiss_first_start_note,
        crate::booharu::context::booharu_set_chapter_brief,
        crate::booharu::read_chapter::booharu_read_chapter,
        crate::booharu::read_chapter::booharu_get_reading_model,
        crate::booharu::explain::booharu_explain_line,
        crate::booharu::agent_history::booharu_get_agent_history,
        crate::booharu::jobs::booharu_get_jobs,
        crate::booharu::qc::booharu_get_qc,
        crate::booharu::qc::booharu_mark_qc_ok,
        crate::booharu::qc::booharu_dismiss_flag,
        crate::booharu::sfx::booharu_add_sfx_subtitle,
        crate::booharu::styles::booharu_reset_lettering,
        crate::booharu::redo::booharu_count_redo,
        crate::booharu::redo::booharu_redo_boxes,
        crate::booharu::words::booharu_style_words,
        crate::booharu::words::booharu_emphasis_patch,
    ];
    move |invoke: Invoke<CefRuntime>| {
        let message = &invoke.message;
        let trace = message
            .headers()
            .get("sentry-trace")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        let _span = tracing::info_span!(
            "command",
            sentry.name = message.command(),
            sentry.op = "command",
            sentry.trace = trace,
        )
        .entered();
        commands(invoke)
    }
}

/// Every command's signature, for the bindings; the same commands as
/// `handler`'s, but the debug build's own.
pub fn bindings(types: &mut Types) -> Vec<Function> {
    specta::function::collect_functions![
        project::list_projects,
        project::create,
        project::open,
        project::delete_project,
        project::close,
        project::get_open_project,
        pages::get_shown_page,
        pages::show,
        scene::query,
        scene::edit,
        scene::undo,
        scene::redo,
        scene::go_to,
        pages::get_page_thumbnail,
        processing::run_pipeline,
        processing::stop_pipeline,
        processing::unload,
        processing::get_pipeline_events,
        agent::run_agent,
        agent::stop_agent,
        agent::reset_agent,
        preferences::get_preferences,
        preferences::save_preferences,
        preferences::list_models,
        preferences::list_vendors,
        preferences::set_api_key,
        preferences::sign_in,
        preferences::stop_sign_in,
        preferences::sign_out,
        fonts::list_fonts,
        fonts::get_font_preview,
        import::import,
        export::export,
        canvas::set_viewport,
        canvas::set_tool,
        canvas::set_brush,
        canvas::set_colors,
        canvas::set_story,
        canvas::style_typing,
        canvas::fit,
        canvas::set_zoom,
        canvas::select,
        processing::fill,
        canvas::deselect,
        // Booharu: Booharu's commands, the same as in `handler`.
        crate::booharu::settings::booharu_get_preferences,
        crate::booharu::settings::booharu_save_preferences,
        crate::booharu::translation::booharu_set_translation,
        crate::booharu::boxes::booharu_find_split_balloons,
        crate::booharu::boxes::booharu_join_balloon,
        crate::booharu::boxes::booharu_join_page,
        crate::booharu::boxes::booharu_split_block,
        crate::booharu::notes::booharu_set_review_note,
        crate::booharu::styles::booharu_count_text_style_targets,
        crate::booharu::styles::booharu_apply_text_styles,
        crate::booharu::fonts::booharu_get_font_favorites,
        crate::booharu::fonts::booharu_set_font_favorite,
        crate::booharu::fonts::booharu_get_font_local_names,
        crate::booharu::runs::booharu_run,
        crate::booharu::lettering::booharu_set_lettering_category,
        crate::booharu::lettering::booharu_get_lettering,
        crate::booharu::context::booharu_list_series,
        crate::booharu::context::booharu_create_series,
        crate::booharu::context::booharu_get_series,
        crate::booharu::context::booharu_save_series,
        crate::booharu::context::booharu_rename_series,
        crate::booharu::context::booharu_delete_series,
        crate::booharu::context::booharu_link_series,
        crate::booharu::context::booharu_get_chapter_context,
        crate::booharu::context::booharu_accept_suggestion,
        crate::booharu::context::booharu_dismiss_suggestion,
        crate::booharu::context::booharu_glossary_usage,
        crate::booharu::context::booharu_fix_term_everywhere,
        crate::booharu::context::booharu_preview_instruction_glossary,
        crate::booharu::context::booharu_import_instruction_glossary,
        crate::booharu::context::booharu_export_series,
        crate::booharu::context::booharu_preview_series_import,
        crate::booharu::context::booharu_import_series,
        crate::booharu::context::booharu_get_first_start_note,
        crate::booharu::context::booharu_dismiss_first_start_note,
        crate::booharu::context::booharu_set_chapter_brief,
        crate::booharu::read_chapter::booharu_read_chapter,
        crate::booharu::read_chapter::booharu_get_reading_model,
        crate::booharu::explain::booharu_explain_line,
        crate::booharu::agent_history::booharu_get_agent_history,
        crate::booharu::jobs::booharu_get_jobs,
        crate::booharu::qc::booharu_get_qc,
        crate::booharu::qc::booharu_mark_qc_ok,
        crate::booharu::qc::booharu_dismiss_flag,
        crate::booharu::sfx::booharu_add_sfx_subtitle,
        crate::booharu::styles::booharu_reset_lettering,
        crate::booharu::redo::booharu_count_redo,
        crate::booharu::redo::booharu_redo_boxes,
        crate::booharu::words::booharu_style_words,
        crate::booharu::words::booharu_emphasis_patch,
    ](types)
}
