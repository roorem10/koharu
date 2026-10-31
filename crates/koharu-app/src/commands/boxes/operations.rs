use std::collections::{BTreeMap, HashSet};

use anyhow::{Context as _, Result, bail};
use koharu_scene::{
    At, Authored, Commit, Edit, EntityId, FitsTo, FlowsIn, Geometry, Inside, OcrAnalysis, Presents,
    RecognizedFrom, Region, Snapshot, SourceText, TextLayout, TextRole, Translation, Typography,
    WritingMode,
};
use serde::Serialize;
use specta::Type;

use super::deletion::remove_layers;
use crate::commands::project::Project;

#[derive(Clone, Debug, Serialize, Type)]
pub struct BoxActionInfo {
    pub can_split: bool,
    pub bubble_boxes: u32,
}

#[derive(Clone, Debug, Serialize, Type)]
pub struct SplitBubbleBox {
    pub layer: EntityId,
    pub source: Option<String>,
    pub translation: Option<String>,
}

#[derive(Clone, Debug, Serialize, Type)]
pub struct SplitBubble {
    pub page: EntityId,
    pub bubble: EntityId,
    pub boxes: Vec<SplitBubbleBox>,
}

#[derive(Clone)]
struct TextBox {
    layer: EntityId,
    content: EntityId,
    region: EntityId,
    bounds: [f64; 4],
    typography: Option<Typography>,
    source: Option<SourceText>,
    translation: Option<Translation>,
}

impl TextBox {
    fn read(snapshot: &Snapshot, layer: EntityId) -> Result<Self> {
        let text = snapshot.text_layer(layer)?;
        let content = text.content()?;
        let region = content
            .source_region()?
            .context("text box has no detected region")?;
        Ok(Self {
            layer,
            content: content.id(),
            region: region.id(),
            bounds: bounds(&region.geometry()?)?,
            typography: text.typography()?,
            source: content.source()?,
            translation: content.translation()?,
        })
    }

    fn writing_mode(&self) -> WritingMode {
        self.typography
            .as_ref()
            .and_then(|t| t.writing_mode)
            .unwrap_or_else(|| {
                if self.bounds[3] - self.bounds[1] > self.bounds[2] - self.bounds[0] {
                    WritingMode::Vertical
                } else {
                    WritingMode::Horizontal
                }
            })
    }
}

fn bounds(geometry: &Geometry) -> Result<[f64; 4]> {
    let mut result = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for point in &geometry.points {
        result[0] = result[0].min(point.x);
        result[1] = result[1].min(point.y);
        result[2] = result[2].max(point.x);
        result[3] = result[3].max(point.y);
    }
    if !result.iter().all(|n| n.is_finite()) || result[2] <= result[0] || result[3] <= result[1] {
        bail!("text region must have nonempty finite geometry");
    }
    Ok(result)
}

fn rectangle([left, top, right, bottom]: [f64; 4]) -> Geometry {
    Geometry::rectangle(left, top, right - left, bottom - top)
}

fn page_of(snapshot: &Snapshot, mut id: EntityId) -> Result<EntityId> {
    while let Some(parent) = snapshot.parent(id)? {
        id = parent;
    }
    snapshot.page(id)?;
    Ok(id)
}

fn bubble_boxes(snapshot: &Snapshot, bubble: EntityId) -> Result<Vec<TextBox>> {
    let mut boxes = snapshot
        .relations_to_as::<FlowsIn>(bubble)
        .map(|relation| TextBox::read(snapshot, relation.value().source))
        .collect::<Result<Vec<_>>>()?;
    // Typography determines the reading axis; source-region positions determine order.
    let mode = boxes
        .first()
        .map(TextBox::writing_mode)
        .unwrap_or(WritingMode::Vertical);
    sort_boxes(&mut boxes, mode);
    Ok(boxes)
}

fn sort_boxes(boxes: &mut [TextBox], mode: WritingMode) {
    boxes.sort_by(|a, b| {
        let order = match mode {
            WritingMode::Vertical => b.bounds[0]
                .total_cmp(&a.bounds[0])
                .then_with(|| a.bounds[1].total_cmp(&b.bounds[1])),
            WritingMode::Horizontal => a.bounds[1]
                .total_cmp(&b.bounds[1])
                .then_with(|| a.bounds[0].total_cmp(&b.bounds[0])),
        };
        order.then_with(|| a.layer.cmp(&b.layer))
    });
}

