//! Private preview storage is relative to the executable, never to the user's normal profile.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};

pub const IDENTIFIER: &str = "rs.koharu.typesetting-preview";

pub fn enabled() -> bool {
    option_env!("KOHARU_PRIVATE_PREVIEW") == Some("1")
}

/// No environment variable, working directory or legacy profile is used as a fallback.
pub fn root() -> Result<Option<PathBuf>> {
    if !enabled() {
        return Ok(None);
    }
    let executable = std::env::current_exe().context("could not locate the preview executable")?;
    let parent = executable
        .parent()
        .context("the preview executable has no directory")?;
    Ok(Some(parent.join("PreviewData")))
}

/// Prepare the isolated directories before any desktop plugin or configuration is loaded.
pub fn initialize() -> Result<Option<PathBuf>> {
    let Some(root) = root()? else { return Ok(None) };
    prepare(&root)?;
    Ok(Some(root))
}

fn prepare(root: &Path) -> Result<()> {
    for path in std::iter::once(root.to_path_buf())
        .chain(["settings", "projects", "packages", "browser", "logs"].map(|name| root.join(name)))
    {
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            ensure!(
                !metadata.file_type().is_symlink(),
                "preview data cannot be a symbolic link: {}",
                path.display()
            );
        }
        fs::create_dir_all(&path).with_context(|| {
            format!(
                "cannot create isolated preview data at {}; move the preview to a writable folder",
                path.display()
            )
        })?;
    }
    // An unwritable portable folder must stop startup instead of falling back to official data.
    let mut file = tempfile::Builder::new()
        .prefix(".write-check-")
        .tempfile_in(root)
        .with_context(|| {
            format!(
                "cannot write isolated preview data at {}; move the preview to a writable folder",
                root.display()
            )
        })?;
    file.write_all(b"Koharu isolated preview\n")
        .context("isolated preview write check failed")?;
    file.as_file()
        .sync_all()
        .context("isolated preview write check failed")?;
    file.close()
        .context("could not remove the preview write check")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separate_executables_keep_separate_persistent_settings() {
        if !enabled() {
            return;
        }
        const CHILD: &str = "KOHARU_PREVIEW_CONFIG_TEST";
        #[derive(Default, serde::Deserialize, serde::Serialize)]
        struct Trial {
            name: String,
        }
        if let Ok(name) = std::env::var(CHILD) {
            let executable = std::env::current_exe().unwrap();
            let expected = executable
                .parent()
                .unwrap()
                .join("PreviewData/settings/config.toml");
            // Check the path before any read/write: a regression must never write official settings.
            assert_eq!(crate::path().unwrap(), expected);
            initialize().unwrap();
            let trial = crate::load::<Trial>("isolation_test").unwrap();
            if !trial.read().unwrap().name.is_empty() {
                assert_eq!(trial.read().unwrap().name, name);
            }
            trial.write().unwrap().name = name;
            trial.save().unwrap();
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let source = std::env::current_exe().unwrap();
        for name in ["preview-a", "preview-b", "preview-a"] {
            let folder = directory.path().join(name);
            fs::create_dir_all(&folder).unwrap();
            let executable = folder.join(source.file_name().unwrap());
            if !executable.exists() {
                fs::copy(&source, &executable).unwrap();
            }
            assert!(
                std::process::Command::new(executable)
                    .args([
                        "--exact",
                        "preview::tests::separate_executables_keep_separate_persistent_settings"
                    ])
                    .env(CHILD, name)
                    .status()
                    .unwrap()
                    .success()
            );
            let text = fs::read_to_string(folder.join("PreviewData/settings/config.toml")).unwrap();
            assert!(text.contains(name));
        }
    }

    #[test]
    fn preparation_keeps_legacy_data_untouched_and_fails_closed() {
        let directory = tempfile::tempdir().unwrap();
        let legacy = directory.path().join("Documents").join("Koharu");
        fs::create_dir_all(&legacy).unwrap();
        let original = legacy.join("original.khrproj");
        fs::write(&original, b"original project").unwrap();
        let root = directory.path().join("portable").join("PreviewData");
        prepare(&root).unwrap();
        assert!(root.join("projects").is_dir());
        assert!(root.join("settings").is_dir());
        assert!(!root.join(".write-check").exists());
        assert_eq!(fs::read(&original).unwrap(), b"original project");

        let blocked = directory.path().join("blocked");
        fs::write(&blocked, b"not a directory").unwrap();
        assert!(prepare(&blocked).is_err());
        assert_eq!(fs::read(&original).unwrap(), b"original project");
    }
}
