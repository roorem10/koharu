use super::*;
use koharu_scene::{
    At, Authored, Generation, PageDraft, ProducerId, Session, SourceText, StyleRun, TextLayout,
    TextLayoutKind,
};
use serde_json::json;

async fn fixture(text: &str) -> (Project, EntityId) {
    let mut session = Session::memory().await.unwrap();
    let mut target = None;
    let patch = session
        .snapshot()
        .patch(|edit| {
            let page = edit.add_page(PageDraft::new("page", 200.0, 200.0), At::End)?;
            let content = edit.add_text_content(page, At::End)?;
            edit.set(
                content,
                &SourceText {
                    text: Authored::user(text.into()),
                    language: None,
                },
            )?;
            target = Some(edit.add_text_layer(
                page,
                At::End,
                content,
                &TextLayout {
                    origin: Origin::User,
                    kind: TextLayoutKind::Paragraph,
                    angle_degrees: None,
                    warp: None,
                },
            )?);
            Ok(())
        })
        .unwrap();
    session.commit(patch).await.unwrap();
    (Project::new(session, "emphasis".into()), target.unwrap())
}

#[test]
fn matching_repeated_words_whole_words_cjk_and_unicode_offsets() {
    let words = ["NO", "NO", "missing", "NO"].map(str::to_owned);
    assert_eq!(
        runs::find_words("🙂 NO! KNOW NO.", &words),
        [(2, 4), (11, 13)]
    );
    assert_eq!(
        runs::find_words("魔王だ魔王", &["魔王".into(), "魔王".into()]),
        [(0, 2), (3, 5)]
    );
    assert_eq!(runs::find_words("cafe\u{301}", &["cafe".into()]), []);
    assert_eq!(
        runs::find_words("say NO now", &["NO now".into(), "NO".into()]),
        [(4, 10)]
    );
}

#[test]
fn overlapping_patches_split_merge_and_clear_without_losing_other_fields() {
    let bold = RunStyle {
        font_weight: Some(700),
        ..Default::default()
    };
    let red = RunStyle {
        color: Some([255, 0, 0, 255]),
        ..Default::default()
    };
    let original = vec![StyleRun {
        start: 0,
        end: 6,
        style: bold.clone(),
    }];
    let patched = runs::patched(&original, 2, 4, Some(&red));
    assert_eq!(patched.len(), 3);
    assert_eq!(patched[1].style.font_weight, Some(700));
    assert_eq!(patched[1].style.color, red.color);
    assert_eq!(runs::patched(&patched, 2, 4, None).len(), 2);
    assert_eq!(
        runs::patched(&original, 6, 8, Some(&bold)),
        vec![StyleRun {
            start: 0,
            end: 8,
            style: bold
        }]
    );
}

#[tokio::test]
async fn category_resolution_copies_fields_and_falls_back_to_default() {
    let (project, layer) = fixture("NO!").await;
    let snapshot = project.snapshot();
    let mut context = ProjectContext::default();
    context.set_user_category(layer, Some(booharu_context::LetteringCategory::Shout));
    let mut settings = EmphasisSettings::default();
    settings.categories.insert(
        "shout".into(),
        RunStyle {
            size_scale: Some(1.3),
            color: Some([255, 0, 0, 255]),
            ..Default::default()
        },
    );
    let style = category_style(&settings, &context, &snapshot, layer).unwrap();
    assert_eq!(style.font_weight, Some(700));
    assert_eq!(style.size_scale, Some(1.3));
    assert_eq!(style.color, Some([255, 0, 0, 255]));
    assert_eq!(
        category_style(&settings, &ProjectContext::default(), &snapshot, layer).unwrap(),
        settings.default
    );
}

