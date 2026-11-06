use super::*;
use koharu_renderer::auto_fit_size_limit;
use koharu_scene::{At, Generation, Origin, PageDraft, ProducerId, Session, TextLayout};

async fn fixture() -> (Project, Vec<(EntityId, EntityId)>) {
    let mut project = Project::new(Session::memory().await.unwrap(), "sizes".into());
    let generation = Generation::new(ProducerId::new("dev.booharu.test").unwrap());
    let mut edit = project.snapshot().edit_as(generation.clone());
    let mut ids = Vec::new();
    for index in 0..2 {
        let page = edit
            .add_page(
                PageDraft::new(format!("page {index}"), 1400.0, 2000.0),
                At::End,
            )
            .unwrap();
        let content = edit.add_text_content(page, At::End).unwrap();
        let layer = edit
            .add_text_layer(
                page,
                At::End,
                content,
                &TextLayout {
                    origin: Origin::Generated(generation.clone()),
                    kind: TextLayoutKind::Paragraph,
                    angle_degrees: None,
                    warp: None,
                },
            )
            .unwrap();
        edit.set(
            layer,
            &Typography {
                origin: Origin::Generated(generation.clone()),
                preferred_font: None,
                font_weight: None,
                font_style: None,
                size: None,
                auto_fit: true,
                color: None,
                stroke_color: None,
                stroke_width: None,
                stroke_method: None,
                stroke_position: None,
                stroke_join: None,
                outer_stroke_color: None,
                outer_stroke_width: None,
                line_spacing: None,
                letter_spacing: None,
                fill_gradient: None,
                alignment: None,
                writing_mode: None,
                extensions: [("dev.booharu.unrelated".into(), "preserved".into())].into(),
            },
        )
        .unwrap();
        ids.push((page, layer));
    }
    project.commit(edit.finish().unwrap()).await.unwrap();
    (project, ids)
}

fn capped(snapshot: &Snapshot, layer: EntityId) -> Typography {
    let mut typography = snapshot.component::<Typography>(layer).unwrap().unwrap();
    set_auto_fit_size_limit(
        &mut typography,
        Some(AutoFitSizeLimit {
            maximum: 32.0,
            minimum: 12.0,
        }),
    )
    .unwrap();
    typography
}

#[tokio::test]
async fn one_undo_per_page_preserves_ownership_extensions_and_rejects_stale_work() {
    let (mut project, ids) = fixture().await;
    let original = project.snapshot();
    let first = capped(&original, ids[0].1);
    let second = capped(&original, ids[1].1);
    let commit = commit_page(
        &mut project,
        &original,
        ids[0].0,
        &[(ids[0].1, first.clone())],
    )
    .await
    .unwrap();
    assert_eq!(project.undo.len(), 1);
    assert_eq!(
        project
            .snapshot()
            .component::<Typography>(ids[0].1)
            .unwrap()
            .unwrap(),
        first
    );
    assert!(
        commit_page(
            &mut project,
            &original,
            ids[1].0,
            &[(ids[1].1, second.clone())],
        )
        .await
        .is_err()
    );
    commit_page(
        &mut project,
        &commit.snapshot,
        ids[1].0,
        &[(ids[1].1, second)],
    )
    .await
    .unwrap();
    assert_eq!(project.undo.len(), 2);
    project.undo().await.unwrap();
    let snapshot = project.snapshot();
    assert!(
        auto_fit_size_limit(&snapshot.component::<Typography>(ids[0].1).unwrap().unwrap())
            .unwrap()
            .is_some()
    );
    assert!(
        auto_fit_size_limit(&snapshot.component::<Typography>(ids[1].1).unwrap().unwrap())
            .unwrap()
            .is_none()
    );
    project.undo().await.unwrap();
    for (_, layer) in ids {
        assert_eq!(
            project.snapshot().component::<Typography>(layer).unwrap(),
            original.component::<Typography>(layer).unwrap()
        );
    }
}

#[tokio::test]
async fn page_scope_rejects_cross_page_updates_and_another_project() {
    let (mut project, ids) = fixture().await;
    let snapshot = project.snapshot();
    let value = capped(&snapshot, ids[1].1);
    assert!(
        commit_page(
            &mut project,
            &snapshot,
            ids[0].0,
            &[(ids[1].1, value.clone())],
        )
        .await
        .is_err()
    );
    let (other, _) = fixture().await;
    assert!(
        commit_page(
            &mut project,
            &other.snapshot(),
            ids[1].0,
            &[(ids[1].1, value)],
        )
        .await
        .is_err()
    );
    assert!(project.undo.is_empty());
}
