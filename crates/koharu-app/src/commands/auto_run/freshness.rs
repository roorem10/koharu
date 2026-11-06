use anyhow::{Context as _, Result, ensure};
use booharu_context::ProjectContext;
use koharu_pipeline::{Request, Stage};
use koharu_scene::{
    AssetRole, EntityId, EntityOrigin, Geometry, Origin, ProjectId, RasterLayer, RasterLayerKind,
    Region, Snapshot,
};
use serde_json::{Value, json};
use tauri::{AppHandle, Manager as _};
use tauri_runtime_cef::CefRuntime;

use super::super::project::CurrentProject;

const INPUTS: &str = "auto_run_inputs";
const READING: &str = "auto_run_reading";
const TYPESET: &str = "auto_run_typeset";

pub(crate) fn hand_context(context: &ProjectContext) -> Value {
    let lines: Vec<_> = context
        .lines
        .iter()
        .filter_map(|(id, line)| {
            let category = line
                .category
                .filter(|category| category.source == booharu_context::LabelSource::User);
            (category.is_some() || line.box_note.is_some())
                .then(|| json!([id, category, line.box_note]))
        })
        .collect();
    json!(lines)
}

// Missing records are deliberately conservative: an earlier manual workflow did
// not authorize replacing its generated results. Once tracked, compare inputs,
// never revisions (a style edit must not cause OCR to run again).
fn changed(previous: Option<&Value>, next: &Value) -> bool {
    previous.is_some_and(|previous| previous != next)
}

fn stage_inputs(
    snapshot: &Snapshot,
    context: &ProjectContext,
    page: EntityId,
    stage: Stage,
) -> Result<Value> {
    let config = koharu_pipeline::PipelineConfig::load()?.read()?.clone();
    let source = snapshot
        .asset(page, &AssetRole::new("source")?)?
        .map(|asset| asset.blob.to_string());
    if stage == Stage::Detection {
        let boxes = koharu_config::load::<koharu_pipeline::BoxSettings>("boxes")?
            .read()?
            .clone();
        return Ok(json!({
            "source": source,
            "model": config.detection()?,
            "boxes": boxes,
            "text_mask": snapshot.asset(page, &AssetRole::new("text-mask")?)?.map(|asset| asset.blob.to_string())
        }));
    }
    if stage == Stage::Inpainting {
        let mut cleanup = Vec::new();
        for entity in snapshot.children(page)? {
            if let Some(layer) = snapshot.component::<RasterLayer>(entity)?
                && layer.kind == RasterLayerKind::Cleanup
            {
                cleanup.push(json!([
                    entity,
                    layer,
                    snapshot.asset(entity, &AssetRole::new("source")?)?.map(|asset| asset.blob.to_string())
                ]));
            }
        }
        return Ok(json!({
            "source": source,
            "text_mask": snapshot.asset(page, &AssetRole::new("text-mask")?)?.map(|asset| asset.blob.to_string()),
            "coo_mask": snapshot.asset(page, &AssetRole::new("coo-mask")?)?.map(|asset| asset.blob.to_string()),
            "model": config.inpainting()?,
            "output": cleanup
        }));
    }
    let mut lines = Vec::new();
    for layer in snapshot.page(page)?.text_layers()? {
        let content = layer.content()?;
        if stage == Stage::Ocr {
            let region = content.source_region()?;
            let geometry = match region {
                Some(region) => snapshot.component::<Geometry>(region.id())?,
                None => snapshot.component::<Geometry>(layer.id())?,
            };
            lines.push(json!([layer.id(), content.id(), geometry, content.source()?]));
        } else {
            lines.push(json!([layer.id(), content.source()?, content.translation()?]));
        }
    }
    if stage == Stage::Ocr {
        return Ok(json!({"source":source,"lines":lines,"model":config.ocr}));
    }
    let hints = super::super::context::translation_hints(context, snapshot)?;
    let terms: Vec<_> = hints
        .glossary
        .iter()
        .map(|term| json!([term.hint.source, term.hint.target, term.hint.note, term.locked]))
        .collect();
    Ok(json!({
        "source": source,
        "lines": lines,
        "config": config.translation,
        "background": hints.background,
        "characters": hints.characters.get(&page),
        "terms": terms,
        "skip": hints.skip,
        "previous_pages": hints.previous_pages,
        "previous_context": koharu_pipeline::previous_context(snapshot, page, hints.previous_pages)?
    }))
}

