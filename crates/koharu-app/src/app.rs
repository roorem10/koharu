use anyhow::{Context as _, Result};
use tauri::{AppHandle, Manager as _, WindowEvent};
use tauri_runtime_cef::{Cef, CefRuntime};
use tokio::sync::Mutex;

use crate::commands::{
    agent::AgentState,
    canvas::CanvasChannel,
    lifecycle::{
        Download, DownloadChannel, DownloadState, Initialization, ModelResources, ProjectChannel,
        ResourceChannel,
    },
    processing::{JobChannel, Processing},
    project::{CurrentProject, ProjectLibrary},
};

pub(crate) async fn initialize(handle: AppHandle<CefRuntime>) -> Result<()> {
    koharu_ml::init()
        .await
        .context("failed to initialize the ML runtime")?;
    let device = koharu_ml::device(false);
    let pipeline = koharu_pipeline::Pipeline::load(device)?;
    handle.manage(pipeline.clone());

    let mut resources = pipeline.subscribe_resources();
    let resource_handle = handle.clone();
    drop(tauri::async_runtime::spawn(async move {
        while resources.changed().await.is_ok() {
            let snapshot = resources.borrow_and_update().clone();
            let resources = resource_handle.state::<ResourceChannel>();
            let mut channel = resources.channel.lock();
            if let Some(current) = channel.as_ref()
                && current.send(ModelResources::from(snapshot)).is_err()
            {
                channel.take();
            }
        }
    }));

    let project = handle
        .state::<CurrentProject>()
        .project
        .lock()
        .await
        .as_ref()
        .map(|project| (project.snapshot(), project.active_page()));
    let desktop = handle.state::<koharu_desktop::Desktop>();
    if let Some((snapshot, page)) = project {
        desktop.show_page(&snapshot, page).await?;
    } else {
        desktop.clear().await;
    }
    Ok(())
}

