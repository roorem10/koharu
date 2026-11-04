use std::collections::HashSet;

use anyhow::{Context as _, Result};
use koharu_scene::{
    AssetInput, AssetRole, EntityId, Patch, RasterLayer, RasterLayerKind, Snapshot,
};

use super::{PrepPageStatus, PrepResult, PrepSkipReason, PrepSkipped, RawPrepOptions, images};

pub(super) fn has_cleanup(snapshot: &Snapshot, page: EntityId) -> Result<bool> {
    for entity in snapshot.descendants(page)? {
        if snapshot
            .component::<RasterLayer>(entity.id())?
            .is_some_and(|layer| layer.kind == RasterLayerKind::Cleanup)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn statuses(snapshot: &Snapshot, pages: Vec<EntityId>) -> Result<Vec<PrepPageStatus>> {
    let original = AssetRole::new("original_source")?;
    unique_pages(pages)
        .into_iter()
        .map(|page| {
            snapshot.page(page)?;
            Ok(PrepPageStatus {
                page,
                has_original: snapshot.asset(page, &original)?.is_some(),
                has_cleanup: has_cleanup(snapshot, page)?,
            })
        })
        .collect()
}

fn unique_pages(pages: Vec<EntityId>) -> Vec<EntityId> {
    let mut seen = HashSet::new();
    pages
        .into_iter()
        .filter(|page| seen.insert(*page))
        .collect()
}

fn skip(
    result: &mut PrepResult,
    snapshot: &Snapshot,
    page: EntityId,
    reason: PrepSkipReason,
) -> Result<()> {
    result.skipped.push(PrepSkipped {
        page,
        label: snapshot.page(page)?.page()?.label,
        reason,
    });
    Ok(())
}

pub(super) async fn prepare_patch(
    snapshot: &Snapshot,
    pages: Vec<EntityId>,
    options: RawPrepOptions,
) -> Result<(Patch, PrepResult)> {
    options.validate()?;
    let source_role = AssetRole::new("source")?;
    let original_role = AssetRole::new("original_source")?;
    let mut edit = snapshot.edit();
    let mut result = PrepResult::default();
    for page in unique_pages(pages) {
        snapshot.page(page)?;
        if has_cleanup(snapshot, page)? {
            skip(&mut result, snapshot, page, PrepSkipReason::Cleanup)?;
            continue;
        }
        let Some(source) = snapshot.asset(page, &source_role)? else {
            skip(&mut result, snapshot, page, PrepSkipReason::MissingSource)?;
            continue;
        };
        let bytes = snapshot.read_blob(source.blob).await?;
        let input = bytes.clone();
        let prepared = tokio_rayon::spawn(move || images::prepare(&input, options))
            .await
            .with_context(|| format!("could not prepare page {page}"))?;
        let Some(prepared) = prepared else {
            skip(&mut result, snapshot, page, PrepSkipReason::Unchanged)?;
            continue;
        };
        if snapshot.asset(page, &original_role)?.is_none() {
            edit.set_asset(
                page,
                &original_role,
                AssetInput::new(bytes.to_vec(), source.media_type, source.metadata.clone()),
            )?;
        }
        edit.set_asset(
            page,
            &source_role,
            AssetInput::new(prepared, "image/png", source.metadata),
        )?;
        result.changed += 1;
    }
    Ok((edit.finish()?, result))
}

pub(super) async fn restore_patch(
    snapshot: &Snapshot,
    pages: Vec<EntityId>,
) -> Result<(Patch, PrepResult)> {
    let source_role = AssetRole::new("source")?;
    let original_role = AssetRole::new("original_source")?;
    let mut edit = snapshot.edit();
    let mut result = PrepResult::default();
    for page in unique_pages(pages) {
        snapshot.page(page)?;
        if has_cleanup(snapshot, page)? {
            skip(&mut result, snapshot, page, PrepSkipReason::Cleanup)?;
            continue;
        }
        let Some(original) = snapshot.asset(page, &original_role)? else {
            skip(&mut result, snapshot, page, PrepSkipReason::NoOriginal)?;
            continue;
        };
        if snapshot
            .asset(page, &source_role)?
            .is_some_and(|source| source.blob == original.blob)
        {
            skip(&mut result, snapshot, page, PrepSkipReason::Unchanged)?;
            continue;
        }
        let bytes = snapshot.read_blob(original.blob).await?;
        edit.set_asset(
            page,
            &source_role,
            AssetInput::new(bytes.to_vec(), original.media_type, original.metadata),
        )?;
        result.changed += 1;
    }
    Ok((edit.finish()?, result))
}
