use super::*;
use booharu_context::SuggestedTerm;
use koharu_scene::{
    At, PageDraft, RunStyle, Session, SourceText, StyleRun, TextLayout, TextLayoutKind, TextRuns,
};

async fn fixture() -> (Project, EntityId, EntityId) {
    let mut session = Session::memory().await.unwrap();
    let mut ids = None;
    let patch = session
        .snapshot()
        .patch(|edit| {
            let page = edit.add_page(PageDraft::new("page", 100.0, 100.0), At::End)?;
            let content = edit.add_text_content(page, At::End)?;
            edit.set(
                content,
                &SourceText {
                    text: Authored::user("魔王リムル".into()),
                    language: None,
                },
            )?;
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
            ids = Some((layer, content));
            Ok(())
        })
        .unwrap();
    session.commit(patch).await.unwrap();
    let (layer, content) = ids.unwrap();
    let generation = Generation::new(ProducerId::new("dev.koharu.pipeline.translation").unwrap());
    let mut edit = session.snapshot().edit_as(generation.clone());
    edit.set(
        content,
        &Translation {
            text: Authored::generated("Demon Lord Rimuru".into(), generation),
            language: None,
        },
    )
    .unwrap();
    session.commit(edit.finish().unwrap()).await.unwrap();
    (Project::new(session, "fixture".into()), layer, content)
}

fn answer() -> LineAnswer {
    LineAnswer {
        best: "Demon King Rimuru".into(),
        explanation: "Use the established title.".into(),
        alternatives: vec![
            "Rimuru, the Demon King".into(),
            "The Demon King, Rimuru".into(),
        ],
        glossary: vec![SuggestedTerm {
            source: "魔王".into(),
            target: "Demon King".into(),
            kind: booharu_context::TermKind::Term,
            note: "title".into(),
        }],
    }
}

fn patch(project: &Project, answer: &LineAnswer) -> Patch {
    let snapshot = project.snapshot();
    let row = super::super::lines::text_rows(&snapshot).unwrap().remove(0);
    result_patch(
        &snapshot,
        &row,
        None,
        answer,
        &LanguageTag::new("en").unwrap(),
        "model".into(),
    )
    .unwrap()
}

#[tokio::test]
async fn wording_record_and_suggestions_are_one_undoable_change() {
    let (mut project, layer, content) = fixture().await;
    let change = patch(&project, &answer());
    let commit = project.commit(change).await.unwrap();
    project.record_commit(&commit);
    assert_eq!(project.undo.len(), 1);
    assert_eq!(
        project
            .snapshot()
            .component::<Translation>(content)
            .unwrap()
            .unwrap()
            .text
            .value,
        answer().best
    );
    let record = result(&project.snapshot(), layer).await.unwrap().unwrap();
    assert_eq!(record.previous, "Demon Lord Rimuru");
    assert_eq!(
        project.load_context().unwrap().suggestions[0].target,
        "Demon King"
    );
    project.undo().await.unwrap();
    assert_eq!(
        project
            .snapshot()
            .component::<Translation>(content)
            .unwrap()
            .unwrap()
            .text
            .value,
        "Demon Lord Rimuru"
    );
    assert!(result(&project.snapshot(), layer).await.unwrap().is_none());
    assert!(project.load_context().unwrap().suggestions.is_empty());
}

#[tokio::test]
async fn hand_edit_wins_before_request_during_request_and_on_rerun() {
    let (mut project, layer, content) = fixture().await;
    let derived = patch(&project, &answer());
    project
        .set_translation(layer, Some("My wording".into()))
        .await
        .unwrap();
    assert!(project.commit_rebased(derived).await.unwrap().is_none());
    let row = super::super::lines::text_rows(&project.snapshot())
        .unwrap()
        .remove(0);
    assert!(!eligible(&row, &SecondPassScope::Hard));
    assert!(!eligible(&row, &SecondPassScope::Puns));
    assert!(eligible(
        &row,
        &SecondPassScope::Boxes {
            layers: vec![layer]
        }
    ));
    // Explicit review still offers choices, while its patch preserves manual text.
    let change = patch(&project, &answer());
    let commit = project.commit(change).await.unwrap();
    project.record_commit(&commit);
    assert_eq!(
        project
            .snapshot()
            .component::<Translation>(content)
            .unwrap()
            .unwrap()
            .text,
        Authored::user("My wording".into())
    );
    assert!(result(&project.snapshot(), layer).await.unwrap().is_some());
    assert!(project.load_context().unwrap().suggestions.is_empty());
    project
        .set_translation(layer, Some(answer().best))
        .await
        .unwrap();
    assert_eq!(project.load_context().unwrap().suggestions.len(), 1);
}

