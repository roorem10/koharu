//! WO-17: auto levels for grey or faded raws.

use koharu_scene::EntityId;
use tauri::AppHandle;
use tauri_runtime_cef::CefRuntime;

/// Called once after pages are imported, with the new pages in import order.
pub(crate) async fn after_import(_handle: &AppHandle<CefRuntime>, _pages: Vec<EntityId>) {}
