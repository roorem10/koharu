//! Automatic review of scene snapshots; disposable results and durable sidecar acknowledgements.
mod cache;
mod rules;

use std::{
    collections::{BTreeMap, BTreeSet},
    hash::{Hash, Hasher},
};

use anyhow::{Context as _, Result};
use booharu_context::{Flag, LetteringCategory, ProjectContext};
use koharu_desktop::Desktop;
use koharu_renderer::{LayerKind, RenderDiagnostic};
use koharu_scene::{AssetRole, EntityId, OcrAnalysis, Origin, Snapshot};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{
    State,
    ipc::{Channel, IpcResponse},
};

use super::{
    Error,
    project::{CurrentProject, Project},
};
use rules::{font_majorities, has_leftover_source, is_hidden};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum QcCheck {
    Overflow,
    TooSmall,
    MissingTranslation,
    EmptySource,
    LeftoverSource,
    OddFont,
    Flag,
}

impl QcCheck {
    fn id(self) -> &'static str {
        match self {
            Self::Overflow => "overflow",
            Self::TooSmall => "too_small",
            Self::MissingTranslation => "missing_translation",
            Self::EmptySource => "empty_source",
            Self::LeftoverSource => "leftover_source",
            Self::OddFont => "odd_font",
            Self::Flag => "flag",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Type)]
pub(crate) struct QcFinding {
    pub page: EntityId,
    pub page_label: String,
    pub layer: EntityId,
    pub check: QcCheck,
    /// A measurement, font name, or the reviewer's message; labels are localized in the UI.
    pub detail: String,
    pub translation: String,
    pub flag: Option<Flag>,
}

#[derive(Clone, Copy, Debug, Deserialize, Type)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub(crate) enum QcScope {
    Page { page: EntityId },
    Project,
}

#[derive(Clone, Debug, Deserialize, Serialize, Type)]
pub(crate) struct QcReport {
    pub revision: koharu_scene::Revision,
    pub findings: Vec<QcFinding>,
    pub hidden: Vec<QcFinding>,
    pub pages_checked: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, Type)]
pub(crate) struct QcProgress {
    pub completed: u32,
    pub total: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, Type)]
#[serde(default)]
pub(crate) struct QcSettings {
    pub min_font_size: f32,
}

