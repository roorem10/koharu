use super::*;
use koharu_scene::{At, Origin, PageDraft, Session, SourceText, TextLayout, TextLayoutKind};

async fn fixture() -> Project {
    let mut session = Session::memory().await.unwrap();
    let patch = session
        .snapshot()
        .patch(|edit| {
            let page = edit.add_page(PageDraft::new("page.png", 100.0, 100.0), At::End)?;
            for text in ["First\n\nline\n", "", "\nSecond"] {
                let content = edit.add_text_content(page, At::End)?;
                edit.add_text_layer(
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
                edit.set(
                    content,
                    &SourceText {
                        text: Authored::user("日本語\n原文".to_owned()),
                        language: None,
                    },
                )?;
                edit.set(
                    content,
                    &Translation {
                        text: Authored::user(text.to_owned()),
                        language: None,
                    },
                )?;
            }
            Ok(())
        })
        .unwrap();
    session.commit(patch).await.unwrap();
    Project::new(session, "script".to_owned())
}

#[tokio::test]
async fn round_trip_preserves_multiline_blank_lines_and_windows_newlines() {
    let project = fixture().await;
    let snapshot = project.snapshot();
    let text = write_script(&project.name, &script_pages(&snapshot).unwrap()).unwrap();
    for text in [text.clone(), text.replace('\n', "\r\n")] {
        let preview = preview_import(&snapshot, &project.name, &text).unwrap();
        assert!(preview.changes.is_empty());
        assert_eq!(preview.unchanged, 3);
        assert_eq!(preview.not_found, 0);
    }
}

#[tokio::test]
async fn two_edits_are_user_translations_with_one_undo_and_source_untouched() {
    let mut project = fixture().await;
    let snapshot = project.snapshot();
    let text = write_script(&project.name, &script_pages(&snapshot).unwrap())
        .unwrap()
        .replace("First\n\nline\n", "New\n\ntranslation\n")
        .replace("\nSecond", "\nChanged");
    let preview = preview_import(&snapshot, &project.name, &text).unwrap();
    assert_eq!(preview.changes.len(), 2);
    assert_eq!(preview.unchanged, 1);
    let first = preview.changes[0].layer;
    let commit = project
        .apply_script(preview.clone())
        .await
        .unwrap()
        .unwrap();
    project.record_commit(&commit);
    assert_eq!(project.undo.len(), 1);
    let current = project.snapshot();
    let content = current.text_layer(first).unwrap().content().unwrap();
    assert!(matches!(
        content.translation().unwrap().unwrap().text.origin,
        Origin::User
    ));
    assert_eq!(
        content.source().unwrap().unwrap().text.value,
        "日本語\n原文"
    );
    assert!(project.apply_script(preview).await.is_err());
    project.undo().await.unwrap();
    assert_eq!(
        write_script(&project.name, &script_pages(&project.snapshot()).unwrap()).unwrap(),
        write_script(&project.name, &script_pages(&snapshot).unwrap()).unwrap()
    );
}

#[tokio::test]
async fn unknown_ids_count_and_broken_or_duplicate_headers_fail() {
    let project = fixture().await;
    let missing = EntityId::new();
    let text = format!(
        "# Booharu script 1\n# Project: script\n\n=== Page 1: page ===\n\n--- Box 1 {missing} ---\n> source\nnew\n\n"
    );
    assert_eq!(
        preview_import(&project.snapshot(), &project.name, &text)
            .unwrap()
            .not_found,
        1
    );
    assert!(parse_script(&text.replace("script 1", "script 2")).is_err());
    assert!(parse_script(&text.replace(" ---\n", " --\n")).is_err());
    assert!(parse_script(&format!("{text}--- Box 2 {missing} ---\ntext\n\n")).is_err());
}
