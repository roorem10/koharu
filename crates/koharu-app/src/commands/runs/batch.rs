use std::{collections::BTreeSet, path::PathBuf, sync::Arc};

use anyhow::{Context as _, Result};
use koharu_pipeline::{Operation, StopToken};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager as _, WebviewWindow, ipc::Channel};
use tauri_runtime_cef::CefRuntime;
use uuid::Uuid;

use super::{RunSettings, require};
use crate::commands::{
    Error, import, lifecycle,
    processing::{JobId, JobState, Processing},
    project::{CurrentProject, ProjectLibrary},
};

#[derive(Clone, Serialize, Type)]
pub struct BatchChapter {
    pub id: Uuid,
    pub name: String,
    #[specta(type = f64)]
    pub pages: usize,
    pub error: Option<String>,
}

#[derive(Clone, Serialize, Type)]
pub struct BatchPlan {
    pub id: Uuid,
    pub chapters: Vec<BatchChapter>,
}

#[derive(Clone)]
struct ChapterInput {
    chapter: BatchChapter,
    paths: Vec<PathBuf>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ChapterStatus {
    Waiting,
    Running,
    Done,
    DoneWithProblems,
    Failed,
    Stopped,
}

#[derive(Clone, Serialize, Type)]
pub struct BatchChapterProgress {
    pub chapter: BatchChapter,
    pub state: ChapterStatus,
    pub failures: Vec<koharu_pipeline::PageFailure>,
    pub error: Option<String>,
}

#[derive(Clone, Serialize, Type)]
pub struct BatchProgress {
    pub chapters: Vec<BatchChapterProgress>,
    pub running: bool,
    pub stopped: bool,
    pub finish_sound: bool,
}

#[derive(Default)]
pub(crate) struct BatchState {
    plan: Mutex<Option<(Uuid, Vec<ChapterInput>)>>,
    control: Mutex<Option<Arc<BatchControl>>>,
}

#[derive(Default)]
struct BatchControl {
    stop: StopToken,
    job: Mutex<Option<JobId>>,
}

impl BatchState {
    pub(crate) fn active(&self) -> bool {
        self.control.lock().is_some()
    }
}

fn unique_name(name: &str, used: &mut BTreeSet<String>) -> String {
    let mut next = name.to_owned();
    let mut number = 2;
    while !used.insert(next.to_lowercase()) {
        next = format!("{name} ({number})");
        number += 1;
    }
    next
}

fn scan(folder: PathBuf, mut used: BTreeSet<String>) -> Result<Vec<ChapterInput>> {
    let mut entries = std::fs::read_dir(&folder)?
        .map(|entry| Ok(entry?.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    alphanumeric_sort::sort_slice_by_os_str_key(&mut entries, |path| path.as_os_str());
    let mut chapters = Vec::new();
    for entry in entries {
        let kind = std::fs::symlink_metadata(&entry)?.file_type();
        let path = entry;
        let name = if kind.is_dir() {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        } else if kind.is_file()
            && path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
                matches!(
                    s.parse::<import::Format>(),
                    Ok(import::Format::Zip | import::Format::Rar | import::Format::Pdf)
                )
            })
        {
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        } else {
            continue;
        };
        let paths = if kind.is_dir() {
            let Ok(selection) = import::collect_paths(vec![path], None) else {
                continue;
            };
            let paths: Vec<_> = selection
                .paths
                .into_iter()
                .filter(|path| {
                    path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
                        matches!(s.parse::<import::Format>(), Ok(import::Format::Raster))
                    })
                })
                .collect();
            if paths.is_empty() {
                continue;
            }
            paths
        } else {
            vec![path]
        };
        // Use the actual importer for counts, including PDF and archive filtering.
        // Only one chapter's decoded pages are held during preview.
        let count = import::import(paths.clone()).map(|pages| pages.len());
        let (pages, error) = match count {
            Ok(pages) => (pages, None),
            Err(error) => (0, Some(format!("{error:#}"))),
        };
        chapters.push(ChapterInput {
            chapter: BatchChapter {
                id: Uuid::new_v4(),
                name: unique_name(&name, &mut used),
                pages,
                error,
            },
            paths,
        });
    }
    Ok(chapters)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn preview_chapter_batch(
    window: WebviewWindow<CefRuntime>,
    handle: AppHandle<CefRuntime>,
) -> std::result::Result<Option<BatchPlan>, Error> {
    require(
        !handle.state::<BatchState>().active(),
        "a chapter batch is already running",
    )?;
    let Some(folder) = rfd::AsyncFileDialog::new()
        .set_parent(&window)
        .pick_folder()
        .await
    else {
        return Ok(None);
    };
    let used = handle
        .state::<ProjectLibrary>()
        .list()?
        .into_iter()
        .map(|project| project.name.to_lowercase())
        .collect();
    let folder = folder.path().to_path_buf();
    let chapters = tokio::task::spawn_blocking(move || scan(folder, used))
        .await
        .context("chapter scan failed")??;
    require(
        !handle.state::<BatchState>().active(),
        "a chapter batch started while scanning",
    )?;
    let id = Uuid::new_v4();
    let plan = BatchPlan {
        id,
        chapters: chapters.iter().map(|input| input.chapter.clone()).collect(),
    };
    *handle.state::<BatchState>().plan.lock() = Some((id, chapters));
    Ok(Some(plan))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn run_chapter_batch(
    plan: Uuid,
    chapters: Vec<Uuid>,
    preset: u32,
    close_project: Option<String>,
    on_progress: Channel<BatchProgress>,
    handle: AppHandle<CefRuntime>,
) -> std::result::Result<(), Error> {
    let settings = RunSettings::load()?;
    let stages = settings
        .presets
        .get(preset as usize)
        .context("choose a run preset")?
        .stages
        .clone();
    require(
        handle.state::<Processing>().stops.lock().is_empty(),
        "processing is already running",
    )?;
    let inputs = {
        let state = handle.state::<BatchState>();
        let saved = state.plan.lock();
        let (saved_id, inputs) = saved.as_ref().context("pick a batch folder first")?;
        require(
            *saved_id == plan,
            "the batch preview changed; pick the chapters again",
        )?;
        require(
            chapters
                .iter()
                .all(|id| inputs.iter().any(|input| input.chapter.id == *id)),
            "unknown batch chapter",
        )?;
        inputs
            .iter()
            .filter(|input| chapters.contains(&input.chapter.id))
            .cloned()
            .collect::<Vec<_>>()
    };
    require(
        !inputs.is_empty()
            && inputs
                .iter()
                .all(|input| input.chapter.error.is_none() && input.chapter.pages > 0),
        "select chapters with readable pages",
    )?;
    {
        let current = handle.state::<CurrentProject>();
        let current = current.project.lock().await;
        if let Some(current) = current.as_ref() {
            require(
                close_project.as_deref() == Some(current.name.as_str()),
                "close the open project before starting the batch",
            )?;
        }
    }
    let control = Arc::new(BatchControl::default());
    {
        let processing = handle.state::<Processing>();
        let stops = processing.stops.lock();
        require(stops.is_empty(), "processing is already running")?;
        let state = handle.state::<BatchState>();
        let mut active = state.control.lock();
        require(active.is_none(), "a chapter batch is already running")?;
        *active = Some(control.clone());
    }
    let mut progress = BatchProgress {
        chapters: inputs
            .iter()
            .map(|input| BatchChapterProgress {
                chapter: input.chapter.clone(),
                state: ChapterStatus::Waiting,
                failures: vec![],
                error: None,
            })
            .collect(),
        running: true,
        stopped: false,
        finish_sound: settings.finish_sound,
    };
    let _ = on_progress.send(progress.clone());
    drop(tokio::spawn(async move {
        if let Err(error) = lifecycle::close_project(handle.clone()).await {
            progress.chapters[0].state = ChapterStatus::Failed;
            progress.chapters[0].error = Some(format!("{error:?}"));
        } else {
            for (index, input) in inputs.into_iter().enumerate() {
                if control.stop.stopped() {
                    break;
                }
                progress.chapters[index].state = ChapterStatus::Running;
                let _ = on_progress.send(progress.clone());
                let result = run_chapter(&handle, input, &control, &stages).await;
                let chapter = &mut progress.chapters[index];
                match result {
                    Ok((name, job)) => {
                        chapter.chapter.name = name;
                        chapter.failures = job.failures;
                        chapter.error = job.error;
                        chapter.state = match job.state {
                            JobState::Finished => ChapterStatus::Done,
                            JobState::FinishedWithProblems => ChapterStatus::DoneWithProblems,
                            JobState::Stopped => {
                                control.stop.stop();
                                ChapterStatus::Stopped
                            }
                            _ => ChapterStatus::Failed,
                        };
                    }
                    Err(error) => {
                        chapter.state = if control.stop.stopped() {
                            ChapterStatus::Stopped
                        } else {
                            ChapterStatus::Failed
                        };
                        chapter.error = Some(format!("{error:#}"));
                    }
                }
                let _ = on_progress.send(progress.clone());
            }
        }
        progress.running = false;
        progress.stopped = control.stop.stopped();
        handle.state::<BatchState>().control.lock().take();
        let _ = on_progress.send(progress);
    }));
    Ok(())
}

async fn run_chapter(
    handle: &AppHandle<CefRuntime>,
    input: ChapterInput,
    control: &BatchControl,
    stages: &[koharu_pipeline::Stage],
) -> Result<(String, super::super::processing::Job)> {
    let current = handle.state::<CurrentProject>();
    if current.project.lock().await.is_some() {
        control.stop.stop();
        anyhow::bail!("the active project changed; the batch was stopped");
    }
    let mut used = handle
        .state::<ProjectLibrary>()
        .list()?
        .into_iter()
        .map(|project| project.name.to_lowercase())
        .collect();
    let name = unique_name(&input.chapter.name, &mut used);
    lifecycle::create_project(name.clone(), handle.clone())
        .await
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let expected = current
        .project
        .lock()
        .await
        .as_ref()
        .context("no batch project is open")?
        .snapshot()
        .project_id();
    let result = async {
        // This uses the same collector and importer as File > Import.
        let pages = tokio::task::spawn_blocking(move || import::import(input.paths))
            .await
            .context("chapter import task failed")??;
        {
            let mut project = current.project.lock().await;
            let project = project.as_mut().context("the batch project was closed")?;
            require(
                project.snapshot().project_id() == expected,
                "the batch project changed",
            )?;
            let commit = project.import_pages(pages).await?;
            project.record_commit(&commit);
            project.reconcile_page();
        }
        if control.stop.stopped() {
            anyhow::bail!("batch stopped after import");
        }
        let id = super::super::processing::start_batch_job(
            handle.clone(),
            Operation::Stages {
                stages: stages.to_vec(),
            },
        )
        .await?;
        *control.job.lock() = Some(id);
        loop {
            let project = current.project.lock().await;
            if !project
                .as_ref()
                .is_some_and(|project| project.snapshot().project_id() == expected)
            {
                control.stop.stop();
                anyhow::bail!("the active project changed; the batch was stopped");
            }
            drop(project);
            let processing = handle.state::<Processing>();
            if control.stop.stopped() {
                stop_at_page_boundary(&processing, id);
            }
            let job = processing
                .jobs
                .lock()
                .get(&id)
                .cloned()
                .context("batch job is no longer available")?;
            if job.state != JobState::Running {
                break Ok(job);
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
    .await;
    control.job.lock().take();
    let still_owned = current
        .project
        .lock()
        .await
        .as_ref()
        .is_some_and(|project| project.snapshot().project_id() == expected);
    if still_owned {
        lifecycle::close_project(handle.clone())
            .await
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    } else {
        control.stop.stop();
    }
    result.map(|job| (name, job))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn stop_chapter_batch(
    handle: AppHandle<CefRuntime>,
) -> std::result::Result<(), Error> {
    let control = handle
        .state::<BatchState>()
        .control
        .lock()
        .clone()
        .context("no chapter batch is running")?;
    control.stop.stop();
    if let Some(job) = *control.job.lock() {
        stop_at_page_boundary(&handle.state::<Processing>(), job);
    }
    Ok(())
}

fn stop_at_page_boundary(processing: &Processing, job: JobId) {
    let idle = processing.jobs.lock().get(&job).is_some_and(|job| {
        job.retry.is_some() || job.model.as_deref() == Some("chapter_context_reading")
    });
    if let Some(stop) = processing.stops.lock().get(&job) {
        if idle {
            stop.stop();
        } else {
            stop.finish_current_page();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    #[test]
    fn scans_three_folders_and_archive_naturally_and_reserves_unique_names() {
        let directory = tempfile::tempdir().unwrap();
        let image = image::DynamicImage::new_rgb8(1, 1);
        let mut png = std::io::Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png).unwrap();
        for name in ["Chapter 10", "Chapter 2", "Chapter 1"] {
            let chapter = directory.path().join(name);
            std::fs::create_dir(&chapter).unwrap();
            std::fs::write(chapter.join("001.png"), png.get_ref()).unwrap();
        }
        let archive = std::fs::File::create(directory.path().join("Chapter 3.cbz")).unwrap();
        let mut archive = zip::ZipWriter::new(archive);
        archive
            .start_file("001.png", zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(png.get_ref()).unwrap();
        archive.finish().unwrap();
        std::fs::write(directory.path().join("Chapter 4.rar"), b"bad archive").unwrap();
        std::fs::write(directory.path().join("ignored.png"), png.get_ref()).unwrap();
        let chapters = scan(
            directory.path().to_owned(),
            ["chapter 1".into(), "chapter 1 (2)".into()].into(),
        )
        .unwrap();
        assert_eq!(
            chapters
                .iter()
                .map(|input| input.chapter.name.as_str())
                .collect::<Vec<_>>(),
            [
                "Chapter 1 (3)",
                "Chapter 2",
                "Chapter 3",
                "Chapter 4",
                "Chapter 10"
            ]
        );
        assert_eq!(
            chapters
                .iter()
                .map(|input| input.chapter.pages)
                .collect::<Vec<_>>(),
            [1, 1, 1, 0, 1]
        );
        assert!(chapters[3].chapter.error.is_some());
        assert!(
            chapters
                .iter()
                .enumerate()
                .all(|(index, input)| index == 3 || input.chapter.error.is_none())
        );
    }
}