pub fn run(context: tauri::Context<CefRuntime>) -> Result<()> {
    if koharu_config::preview::enabled() {
        anyhow::ensure!(
            context.config().identifier == koharu_config::preview::IDENTIFIER,
            "private previews require their separate application identifier"
        );
    }
    let preview_root = koharu_config::preview::initialize()?;
    let cef = Cef::default();
    let cef = match &preview_root {
        Some(root) => cef
            .root_cache_path(root.join("browser"))
            .log_file(root.join("logs").join("cef.log")),
        None => cef,
    };
    let log_target = match &preview_root {
        Some(root) => tauri_plugin_log::TargetKind::Folder {
            path: root.join("logs"),
            file_name: Some("koharu-preview".into()),
        },
        None => tauri_plugin_log::TargetKind::LogDir { file_name: None },
    };
    let window_state = tauri_plugin_window_state::Builder::default().with_state_flags(
        tauri_plugin_window_state::StateFlags::SIZE
            | tauri_plugin_window_state::StateFlags::POSITION
            | tauri_plugin_window_state::StateFlags::MAXIMIZED
            | tauri_plugin_window_state::StateFlags::FULLSCREEN,
    );
    let window_state = match &preview_root {
        Some(root) => window_state.with_filename(
            root.join("settings")
                .join("window-state.json")
                .to_string_lossy()
                .into_owned(),
        ),
        None => window_state,
    };
    #[cfg(debug_assertions)]
    let cef = cef.remote_debugging(tauri_runtime_cef::RemoteDebugging::Port {
        port: 4000,
        allowed_origins: Vec::new(),
    });
    #[cfg(target_os = "linux")]
    let cef = cef
        .enable_features(["Vulkan", "VulkanFromANGLE"])
        .command_line_args([
            ("--enable-unsafe-webgpu", None),
            ("use-angle", Some("vulkan")),
            ("--ozone-platform", Some("x11")),
        ]);
    let builder = tauri::Builder::<CefRuntime>::new()
        .runtime(cef)
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(tauri_plugin_log::log::LevelFilter::Info)
                .max_file_size(1_000_000)
                .clear_targets()
                .target(tauri_plugin_log::Target::new(log_target))
                .build(),
        )
        .plugin(tauri_plugin_single_instance::init(|handle, _, _| {
            if let Some(window) = handle.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(window_state.build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(crate::commands::bindings().invoke_handler());
    let builder = if preview_root.is_some() {
        builder
    } else {
        builder.plugin(tauri_plugin_updater::Builder::new().build())
    };
    builder
        .setup(move |application| {
            if let Some(root) = &preview_root {
                koharu_runtime::Store::configure(root.join("packages"))?;
            }
            #[cfg(all(target_os = "windows", not(debug_assertions)))]
            if preview_root.is_none() {
                koharu_runtime::Store::configure(
                    application
                        .path()
                        .resource_dir()
                        .context("failed to locate Koharu's installation directory")?
                        .join("store"),
                )?;
            }

            application.manage(CurrentProject {
                project: Mutex::new(None),
            });
            application.manage(ProjectLibrary::new()?);
            application.manage(Processing::default());
            application.manage(CanvasChannel::default());
            application.manage(JobChannel::default());
            application.manage(DownloadChannel::default());
            application.manage(ResourceChannel::default());
            application.manage(ProjectChannel::default());
            application.manage(Initialization::default());

            let handle = application.handle().clone();
            application.manage(koharu_desktop::Desktop::new()?);
            application.manage(AgentState::new(handle.clone())?);

            let window_config = application
                .config()
                .app
                .windows
                .iter()
                .find(|window| window.label == "main")
                .context("the main Tauri window configuration is unavailable")?;
            let window = tauri::WebviewWindowBuilder::from_config(application, window_config)?;
            // Keep the window profile separate from CEF's user-data root.
            let window = match &preview_root {
                Some(root) => window.data_directory(root.join("browser").join("editor")),
                None => window,
            };
            let window = window.build().context("failed to create the main window")?;
            window.show().context("failed to show the main window")?;
            window
                .set_focus()
                .context("failed to focus the main window")?;
            let initialization_handle = handle.clone();
            drop(tauri::async_runtime::spawn(async move {
                initialize(initialization_handle.clone())
                    .await
                    .expect("failed to initialize the desktop runtime");
                initialization_handle.state::<Initialization>().ready();
            }));

            let mut downloads = koharu_runtime::download::subscribe();
            let download_handle = handle.clone();
            drop(tauri::async_runtime::spawn(async move {
                loop {
                    match downloads.recv().await {
                        Ok(event) => {
                            let download = match event {
                                koharu_runtime::download::Event::Started { id, name } => Download {
                                    id,
                                    state: DownloadState::Running,
                                    name: Some(name),
                                    completed: 0,
                                    total: 0,
                                    error: None,
                                },
                                koharu_runtime::download::Event::Progress {
                                    id,
                                    name,
                                    completed,
                                    total,
                                } => Download {
                                    id,
                                    state: DownloadState::Running,
                                    name: Some(name),
                                    completed,
                                    total,
                                    error: None,
                                },
                                koharu_runtime::download::Event::Finished { id } => Download {
                                    id,
                                    state: DownloadState::Finished,
                                    name: None,
                                    completed: 0,
                                    total: 0,
                                    error: None,
                                },
                                koharu_runtime::download::Event::Failed { id, name, error } => {
                                    Download {
                                        id,
                                        state: DownloadState::Failed,
                                        name: Some(name),
                                        completed: 0,
                                        total: 0,
                                        error: Some(error),
                                    }
                                }
                            };
                            let downloads = download_handle.state::<DownloadChannel>();
                            let mut channel = downloads.channel.lock();
                            if let Some(current) = channel.as_ref()
                                && current.send(download).is_err()
                            {
                                channel.take();
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(skipped, "download channel fell behind");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }));

            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(
                event,
                WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed
            ) {
                let processing = window.state::<Processing>();
                for stop in processing.stops.lock().values() {
                    stop.stop();
                }
                processing.stops.lock().clear();
                processing.jobs.lock().clear();
                window.state::<AgentState>().cancel_all();
            }
        })
        .run(context)?;
    Ok(())
}
