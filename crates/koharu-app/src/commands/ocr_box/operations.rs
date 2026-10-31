use std::collections::BTreeSet;

use anyhow::{Context as _, Result, ensure};
use booharu_context::LetteringCategory;
use koharu_pipeline::{
    Bounds,
    bubble_shape::{ShapeThresholds, classify, measure, style_for_shape},
};
use koharu_renderer::TextStyles;
use koharu_scene::{
    At, Authored, BubbleRegion, Commit, EntityId, Geometry, OcrAnalysis, Origin, Region,
    RegionSpec as _, Snapshot, SourceText, StrokeMethod, TextAlignment, TextDirection, TextLayout,
    TextLayoutKind, TextRegion, TextRole, Translation, Typography, WritingMode,
};

use super::{
    RedoAction, RedoInfo,
    geometry::{choose_bubble, clip_rect, extents, rectangle, subtitle_rect},
    settings::OcrBoxSettings,
};
use crate::commands::{
    boxes::create::{BoxDraft, add_detected_style_box},
    project::Project,
};

pub(super) fn page_of(snapshot: &Snapshot, mut id: EntityId) -> Result<EntityId> {
    while let Some(parent) = snapshot.parent(id)? {
        id = parent;
    }
    snapshot.page(id)?;
    Ok(id)
}

pub(super) fn box_geometry(
    snapshot: &Snapshot,
    layer: EntityId,
    source_first: bool,
) -> Result<Geometry> {
    let text = snapshot.text_layer(layer)?;
    let region = text.content()?.source_region()?;
    if source_first && let Some(region) = region {
        return Ok(region.geometry()?);
    }
    if let Some(geometry) = snapshot.component::<Geometry>(layer)? {
        return Ok(geometry);
    }
    if let Some(region) = region {
        return Ok(region.geometry()?);
    }
    if let Some(target) = text.fit_target()? {
        return Ok(target.geometry()?);
    }
    anyhow::bail!("text box has no frame")
}

pub(super) fn unique_layers(snapshot: &Snapshot, layers: &[EntityId]) -> Result<Vec<EntityId>> {
    ensure!(!layers.is_empty(), "select at least one text box");
    let mut seen = BTreeSet::new();
    for layer in layers {
        snapshot.text_layer(*layer)?;
        seen.insert(*layer);
    }
    Ok(seen.into_iter().collect())
}

pub(super) fn redo_info(snapshot: &Snapshot, layers: &[EntityId]) -> Result<RedoInfo> {
    let mut info = RedoInfo::default();
    for layer in unique_layers(snapshot, layers)? {
        let content = snapshot.text_layer(layer)?.content()?;
        info.user_sources += u32::from(
            content
                .source()?
                .is_some_and(|source| source.text.origin == Origin::User),
        );
        info.user_translations += u32::from(
            content
                .translation()?
                .is_some_and(|text| text.text.origin == Origin::User),
        );
    }
    Ok(info)
}

fn typography() -> Typography {
    Typography {
        origin: Origin::User,
        preferred_font: None,
        font_weight: None,
        font_style: None,
        size: None,
        auto_fit: true,
        color: None,
        stroke_color: None,
        stroke_width: None,
        stroke_method: Some(StrokeMethod::Contour),
        stroke_position: None,
        stroke_join: None,
        outer_stroke_color: None,
        outer_stroke_width: None,
        line_spacing: None,
        letter_spacing: None,
        fill_gradient: None,
        alignment: None,
        writing_mode: None,
        extensions: Default::default(),
    }
}

impl Project {
    pub(super) async fn create_ocr_box(
        &mut self,
        page: EntityId,
        rect: Bounds,
        styles: &TextStyles,
    ) -> Result<(Commit, EntityId)> {
        let snapshot = self.snapshot();
        let size = snapshot.page(page)?.page()?;
        let rect = clip_rect(rect, size.width, size.height)?;
        let geometry = rectangle(rect);
        let mut bubbles = Vec::new();
        for entity in snapshot.descendants(page)? {
            if snapshot
                .component::<Region>(entity.id())?
                .is_some_and(|region| region.kind == BubbleRegion::kind())
                && let Some(geometry) = snapshot.component::<Geometry>(entity.id())?
            {
                bubbles.push((entity.id(), geometry));
            }
        }
        let bubble = choose_bubble(&bubbles, rect);
        let shape = bubble
            .and_then(|id| bubbles.iter().find(|(candidate, _)| *candidate == id))
            .and_then(|(_, geometry)| measure(&geometry.points))
            .map(|m| classify(&m, &ShapeThresholds::default()));
        let mut typography = typography();
        styles
            .default_style
            .apply_to_detected(&mut typography, false);
        if let Some(style) = style_for_shape(styles, shape) {
            style.apply_to_detected(&mut typography, false);
        }
        let mut edit = snapshot.edit();
        let added = add_detected_style_box(
            &mut edit,
            BoxDraft {
                page,
                at: At::End,
                region: Region {
                    origin: Origin::User,
                    kind: TextRegion::kind(),
                    label: None,
                },
                geometry: geometry.clone(),
                role: Some(TextRole {
                    origin: Origin::User,
                    role: if bubble.is_some() {
                        "dev.koharu.text.dialogue".into()
                    } else {
                        "dev.koharu.text.free-text".into()
                    },
                }),
                layout: TextLayout {
                    origin: Origin::User,
                    kind: TextLayoutKind::Paragraph,
                    angle_degrees: None,
                    warp: None,
                },
                typography: Some(typography),
                layer_geometry: Some(geometry),
                bubble,
            },
        )?;
        Ok((self.commit(edit.finish()?).await?, added.layer))
    }

