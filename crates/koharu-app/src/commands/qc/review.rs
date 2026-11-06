use anyhow::{Context as _, Result};
use booharu_context::{LabelSource, LineCategory, ProjectContext};
use koharu_scene::{EntityId, Snapshot};
use serde::Serialize;
use specta::Type;
use tauri::State;

use super::{
    QcCheck, QcFinding, QcReport,
    fix::{QcFixAction, auto_fixes},
};
use crate::commands::{
    Error,
    project::{CurrentProject, Project},
};

#[derive(Clone, Copy, Debug, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub(crate) enum QcReviewKind {
    Problem,
    AutoFixed,
    LetteringUnsure,
    // Future producers can join the same review list and acknowledgement workflow.
    SpeakerUnsure,
    OcrCorrected,
}

#[derive(Clone, Debug, Serialize, Type)]
pub(crate) struct QcReviewItem {
    pub finding: QcFinding,
    pub kind: QcReviewKind,
    pub previous_translation: Option<String>,
}

#[derive(Clone, Debug, Serialize, Type)]
pub(crate) struct QcFixRecord {
    pub reason: QcCheck,
    pub action: QcFixAction,
    pub previous_source: Option<String>,
    pub previous_translation: Option<String>,
}

fn unsure(label: Option<LineCategory>, threshold: f32) -> bool {
    label.is_some_and(|label| {
        label.source == LabelSource::Model
            && label
                .confidence
                .is_some_and(|confidence| confidence < threshold)
    })
}

pub(crate) fn history(snapshot: &Snapshot, layer: EntityId) -> Result<Vec<QcFixRecord>> {
    snapshot.text_layer(layer)?;
    Ok(auto_fixes(snapshot, layer)?
        .into_iter()
        .map(|record| QcFixRecord {
            reason: record.reason,
            action: record.action,
            previous_source: record.before.source.map(|source| source.text.value),
            previous_translation: record.before.translation.map(|text| text.text.value),
        })
        .collect())
}

pub(crate) fn list(project: &Project) -> Result<Vec<QcReviewItem>> {
    let snapshot = project.snapshot();
    let context = project.load_context()?;
    let report = super::cache::with_cache(&snapshot, project.directory(), |cache| {
        cache.report(&snapshot, &context)
    });
    let threshold = crate::commands::lettering::LetteringSettings::load()?
        .read()?
        .model_confidence;
    let markers = marker_items(&snapshot, &context, false, threshold)?;
    let mut items = Vec::new();
    // Page order follows the project, including pages with review markers but no open QC.
    for page in snapshot.pages() {
        items.extend(
            report
                .findings
                .iter()
                .filter(|finding| finding.page == page.id())
                .map(|finding| QcReviewItem {
                    finding: finding.clone(),
                    kind: QcReviewKind::Problem,
                    previous_translation: None,
                }),
        );
        items.extend(
            markers
                .iter()
                .filter(|item| item.finding.page == page.id())
                .cloned(),
        );
    }
    Ok(items)
}

pub(super) fn append_hidden(
    snapshot: &Snapshot,
    context: &ProjectContext,
    report: &mut QcReport,
) -> Result<()> {
    let threshold = crate::commands::lettering::LetteringSettings::load()?
        .read()?
        .model_confidence;
    report.hidden.extend(
        marker_items(snapshot, context, true, threshold)?
            .into_iter()
            .map(|item| item.finding),
    );
    Ok(())
}

