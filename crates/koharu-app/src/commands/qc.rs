//! Read-only review of one scene snapshot; acknowledgements live in the sidecar.
mod rules;

use std::collections::{BTreeMap, BTreeSet};

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

use super::{Error, project::CurrentProject};
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

#[derive(Clone, Debug, Serialize, Type)]
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

#[derive(Clone, Debug, Serialize, Type)]
pub(crate) struct QcReport {
    pub revision: koharu_scene::Revision,
    pub findings: Vec<QcFinding>,
    pub hidden: Vec<QcFinding>,
    pub pages_checked: u32,
}

#[derive(Clone, Debug, Serialize, Type)]
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
    project: State<'_, CurrentProject>,
    desktop: State<'_, Desktop>,
) -> std::result::Result<QcReport, Error> {
    let (snapshot, context) = {
        let project = project.project.lock().await;
        let project = project.as_ref().context("no project is open")?;
        (project.snapshot(), project.load_context()?)
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
    let total = pages.len() as u32;
    let mut report = QcReport {
        revision: snapshot.revision(),
        findings: Vec::new(),
        hidden: Vec::new(),
        pages_checked: total,
    };
    let renderer = desktop.renderer();
    let _ = on_progress.send(QcProgress {
        completed: 0,
        total,
    });
    for (index, page) in pages.into_iter().enumerate() {
        // The frame and its source pixels are released before preparing the next page.
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
            if let Some(size) = small_size {
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
        let _ = on_progress.send(QcProgress {
            completed: index as u32 + 1,
            total,
        });
    }
    Ok(report)
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
    if is_hidden(context.lines.get(&row.layer), check) {
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