    pub(super) async fn clear_redo_edits(
        &mut self,
        layers: &[EntityId],
        action: RedoAction,
        overwrite: bool,
    ) -> Result<Option<Commit>> {
        let snapshot = self.snapshot();
        let layers = unique_layers(&snapshot, layers)?;
        let info = redo_info(&snapshot, &layers)?;
        let count = match action {
            RedoAction::Read => info.user_sources,
            RedoAction::Translate => info.user_translations,
            RedoAction::Clean => 0,
        };
        ensure!(
            count == 0 || overwrite,
            "confirm overwriting hand-edited text before running again"
        );
        if count == 0 {
            return Ok(None);
        }
        let patch = snapshot.patch(|edit| {
            for layer in layers {
                let content = snapshot.text_layer(layer)?.content()?;
                match action {
                    RedoAction::Read
                        if content
                            .source()?
                            .is_some_and(|text| text.text.origin == Origin::User) =>
                    {
                        edit.reset_source_text(
                            content.id(),
                            koharu_scene::ProducerId::new("dev.koharu.pipeline.ocr")?,
                        )?;
                    }
                    RedoAction::Translate
                        if content
                            .translation()?
                            .is_some_and(|text| text.text.origin == Origin::User) =>
                    {
                        edit.remove::<Translation>(content.id())?
                    }
                    _ => {}
                }
            }
            Ok(())
        })?;
        Ok(Some(self.commit(patch).await?))
    }

    pub(super) async fn create_sfx_subtitle(
        &mut self,
        layer: EntityId,
        settings: &OcrBoxSettings,
        styles: &TextStyles,
    ) -> Result<(Commit, EntityId, bool)> {
        let snapshot = self.snapshot();
        ensure!(
            self.load_context()?
                .category(layer)
                .is_some_and(|label| label.value == LetteringCategory::Sfx),
            "select an SFX box to add a subtitle"
        );
        let text = snapshot.text_layer(layer)?;
        let content = text.content()?;
        let source = content.source()?;
        let translation = content
            .translation()?
            .filter(|text| !text.text.value.trim().is_empty());
        let needs_translation = translation.is_none();
        let page = page_of(&snapshot, layer)?;
        let size = snapshot.page(page)?.page()?;
        let geometry = box_geometry(&snapshot, layer, false)?;
        let bounds = extents(&geometry)?;
        let direction = if let Some(region) = content.source_region()? {
            snapshot
                .component::<OcrAnalysis>(region.id())?
                .map(|analysis| analysis.direction)
        } else {
            None
        };
        let vertical = match direction {
            Some(TextDirection::Vertical) => true,
            Some(TextDirection::Horizontal) => false,
            _ => text
                .typography()?
                .and_then(|t| t.writing_mode)
                .map(|m| m == WritingMode::Vertical)
                .unwrap_or(bounds.height >= bounds.width * 1.15),
        };
        let frame = rectangle(subtitle_rect(
            bounds,
            vertical,
            settings.subtitle_size,
            size.width,
            size.height,
        ));
        let mut typography = typography();
        styles.default_style.apply_to(&mut typography);
        typography.preferred_font = settings
            .subtitle_font_family
            .clone()
            .or(typography.preferred_font);
        typography.size = Some(settings.subtitle_size);
        typography.auto_fit = false;
        typography.color = Some(settings.subtitle_fill);
        typography.fill_gradient = None;
        typography.stroke_color = Some(settings.subtitle_outline);
        typography.stroke_width = Some(2.);
        typography.outer_stroke_color = None;
        typography.outer_stroke_width = None;
        typography.writing_mode = Some(WritingMode::Horizontal);
        typography.alignment = Some(TextAlignment::Start);
        let mut created = None;
        let patch = snapshot.patch(|edit| {
            let content = edit.add_text_content(page, At::End)?;
            let mut source = source.unwrap_or(SourceText {
                text: Authored::user(String::new()),
                language: None,
            });
            source.text = Authored::user(source.text.value);
            edit.set(content, &source)?;
            if let Some(translation) = translation {
                edit.set(content, &translation)?;
            }
            let added = edit.add_text_layer(
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
            edit.set(added, &frame)?;
            edit.set(added, &typography)?;
            created = Some(added);
            Ok(())
        })?;
        let added = created.context("subtitle was not created")?;
        // Save the label before any translation request takes its hints snapshot.
        self.update_context(|context| {
            context.set_user_category(added, Some(LetteringCategory::SmallText))
        })?;
        let commit = match self.commit(patch).await {
            Ok(commit) => commit,
            Err(error) => {
                let _ = self.update_context(|context| context.set_user_category(added, None));
                return Err(error);
            }
        };
        Ok((commit, added, needs_translation))
    }
}
