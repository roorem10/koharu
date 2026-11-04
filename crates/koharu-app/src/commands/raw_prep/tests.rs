use std::io::Cursor;

use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
use koharu_scene::{
    AssetInput, AssetMetadata, AssetRole, At, Origin, PageDraft, RasterLayer, RasterLayerKind,
    Session,
};

use super::{operations::*, *};
use crate::commands::project::Project;

async fn project(kinds: &[Option<RasterLayerKind>]) -> (Project, Vec<EntityId>) {
    let mut session = Session::memory().await.unwrap();
    let mut edit = session.snapshot().edit();
    let source = RgbImage::from_fn(3, 1, |x, _| {
        Rgb([match x {
            0 => 40,
            1 => 195,
            _ => 200,
        }; 3])
    });
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(source)
        .write_to(&mut bytes, ImageFormat::Png)
        .unwrap();
    let mut pages = Vec::new();
    for (index, kind) in kinds.iter().enumerate() {
        let page = edit
            .add_page(PageDraft::new(format!("{index}.png"), 3.0, 1.0), At::End)
            .unwrap();
        edit.set_asset(
            page,
            &AssetRole::new("source").unwrap(),
            AssetInput::new(
                bytes.get_ref().clone(),
                "image/png",
                AssetMetadata {
                    width: Some(3),
                    height: Some(1),
                    attributes: Default::default(),
                },
            ),
        )
        .unwrap();
        if let Some(kind) = kind {
            let parent = edit.add_entity(page, At::End).unwrap();
            let layer = edit.add_entity(parent, At::End).unwrap();
            edit.set(
                layer,
                &RasterLayer {
                    origin: Origin::User,
                    name: "Pixels".into(),
                    kind: *kind,
                },
            )
            .unwrap();
        }
        pages.push(page);
    }
    session.commit(edit.finish().unwrap()).await.unwrap();
    (Project::new(session, "Raw prep".into()), pages)
}

fn asset(
    snapshot: &koharu_scene::Snapshot,
    page: EntityId,
    role: &str,
) -> Option<koharu_scene::Asset> {
    snapshot
        .asset(page, &AssetRole::new(role).unwrap())
        .unwrap()
}

#[tokio::test]
async fn skips_cleanup_in_subgroups_but_keeps_paint_and_unprocessed_pages() {
    let (mut project, pages) = project(&[
        Some(RasterLayerKind::Cleanup),
        Some(RasterLayerKind::Paint),
        None,
    ])
    .await;
    let snapshot = project.snapshot();
    let (patch, result) = prepare_patch(
        &snapshot,
        vec![pages[0], pages[1], pages[2], pages[2]],
        RawPrepOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.changed, 2);
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.skipped[0].reason, PrepSkipReason::Cleanup);
    assert_eq!(result.skipped[0].page, pages[0]);
    let commit = project.commit(patch).await.unwrap();
    project.record_commit(&commit);
    assert_eq!(project.undo.len(), 1);
    assert_eq!(
        asset(&project.snapshot(), pages[0], "source"),
        asset(&snapshot, pages[0], "source")
    );
    assert!(asset(&project.snapshot(), pages[0], "original_source").is_none());
    project.undo().await.unwrap();
    for page in pages {
        assert_eq!(
            asset(&project.snapshot(), page, "source"),
            asset(&snapshot, page, "source")
        );
        assert!(asset(&project.snapshot(), page, "original_source").is_none());
    }
}