#[tokio::test]
async fn settled_terms_replace_provisional_suggestions_survive_rerun_and_dismiss() {
    let (mut project, layer, _) = fixture().await;
    let provisional = GlossaryEntry {
        id: "provisional".into(),
        source: "魔王".into(),
        target: "Demon Lord".into(),
        ..Default::default()
    };
    let mut raw = ProjectContext {
        suggestions: vec![provisional.clone()],
        ..Default::default()
    };
    let change = patch(&project, &answer());
    let commit = project.commit(change).await.unwrap();
    project.record_commit(&commit);
    let mut visible = raw.clone();
    merge_suggestions(&project.snapshot(), &mut visible).unwrap();
    assert_eq!(visible.suggestions.len(), 1);
    assert_eq!(visible.suggestions[0].target, "Demon King");
    let id = visible.suggestions[0].id.clone();
    let mut again = answer();
    again.glossary.clear();
    let change = patch(&project, &again);
    project.commit(change).await.unwrap();
    assert_eq!(project.load_context().unwrap().suggestions[0].id, id);
    dismiss_suggestion(&mut raw, &id, Some("魔王"));
    merge_suggestions(&project.snapshot(), &mut raw).unwrap();
    assert!(raw.suggestions.is_empty());
    // Changing OCR invalidates review history and its suggestions.
    project
        .set_source_text(layer, "別の文".into())
        .await
        .unwrap();
    assert!(result(&project.snapshot(), layer).await.unwrap().is_none());
    assert!(project.load_context().unwrap().suggestions.is_empty());
}

#[tokio::test]
async fn missing_translation_is_eligible_for_qc_second_pass() {
    let (mut project, layer, content) = fixture().await;
    project.set_translation(layer, None).await.unwrap();
    let row = super::super::lines::text_rows(&project.snapshot())
        .unwrap()
        .remove(0);
    assert!(eligible(&row, &SecondPassScope::Hard));
    let change = patch(&project, &answer());
    project.commit(change).await.unwrap();
    assert_eq!(
        project
            .snapshot()
            .component::<Translation>(content)
            .unwrap()
            .unwrap()
            .text
            .value,
        answer().best
    );
}

#[tokio::test]
async fn word_style_remap_and_review_share_one_undo_and_redo_step() {
    let (mut project, layer, content) = fixture().await;
    let before = TextRuns {
        origin: Origin::User,
        runs: vec![StyleRun {
            start: 11,
            end: 17,
            style: RunStyle {
                font_weight: Some(700),
                ..Default::default()
            },
        }],
    };
    let style = project
        .snapshot()
        .patch(|edit| edit.set(layer, &before))
        .unwrap();
    project.commit(style).await.unwrap();
    let mut answer = answer();
    answer.best = "The Demon King Rimuru".into();
    let snapshot = project.snapshot();
    let change = patch(&project, &answer);
    let remap = snapshot
        .patch(|edit| {
            super::super::super::rich_text::on_text_edit(edit, &snapshot, layer, &answer.best)
        })
        .unwrap();
    assert!(!remap.is_empty());
    commit_result(&mut project, change, remap)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(project.undo.len(), 1);
    let after = project
        .snapshot()
        .component::<TextRuns>(layer)
        .unwrap()
        .unwrap();
    assert_eq!((after.runs[0].start, after.runs[0].end), (15, 21));
    assert_eq!(after.runs[0].style, before.runs[0].style);
    assert!(result(&project.snapshot(), layer).await.unwrap().is_some());
    assert_eq!(project.load_context().unwrap().suggestions.len(), 1);

    project.undo().await.unwrap();
    assert!(project.undo.is_empty());
    assert_eq!(
        project.snapshot().component::<TextRuns>(layer).unwrap(),
        Some(before)
    );
    assert_eq!(
        project
            .snapshot()
            .component::<Translation>(content)
            .unwrap()
            .unwrap()
            .text
            .value,
        "Demon Lord Rimuru"
    );
    assert!(result(&project.snapshot(), layer).await.unwrap().is_none());
    assert!(project.load_context().unwrap().suggestions.is_empty());

    project.redo().await.unwrap();
    assert_eq!(project.undo.len(), 1);
    assert_eq!(
        project.snapshot().component::<TextRuns>(layer).unwrap(),
        Some(after)
    );
    assert_eq!(
        project
            .snapshot()
            .component::<Translation>(content)
            .unwrap()
            .unwrap()
            .text
            .value,
        answer.best
    );
    assert!(result(&project.snapshot(), layer).await.unwrap().is_some());
    assert_eq!(project.load_context().unwrap().suggestions.len(), 1);
}
