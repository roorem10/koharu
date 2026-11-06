mod ink;

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

    async fn load(&self, input: &StageInput) -> Result<()> {
        // Empty input must work even when OCR weights are not installed. Use
        // the same immutable snapshot and crop evidence again in process.
        if targets(input).await?.iter().all(|target| target.ink.is_blank()) {
            return Ok(());
        }
        self.model
            .ensure(|| Model::load(self.device.clone(), &self.config))
            .await
    }

    async fn process(&self, input: StageInput) -> Result<koharu_scene::Patch> {
        let targets = targets(&input).await?;
        let results = if targets.iter().all(|target| target.ink.is_blank()) {
            targets
                .into_iter()
                .map(|target| target.into_result(String::new()))
                .collect()
        } else {
            self.model
                .lock()
                .await
                .as_ref()
                .ok_or_else(|| anyhow!("OCR model is not loaded"))?
                .run(targets)
                .await?
        };
        ocr_patch(&input, results, self.model())
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

    async fn run(&self, targets: Vec<OcrTarget>) -> Result<Vec<OcrResult>> {
        match self {
            Self::Manga(model) => {
                infer_text(model.clone(), targets, |model, image| {
                    model.inference(image)
                })
                .await
            }
            Self::Baberu(model) => {
                infer_text(model.clone(), targets, |model, image| {
                    model.inference(image)
                })
                .await
            }
            Self::Hayai(model) => {
                infer_text(model.clone(), targets, |model, image| {
                    model.inference(image)
                })
                .await
            }
            Self::Paddle(model) => {
                infer_text(model.clone(), targets, |model, image| {
                    Ok(model.inference(image, PaddleOCRVLTask::Ocr)?.text)
                })
                .await
            }
        }
    }
}