pub(super) fn refresh(
    snapshot: &Snapshot,
    context: &ProjectContext,
    request: &mut Request,
) -> Result<()> {
    ensure!(
        context.other.get(INPUTS).is_none_or(Value::is_object),
        "invalid Auto run input record"
    );
    request.refresh.clear();
    for page in super::scoped_pages(snapshot, &request.scope)? {
        for stage in Stage::ALL {
            let key = format!("{stage}:{page}");
            let previous = context.other.get(INPUTS).and_then(|value| value.get(&key));
            if changed(previous, &stage_inputs(snapshot, context, page, stage)?)
                && (stage != Stage::Detection || detection_can_refresh(snapshot, context, page)?)
            {
                request.refresh.insert((stage, page));
            }
        }
    }
    Ok(())
}

pub(crate) fn remember_stage(
    project: &super::super::project::Project,
    page: EntityId,
    stage: Stage,
) -> Result<()> {
    let snapshot = project.snapshot();
    let context = project.load_context()?;
    let key = format!("{stage}:{page}");
    let inputs = stage_inputs(&snapshot, &context, page, stage)?;
    ensure!(
        context.other.get(INPUTS).is_none_or(Value::is_object),
        "invalid Auto run input record"
    );
    project.update_context(|context| {
        let entries = context.other.entry(INPUTS).or_insert_with(|| json!({}));
        if let Some(entries) = entries.as_object_mut() {
            entries.insert(key, inputs);
        }
    })?;
    Ok(())
}

fn typeset_inputs(
    snapshot: &Snapshot,
    context: &ProjectContext,
    page: EntityId,
) -> Result<Value> {
    let styles = super::super::styles::load_text_styles()?.read()?.clone();
    let lettering = super::super::lettering::LetteringSettings::load()?
        .read()?
        .clone();
    let emphasis = koharu_translator::EmphasisSettings::load()?.read()?.clone();
    let colors = koharu_pipeline::PipelineConfig::load()?
        .read()?
        .detect_text_color;
    let mut categories = Vec::new();
    for layer in snapshot.page(page)?.text_layers()? {
        categories.push(json!([layer.id(), context.category(layer.id())]));
    }
    Ok(json!([styles, lettering, emphasis, colors, categories]))
}

