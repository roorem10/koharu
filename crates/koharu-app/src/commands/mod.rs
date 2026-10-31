pub(crate) mod agent;
pub(crate) mod boxes;
pub(crate) mod canvas;
pub(crate) mod context;
pub(crate) mod editing;
pub(crate) mod files;
pub(crate) mod fonts;
pub(crate) mod import;
pub(crate) mod lettering;
pub(crate) mod lifecycle;
pub(crate) mod line_labels;
pub(crate) mod mcp;
pub(crate) mod notes;
pub(crate) mod output;
pub(crate) mod preferences;
pub(crate) mod processing;
pub(crate) mod project;
pub(crate) mod qc;
pub(crate) mod runs;
pub(crate) mod styles;

use parking_lot::Mutex;
use serde::Serialize;
use specta::Type;
use tauri::ipc::{Channel, IpcResponse};

#[derive(Debug, Type)]
#[specta(transparent)]
pub(crate) struct Error(#[specta(type = String)] anyhow::Error);

impl<E> From<E> for Error
where
    E: Into<anyhow::Error>,
{
    fn from(error: E) -> Self {
        Self(error.into())
    }
}

impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&format!("{:#}", self.0))
    }
}

pub(crate) trait ChannelExt<T> {
    fn publish(&self, value: T);
}

impl<T: IpcResponse> ChannelExt<T> for Mutex<Option<Channel<T>>> {
    fn publish(&self, value: T) {
        let mut channel = self.lock();
        if channel
            .as_ref()
            .is_some_and(|channel| channel.send(value).is_err())
        {
            channel.take();
        }
    }
}

pub fn bindings() -> tauri_specta::Builder<tauri_runtime_cef::CefRuntime> {
    use tauri_specta::{Builder, ErrorHandlingMode, collect_commands};

    Builder::new()
        .commands(collect_commands![
            agent::get_agent_status,
            agent::login_agent,
            agent::logout_agent,
            agent::save_agent_config,
            agent::run_agent,
            agent::cancel_agent,
            lifecycle::subscribe,
            lifecycle::get_project,
            lifecycle::get_pages,
            lifecycle::get_page,
            lifecycle::list_projects,
            lifecycle::create_project,
            lifecycle::open_project,
            lifecycle::delete_project,
            lifecycle::close_project,
            lifecycle::import,
            lifecycle::import_dropped,
            lifecycle::select_page,
            editing::rename_page,
            editing::delete_pages,
            editing::move_page,
            editing::set_source_text,
            editing::set_translation,
            editing::set_typography,
            editing::set_geometry,
            editing::set_visibility,
            editing::delete_layers,
            editing::move_layer,
            editing::undo,
            editing::redo,
            processing::process,
            processing::stop_job,
            processing::unload_models,
            output::export,
            output::get_thumbnail,
            fonts::get_fonts,
            fonts::get_font_preview,
            fonts::get_font_favorites,
            fonts::set_font_favorite,
            preferences::save_preferences,
            preferences::get_preferences,
            preferences::get_translation_models,
            preferences::get_shortcuts,
            preferences::save_shortcuts,
            preferences::get_wheel_zoom,
            preferences::save_wheel_zoom,
            styles::get_text_styles,
            styles::save_text_styles,
            styles::count_text_style_targets,
            styles::apply_shape_rules,
            styles::get_text_bubble_shape,
            line_labels::get_line_category,
            line_labels::set_line_category,
            // WO-18 (chapter context) commands go here.
            context::history::get_agent_history,
            context::history::new_agent_conversation,
            context::lines::check_glossary,
            context::lines::dismiss_line_flag,
            context::lines::glossary_usage,
            context::lines::explain_line,
            context::reading::read_chapter,
            context::series::list_series,
            context::series::create_series,
            context::series::get_series,
            context::series::save_series,
            context::series::rename_series,
            context::series::delete_series,
            context::series::set_project_series,
            context::series::export_series,
            context::series::preview_series_import,
            context::series::import_series,
            context::series::preview_instruction_glossary,
            context::series::import_instruction_glossary,
            context::get_chapter_context,
            context::save_chapter_summary,
            context::dismiss_suggestion,
            context::accept_suggestion,
            context::settings::get_context_settings,
            context::settings::save_context_settings,
            canvas::get_canvas_manifest,
            canvas::get_canvas_resource,
            canvas::prepare_canvas_page,
            canvas::get_canvas_page_manifest,
            canvas::get_canvas_page_resource,
            canvas::add_point_text,
            canvas::add_text_box,
            canvas::commit_paint,
            canvas::commit_erase,
            canvas::commit_transform,
            canvas::commit_inpaint,
            // WO-19 (lettering categories) commands go here.
            lettering::get_effective_category,
            lettering::get_lettering_settings,
            lettering::save_lettering_settings,
            // WO-20 (one box per bubble) commands go here.
            boxes::get_box_settings,
            boxes::save_box_settings,
            boxes::redetect_page,
            boxes::get_box_actions,
            boxes::join_bubble_boxes,
            boxes::join_page_bubbles,
            boxes::split_box,
            // WO-12 (QC list) commands go here.
            qc::run_qc,
            qc::mark_qc_ok,
            qc::get_qc_settings,
            qc::save_qc_settings,
            qc::get_page_source,
            // WO-13 (files and export) commands go here.
            files::get_library_settings,
            files::save_library_settings,
            output::export_selected_pages,
            files::scripts::export_script,
            files::scripts::preview_script_import,
            files::scripts::apply_script_import,
            files::library::get_library_paths,
            files::library::choose_library_folder,
            files::library::reset_library_folder,
            files::library::open_library_folder,
            files::library::rename_project,
            files::library::duplicate_project,
            // WO-14 (runs and retries) commands go here.
            runs::get_run_settings,
            runs::save_run_settings,
            runs::retry_failed_pages,
            runs::preview_chapter_batch,
            runs::run_chapter_batch,
            runs::stop_chapter_batch,
            // WO-06 (text style extras and box notes) commands go here.
            // WO-21 (MCP server) commands go here.
        ])
        .disable_serde_phases()
        .error_handling(ErrorHandlingMode::Throw)
}