async fn targets(input: &StageInput) -> Result<Vec<OcrTarget>> {
    let page = input.page;
    let source = input
        .images
        .get(&input.scene, page, "source")
        .await?
        .ok_or_else(|| anyhow!("page {page} has no source image"))?;
    let input = input.clone();
    tokio_rayon::spawn(move || collect_targets(&input, &source)).await
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
            let image = crop(source, &geometry)
                .with_context(|| format!("text region {region} is outside its source image"))?;
            targets.push(OcrTarget {
                content,
                frame: region,
                detected: true,
                ink: ink::measure(&image),
                image,
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
            let image = crop(source, &geometry).with_context(|| {
                format!("text layer {} is outside its source image", layer.id())
            })?;
            targets.push(OcrTarget {
                content: content.id(),
                frame: layer.id(),
                detected: false,
                ink: ink::measure(&image),
                image,
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
    ink: ink::InkEvidence,
}

impl OcrTarget {
    fn into_result(self, text: String) -> OcrResult {
        OcrResult {
            content: self.content,
            frame: self.frame,
            detected: self.detected,
            geometry: self.geometry,
            previous: self.previous,
            text,
        }
    }
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
                let text = if target.ink.is_blank() {
                    String::new()
                } else {
                    let text = inference(&model, &target.image)?;
                    // Inspect raw output before placeholder normalization can turn
                    // a repetitive hallucination into a plausible ellipsis.
                    if target.ink.reject_output(&text) {
                        String::new()
                    } else {
                        normalize_ocr_text(text)
                    }
                };
                Ok(target.into_result(text))
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
        let image = DynamicImage::ImageLuma8(image::GrayImage::from_fn(100, 100, |x, y| {
            image::Luma([if x == 20 && (25..45).contains(&y) { 0 } else { 255 }])
        }));
        let targets = collect_targets(&input, &image).unwrap();
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

    #[tokio::test]
    async fn blank_reread_needs_no_model_and_commits_an_undoable_empty_source() {
        use koharu_scene::{AssetInput, AssetMetadata, AssetRole};

        let (mut session, page, boxes) = manual_boxes().await;
        let mut png = std::io::Cursor::new(Vec::new());
        DynamicImage::new_rgb8(100, 100)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let mut edit = session.snapshot().edit();
        edit.set_asset(
            page,
            &AssetRole::new("source").unwrap(),
            AssetInput::new(
                png.into_inner(),
                "image/png",
                AssetMetadata {
                    width: Some(100),
                    height: Some(100),
                    attributes: Default::default(),
                },
            ),
        )
        .unwrap();
        // O-tool boxes have a user-authored region; they use the detected-target
        // path even though the user drew them. Keep the second box regionless.
        let region = edit
            .add_analysis_region::<TextRegion>(
                page,
                At::End,
                &Geometry::rectangle(10., 20., 20., 30.),
                None,
            )
            .unwrap();
        edit.relate::<RecognizedFrom>(boxes[0].1, region).unwrap();
        session.commit(edit.finish().unwrap()).await.unwrap();
        let generated = generation(PRODUCER, "test").unwrap();
        let mut edit = session.snapshot().edit_as(generated.clone());
        edit.set(
            boxes[0].1,
            &SourceText {
                text: Authored::generated("invented text".into(), generated),
                language: None,
            },
        )
        .unwrap();
        session.commit(edit.finish().unwrap()).await.unwrap();
        let snapshot = session.snapshot();
        let scope = crate::scope::NormalizedScope::new(
            &snapshot,
            &crate::Scope::Entities(vec![boxes[0].0]),
            &[crate::Stage::Ocr],
        )
        .unwrap();
        let input = StageInput::new(
            snapshot.clone(),
            page,
            scope.entities(),
            None,
            Arc::new(crate::ImageCache::default()),
            None,
        );
        let processor = Processor::new(OcrModel::MangaOcr, koharu_ml::Device::cpu());
        processor.load(&input).await.unwrap();
        assert!(processor.model.lock().await.is_none());
        let patch = processor.process(input).await.unwrap();
        let hand_edit = snapshot
            .patch(|edit| {
                edit.set(
                    boxes[0].1,
                    &SourceText {
                        text: Authored::user("my edit".into()),
                        language: None,
                    },
                )
            })
            .unwrap();
        assert!(
            patch
                .rebase_on(&snapshot.preview([&hand_edit]).unwrap())
                .is_err()
        );
        let commit = session.commit(patch).await.unwrap();
        let content = commit
            .snapshot
            .text_layer(boxes[0].0)
            .unwrap()
            .content()
            .unwrap();
        let source = content.source().unwrap().unwrap();
        assert!(source.text.value.is_empty());
        assert!(matches!(source.text.origin, Origin::Generated(_)));
        assert_eq!(
            content.translation().unwrap().unwrap().text.value,
            "Keep my translation"
        );
        assert!(
            commit
                .snapshot
                .component::<OcrAnalysis>(region)
                .unwrap()
                .is_some()
        );
        session.undo(commit.revision).await.unwrap();
        assert_eq!(
            session
                .snapshot()
                .component::<SourceText>(boxes[0].1)
                .unwrap()
                .unwrap()
                .text
                .value,
            "invented text"
        );
    }

    #[tokio::test]
    async fn mixed_crops_skip_blank_inference_and_filter_only_near_blank_repetition() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let blank = image::GrayImage::from_pixel(64, 64, image::Luma([255]));
        let mut dust = blank.clone();
        dust.put_pixel(32, 32, image::Luma([0]));
        let mut stroke = blank.clone();
        for y in 25..45 {
            stroke.put_pixel(32, y, image::Luma([252]));
        }
        let targets = [blank, dust.clone(), stroke, dust]
            .into_iter()
            .map(|image| {
                let image = DynamicImage::ImageLuma8(image);
                OcrTarget {
                    content: EntityId::new(),
                    frame: EntityId::new(),
                    detected: false,
                    geometry: Geometry::rectangle(0., 0., 64., 64.),
                    previous: None,
                    ink: ink::measure(&image),
                    image,
                }
            })
            .collect();
        let calls = Arc::new(Mutex::new(AtomicUsize::new(0)));
        let results = infer_text(calls.clone(), targets, |calls, _| {
            Ok(["☐ ☐", "あああ", "ordinary text"][calls.fetch_add(1, Ordering::Relaxed)].into())
        })
        .await
        .unwrap();
        assert_eq!(calls.lock().unwrap().load(Ordering::Relaxed), 3);
        assert_eq!(
            results
                .iter()
                .map(|result| result.text.as_str())
                .collect::<Vec<_>>(),
            ["", "", "あああ", "ordinary text"]
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