pub(super) fn typeset_changed(
    snapshot: &Snapshot,
    context: &ProjectContext,
    pages: &[EntityId],
) -> Result<bool> {
    ensure!(
        context.other.get(TYPESET).is_none_or(Value::is_object),
        "invalid Auto run typesetting record"
    );
    for &page in pages {
        let key = page.to_string();
        if changed(
            context.other.get(TYPESET).and_then(|value| value.get(&key)),
            &typeset_inputs(snapshot, context, page)?,
        ) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn remember_typeset(
    project: &super::super::project::Project,
    pages: &[EntityId],
    replace: bool,
) -> Result<()> {
    let snapshot = project.snapshot();
    let context = project.load_context()?;
    ensure!(
        context.other.get(TYPESET).is_none_or(Value::is_object),
        "invalid Auto run typesetting record"
    );
    let mut values = Vec::new();
    for &page in pages {
        let key = page.to_string();
        if replace
            || context
                .other
                .get(TYPESET)
                .and_then(|value| value.get(&key))
                .is_none()
        {
            values.push((key, typeset_inputs(&snapshot, &context, page)?));
        }
    }
    project.update_context(|context| {
        let entries = context.other.entry(TYPESET).or_insert_with(|| json!({}));
        if let Some(entries) = entries.as_object_mut() {
            entries.extend(values);
        }
    })?;
    Ok(())
}

fn detection_can_refresh(
    snapshot: &Snapshot,
    context: &ProjectContext,
    page: EntityId,
) -> Result<bool> {
    // Redetection replaces generated identities. A page with any hand-owned
    // content or presentation keeps its geometry, including sidecar-only labels.
    for layer in snapshot.page(page)?.text_layers()? {
        let content = layer.content()?;
        if super::super::rich_text::has_manual_style(snapshot, layer.id())?
            || content
                .source()?
                .is_some_and(|text| text.text.origin == Origin::User)
            || content
                .translation()?
                .is_some_and(|text| text.text.origin == Origin::User)
            || context.lines.get(&layer.id()).is_some_and(|line| {
                line.category.is_some_and(|category| {
                    category.source == booharu_context::LabelSource::User
                }) || line.box_note.is_some()
            })
        {
            return Ok(false);
        }
    }
    for entity in snapshot.descendants(page)? {
        if (snapshot.component::<Region>(entity.id())?.is_some()
            || snapshot.component::<RasterLayer>(entity.id())?.is_some())
            && snapshot
                .component::<EntityOrigin>(entity.id())?
                .is_none_or(|origin| origin.origin == Origin::User)
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn reading_inputs(
    snapshot: &Snapshot,
    context: &ProjectContext,
    pages: &[EntityId],
) -> Result<Value> {
    let mut inputs = Vec::new();
    for &page in pages {
        let mut lines = Vec::new();
        for layer in snapshot.page(page)?.text_layers()? {
            lines.push(json!([layer.id(), layer.content()?.source()?]));
        }
        inputs.push(json!([
            page,
            snapshot.asset(page, &AssetRole::new("source")?)?.map(|asset| asset.blob.to_string()),
            lines
        ]));
    }
    let model = super::super::context::reading::get_reading_model()
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    Ok(json!({
        "pages": inputs,
        "model": model.selection,
        "series": super::super::context::linked_series(context)?,
        "read_at": context.brief.as_ref().and_then(|brief| brief.read_at.as_deref())
    }))
}

pub(super) fn reading_current(
    snapshot: &Snapshot,
    context: &ProjectContext,
    pages: &[EntityId],
) -> Result<bool> {
    let has_brief = context
        .brief
        .as_ref()
        .is_some_and(|brief| !brief.summary.trim().is_empty());
    Ok(has_brief
        && !changed(
            context.other.get(READING),
            &reading_inputs(snapshot, context, pages)?,
        ))
}

pub(super) async fn remember_reading(
    handle: &AppHandle<CefRuntime>,
    expected: ProjectId,
    pages: &[EntityId],
) -> Result<()> {
    let state = handle.state::<CurrentProject>();
    let projects = state.project.lock().await;
    let project = projects.as_ref().context("no project is open")?;
    let snapshot = project.snapshot();
    ensure!(
        snapshot.project_id() == expected,
        "project changed during Auto run"
    );
    let inputs = reading_inputs(&snapshot, &project.load_context()?, pages)?;
    project.update_context(|context| {
        context.other.insert(READING.into(), inputs);
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_observed_changed_inputs_invalidate_completed_work() {
        let current = json!({"source":"a","model":"one"});
        assert!(!changed(None, &current));
        assert!(!changed(Some(&current), &current));
        assert!(changed(
            Some(&current),
            &json!({"source":"b","model":"one"})
        ));
        assert!(changed(
            Some(&current),
            &json!({"source":"a","model":"two"})
        ));
    }

    #[test]
    fn automatic_metadata_does_not_hide_a_new_hand_label() {
        let mut context = ProjectContext::default();
        let original = hand_context(&context);
        let layer = EntityId::new();
        context.set_model_category(layer, booharu_context::LetteringCategory::Shout, Some(0.9));
        context
            .other
            .insert(INPUTS.into(), json!({"generated":"work"}));
        assert_eq!(hand_context(&context), original);
        context.set_user_category(layer, Some(booharu_context::LetteringCategory::Whisper));
        assert_ne!(hand_context(&context), original);
    }

    #[tokio::test]
    async fn redetection_never_replaces_a_hand_category_or_presentation() {
        use koharu_scene::{
            At, Generation, PageDraft, ProducerId, Session, TextLayout, TextLayoutKind,
        };
        let mut session = Session::memory().await.unwrap();
        let generation = Generation::new(ProducerId::new("dev.koharu.pipeline.detection").unwrap());
        let mut edit = session.snapshot().edit_as(generation.clone());
        let page = edit
            .add_page(PageDraft::new("page", 100.0, 100.0), At::End)
            .unwrap();
        let content = edit.add_text_content(page, At::End).unwrap();
        let layer = edit
            .add_text_layer(
                page,
                At::End,
                content,
                &TextLayout {
                    origin: Origin::Generated(generation),
                    kind: TextLayoutKind::Paragraph,
                    angle_degrees: None,
                    warp: None,
                },
            )
            .unwrap();
        session.commit(edit.finish().unwrap()).await.unwrap();
        let mut context = ProjectContext::default();
        assert!(detection_can_refresh(&session.snapshot(), &context, page).unwrap());
        context.set_user_category(layer, Some(booharu_context::LetteringCategory::Shout));
        assert!(!detection_can_refresh(&session.snapshot(), &context, page).unwrap());
        context.set_user_category(layer, None);
        let patch = session
            .snapshot()
            .patch(|edit| edit.promote_entity_to_user(layer))
            .unwrap();
        session.commit(patch).await.unwrap();
        assert!(!detection_can_refresh(&session.snapshot(), &context, page).unwrap());
    }
}
