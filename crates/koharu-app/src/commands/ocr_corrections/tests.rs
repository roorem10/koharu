use super::*;
use koharu_scene::{
    At, Geometry, LanguageTag, OcrAnalysis, PageDraft, RecognizedFrom, Session, TextDirection,
    TextLayout, TextLayoutKind, TextRegion, Translation,
};

fn correction() -> OcrCorrection {
    OcrCorrection {
        original: "未だ".into(),
        suggested: "末だ".into(),
        reason: "The upper stroke is longer in the image.".into(),
        dismissed: false,
    }
}

fn source(text: &str, producer: &str) -> SourceText {
    SourceText {
        text: Authored::generated(
            text.into(),
            Generation::new(ProducerId::new(producer).unwrap()),
        ),
        language: Some(LanguageTag::new("ja").unwrap()),
    }
}

#[test]
fn hand_edits_always_win_even_when_the_value_matches_the_observed_ocr() {
    let correction = correction();
    for value in [
        correction.original.as_str(),
        correction.suggested.as_str(),
        "hand edit",
    ] {
        let source = SourceText {
            text: Authored::user(value.to_owned()),
            language: None,
        };
        assert_eq!(
            correction_status(Some(&source), &correction),
            OcrCorrectionStatus::HandEdited
        );
    }
    assert_eq!(
        correction_status(Some(&source(&correction.original, OCR_PRODUCER)), &correction),
        OcrCorrectionStatus::Pending
    );
    assert_eq!(
        correction_status(Some(&source("new OCR", OCR_PRODUCER)), &correction),
        OcrCorrectionStatus::Stale
    );
    assert_eq!(
        correction_status(Some(&source(&correction.original, "test.other")), &correction),
        OcrCorrectionStatus::Stale
    );
    let mut dismissed = correction.clone();
    dismissed.dismissed = true;
    assert_eq!(
        correction_status(Some(&source(&correction.original, OCR_PRODUCER)), &dismissed),
        OcrCorrectionStatus::Dismissed
    );
}

async fn fixture(
    directory: &std::path::Path,
) -> (Project, Vec<(EntityId, EntityId, EntityId, EntityId)>) {
    let mut session = Session::memory().await.unwrap();
    let mut rows = Vec::new();
    let patch = session
        .snapshot()
        .patch(|edit| {
            for page_number in 0..2 {
                let page = edit.add_page(
                    PageDraft::new(format!("page {page_number}"), 200., 200.),
                    At::End,
                )?;
                for _ in 0..2 {
                    let region = edit.add_analysis_region::<TextRegion>(
                        page,
                        At::End,
                        &Geometry::rectangle(10., 10., 40., 80.),
                        None,
                    )?;
                    let content = edit.add_text_content(page, At::End)?;
                    edit.relate::<RecognizedFrom>(content, region)?;
                    let layer = edit.add_text_layer(
                        page,
                        At::End,
                        content,
                        &TextLayout {
                            origin: Origin::User,
                            kind: TextLayoutKind::Paragraph,
                            angle_degrees: None,
                            warp: None,
                        },
                    )?;
                    rows.push((page, layer, content, region));
                }
            }
            Ok(())
        })
        .unwrap();
    session.commit(patch).await.unwrap();
    let generation = Generation::new(ProducerId::new(OCR_PRODUCER).unwrap());
    let mut edit = session.snapshot().edit_as(generation.clone());
    for (_, _, content, region) in &rows {
        edit.set(*content, &source(&correction().original, OCR_PRODUCER))
            .unwrap();
        edit.set(
            *region,
            &OcrAnalysis {
                origin: Origin::Generated(generation.clone()),
                direction: TextDirection::Vertical,
                confidence: None,
                line_boundaries: Vec::new(),
            },
        )
        .unwrap();
    }
    session.commit(edit.finish().unwrap()).await.unwrap();
    let patch = session
        .snapshot()
        .patch(|edit| {
            for (_, _, content, _) in &rows {
                edit.set(
                    *content,
                    &Translation {
                        text: Authored::user("old translation".into()),
                        language: Some(LanguageTag::new("en").unwrap()),
                    },
                )?;
            }
            Ok(())
        })
        .unwrap();
    session.commit(patch).await.unwrap();
    let mut project = Project::new(session, "corrections".into());
    project.directory = Some(directory.to_path_buf());
    project
        .update_context(|context| {
            for (page, layer, _, _) in &rows {
                let line = context.lines.entry(*layer).or_default();
                line.ocr_correction = Some(correction());
                line.other.insert(
                    EVIDENCE.into(),
                    evidence(&project.snapshot(), *layer, *page).unwrap(),
                );
            }
        })
        .unwrap();
    (project, rows)
}

#[tokio::test]
async fn changed_recognition_region_rejects_an_old_suggestion() {
    let directory = tempfile::tempdir().unwrap();
    let (mut project, rows) = fixture(directory.path()).await;
    let (page, layer, content, region) = rows[0];
    let source = project.snapshot().component::<SourceText>(content).unwrap();
    let patch = project
        .snapshot()
        .patch(|edit| edit.set(region, &Geometry::rectangle(40., 40., 50., 90.)))
        .unwrap();
    project.commit(patch).await.unwrap();
    let report = suggestions(&project, &Scope::Entities(vec![layer])).unwrap();
    assert_eq!(report.suggestions[0].status, OcrCorrectionStatus::Stale);
    assert!(
        apply_page(&mut project, page, &[layer])
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        project.snapshot().component::<SourceText>(content).unwrap(),
        source
    );
    assert!(project.undo.is_empty());
}