fn groups_on_page(snapshot: &Snapshot, page: EntityId) -> Result<Vec<(EntityId, Vec<TextBox>)>> {
    snapshot.page(page)?;
    let mut groups = BTreeMap::new();
    for entity in snapshot.descendants(page)? {
        if let Some(relation) = snapshot.relation_from::<FlowsIn>(entity.id())? {
            groups.entry(relation.value().target).or_insert(());
        }
    }
    groups
        .into_keys()
        .map(|bubble| Ok((bubble, bubble_boxes(snapshot, bubble)?)))
        .filter(|value: &Result<(_, Vec<_>)>| !matches!(value, Ok((_, boxes)) if boxes.len() < 2))
        .collect()
}

fn copy_role(
    snapshot: &Snapshot,
    edit: &mut Edit,
    from: EntityId,
    to: EntityId,
) -> koharu_scene::Result<()> {
    if let Some(role) = snapshot.component::<TextRole>(from)? {
        edit.set(to, &role)?;
    }
    Ok(())
}

// Shared presentations must keep their original semantic content and OCR geometry.
fn exclusive_content(
    snapshot: &Snapshot,
    edit: &mut Edit,
    text: &TextBox,
    page: EntityId,
    participants: &HashSet<EntityId>,
) -> koharu_scene::Result<EntityId> {
    if snapshot
        .relations_to_as::<Presents>(text.content)
        .all(|r| participants.contains(&r.value().source))
    {
        edit.promote_entity_to_user(text.content)?;
        return Ok(text.content);
    }
    let content = edit.add_text_content(page, At::End)?;
    copy_role(snapshot, edit, text.content, content)?;
    edit.remove_relation(
        snapshot
            .relation_from::<Presents>(text.layer)?
            .expect("text has presentation")
            .id(),
    )?;
    edit.relate::<Presents>(text.layer, content)?;
    Ok(content)
}

fn exclusive_region(
    snapshot: &Snapshot,
    edit: &mut Edit,
    text: &TextBox,
    page: EntityId,
    content: EntityId,
    participants: &HashSet<EntityId>,
) -> koharu_scene::Result<EntityId> {
    if content == text.content
        && snapshot
            .relations_to(text.region, None)
            .all(|relation| participants.contains(&relation.value().source))
    {
        edit.promote_entity_to_user(text.region)?;
        return Ok(text.region);
    }
    let region = edit.add_entity(page, At::End)?;
    edit.set(
        region,
        &snapshot
            .component::<Region>(text.region)?
            .expect("source region has metadata"),
    )?;
    edit.set(region, &rectangle(text.bounds))?;
    if content == text.content {
        edit.remove_relation(
            snapshot
                .relation_from::<RecognizedFrom>(content)?
                .expect("text has region")
                .id(),
        )?;
    }
    edit.relate::<RecognizedFrom>(content, region)?;
    if let Some(fit) = snapshot.relation_from::<FitsTo>(text.layer)? {
        edit.remove_relation(fit.id())?;
        edit.relate::<FitsTo>(text.layer, region)?;
    }
    for inside in snapshot.relations_from_as::<Inside>(text.region) {
        edit.relate::<Inside>(region, inside.value().target)?;
    }
    Ok(region)
}

