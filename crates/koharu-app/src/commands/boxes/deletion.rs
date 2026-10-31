use std::collections::HashSet;

use koharu_scene::{
    Edit, EntityId, EntityOrigin, Origin, Presents, RecognizedFrom, Region, RemovePolicy, Snapshot,
};

pub(crate) const DETECTION_PRODUCER: &str = "dev.koharu.pipeline.detection";

pub(crate) fn remove_layers(
    snapshot: &Snapshot,
    edit: &mut Edit,
    layers: &[EntityId],
    include_user_regions: bool,
) -> koharu_scene::Result<()> {
    let mut removed = HashSet::new();
    for &layer in layers {
        if snapshot.page(layer).is_ok() {
            return Err(koharu_scene::Error::Invalid(
                "delete pages with delete_pages".into(),
            ));
        }
        removed.insert(layer);
        removed.extend(snapshot.descendants(layer)?.map(|entity| entity.id()));
    }
    let cascaded = removed.clone();
    let mut contents = HashSet::new();
    for &layer in &removed {
        if let Some(relation) = snapshot.relation_from::<Presents>(layer)? {
            let content = relation.value().target;
            if snapshot
                .relations_to_as::<Presents>(content)
                .all(|relation| removed.contains(&relation.value().source))
            {
                contents.insert(content);
            }
        }
    }
    let mut regions = HashSet::new();
    for &content in &contents {
        if let Some(relation) = snapshot.relation_from::<RecognizedFrom>(content)? {
            let region = relation.value().target;
            if snapshot.component::<Region>(region)?.is_some()
                && (include_user_regions || snapshot.component::<EntityOrigin>(region)?.is_some_and(|value| {
                    matches!(value.origin, Origin::Generated(owner) if owner.producer.as_str() == DETECTION_PRODUCER)
                })) {
                regions.insert(region);
            }
        }
    }
    removed.extend(&contents);
    regions.retain(|&region| {
        snapshot
            .relations_to(region, None)
            .all(|relation| removed.contains(&relation.value().source))
    });
    for &layer in layers {
        edit.remove_entity(layer, RemovePolicy::Cascade)?;
    }
    for content in contents {
        if !cascaded.contains(&content) {
            edit.remove_entity(content, RemovePolicy::Cascade)?;
        }
    }
    for region in regions {
        if !cascaded.contains(&region) {
            edit.remove_entity(region, RemovePolicy::Cascade)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::project::Project;
    use koharu_scene::{
        At, FitsTo, Generation, Geometry, PageDraft, ProducerId, Session, TextLayout,
        TextLayoutKind, TextRegion,
    };

    async fn fixture(
        generated: bool,
        shared_region: bool,
    ) -> (Project, EntityId, EntityId, EntityId, EntityId) {
        let mut session = Session::memory().await.unwrap();
        let mut page = None;
        let patch = session
            .snapshot()
            .patch(|edit| {
                page = Some(edit.add_page(PageDraft::new("page", 200., 200.), At::End)?);
                Ok(())
            })
            .unwrap();
        session.commit(patch).await.unwrap();
        let snapshot = session.snapshot();
        let mut edit = if generated {
            snapshot.edit_as(Generation::new(
                ProducerId::new(DETECTION_PRODUCER).unwrap(),
            ))
        } else {
            snapshot.edit()
        };
        let page = page.unwrap();
        let region = edit
            .add_analysis_region::<TextRegion>(
                page,
                At::End,
                &Geometry::rectangle(10., 10., 30., 60.),
                None,
            )
            .unwrap();
        let content = edit.add_text_content(page, At::End).unwrap();
        edit.relate::<RecognizedFrom>(content, region).unwrap();
        let layout = TextLayout {
            origin: Origin::User,
            kind: TextLayoutKind::Paragraph,
            angle_degrees: None,
        };
        let layer = edit
            .add_text_layer(page, At::End, content, &layout)
            .unwrap();
        edit.relate::<FitsTo>(layer, region).unwrap();
        if shared_region {
            let other = edit.add_text_content(page, At::End).unwrap();
            edit.relate::<RecognizedFrom>(other, region).unwrap();
            let other_layer = edit.add_text_layer(page, At::End, other, &layout).unwrap();
            edit.relate::<FitsTo>(other_layer, region).unwrap();
        }
        session.commit(edit.finish().unwrap()).await.unwrap();
        (
            Project::new(session, "boxes".into()),
            page,
            layer,
            content,
            region,
        )
    }

    #[tokio::test]
    async fn deletion_removes_the_orphaned_detected_region_and_undo_restores_all() {
        let (mut project, _, layer, content, region) = fixture(true, false).await;
        let commit = project.delete_layers(vec![layer]).await.unwrap();
        project.record_commit(&commit);
        for id in [layer, content, region] {
            assert!(project.snapshot().entity(id).is_err());
        }
        project.undo().await.unwrap();
        for id in [layer, content, region] {
            assert!(project.snapshot().entity(id).is_ok());
        }
        assert!(project.undo.is_empty());
    }

    #[tokio::test]
    async fn deletion_preserves_user_regions_and_regions_still_in_use() {
        for (generated, shared) in [(false, false), (true, true)] {
            let (mut project, _, layer, _, region) = fixture(generated, shared).await;
            project.delete_layers(vec![layer]).await.unwrap();
            assert!(project.snapshot().entity(region).is_ok());
        }
    }
}
