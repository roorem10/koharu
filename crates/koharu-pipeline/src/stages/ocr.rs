use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use super::{StageInput, StageProcessor, finish, generation};
use crate::{ModelCell, OcrModel, scope::geometry_extents};
use anyhow::{Context as _, Result, anyhow, bail};
use async_trait::async_trait;
use image::DynamicImage;
use koharu_ml::{
    baberu_ocr::BaberuOcr, hayai_ocr::HayaiOcr, manga_ocr::MangaOcr,
    paddle_ocr_vl::PaddleOCRVLTask, paddle_ocr_vl_quantized::PaddleOCRVLQuantized,
};
use koharu_scene::{
    Authored, EntityId, Geometry, LanguageTag, OcrAnalysis, Origin, RecognizedFrom, Region,
    RegionSpec, SourceText, TextDirection, TextLayout, TextRegion,
};

const PRODUCER: &str = "dev.koharu.pipeline.ocr";

pub(super) struct Processor {
    config: OcrModel,
    device: koharu_ml::Device,
    model: ModelCell<Model>,
}

impl Processor {
    pub(super) fn new(config: OcrModel, device: koharu_ml::Device) -> Self {
        Self {
            config,
            device,
            model: ModelCell::new(),
        }
    }
}

#[async_trait]
impl StageProcessor for Processor {
    fn model(&self) -> &'static str {
        match self.config {
            OcrModel::MangaOcr => "manga-ocr",
            OcrModel::BaberuOcr => "baberu-ocr",
            OcrModel::HayaiOcr => "hayai-ocr",
            OcrModel::PaddleOcrVl1_6 => "paddleocr-vl-1.6",
        }
    }

    fn skip(&self, input: &StageInput) -> Result<bool> {
        if !input.resume {
            return Ok(false);
        }
        if let Some(group) = input.scene.page(input.page)?.text_group()? {
            for layer in group.text_layers()? {
                if input.contains_entity(layer.id())?
                    && !layer.content()?.source()?.is_some_and(|source| {
                        !source.text.value.trim().is_empty()
                            || matches!(source.text.origin, Origin::User)
                    })
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    fn unload(&self) -> bool {
        self.model.unload()
    }

    async fn load(&self) -> Result<()> {
        self.model
            .ensure(|| Model::load(self.device.clone(), &self.config))
            .await
    }

    async fn process(&self, input: StageInput) -> Result<koharu_scene::Patch> {
        self.model
            .lock()
            .await
            .as_ref()
            .ok_or_else(|| anyhow!("OCR model is not loaded"))?
            .run(input)
            .await
    }
}

enum Model {
    Manga(Arc<Mutex<MangaOcr>>),
    Baberu(Arc<Mutex<BaberuOcr>>),
    Hayai(Arc<Mutex<HayaiOcr>>),
    Paddle(Arc<Mutex<PaddleOCRVLQuantized>>),
}

impl Model {
    async fn load(device: koharu_ml::Device, config: &OcrModel) -> Result<Self> {
        match config {
            OcrModel::MangaOcr => Ok(Self::Manga(Arc::new(Mutex::new(
                MangaOcr::load(device).await?,
            )))),
            OcrModel::BaberuOcr => Ok(Self::Baberu(Arc::new(Mutex::new(
                BaberuOcr::load(device).await?,
            )))),
            OcrModel::HayaiOcr => Ok(Self::Hayai(Arc::new(Mutex::new(
                HayaiOcr::load(device).await?,
            )))),
            OcrModel::PaddleOcrVl1_6 => Ok(Self::Paddle(Arc::new(Mutex::new(
                PaddleOCRVLQuantized::load(device).await?,
            )))),
        }
    }

    async fn run(&self, input: StageInput) -> Result<koharu_scene::Patch> {
        let model_name = match self {
            Self::Manga(_) => "manga-ocr",
            Self::Baberu(_) => "baberu-ocr",
            Self::Hayai(_) => "hayai-ocr",
            Self::Paddle(_) => "paddleocr-vl-1.6",
        };
        let page = input.page;
        let mut targets = Vec::new();
        let source = input
            .images
            .get(&input.scene, page, "source")
            .await?
            .ok_or_else(|| anyhow!("page {page} has no source image"))?;
        targets.extend(collect_targets(&input, &source)?);

        let results = match self {
            Self::Manga(model) => {
                infer_text(model.clone(), targets, |model, image| {
                    model.inference(image)
                })
                .await?
            }
            Self::Baberu(model) => {
                infer_text(model.clone(), targets, |model, image| {
                    model.inference(image)
                })
                .await?
            }
            Self::Hayai(model) => {
                infer_text(model.clone(), targets, |model, image| {
                    model.inference(image)
                })
                .await?
            }
            Self::Paddle(model) => {
                infer_text(model.clone(), targets, |model, image| {
                    Ok(model.inference(image, PaddleOCRVLTask::Ocr)?.text)
                })
                .await?
            }
        };

        ocr_patch(&input, results, model_name)
    }
}

fn collect_targets(input: &StageInput, source: &DynamicImage) -> Result<Vec<OcrTarget>> {
    let mut targets = Vec::new();
    let mut seen = HashSet::new();
    let can_read = |previous: &Option<SourceText>| {
        !previous.as_ref().is_some_and(|value| {
            matches!(value.text.origin, Origin::User)
                || (input.resume && !value.text.value.trim().is_empty())
        })
    };
    for entity in input.scene.descendants(input.page)? {
        let region = entity.id();
        if !input.contains_entity(region)?
            || !input
                .scene
                .component::<Region>(region)?
                .is_some_and(|value| value.kind == TextRegion::kind())
        {
            continue;
        }
        let geometry = input
            .scene
            .component::<Geometry>(region)?
            .ok_or_else(|| anyhow!("text region {region} has no geometry"))?;
        for relation in input.scene.relations_to_as::<RecognizedFrom>(region) {
            let content = relation.value().source;
            let previous = input.scene.component::<SourceText>(content)?;
            if !can_read(&previous) || !seen.insert(content) {
                continue;
            }
            targets.push(OcrTarget {
                content,
                frame: region,
                detected: true,
                image: crop(source, &geometry)
                    .with_context(|| format!("text region {region} is outside its source image"))?,
                geometry: geometry.clone(),
                previous,
            });
        }
    }
    if let Some(group) = input.scene.page(input.page)?.text_group()? {
        for layer in group.text_layers()? {
            if !input.contains_entity(layer.id())? {
                continue;
            }
            let content = layer.content()?;
            let previous = content.source()?;
            if content.source_region()?.is_some()
                || !can_read(&previous)
                || !seen.insert(content.id())
            {
                continue;
            }
            let Some(geometry) = input.scene.component::<Geometry>(layer.id())? else {
                continue;
            };
            targets.push(OcrTarget {
                content: content.id(),
                frame: layer.id(),
                detected: false,
                image: crop(source, &geometry).with_context(|| {
                    format!("text layer {} is outside its source image", layer.id())
                })?,
                geometry,
                previous,
            });
        }
    }
    Ok(targets)
}

fn ocr_patch(
    input: &StageInput,
    results: Vec<OcrResult>,
    model: &str,
) -> Result<koharu_scene::Patch> {
    let generation = generation(PRODUCER, model)?;
    let mut edit = input.scene.edit_as(generation.clone());
    edit.observe_assets(input.page)?;
    for result in &results {
        if result.detected {
            edit.observe::<Region>(result.frame)?;
        } else {
            edit.observe::<TextLayout>(result.frame)?;
        }
        edit.observe::<Geometry>(result.frame)?;
        edit.observe::<SourceText>(result.content)?;
    }
    for result in results {
        let language = result
            .previous
            .and_then(|value| value.language)
            .or_else(|| LanguageTag::new("ja-JP").ok());
        edit.set(
            result.content,
            &SourceText {
                text: Authored::generated(result.text, generation.clone()),
                language,
            },
        )?;
        if result.detected {
            let (min_x, min_y, max_x, max_y) = geometry_extents(&result.geometry)
                .ok_or_else(|| anyhow!("text region {} has empty geometry", result.frame))?;
            edit.set(
                result.frame,
                &OcrAnalysis {
                    origin: Origin::Generated(generation.clone()),
                    direction: if max_y - min_y >= (max_x - min_x) * 1.15 {
                        TextDirection::Vertical
                    } else {
                        TextDirection::Horizontal
                    },
                    confidence: None,
                    line_boundaries: Vec::new(),
                },
            )?;
        }
    }
    finish(edit)
}

struct OcrTarget {
    content: EntityId,
    frame: EntityId,
    detected: bool,
    geometry: Geometry,
    previous: Option<SourceText>,
    image: DynamicImage,
}

struct OcrResult {
    content: EntityId,
    frame: EntityId,
    detected: bool,
    geometry: Geometry,
    previous: Option<SourceText>,
    text: String,
}

async fn infer_text<M: Send + 'static>(
    model: Arc<Mutex<M>>,
    targets: Vec<OcrTarget>,
    inference: impl Fn(&M, &DynamicImage) -> Result<String> + Send + Sync + 'static,
) -> Result<Vec<OcrResult>> {
    tokio_rayon::spawn(move || {
        let model = model
            .lock()
            .map_err(|_| anyhow!("OCR model lock is poisoned"))?;
        targets
            .into_iter()
            .map(|target| {
                Ok(OcrResult {
                    content: target.content,
                    frame: target.frame,
                    detected: target.detected,
                    geometry: target.geometry,
                    previous: target.previous,
                    text: normalize_ocr_text(inference(&model, &target.image)?),
                })
            })
            .collect()
    })
    .await
}

// Manga OCR can emit replacement-box glyphs for an isolated Japanese ellipsis.
// Normalize only an all-placeholder sequence so ordinary OCR output is preserved.
fn normalize_ocr_text(text: String) -> String {
    let visible = text
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<Vec<_>>();
    if visible.len() >= 2
        && visible
            .iter()
            .all(|character| matches!(character, '☐' | '□' | '▢' | '▣' | '�'))
    {
        "…".to_owned()
    } else {
        text
    }
}

fn crop(source: &DynamicImage, geometry: &Geometry) -> Result<DynamicImage> {
    let (min_x, min_y, max_x, max_y) =
        geometry_extents(geometry).ok_or_else(|| anyhow!("geometry is empty"))?;
    let x = min_x.floor().max(0.0) as u32;
    let y = min_y.floor().max(0.0) as u32;
    let right = max_x.ceil().max(0.0).min(f64::from(source.width())) as u32;
    let bottom = max_y.ceil().max(0.0).min(f64::from(source.height())) as u32;
    if right <= x || bottom <= y {
        bail!("geometry does not overlap the image");
    }
    Ok(source.crop_imm(x, y, right - x, bottom - y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use koharu_scene::{At, PageDraft, Session, TextLayoutKind, Translation};

    async fn manual_boxes() -> (Session, EntityId, Vec<(EntityId, EntityId)>) {
        let mut session = Session::memory().await.unwrap();
        let mut boxes = Vec::new();
        let mut page = None;
        let patch = session
            .snapshot()
            .patch(|edit| {
                let id = edit.add_page(PageDraft::new("page", 100., 100.), At::End)?;
                page = Some(id);
                for _ in 0..2 {
                    let content = edit.add_text_content(id, At::End)?;
                    edit.reset_source_text(content, koharu_scene::ProducerId::new(PRODUCER)?)?;
                    let layer = edit.add_text_layer(
                        id,
                        At::End,
                        content,
                        &TextLayout {
                            origin: Origin::User,
                            kind: TextLayoutKind::Paragraph,
                            angle_degrees: None,
                            warp: None,
                        },
                    )?;
                    edit.set(layer, &Geometry::rectangle(10., 20., 20., 30.))?;
                    edit.set(
                        content,
                        &Translation {
                            text: Authored::user("Keep my translation".into()),
                            language: None,
                        },
                    )?;
                    boxes.push((layer, content));
                }
                Ok(())
            })
            .unwrap();
        session.commit(patch).await.unwrap();
        (session, page.unwrap(), boxes)
    }

    #[tokio::test]
    async fn reads_only_the_scoped_regionless_layer_and_keeps_its_translation() {
        let (mut session, page, boxes) = manual_boxes().await;
        let input = StageInput::new(
            session.snapshot(),
            page,
            Some(Arc::new([boxes[0].0].into_iter().collect())),
            None,
            Arc::new(crate::ImageCache::default()),
            None,
        );
        let targets = collect_targets(&input, &DynamicImage::new_rgb8(100, 100)).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(
            (targets[0].image.width(), targets[0].image.height()),
            (20, 30)
        );
        let results = infer_text(Arc::new(Mutex::new(())), targets, |_, _| Ok("ドン".into()))
            .await
            .unwrap();
        let patch = ocr_patch(&input, results, "test").unwrap();
        let snapshot = session.commit(patch).await.unwrap().snapshot;
        let content = snapshot.text_layer(boxes[0].0).unwrap().content().unwrap();
        assert_eq!(content.source().unwrap().unwrap().text.value, "ドン");
        assert_eq!(
            content.translation().unwrap().unwrap().text.value,
            "Keep my translation"
        );
        assert!(content.source_region().unwrap().is_none());
        assert_eq!(
            snapshot
                .component::<SourceText>(boxes[1].1)
                .unwrap()
                .unwrap()
                .text
                .value,
            ""
        );
    }

    #[tokio::test]
    async fn skips_a_hand_edited_source_in_a_regionless_layer_even_when_empty() {
        let (mut session, page, boxes) = manual_boxes().await;
        let patch = session
            .snapshot()
            .patch(|edit| {
                for ((_, content), text) in boxes.iter().zip(["My source", ""]) {
                    edit.set(
                        *content,
                        &SourceText {
                            text: Authored::user(text.into()),
                            language: None,
                        },
                    )?;
                }
                Ok(())
            })
            .unwrap();
        let snapshot = session.commit(patch).await.unwrap().snapshot;
        let input = StageInput::new(
            snapshot,
            page,
            None,
            None,
            Arc::new(crate::ImageCache::default()),
            None,
        );
        assert!(
            collect_targets(&input, &DynamicImage::new_rgb8(100, 100))
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn resume_skips_generated_sources_but_an_explicit_read_replaces_them() {
        let (mut session, page, boxes) = manual_boxes().await;
        let generated = generation(PRODUCER, "test").unwrap();
        let snapshot = session.snapshot();
        let mut edit = snapshot.edit_as(generated.clone());
        edit.set(
            boxes[0].1,
            &SourceText {
                text: Authored::generated("previous".into(), generated),
                language: None,
            },
        )
        .unwrap();
        let snapshot = session
            .commit(edit.finish().unwrap())
            .await
            .unwrap()
            .snapshot;
        let input = StageInput::new(
            snapshot,
            page,
            Some(Arc::new([boxes[0].0].into_iter().collect())),
            None,
            Arc::new(crate::ImageCache::default()),
            None,
        );
        let image = DynamicImage::new_rgb8(100, 100);
        assert_eq!(collect_targets(&input, &image).unwrap().len(), 1);
        assert!(
            collect_targets(&input.with_resume(true), &image)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn repeated_placeholder_glyphs_are_an_ellipsis() {
        assert_eq!(normalize_ocr_text("☐ ☐ ☐".to_owned()), "…");
        assert_eq!(normalize_ocr_text("□\n□".to_owned()), "…");
    }

    #[test]
    fn ordinary_text_and_single_boxes_are_unchanged() {
        assert_eq!(normalize_ocr_text("待って…".to_owned()), "待って…");
        assert_eq!(normalize_ocr_text("☐".to_owned()), "☐");
    }
}