fn marker_items(
    snapshot: &Snapshot,
    context: &ProjectContext,
    hidden: bool,
    threshold: f32,
) -> Result<Vec<QcReviewItem>> {
    let mut items = Vec::new();
    for page in snapshot.pages() {
        let label = page.page()?.label;
        for layer in page.text_layers()? {
            let translation = layer
                .content()?
                .translation()?
                .map(|text| text.text.value)
                .unwrap_or_default();
            if super::rules::is_hidden(context.lines.get(&layer.id()), QcCheck::AutoFixed) == hidden
                && let Some(record) = history(snapshot, layer.id())?.pop()
            {
                items.push(QcReviewItem {
                    finding: QcFinding {
                        page: page.id(),
                        page_label: label.clone(),
                        layer: layer.id(),
                        check: QcCheck::AutoFixed,
                        detail: serde_json::to_value(record.action)?
                            .as_str()
                            .context("QC fix action must be a string")?
                            .to_owned(),
                        translation: translation.clone(),
                        flag: None,
                    },
                    kind: QcReviewKind::AutoFixed,
                    previous_translation: record.previous_translation,
                });
            }
            if unsure(context.category(layer.id()), threshold)
                && super::rules::is_hidden(
                    context.lines.get(&layer.id()),
                    QcCheck::LetteringUnsure,
                ) == hidden
            {
                items.push(QcReviewItem {
                    finding: QcFinding {
                        page: page.id(),
                        page_label: label.clone(),
                        layer: layer.id(),
                        check: QcCheck::LetteringUnsure,
                        detail: context
                            .category(layer.id())
                            .expect("unsure category exists")
                            .value
                            .as_str()
                            .to_owned(),
                        translation,
                        flag: None,
                    },
                    kind: QcReviewKind::LetteringUnsure,
                    previous_translation: None,
                });
            }
        }
    }
    Ok(items)
}

pub(crate) fn acknowledge(
    project: &Project,
    layer: EntityId,
    check: QcCheck,
    flag: Option<&booharu_context::Flag>,
    ok: bool,
) -> Result<()> {
    anyhow::ensure!(
        (check == QcCheck::Flag) == flag.is_some(),
        "invalid QC finding"
    );
    project.snapshot().text_layer(layer)?;
    let id = super::acknowledgement(check, flag);
    project.update_context(|context: &mut ProjectContext| {
        let line = context.lines.entry(layer).or_default();
        line.qc_ok.retain(|entry| entry != &id);
        if ok {
            line.qc_ok.push(id);
        }
    })?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_qc_review(
    project: State<'_, CurrentProject>,
) -> std::result::Result<Vec<QcReviewItem>, Error> {
    let projects = project.project.lock().await;
    Ok(list(projects.as_ref().context("no project is open")?)?)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_qc_fix_history(
    layer: EntityId,
    project: State<'_, CurrentProject>,
) -> std::result::Result<Vec<QcFixRecord>, Error> {
    let projects = project.project.lock().await;
    let snapshot = projects.as_ref().context("no project is open")?.snapshot();
    Ok(history(&snapshot, layer)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use booharu_context::LetteringCategory;

    #[test]
    fn low_confidence_review_never_overrides_a_hand_chosen_label() {
        let label = LineCategory {
            value: LetteringCategory::Shout,
            source: LabelSource::Model,
            confidence: Some(0.4),
        };
        assert!(unsure(Some(label), 0.6));
        assert!(!unsure(
            Some(LineCategory { source: LabelSource::User, ..label }),
            0.6,
        ));
        assert!(!unsure(
            Some(LineCategory { confidence: Some(0.6), ..label }),
            0.6,
        ));
        assert!(!unsure(
            Some(LineCategory { confidence: None, ..label }),
            0.6,
        ));
        assert!(!unsure(None, 0.6));
    }

    #[tokio::test]
    async fn acknowledged_review_markers_remain_available_to_undo_ok() {
        let (_directory, project, _page, layer) = super::super::tests::chapter().await;
        project
            .update_context(|context| {
                context.set_model_category(layer, LetteringCategory::Shout, Some(0.0));
            })
            .unwrap();
        let markers = |hidden| {
            marker_items(
                &project.snapshot(),
                &project.load_context().unwrap(),
                hidden,
                0.6,
            )
            .unwrap()
        };
        assert!(markers(false).iter().any(|item| {
            item.finding.layer == layer && item.finding.check == QcCheck::LetteringUnsure
        }));
        acknowledge(&project, layer, QcCheck::LetteringUnsure, None, true).unwrap();
        assert!(markers(false).is_empty());
        assert!(markers(true).iter().any(|item| {
            item.finding.layer == layer && item.finding.check == QcCheck::LetteringUnsure
        }));
        acknowledge(&project, layer, QcCheck::LetteringUnsure, None, false).unwrap();
        assert!(markers(false).iter().any(|item| {
            item.finding.layer == layer && item.finding.check == QcCheck::LetteringUnsure
        }));
    }
}