impl Default for QcSettings {
    fn default() -> Self {
        Self {
            min_font_size: 12.0,
        }
    }
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_qc_settings() -> std::result::Result<QcSettings, Error> {
    Ok(koharu_config::load::<QcSettings>("qc")?.read()?.clone())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn save_qc_settings(
    settings: QcSettings,
) -> std::result::Result<QcSettings, Error> {
    if !settings.min_font_size.is_finite() || settings.min_font_size <= 0.0 {
        return Err(anyhow::anyhow!("minimum font size must be positive and finite").into());
    }
    let config = koharu_config::load::<QcSettings>("qc")?;
    let mut value = config.write()?;
    *value = settings.clone();
    value.save()?;
    Ok(settings)
}

struct Row {
    page: EntityId,
    page_label: String,
    layer: EntityId,
    source: String,
    translation: String,
    detected: bool,
    ocr_checked: bool,
    category: Option<LetteringCategory>,
    font: Option<String>,
    user_font: bool,
}

fn rows(snapshot: &Snapshot, context: &ProjectContext) -> Result<Vec<Row>> {
    let mut rows = Vec::new();
    let lettering = super::lettering::LetteringSettings::load()?.read()?.clone();
    for page in snapshot.pages() {
        let label = page.page()?.label;
        for layer in page.text_layers()? {
            let content = layer.content()?;
            let source = content.source()?;
            let region = content.source_region()?;
            let typography = layer.typography()?;
            let ocr_checked = source.is_some()
                || match region {
                    Some(region) => snapshot.component::<OcrAnalysis>(region.id())?.is_some(),
                    None => false,
                };
            rows.push(Row {
                page: page.id(),
                page_label: label.clone(),
                layer: layer.id(),
                source: source.map(|source| source.text.value).unwrap_or_default(),
                translation: content
                    .translation()?
                    .map(|text| text.text.value)
                    .unwrap_or_default(),
                detected: region.is_some(),
                ocr_checked,
                category: Some(
                    super::lettering::resolve_category(
                        context,
                        snapshot,
                        layer.id(),
                        &koharu_pipeline::bubble_shape::text_bubble_shape(snapshot, layer.id())?,
                        &lettering,
                    )?
                    .category,
                ),
                font: typography
                    .as_ref()
                    .and_then(|value| value.preferred_font.clone()),
                user_font: typography.is_some_and(|value| matches!(value.origin, Origin::User)),
            });
        }
    }
    Ok(rows)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn run_qc(
    scope: QcScope,
    on_progress: Channel<QcProgress>,
    automatic: Option<bool>,
    project: State<'_, CurrentProject>,
    desktop: State<'_, Desktop>,
) -> std::result::Result<QcReport, Error> {
    run_checks(
        scope,
        Some(on_progress),
        automatic.unwrap_or(false),
        &project,
        &desktop,
    )
    .await
}

async fn run_checks(
    scope: QcScope,
    on_progress: Option<Channel<QcProgress>>,
    automatic: bool,
    project: &CurrentProject,
    desktop: &Desktop,
) -> std::result::Result<QcReport, Error> {
    let _checking = cache::CHECK.lock().await;
    let (snapshot, context, directory) = {
        let projects = project.project.lock().await;
        let project = projects.as_ref().context("no project is open")?;
        super::context::refresh_flags(project)?;
        (
            project.snapshot(),
            project.load_context()?,
            project.directory().map(std::path::Path::to_path_buf),
        )
    };
    let settings = koharu_config::load::<QcSettings>("qc")?.read()?.clone();
    let rows = rows(&snapshot, &context)?;
    let majorities = font_majorities(&rows);
    let skip = super::lettering::translation_skip(&context, &snapshot)?;
    let pages = match scope {
        QcScope::Page { page } => {
            snapshot.page(page)?;
            vec![page]
        }
        QcScope::Project => snapshot.pages().map(|page| page.id()).collect(),
    };
    let complete = matches!(scope, QcScope::Project);
    let total = pages.len() as u32;
    let renderer = desktop.renderer();
    for_progress(&on_progress, 0, total);
    for (index, page) in pages.into_iter().enumerate() {
        let qualified = !automatic || qualifies(&snapshot, page, &rows, &skip)?;
        let signature = signature(&snapshot, &context, page, &settings, &majorities, &skip)?;
        let unchanged = cache::with_cache(&snapshot, directory.as_deref(), |cache| {
            cache
                .pages
                .get(&page)
                .is_some_and(|entry| entry.signature == signature)
        });
        if !qualified || unchanged {
            if !qualified {
                cache::with_cache(&snapshot, directory.as_deref(), |cache| {
                    cache.pages.remove(&page);
                });
            }
            for_progress(&on_progress, index as u32 + 1, total);
            continue;
        }
        let mut report = QcReport {
            revision: snapshot.revision(),
            findings: Vec::new(),
            hidden: Vec::new(),
            pages_checked: 1,
        };
        // Release each frame before preparing the next page.
        let frame = renderer.render(&snapshot, page).await?;
        for row in rows.iter().filter(|row| row.page == page) {
            let mut checks = Vec::new();
            let mut small_size = None;
            for diagnostic in frame.diagnostics() {
                match diagnostic {
                    RenderDiagnostic::TextOverflow { entity, .. } if *entity == row.layer => {
                        checks.push((QcCheck::Overflow, String::new()));
                    }
                    RenderDiagnostic::TextBelowReadableSize {
                        entity, font_size, ..
                    } if *entity == row.layer => {
                        small_size = Some(*font_size);
                    }
                    _ => {}
                }
            }
            if let Some(layer) = frame.layer(row.layer)
                && let LayerKind::Text(text) = layer.kind()
                && text.font_size < settings.min_font_size
            {
                small_size = Some(text.font_size);
            }
            if let Some(size) = small_size
                && size < settings.min_font_size
            {
                checks.push((QcCheck::TooSmall, format!("{size:.1}")));
            }
            text_checks(row, &skip, &majorities, &mut checks);
            for (check, detail) in checks {
                push_finding(&mut report, &context, row, check, detail, None);
            }
            if let Some(line) = context.lines.get(&row.layer) {
                for flag in &line.flags {
                    let detail = match flag {
                        Flag::Review { message, .. } => message.clone(),
                        Flag::Glossary { term, .. } => term.clone(),
                    };
                    push_finding(
                        &mut report,
                        &context,
                        row,
                        QcCheck::Flag,
                        detail,
                        Some(flag.clone()),
                    );
                }
            }
        }
        drop(frame);
        {
            let projects = project.project.lock().await;
            let current = projects.as_ref().context("no project is open")?;
            if current.snapshot().project_id() != snapshot.project_id()
                || current.snapshot().revision() != snapshot.revision()
            {
                return Err(anyhow::anyhow!("project changed during QC").into());
            }
            let latest_context = current.load_context()?;
            if signature
                != self::signature(
                    &snapshot,
                    &latest_context,
                    page,
                    &settings,
                    &majorities,
                    &skip,
                )?
            {
                return Err(anyhow::anyhow!("context changed during QC").into());
            }
            report.findings.extend(report.hidden);
            cache::with_cache(&snapshot, directory.as_deref(), |cache| {
                cache.pages.insert(
                    page,
                    cache::PageResults {
                        signature,
                        findings: report.findings,
                    },
                );
                cache.save(directory.as_deref(), &snapshot, false);
            });
        }
        for_progress(&on_progress, index as u32 + 1, total);
        tokio::task::yield_now().await;
    }
    let projects = project.project.lock().await;
    let current = projects.as_ref().context("no project is open")?;
    if current.snapshot().project_id() != snapshot.project_id()
        || current.snapshot().revision() != snapshot.revision()
    {
        return Err(anyhow::anyhow!("project changed during QC").into());
    }
    let context = current.load_context()?;
    Ok(cache::with_cache(
        &snapshot,
        directory.as_deref(),
        |cache| {
            cache.save(directory.as_deref(), &snapshot, complete);
            cache.report(&snapshot, &context)
        },
    ))
}

fn for_progress(channel: &Option<Channel<QcProgress>>, completed: u32, total: u32) {
    if let Some(channel) = channel {
        let _ = channel.send(QcProgress { completed, total });
    }
}

fn qualifies(
    snapshot: &Snapshot,
    page: EntityId,
    rows: &[Row],
    skip: &BTreeSet<EntityId>,
) -> Result<bool> {
    let mut has_boxes = false;
    for row in rows.iter().filter(|row| row.page == page) {
        has_boxes = true;
        if !skip.contains(&row.layer) && row.translation.trim().is_empty() {
            return Ok(false);
        }
    }
    if !has_boxes {
        return Ok(false);
    }
    for entity in snapshot.descendants(page)? {
        if snapshot
            .component::<koharu_scene::RasterLayer>(entity.id())?
            .is_some_and(|raster| raster.kind == koharu_scene::RasterLayerKind::Cleanup)
            && snapshot
                .asset(entity.id(), &AssetRole::new("source")?)?
                .is_some()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn signature(
    snapshot: &Snapshot,
    context: &ProjectContext,
    page: EntityId,
    settings: &QcSettings,
    majorities: &BTreeMap<LetteringCategory, Option<String>>,
    skip: &BTreeSet<EntityId>,
) -> Result<u64> {
    let page_data = Project::page(snapshot, page)?;
    let mut lines = BTreeMap::new();
    let mut layouts = Vec::new();
    for layer in snapshot.page(page)?.text_layers()? {
        if let Some(line) = context.lines.get(&layer.id()) {
            let mut line = line.clone();
            line.qc_ok.clear();
            lines.insert(layer.id(), line);
        }
        layouts.push(snapshot.component::<koharu_scene::TextLayout>(layer.id())?);
    }
    let styles = koharu_config::load::<koharu_renderer::TextStyles>("text-styles")?
        .read()?
        .clone();
    let bytes = serde_json::to_vec(&(
        page_data, layouts, lines, settings, majorities, skip, styles,
    ))?;
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hash);
    Ok(hash.finish())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_qc_results(
    project: State<'_, CurrentProject>,
) -> std::result::Result<QcReport, Error> {
    let projects = project.project.lock().await;
    let project = projects.as_ref().context("no project is open")?;
    let snapshot = project.snapshot();
    let context = project.load_context()?;
    Ok(cache::with_cache(&snapshot, project.directory(), |cache| {
        cache.report(&snapshot, &context)
    }))
}

fn acknowledgement(check: QcCheck, flag: Option<&Flag>) -> String {
    match flag {
        Some(flag) => super::context::lines::flag_acknowledgement(flag),
        None => check.id().into(),
    }
}

fn finding_hidden(context: &ProjectContext, finding: &QcFinding) -> bool {
    if finding.check != QcCheck::Flag {
        return is_hidden(context.lines.get(&finding.layer), finding.check);
    }
    context.lines.get(&finding.layer).is_some_and(|line| {
        line.qc_ok
            .contains(&acknowledgement(finding.check, finding.flag.as_ref()))
    })
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn mark_qc_finding(
    layer: EntityId,
    check: QcCheck,
    flag: Option<Flag>,
    ok: bool,
    project: State<'_, CurrentProject>,
) -> std::result::Result<(), Error> {
    if (check == QcCheck::Flag) != flag.is_some() {
        return Err(anyhow::anyhow!("invalid QC finding").into());
    }
    let projects = project.project.lock().await;
    let project = projects.as_ref().context("no project is open")?;
    project.snapshot().text_layer(layer)?;
    let id = acknowledgement(check, flag.as_ref());
    project.update_context(|context| {
        let line = context.lines.entry(layer).or_default();
        line.qc_ok.retain(|entry| entry != &id);
        if ok {
            line.qc_ok.push(id);
        }
    })?;
    Ok(())
}

fn text_checks(
    row: &Row,
    skip: &BTreeSet<EntityId>,
    majorities: &BTreeMap<LetteringCategory, Option<String>>,
    checks: &mut Vec<(QcCheck, String)>,
) {
    if !row.source.trim().is_empty()
        && row.translation.trim().is_empty()
        && !skip.contains(&row.layer)
    {
        checks.push((QcCheck::MissingTranslation, String::new()));
    }
    if row.detected && row.ocr_checked && row.source.trim().is_empty() {
        checks.push((QcCheck::EmptySource, String::new()));
    }
    if has_leftover_source(&row.source, &row.translation) {
        checks.push((QcCheck::LeftoverSource, String::new()));
    }
    if !row.user_font
        && let Some(font) = row.category.and_then(|category| majorities.get(&category))
        && *font != row.font
    {
        checks.push((QcCheck::OddFont, font.clone().unwrap_or_default()));
    }
}

fn push_finding(
    report: &mut QcReport,
    context: &ProjectContext,
    row: &Row,
    check: QcCheck,
    detail: String,
    flag: Option<Flag>,
) {
    let finding = QcFinding {
        page: row.page,
        page_label: row.page_label.clone(),
        layer: row.layer,
        check,
        detail,
        translation: row.translation.clone(),
        flag,
    };
    if finding_hidden(context, &finding) {
        report.hidden.push(finding);
    } else {
        report.findings.push(finding);
    }
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn mark_qc_ok(
    layer: EntityId,
    check: QcCheck,
    ok: bool,
    project: State<'_, CurrentProject>,
) -> std::result::Result<(), Error> {
    if check == QcCheck::Flag {
        return Err(anyhow::anyhow!("review flags must use dismiss_line_flag").into());
    }
    let project = project.project.lock().await;
    let project = project.as_ref().context("no project is open")?;
    project.snapshot().text_layer(layer)?;
    project.update_context(|context| {
        let line = context.lines.entry(layer).or_default();
        line.qc_ok.retain(|id| id != check.id());
        if ok {
            line.qc_ok.push(check.id().to_owned());
        }
    })?;
    Ok(())
}

#[derive(Type)]
#[specta(transparent)]
pub(crate) struct SourceBytes(#[specta(type = Vec<u8>)] Vec<u8>);

impl IpcResponse for SourceBytes {
    fn body(self) -> tauri::Result<tauri::ipc::InvokeResponseBody> {
        Ok(self.0.into())
    }
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_page_source(
    page: EntityId,
    project: State<'_, CurrentProject>,
) -> std::result::Result<SourceBytes, Error> {
    let snapshot = project
        .project
        .lock()
        .await
        .as_ref()
        .context("no project is open")?
        .snapshot();
    snapshot.page(page)?;
    let blob = snapshot
        .asset(page, &AssetRole::new("source")?)?
        .context("page has no source image")?
        .blob;
    Ok(SourceBytes(snapshot.read_blob(blob).await?.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use koharu_scene::{
        AssetInput, AssetMetadata, At, Authored, Geometry, PageDraft, RasterLayer, RasterLayerKind,
        SourceText, TextLayout, TextLayoutKind, Translation,
    };
    use std::{io::Cursor, sync::Arc};

    async fn chapter() -> (tempfile::TempDir, Project, EntityId, EntityId) {
        let directory = tempfile::tempdir().unwrap();
        let mut project = Project::create(
            "QC fixture".into(),
            directory.path().join("chapter.khrproj"),
        )
        .await
        .unwrap();
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            256,
            256,
            image::Rgba([255, 255, 255, 255]),
        ))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
        let image = Arc::<[u8]>::from(png.into_inner());
        let mut ids = None;
        let patch = project
            .snapshot()
            .patch(|edit| {
                let page = edit.add_page(PageDraft::new("01.png", 256., 256.), At::End)?;
                let asset = || {
                    AssetInput::new(
                        image.clone(),
                        "image/png",
                        AssetMetadata {
                            width: Some(256),
                            height: Some(256),
                            attributes: Default::default(),
                        },
                    )
                };
                edit.set_asset(page, &AssetRole::new("source")?, asset())?;
                let content = edit.add_text_content(page, At::End)?;
                edit.set(
                    content,
                    &SourceText {
                        text: Authored::user("魔王".into()),
                        language: None,
                    },
                )?;
                edit.set(
                    content,
                    &Translation {
                        text: Authored::user("The king".into()),
                        language: None,
                    },
                )?;
                let layer = edit.add_text_layer(
                    page,
                    At::End,
                    content,
                    &TextLayout {
                        origin: Origin::User,
                        kind: TextLayoutKind::Point,
                        angle_degrees: None,
                        warp: None,
                    },
                )?;
                edit.set(layer, &Geometry::rectangle(8., 8., 200., 100.))?;
                let cleanup = edit.add_entity(page, At::Start)?;
                edit.set(
                    cleanup,
                    &RasterLayer {
                        origin: Origin::User,
                        name: "Cleanup".into(),
                        kind: RasterLayerKind::Cleanup,
                    },
                )?;
                edit.set_asset(cleanup, &AssetRole::new("source")?, asset())?;
                ids = Some((page, layer));
                Ok(())
            })
            .unwrap();
        project.commit(patch).await.unwrap();
        let (page, layer) = ids.unwrap();
        (directory, project, page, layer)
    }

    #[tokio::test]
    async fn automatic_eligibility_needs_boxes_translation_and_cleanup_and_respects_sfx_skip() {
        let (_directory, mut project, page, layer) = chapter().await;
        let snapshot = project.snapshot();
        let rows = rows(&snapshot, &project.load_context().unwrap()).unwrap();
        assert!(qualifies(&snapshot, page, &rows, &BTreeSet::new()).unwrap());
        project.set_translation(layer, None).await.unwrap();
        let snapshot = project.snapshot();
        let rows = self::rows(&snapshot, &project.load_context().unwrap()).unwrap();
        assert!(!qualifies(&snapshot, page, &rows, &BTreeSet::new()).unwrap());
        assert!(qualifies(&snapshot, page, &rows, &BTreeSet::from([layer])).unwrap());
        let cleanup = snapshot
            .descendants(page)
            .unwrap()
            .find(|entity| {
                snapshot
                    .component::<RasterLayer>(entity.id())
                    .unwrap()
                    .is_some()
            })
            .unwrap()
            .id();
        let patch = snapshot
            .patch(|edit| edit.remove_entity(cleanup, koharu_scene::RemovePolicy::Cascade))
            .unwrap();
        project.commit(patch).await.unwrap();
        assert!(!qualifies(&project.snapshot(), page, &rows, &BTreeSet::from([layer])).unwrap());
    }

    #[tokio::test]
    async fn qc_refreshes_glossary_without_check_glossary_and_keeps_acknowledged_flags_reversible()
    {
        let (_directory, project, _page, layer) = chapter().await;
        let mut series = booharu_context::Series::new("QC glossary fixture".into()).unwrap();
        series.glossary.push(booharu_context::GlossaryEntry {
            source: "魔王".into(),
            target: "Demon King".into(),
            locked: true,
            ..Default::default()
        });
        super::super::context::series::save_series(series.clone()).unwrap();
        project
            .update_context(|context| context.series = Some(series.id.clone()))
            .unwrap();
        let current = CurrentProject {
            project: tokio::sync::Mutex::new(Some(project)),
        };
        let desktop = Desktop::new().unwrap();
        let report = run_checks(QcScope::Project, None, true, &current, &desktop)
            .await
            .unwrap();
        let finding = report.findings.iter().find(|finding| matches!(&finding.flag, Some(Flag::Glossary { term, .. }) if term == "魔王")).unwrap();
        let id = acknowledgement(QcCheck::Flag, finding.flag.as_ref());
        {
            let projects = current.project.lock().await;
            projects
                .as_ref()
                .unwrap()
                .update_context(|context| {
                    context
                        .lines
                        .get_mut(&layer)
                        .unwrap()
                        .qc_ok
                        .push(id.clone())
                })
                .unwrap();
        }
        let hidden = run_checks(QcScope::Project, None, true, &current, &desktop)
            .await
            .unwrap();
        assert!(hidden.hidden.iter().any(|finding| finding.flag.is_some()));
        {
            let projects = current.project.lock().await;
            projects
                .as_ref()
                .unwrap()
                .update_context(|context| {
                    context
                        .lines
                        .get_mut(&layer)
                        .unwrap()
                        .qc_ok
                        .retain(|entry| entry != &id)
                })
                .unwrap();
        }
        let restored = run_checks(QcScope::Project, None, true, &current, &desktop)
            .await
            .unwrap();
        assert!(
            restored
                .findings
                .iter()
                .any(|finding| finding.flag.is_some())
        );
        super::super::context::series::delete_series(series.id).unwrap();
    }
}