fn join_group(snapshot: &Snapshot, edit: &mut Edit, boxes: &[TextBox]) -> Result<EntityId> {
    let first = boxes.first().context("bubble has no text boxes")?;
    let page = page_of(snapshot, first.layer)?;
    let participants = boxes
        .iter()
        .flat_map(|b| [b.layer, b.content])
        .collect::<HashSet<_>>();
    let content = exclusive_content(snapshot, edit, first, page, &participants)?;
    let region = exclusive_region(snapshot, edit, first, page, content, &participants)?;
    let union = boxes.iter().skip(1).fold(first.bounds, |a, b| {
        [
            a[0].min(b.bounds[0]),
            a[1].min(b.bounds[1]),
            a[2].max(b.bounds[2]),
            a[3].max(b.bounds[3]),
        ]
    });
    edit.promote_entity_to_user(first.layer)?;
    edit.set(region, &rectangle(union))?;
    edit.remove::<OcrAnalysis>(region)?;
    let mut seen = HashSet::new();
    let contents = boxes
        .iter()
        .filter(|b| seen.insert(b.content))
        .collect::<Vec<_>>();
    let sources = contents
        .iter()
        .filter_map(|b| b.source.as_ref())
        .collect::<Vec<_>>();
    if let Some(first) = sources.first() {
        edit.set(
            content,
            &SourceText {
                language: first.language.clone(),
                text: Authored::user(
                    sources
                        .iter()
                        .map(|s| s.text.value.as_str())
                        .filter(|s| !s.trim().is_empty())
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
            },
        )?;
    } else {
        edit.remove::<SourceText>(content)?;
    }
    let translations = contents
        .iter()
        .filter_map(|b| b.translation.as_ref())
        .collect::<Vec<_>>();
    if let Some(first) = translations.first() {
        edit.set(
            content,
            &Translation {
                language: first.language.clone(),
                text: Authored::user(
                    translations
                        .iter()
                        .map(|t| t.text.value.as_str())
                        .filter(|s| !s.trim().is_empty())
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
            },
        )?;
    } else {
        edit.remove::<Translation>(content)?;
    }
    remove_layers(
        snapshot,
        edit,
        &boxes.iter().skip(1).map(|b| b.layer).collect::<Vec<_>>(),
        true,
    )?;
    Ok(first.layer)
}

impl Project {
    pub(crate) fn box_action_info(&self, layer: EntityId) -> Result<BoxActionInfo> {
        let snapshot = self.snapshot();
        let text = snapshot.text_layer(layer)?;
        let count = text
            .balloon_target()?
            .map(|bubble| snapshot.relations_to_as::<FlowsIn>(bubble.id()).count())
            .unwrap_or(0);
        Ok(BoxActionInfo {
            can_split: text.content()?.source_region()?.is_some(),
            bubble_boxes: count as u32,
        })
    }

    pub(crate) fn find_split_bubbles(&self, page: Option<EntityId>) -> Result<Vec<SplitBubble>> {
        let snapshot = self.snapshot();
        let pages = page
            .map(|page| vec![page])
            .unwrap_or_else(|| snapshot.pages().map(|p| p.id()).collect());
        let mut result = Vec::new();
        for page in pages {
            for (bubble, boxes) in groups_on_page(&snapshot, page)? {
                result.push(SplitBubble {
                    page,
                    bubble,
                    boxes: boxes
                        .into_iter()
                        .map(|b| SplitBubbleBox {
                            layer: b.layer,
                            source: b.source.map(|s| s.text.value),
                            translation: b.translation.map(|t| t.text.value),
                        })
                        .collect(),
                });
            }
        }
        Ok(result)
    }

    pub(crate) async fn join_bubble_boxes(
        &mut self,
        layer: EntityId,
    ) -> Result<(Option<Commit>, EntityId)> {
        let snapshot = self.snapshot();
        let selected = TextBox::read(&snapshot, layer)?;
        let bubble = snapshot
            .relation_from::<FlowsIn>(layer)?
            .context("text box is not linked to a bubble")?
            .value()
            .target;
        let mut boxes = bubble_boxes(&snapshot, bubble)?;
        sort_boxes(&mut boxes, selected.writing_mode());
        if boxes.len() < 2 {
            return Ok((None, layer));
        }
        let kept = boxes[0].layer;
        let mut edit = snapshot.edit();
        join_group(&snapshot, &mut edit, &boxes)?;
        Ok((Some(self.commit(edit.finish()?).await?), kept))
    }

    pub(crate) async fn join_page_bubbles(
        &mut self,
        page: EntityId,
    ) -> Result<(Option<Commit>, u32)> {
        let snapshot = self.snapshot();
        let groups = groups_on_page(&snapshot, page)?;
        if groups.is_empty() {
            return Ok((None, 0));
        }
        let mut edit = snapshot.edit();
        for (_, boxes) in &groups {
            join_group(&snapshot, &mut edit, boxes)?;
        }
        Ok((
            Some(self.commit(edit.finish()?).await?),
            groups.len() as u32,
        ))
    }

    pub(crate) async fn split_box(&mut self, layer: EntityId) -> Result<(Commit, [EntityId; 2])> {
        let snapshot = self.snapshot();
        let text = TextBox::read(&snapshot, layer)?;
        let page = page_of(&snapshot, layer)?;
        let [left, top, right, bottom] = text.bounds;
        let (first, second) = match text.writing_mode() {
            WritingMode::Vertical => {
                let middle = (left + right) / 2.;
                ([middle, top, right, bottom], [left, top, middle, bottom])
            }
            WritingMode::Horizontal => {
                let middle = (top + bottom) / 2.;
                ([left, top, right, middle], [left, middle, right, bottom])
            }
        };
        let mut edit = snapshot.edit();
        let participants = [layer, text.content].into_iter().collect();
        let content = exclusive_content(&snapshot, &mut edit, &text, page, &participants)?;
        let region = exclusive_region(&snapshot, &mut edit, &text, page, content, &participants)?;
        edit.promote_entity_to_user(layer)?;
        edit.set(region, &rectangle(first))?;
        edit.remove::<OcrAnalysis>(region)?;
        edit.remove::<SourceText>(content)?;
        edit.remove::<Translation>(content)?;
        let new_region = edit.add_entity(page, At::End)?;
        edit.set(
            new_region,
            &snapshot
                .component::<Region>(text.region)?
                .context("missing text region metadata")?,
        )?;
        edit.set(new_region, &rectangle(second))?;
        let new_content = edit.add_text_content(page, At::End)?;
        copy_role(&snapshot, &mut edit, text.content, new_content)?;
        edit.relate::<RecognizedFrom>(new_content, new_region)?;
        let new_layer = edit.add_text_layer(
            page,
            At::After(layer),
            new_content,
            &snapshot
                .component::<TextLayout>(layer)?
                .context("missing text layout")?,
        )?;
        if let Some(typography) = &text.typography {
            edit.set(new_layer, typography)?;
        }
        if snapshot.component::<Geometry>(layer)?.is_some() {
            edit.set(layer, &rectangle(first))?;
            edit.set(new_layer, &rectangle(second))?;
        }
        if let Some(bubble) = snapshot.relation_from::<FlowsIn>(layer)? {
            edit.relate::<FlowsIn>(new_layer, bubble.value().target)?;
            edit.relate::<Inside>(new_region, bubble.value().target)?;
        } else {
            edit.relate::<FitsTo>(new_layer, new_region)?;
        }
        Ok((self.commit(edit.finish()?).await?, [layer, new_layer]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use koharu_scene::{
        Generation, Origin, PageDraft, ProducerId, Session, TextLayoutKind, TextRegion,
    };

    async fn fixture(mode: WritingMode, bubbles: usize) -> (Project, EntityId, Vec<Vec<EntityId>>) {
        let mut session = Session::memory().await.unwrap();
        let mut page = None;
        let patch = session
            .snapshot()
            .patch(|edit| {
                page = Some(edit.add_page(PageDraft::new("page", 400., 400.), At::End)?);
                Ok(())
            })
            .unwrap();
        session.commit(patch).await.unwrap();
        let generation =
            Generation::new(ProducerId::new(super::super::deletion::DETECTION_PRODUCER).unwrap());
        let snapshot = session.snapshot();
        let mut edit = snapshot.edit_as(generation.clone());
        let page = page.unwrap();
        let mut layers = Vec::new();
        for index in 0..bubbles {
            let offset = index as f64 * 150.;
            let bubble = edit
                .add_analysis_region::<koharu_scene::BubbleRegion>(
                    page,
                    At::End,
                    &Geometry::rectangle(offset, 0., 140., 140.),
                    None,
                )
                .unwrap();
            let mut group = Vec::new();
            for (column, label) in [(0., "first"), (40., "second")] {
                let geometry = if mode == WritingMode::Vertical {
                    Geometry::rectangle(offset + 80. - column, 20., 20., 80.)
                } else {
                    Geometry::rectangle(offset + 20., 20. + column, 80., 20.)
                };
                let region = edit
                    .add_analysis_region::<TextRegion>(page, At::End, &geometry, None)
                    .unwrap();
                let content = edit.add_text_content(page, At::End).unwrap();
                edit.set(
                    content,
                    &SourceText {
                        text: Authored::generated(label.to_owned(), generation.clone()),
                        language: None,
                    },
                )
                .unwrap();
                edit.set(
                    content,
                    &Translation {
                        text: Authored::generated(
                            format!("translated {label}"),
                            generation.clone(),
                        ),
                        language: None,
                    },
                )
                .unwrap();
                edit.relate::<RecognizedFrom>(content, region).unwrap();
                edit.relate::<Inside>(region, bubble).unwrap();
                let layer = edit
                    .add_text_layer(
                        page,
                        At::End,
                        content,
                        &TextLayout {
                            origin: Origin::User,
                            kind: TextLayoutKind::Paragraph,
                            angle_degrees: None,
                        },
                    )
                    .unwrap();
                edit.set(
                    layer,
                    &Typography {
                        origin: Origin::Generated(generation.clone()),
                        preferred_font: Some("Example".into()),
                        font_weight: None,
                        font_style: None,
                        size: Some(22.),
                        auto_fit: false,
                        color: None,
                        stroke_color: None,
                        stroke_width: None,
                        stroke_method: None,
                        stroke_position: None,
                        stroke_join: None,
                        outer_stroke_color: None,
                        outer_stroke_width: None,
                        alignment: None,
                        writing_mode: Some(mode),
                        extensions: Default::default(),
                    },
                )
                .unwrap();
                edit.relate::<FlowsIn>(layer, bubble).unwrap();
                group.push(layer);
            }
            layers.push(group);
        }
        session.commit(edit.finish().unwrap()).await.unwrap();
        (Project::new(session, "boxes".into()), page, layers)
    }

    #[tokio::test]
    async fn joining_uses_reading_order_preserves_manual_text_and_undo_restores_regions() {
        for mode in [WritingMode::Vertical, WritingMode::Horizontal] {
            let (mut project, page, groups) = fixture(mode, 1).await;
            let first = groups[0][0];
            let second = groups[0][1];
            project
                .set_translation(second, Some("manual".into()))
                .await
                .unwrap();
            let before = project.snapshot();
            let region = TextBox::read(&before, second).unwrap().region;
            let (commit, kept) = project.join_bubble_boxes(second).await.unwrap();
            project.record_commit(&commit.unwrap());
            assert_eq!(kept, first);
            let joined = TextBox::read(&project.snapshot(), kept).unwrap();
            assert_eq!(joined.source.unwrap().text.value, "first\nsecond");
            let translated = joined.translation.unwrap();
            assert_eq!(translated.text.value, "translated first\nmanual");
            assert_eq!(translated.text.origin, Origin::User);
            assert!(project.snapshot().entity(second).is_err());
            assert!(project.snapshot().entity(region).is_err());
            assert!(project.find_split_bubbles(Some(page)).unwrap().is_empty());
            assert_eq!(
                joined.bounds,
                if mode == WritingMode::Vertical {
                    [40., 20., 100., 100.]
                } else {
                    [20., 20., 100., 80.]
                }
            );
            project.undo().await.unwrap();
            assert!(project.snapshot().entity(second).is_ok());
            assert!(project.snapshot().entity(region).is_ok());
            assert_eq!(project.find_split_bubbles(Some(page)).unwrap().len(), 1);
        }
    }

    #[tokio::test]
    async fn a_page_join_is_one_undo_step_and_a_noop_adds_no_commit() {
        let (mut project, page, _) = fixture(WritingMode::Vertical, 2).await;
        let (commit, count) = project.join_page_bubbles(page).await.unwrap();
        project.record_commit(&commit.unwrap());
        assert_eq!(count, 2);
        assert!(project.find_split_bubbles(None).unwrap().is_empty());
        assert!(project.join_page_bubbles(page).await.unwrap().0.is_none());
        project.undo().await.unwrap();
        assert_eq!(project.find_split_bubbles(None).unwrap().len(), 2);
        assert!(project.undo.is_empty());
    }

    #[tokio::test]
    async fn split_clears_text_preserves_typography_and_relations_and_is_undoable() {
        for mode in [WritingMode::Vertical, WritingMode::Horizontal] {
            let (mut project, _, groups) = fixture(mode, 1).await;
            let layer = groups[0][0];
            let original = TextBox::read(&project.snapshot(), layer).unwrap();
            let (commit, layers) = project.split_box(layer).await.unwrap();
            project.record_commit(&commit);
            let snapshot = project.snapshot();
            for id in layers {
                let text = TextBox::read(&snapshot, id).unwrap();
                assert!(text.source.is_none());
                assert!(text.translation.is_none());
                assert_eq!(
                    text.typography.unwrap().preferred_font,
                    Some("Example".into())
                );
                assert!(snapshot.relation_from::<FlowsIn>(id).unwrap().is_some());
            }
            let first = TextBox::read(&snapshot, layers[0]).unwrap();
            let second = TextBox::read(&snapshot, layers[1]).unwrap();
            if mode == WritingMode::Vertical {
                assert_eq!(first.bounds, [90., 20., 100., 100.]);
                assert_eq!(second.bounds, [80., 20., 90., 100.]);
            } else {
                assert_eq!(first.bounds, [20., 20., 100., 30.]);
                assert_eq!(second.bounds, [20., 30., 100., 40.]);
            }
            project.undo().await.unwrap();
            assert!(project.snapshot().entity(layers[1]).is_err());
            assert_eq!(
                TextBox::read(&project.snapshot(), layer).unwrap().bounds,
                original.bounds
            );
        }
    }

    #[tokio::test]
    async fn joining_skips_empty_text_and_uses_shape_when_typography_is_missing() {
        let (mut project, _, groups) = fixture(WritingMode::Vertical, 1).await;
        let snapshot = project.snapshot();
        let content = TextBox::read(&snapshot, groups[0][1]).unwrap().content;
        let patch = snapshot
            .patch(|edit| {
                for &layer in &groups[0] {
                    edit.remove::<Typography>(layer)?;
                }
                edit.set(
                    content,
                    &SourceText {
                        text: Authored::user(String::new()),
                        language: None,
                    },
                )
            })
            .unwrap();
        project.commit(patch).await.unwrap();
        let (_, kept) = project.join_bubble_boxes(groups[0][1]).await.unwrap();
        assert_eq!(kept, groups[0][0]);
        assert_eq!(
            TextBox::read(&project.snapshot(), kept)
                .unwrap()
                .source
                .unwrap()
                .text
                .value,
            "first"
        );
    }
    #[tokio::test]
    async fn join_and_split_do_not_change_shared_content_or_regions_in_other_presentations() {
        for split in [false, true] {
            let (mut project, page, groups) = fixture(WritingMode::Vertical, 1).await;
            let layer = groups[0][0];
            let original = TextBox::read(&project.snapshot(), layer).unwrap();
            let snapshot = project.snapshot();
            let mut outside = None;
            let patch = snapshot
                .patch(|edit| {
                    let other = edit.add_text_layer(
                        page,
                        At::End,
                        original.content,
                        &TextLayout {
                            origin: Origin::User,
                            kind: TextLayoutKind::Paragraph,
                            angle_degrees: None,
                        },
                    )?;
                    edit.relate::<FitsTo>(other, original.region)?;
                    outside = Some(other);
                    Ok(())
                })
                .unwrap();
            project.commit(patch).await.unwrap();
            if split {
                project.split_box(layer).await.unwrap();
            } else {
                project.join_bubble_boxes(layer).await.unwrap();
            }
            let other = TextBox::read(&project.snapshot(), outside.unwrap()).unwrap();
            assert_eq!(other.region, original.region);
            assert_eq!(other.bounds, original.bounds);
            assert_eq!(other.source.unwrap().text.value, "first");
            let edited = TextBox::read(&project.snapshot(), layer).unwrap();
            assert_ne!(edited.region, original.region);
            assert_ne!(edited.content, original.content);
        }
    }
    #[tokio::test]
    async fn joining_a_split_removes_its_user_owned_orphan_region() {
        let (mut project, _, groups) = fixture(WritingMode::Vertical, 1).await;
        let layer = groups[0][0];
        let (_, split) = project.split_box(layer).await.unwrap();
        let new_region = TextBox::read(&project.snapshot(), split[1]).unwrap().region;
        project.join_bubble_boxes(layer).await.unwrap();
        assert!(project.snapshot().entity(split[1]).is_err());
        assert!(project.snapshot().entity(new_region).is_err());
    }
}