#[tokio::test]
async fn second_run_keeps_first_original_and_restore_is_one_undo_step() {
    let (mut project, pages) = project(&[None, None]).await;
    let original = project.snapshot();
    let (patch, _) = prepare_patch(&original, pages.clone(), RawPrepOptions::default())
        .await
        .unwrap();
    let commit = project.commit(patch).await.unwrap();
    project.record_commit(&commit);
    let options = RawPrepOptions {
        whiten: true,
        ..Default::default()
    };
    let (patch, result) = prepare_patch(&project.snapshot(), pages.clone(), options)
        .await
        .unwrap();
    assert_eq!(result.changed, 2);
    let commit = project.commit(patch).await.unwrap();
    project.record_commit(&commit);
    let prepared = project.snapshot();
    for &page in &pages {
        let first = asset(&original, page, "source").unwrap();
        assert_eq!(
            asset(&prepared, page, "original_source").unwrap().blob,
            first.blob
        );
        assert_ne!(asset(&prepared, page, "source").unwrap().blob, first.blob);
    }
    let (patch, result) = restore_patch(&prepared, pages.clone()).await.unwrap();
    assert_eq!(result.changed, 2);
    let commit = project.commit(patch).await.unwrap();
    project.record_commit(&commit);
    assert_eq!(project.undo.len(), 3);
    for &page in &pages {
        assert_eq!(
            asset(&project.snapshot(), page, "source").unwrap().blob,
            asset(&original, page, "source").unwrap().blob
        );
    }
    project.undo().await.unwrap();
    for &page in &pages {
        assert_eq!(
            asset(&project.snapshot(), page, "source"),
            asset(&prepared, page, "source")
        );
    }
    project.redo().await.unwrap();
    let (patch, result) = restore_patch(&project.snapshot(), pages).await.unwrap();
    assert!(patch.is_empty());
    assert_eq!(result.changed, 0);
    assert!(
        result
            .skipped
            .iter()
            .all(|page| page.reason == PrepSkipReason::Unchanged)
    );
}

#[tokio::test]
async fn restore_skips_cleanup_even_when_an_original_exists() {
    let (mut project, pages) = project(&[None, None]).await;
    let (patch, _) = prepare_patch(
        &project.snapshot(),
        pages.clone(),
        RawPrepOptions::default(),
    )
    .await
    .unwrap();
    project.commit(patch).await.unwrap();
    let patch = project
        .snapshot()
        .patch(|edit| {
            let layer = edit.add_entity(pages[0], At::End)?;
            edit.set(
                layer,
                &RasterLayer {
                    origin: Origin::User,
                    name: "Cleanup".into(),
                    kind: RasterLayerKind::Cleanup,
                },
            )
        })
        .unwrap();
    project.commit(patch).await.unwrap();
    let snapshot = project.snapshot();
    let (patch, result) = restore_patch(&snapshot, pages.clone()).await.unwrap();
    assert_eq!(result.changed, 1);
    assert_eq!(result.skipped[0].reason, PrepSkipReason::Cleanup);
    project.commit(patch).await.unwrap();
    assert_eq!(
        asset(&project.snapshot(), pages[0], "source"),
        asset(&snapshot, pages[0], "source")
    );
    let status = statuses(&project.snapshot(), pages).unwrap();
    assert!(status[0].has_original && status[0].has_cleanup);
    assert!(status[1].has_original && !status[1].has_cleanup);
}

#[tokio::test]
async fn noops_do_not_create_originals_and_stale_preparation_cannot_overwrite_edits() {
    let (mut project, pages) = project(&[None]).await;
    let options = RawPrepOptions {
        levels: false,
        ..Default::default()
    };
    let (patch, result) = prepare_patch(&project.snapshot(), pages.clone(), options)
        .await
        .unwrap();
    assert!(patch.is_empty());
    assert_eq!(result.skipped[0].reason, PrepSkipReason::Unchanged);
    assert!(asset(&project.snapshot(), pages[0], "original_source").is_none());
    let (_, result) = restore_patch(&project.snapshot(), pages.clone())
        .await
        .unwrap();
    assert_eq!(result.skipped[0].reason, PrepSkipReason::NoOriginal);
    let (prepared, _) = prepare_patch(
        &project.snapshot(),
        pages.clone(),
        RawPrepOptions::default(),
    )
    .await
    .unwrap();
    let patch = project
        .snapshot()
        .patch(|edit| edit.set_page(pages[0], PageDraft::new("Hand edit", 3., 1.)))
        .unwrap();
    project.commit(patch).await.unwrap();
    assert!(project.commit(prepared).await.is_err());
    assert!(asset(&project.snapshot(), pages[0], "original_source").is_none());
}