#[tokio::test]
async fn each_page_is_one_undo_step_preserving_analysis_translation_and_original() {
    let directory = tempfile::tempdir().unwrap();
    let (mut project, rows) = fixture(directory.path()).await;
    let before = project.snapshot();
    for page in [rows[0].0, rows[2].0] {
        let layers: Vec<_> = rows
            .iter()
            .filter(|row| row.0 == page)
            .map(|row| row.1)
            .collect();
        let (_, count) = apply_page(&mut project, page, &layers).await.unwrap().unwrap();
        assert_eq!(count, 2);
        assert!(apply_page(&mut project, page, &layers).await.unwrap().is_none());
    }
    assert_eq!(project.undo.len(), 2);
    let report = suggestions(&project, &Scope::Project).unwrap();
    assert!(
        report
            .suggestions
            .iter()
            .all(|row| row.status == OcrCorrectionStatus::Applied)
    );
    for (_, layer, content, region) in &rows {
        let after = project.snapshot();
        assert_eq!(
            after.component::<OcrAnalysis>(*region).unwrap(),
            before.component::<OcrAnalysis>(*region).unwrap()
        );
        assert_eq!(
            after.component::<Translation>(*content).unwrap(),
            before.component::<Translation>(*content).unwrap()
        );
        assert_eq!(
            after.component::<SourceText>(*content).unwrap().unwrap().language,
            Some(LanguageTag::new("ja").unwrap())
        );
        assert_eq!(
            after
                .text_layer(*layer)
                .unwrap()
                .content()
                .unwrap()
                .source_region()
                .unwrap()
                .unwrap()
                .id(),
            *region
        );
        assert_eq!(
            project.load_context().unwrap().lines[layer]
                .ocr_correction
                .as_ref()
                .unwrap()
                .original,
            correction().original
        );
    }
    let mut context = project.load_context().unwrap();
    refresh_translation_flags(&project.snapshot(), &mut context).unwrap();
    assert!(
        context.lines[&rows[0].1]
            .flags
            .iter()
            .any(|flag| matches!(flag, Flag::OcrSourceChanged { .. }))
    );
    project.undo().await.unwrap();
    refresh_translation_flags(&project.snapshot(), &mut context).unwrap();
    assert!(context.lines[&rows[2].1].flags.is_empty());
    for row in &rows[2..] {
        assert_eq!(
            project.snapshot().component::<SourceText>(row.2).unwrap(),
            before.component::<SourceText>(row.2).unwrap()
        );
    }
    project.redo().await.unwrap();
    refresh_translation_flags(&project.snapshot(), &mut context).unwrap();
    assert!(
        context.lines[&rows[2].1]
            .flags
            .iter()
            .any(|flag| matches!(flag, Flag::OcrSourceChanged { .. }))
    );
    project
        .set_translation(rows[2].1, Some("new translation".into()))
        .await
        .unwrap();
    refresh_translation_flags(&project.snapshot(), &mut context).unwrap();
    assert!(context.lines[&rows[2].1].flags.is_empty());
}

#[tokio::test]
async fn dismissed_translation_review_stays_dismissed_until_the_values_change() {
    let directory = tempfile::tempdir().unwrap();
    let (mut project, rows) = fixture(directory.path()).await;
    let (page, layer, _, _) = rows[0];
    apply_page(&mut project, page, &[layer])
        .await
        .unwrap()
        .unwrap();
    let mut context = project.load_context().unwrap();
    refresh_translation_flags(&project.snapshot(), &mut context).unwrap();
    assert!(
        context.lines[&layer]
            .flags
            .iter()
            .any(|flag| matches!(flag, Flag::OcrSourceChanged { .. }))
    );
    context
        .lines
        .get_mut(&layer)
        .unwrap()
        .dismissed
        .push(STALE_DISMISSED.into());
    for _ in 0..2 {
        refresh_translation_flags(&project.snapshot(), &mut context).unwrap();
        assert!(context.lines[&layer].flags.is_empty());
        assert!(
            context.lines[&layer]
                .dismissed
                .iter()
                .any(|value| value == STALE_DISMISSED)
        );
    }
    project
        .set_translation(layer, Some("reviewed translation".into()))
        .await
        .unwrap();
    refresh_translation_flags(&project.snapshot(), &mut context).unwrap();
    assert!(context.lines[&layer].dismissed.is_empty());
}

#[tokio::test]
async fn selection_and_late_hand_edits_cannot_apply_other_corrections() {
    let directory = tempfile::tempdir().unwrap();
    let (mut project, rows) = fixture(directory.path()).await;
    project
        .set_source_text(rows[0].1, correction().original)
        .await
        .unwrap();
    let before = project.snapshot();
    assert!(
        apply_page(&mut project, rows[0].0, &[rows[0].1])
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(project.snapshot().revision(), before.revision());
    let report = suggestions(&project, &Scope::Entities(vec![rows[1].1])).unwrap();
    assert_eq!(report.suggestions.len(), 1);
    assert_eq!(report.suggestions[0].layer, rows[1].1);
    assert!(suggestions(&project, &Scope::Entities(vec![EntityId::new()])).is_err());
    let (_, count) = apply_page(&mut project, rows[0].0, &[rows[1].1, rows[2].1])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(
        project.snapshot().component::<SourceText>(rows[0].2).unwrap(),
        before.component::<SourceText>(rows[0].2).unwrap()
    );
    assert_eq!(
        project.snapshot().component::<SourceText>(rows[2].2).unwrap(),
        before.component::<SourceText>(rows[2].2).unwrap()
    );
}
