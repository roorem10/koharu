use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, ensure};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager as _, State, WebviewWindow};
use tauri_plugin_opener::OpenerExt as _;
use tauri_runtime_cef::CefRuntime;

use super::super::{
    Error,
    project::{CurrentProject, ProjectLibrary},
};
use super::{LibrarySettings, load_settings, prepare_folder};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LibraryFolder {
    Projects,
}

#[derive(Clone, Debug, Serialize, Type)]
pub struct LibraryPaths {
    pub projects_folder: PathBuf,
    pub default_projects_folder: PathBuf,
}

#[tauri::command]
#[specta::specta]
pub(crate) fn get_library_paths(
    _handle: AppHandle<CefRuntime>,
    library: State<'_, ProjectLibrary>,
) -> std::result::Result<LibraryPaths, Error> {
    Ok(LibraryPaths {
        projects_folder: library.root().to_owned(),
        default_projects_folder: ProjectLibrary::default_root()?,
    })
}

fn next_folder(
    kind: LibraryFolder,
    library: &ProjectLibrary,
    _handle: &AppHandle<CefRuntime>,
) -> Result<PathBuf> {
    let settings = load_settings()?;
    match kind {
        LibraryFolder::Projects => match settings.projects_folder {
            Some(path) => Ok(path),
            None => ProjectLibrary::default_root().or_else(|_| Ok(library.root().to_owned())),
        },
    }
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn choose_library_folder(
    kind: LibraryFolder,
    window: WebviewWindow<CefRuntime>,
    library: State<'_, ProjectLibrary>,
) -> std::result::Result<Option<LibrarySettings>, Error> {
    let dialog = rfd::AsyncFileDialog::new()
        .set_parent(&window)
        .set_directory(next_folder(kind, &library, window.app_handle())?);
    let Some(folder) = dialog.pick_folder().await else {
        return Ok(None);
    };
    let path = folder.path().to_owned();
    prepare_folder(&path)?;
    let config = koharu_config::load::<LibrarySettings>("library")?;
    let mut settings = config.write()?;
    match kind {
        LibraryFolder::Projects => settings.projects_folder = Some(path),
    }
    let saved = settings.clone();
    settings.save()?;
    Ok(Some(saved))
}

#[tauri::command]
#[specta::specta]
pub(crate) fn reset_library_folder(
    kind: LibraryFolder,
) -> std::result::Result<LibrarySettings, Error> {
    let config = koharu_config::load::<LibrarySettings>("library")?;
    let mut settings = config.write()?;
    match kind {
        LibraryFolder::Projects => settings.projects_folder = None,
    }
    let saved = settings.clone();
    settings.save()?;
    Ok(saved)
}

#[tauri::command]
#[specta::specta]
pub(crate) fn open_library_folder(
    kind: LibraryFolder,
    handle: AppHandle<CefRuntime>,
    library: State<'_, ProjectLibrary>,
) -> std::result::Result<(), Error> {
    let path = next_folder(kind, &library, &handle)?;
    prepare_folder(&path)?;
    handle
        .opener()
        .open_path(path.to_string_lossy().into_owned(), None::<&str>)?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn rename_project(
    name: String,
    new_name: String,
    project: State<'_, CurrentProject>,
    library: State<'_, ProjectLibrary>,
) -> std::result::Result<(), Error> {
    let current = project.project.lock().await;
    let active = current.as_ref().map(|project| project.name.clone());
    let library = library.inner().clone();
    tokio::task::spawn_blocking(move || library.rename(&name, &new_name, active.as_deref()))
        .await
        .context("project rename task failed")??;
    drop(current);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn duplicate_project(
    name: String,
    project: State<'_, CurrentProject>,
    library: State<'_, ProjectLibrary>,
) -> std::result::Result<String, Error> {
    // Freeze project writes while copying, including when called by an agent.
    let current = project.project.lock().await;
    let library = library.inner().clone();
    let name = tokio::task::spawn_blocking(move || library.duplicate(&name))
        .await
        .context("project duplication task failed")??;
    drop(current);
    Ok(name)
}

pub(crate) fn copy_directory(source: &Path, destination: &Path) -> Result<()> {
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        ensure!(
            !kind.is_symlink(),
            "cannot duplicate a project containing symbolic links"
        );
        let target = destination.join(entry.file_name());
        if kind.is_dir() {
            std::fs::create_dir(&target)?;
            copy_directory(&entry.path(), &target)?;
        } else {
            ensure!(kind.is_file(), "project contains an unsupported file type");
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_rejects_open_invalid_and_existing_names() {
        let root = tempfile::tempdir().unwrap();
        let library = ProjectLibrary::from_root(root.path().to_owned()).unwrap();
        std::fs::create_dir(root.path().join("chapter.khrproj")).unwrap();
        std::fs::create_dir(root.path().join("Existing.khrproj")).unwrap();
        assert!(library.rename("chapter", "new", Some("chapter")).is_err());
        assert!(library.rename(" chapter ", "new", Some("chapter")).is_err());
        for name in ["existing", "bad/name", "CON", "bad.", "bad ", ""] {
            assert!(library.rename("chapter", name, None).is_err(), "{name}");
        }
        library.rename("chapter", "renamed", None).unwrap();
        assert!(root.path().join("renamed.khrproj").is_dir());
        assert!(!root.path().join("chapter.khrproj").exists());
    }

    #[test]
    fn duplicate_copies_states_blobs_and_context_and_uses_unique_names() {
        let root = tempfile::tempdir().unwrap();
        let library = ProjectLibrary::from_root(root.path().to_owned()).unwrap();
        let source = root.path().join("chapter.khrproj");
        std::fs::create_dir_all(source.join("blobs")).unwrap();
        for name in [
            "state-a.khr",
            "state-b.khr",
            "booharu-context.json",
            "blobs/test",
        ] {
            std::fs::write(source.join(name), name.as_bytes()).unwrap();
        }
        assert_eq!(library.duplicate("chapter").unwrap(), "chapter copy");
        assert_eq!(library.duplicate("chapter").unwrap(), "chapter copy 2");
        for name in [
            "state-a.khr",
            "state-b.khr",
            "booharu-context.json",
            "blobs/test",
        ] {
            assert_eq!(
                std::fs::read(root.path().join("chapter copy.khrproj").join(name)).unwrap(),
                name.as_bytes()
            );
        }
        std::fs::write(
            root.path()
                .join("chapter copy.khrproj/booharu-context.json"),
            "changed",
        )
        .unwrap();
        assert_eq!(
            std::fs::read(source.join("booharu-context.json")).unwrap(),
            b"booharu-context.json"
        );
    }

    #[cfg(unix)]
    #[test]
    fn failed_copy_cleans_the_staging_directory() {
        let root = tempfile::tempdir().unwrap();
        let library = ProjectLibrary::from_root(root.path().to_owned()).unwrap();
        let source = root.path().join("chapter.khrproj");
        std::fs::create_dir(&source).unwrap();
        std::os::unix::fs::symlink(root.path(), source.join("loop")).unwrap();
        assert!(library.duplicate("chapter").is_err());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }
}