#[tokio::test]
async fn hints_create_generated_runs_remove_the_component_and_honor_disable() {
    for enabled in [true, false] {
        let (mut project, layer) = fixture("NO! NO 魔王だ魔王").await;
        let generation =
            Generation::new(ProducerId::new("dev.koharu.pipeline.translation").unwrap());
        let snapshot = project.snapshot();
        let mut edit = snapshot.edit_as(generation.clone());
        edit.set(
            layer,
            &EmphasisHints {
                origin: Origin::Generated(generation.clone()),
                words: ["NO", "NO", "missing", "魔王", "魔王"]
                    .map(str::to_owned)
                    .to_vec(),
            },
        )
        .unwrap();
        project.commit(edit.finish().unwrap()).await.unwrap();
        let settings = EmphasisSettings {
            enabled_for_ai: enabled,
            ..Default::default()
        };
        let snapshot = project.snapshot();
        let patch = snapshot
            .patch(|edit| {
                // Mirrors the shared after_translation hook: runs are cleared first.
                edit.remove::<TextRuns>(layer)?;
                apply_hints_with(edit, &snapshot, &[layer], &settings, |_| {
                    Ok(settings.default.clone())
                })
            })
            .unwrap();
        project.commit(patch).await.unwrap();
        let snapshot = project.snapshot();
        assert!(
            snapshot
                .component::<EmphasisHints>(layer)
                .unwrap()
                .is_none()
        );
        let result = snapshot.component::<TextRuns>(layer).unwrap();
        if enabled {
            let result = result.unwrap();
            assert_eq!(result.origin, Origin::Generated(generation));
            assert_eq!(
                result
                    .runs
                    .iter()
                    .map(|r| (r.start, r.end))
                    .collect::<Vec<_>>(),
                [(0, 2), (4, 6), (7, 9), (10, 12)]
            );
            assert!(result.runs.iter().all(|run| run.style == settings.default));
        } else {
            assert!(result.is_none());
        }
    }
}

#[tokio::test]
async fn agent_by_text_range_emphasis_and_clear_are_one_undo_step_each() {
    let (mut project, layer) = fixture("🙂 NO! NO!").await;
    for args in [
        json!({"layer":layer.to_string(),"text":"NO","size_percent":130}),
        json!({"layer":layer.to_string(),"start":6,"end":8,"style":"emphasis"}),
        json!({"layer":layer.to_string(),"text":"NO","clear":true}),
    ] {
        let before = project.snapshot().component::<TextRuns>(layer).unwrap();
        let args: agent::StyleWords = serde_json::from_value(args).unwrap();
        let commit = project.style_words(args).await.unwrap();
        project.record_commit(&commit);
        let styled = project
            .snapshot()
            .component::<TextRuns>(layer)
            .unwrap()
            .unwrap();
        assert_eq!(styled.origin, Origin::User);
        project.undo().await.unwrap();
        assert_eq!(
            project.snapshot().component::<TextRuns>(layer).unwrap(),
            before
        );
        project.redo().await.unwrap();
    }
    let runs = project
        .snapshot()
        .component::<TextRuns>(layer)
        .unwrap()
        .unwrap()
        .runs;
    assert_eq!(runs.len(), 1);
    assert_eq!((runs[0].start, runs[0].end), (6, 8));
    assert_eq!(runs[0].style.font_weight, Some(700));
}

#[tokio::test]
async fn agent_rejects_ambiguous_invalid_ranges_and_values_without_changes() {
    let (mut project, layer) = fixture("NO!").await;
    let revision = project.snapshot().revision();
    for mut args in [
        json!({"text":"missing","bold":true}),
        json!({"text":"NO","start":0,"end":2,"bold":true}),
        json!({"start":1,"end":99,"bold":true}),
        json!({"start":2,"end":2,"clear":true}),
        json!({"text":"NO","style":"emphasis","bold":true}),
        json!({"text":"NO","size_percent":0}),
        json!({"text":"NO","clear":true,"bold":false}),
        json!({"text":"NO"}),
    ] {
        args["layer"] = json!(layer.to_string());
        assert!(
            project
                .style_words(serde_json::from_value(args).unwrap())
                .await
                .is_err()
        );
    }
    assert_eq!(project.snapshot().revision(), revision);
}
