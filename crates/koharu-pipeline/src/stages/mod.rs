pub mod bubble_shape;
mod detection;
pub(crate) mod inpainting;
mod ocr;
mod translation;

use std::{collections::BTreeSet, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use koharu_scene::{Edit, EntityId, Generation, Patch, ProducerId, Snapshot};

pub use detection::{BoxSettings, KoharuLayoutRFDetrSeg2XLConfig};
pub use inpainting::{
    CleanupSettings, Flux2KleinConfig, PaintTarget, RemoveMode, RoremMixedConfig,
};

use crate::{Bounds, ImageCache, InpaintingMask, PipelineConfig, Stage, TranslationHints};

const EMPTY_MASK: &str = "dev.koharu.mask.empty";

#[derive(Clone)]
pub(crate) struct StageInput {
    pub(crate) scene: koharu_scene::Snapshot,
    pub(crate) page: EntityId,
    entities: Option<Arc<BTreeSet<EntityId>>>,
    region: Option<Bounds>,
    pub(crate) images: Arc<ImageCache>,
    inpainting_mask: Option<InpaintingMask>,
    translation_hints: Arc<TranslationHints>,
    redetect: bool,
    resume: bool,
    refresh: bool,
    pub(crate) cleanup_check: bool,
    cleanup_retry: bool,
}

impl StageInput {
    pub(crate) fn new(
        scene: Snapshot,
        page: EntityId,
        entities: Option<Arc<BTreeSet<EntityId>>>,
        region: Option<Bounds>,
        images: Arc<ImageCache>,
        inpainting_mask: Option<InpaintingMask>,
    ) -> Self {
        Self {
            scene,
            page,
            entities,
            region,
            images,
            inpainting_mask,
            translation_hints: Arc::default(),
            redetect: false,
            resume: false,
            refresh: false,
            cleanup_check: false,
            cleanup_retry: false,
        }
    }

    #[must_use]
    pub(crate) fn with_resume(mut self, resume: bool) -> Self {
        self.resume = resume;
        self
    }

    #[must_use]
    pub(crate) fn with_refresh(mut self, refresh: bool) -> Self {
        self.refresh = refresh;
        self
    }

    #[must_use]
    pub(crate) fn with_redetect(mut self, redetect: bool) -> Self {
        self.redetect = redetect;
        self
    }

    #[must_use]
    pub(crate) fn with_translation_hints(mut self, hints: Arc<TranslationHints>) -> Self {
        self.translation_hints = hints;
        self
    }

    pub(crate) fn page(&self) -> EntityId {
        self.page
    }

    pub(crate) fn with_cleanup_check(mut self, value: bool) -> Self {
        self.cleanup_check = value;
        self
    }

    pub(crate) fn with_cleanup_retry(mut self) -> Self {
        self.cleanup_retry = true;
        self
    }

    fn contains_entity(&self, entity: EntityId) -> Result<bool> {
        crate::scope::contains_entity(
            &self.scene,
            self.page,
            self.entities.as_deref(),
            self.region,
            entity,
        )
    }
}

#[async_trait]
pub(crate) trait StageProcessor: Send + Sync {
    fn model(&self) -> &'static str;
    fn skip(&self, _input: &StageInput) -> Result<bool> {
        Ok(false)
    }
    fn unload(&self) -> bool;
    async fn load(&self) -> Result<()>;
    async fn process(&self, input: StageInput) -> Result<Patch>;
}

pub(crate) struct Stages {
    detection: detection::Processor,
    ocr: ocr::Processor,
    translation: translation::Processor,
    inpainting: inpainting::Processor,
}

impl Stages {
    pub(crate) async fn detect_image(
        &self,
        image: Arc<image::DynamicImage>,
    ) -> Result<koharu_ml::koharu_layout_rfdetr_seg_2xl::KoharuLayoutDetections> {
        self.detection.load().await?;
        self.detection.detect_image(image).await
    }

    pub(crate) fn new(
        config: &PipelineConfig,
        translator: koharu_translator::Translator,
        device: &koharu_ml::Device,
    ) -> Result<Self> {
        Ok(Self {
            detection: detection::Processor::new(
                config.detection()?,
                device.clone(),
                detection::TextColorOptions {
                    detect_text_color: config.detect_text_color,
                },
                koharu_renderer::TextStyles::load(|| Ok(config.default_text_color))?,
            ),
            ocr: ocr::Processor::new(config.ocr.clone(), device.clone()),
            translation: translation::Processor::new(config.translation.clone(), translator),
            inpainting: inpainting::Processor::new(config.inpainting()?, device.clone())?,
        })
    }

    fn processor(&self, stage: Stage) -> &dyn StageProcessor {
        match stage {
            Stage::Detection => &self.detection,
            Stage::Ocr => &self.ocr,
            Stage::Translation => &self.translation,
            Stage::Inpainting => &self.inpainting,
        }
    }

    pub(crate) fn model(&self, stage: Stage) -> &'static str {
        self.processor(stage).model()
    }

    pub(crate) fn skip(&self, stage: Stage, input: &StageInput) -> Result<bool> {
        self.processor(stage).skip(input)
    }

    pub(crate) async fn load(&self, stage: Stage) -> Result<()> {
        self.processor(stage).load().await
    }

    pub(crate) async fn process(&self, stage: Stage, input: StageInput) -> Result<Patch> {
        self.processor(stage).process(input).await
    }

    pub(crate) fn unload(&self, stage: Stage) -> bool {
        self.processor(stage).unload()
    }
}

fn generation(producer: &str, model: &str) -> Result<Generation> {
    let mut generation = Generation::new(ProducerId::new(producer)?);
    generation.model = Some(model.to_owned());
    Ok(generation)
}

fn finish(edit: Edit) -> Result<Patch> {
    edit.finish().map_err(Into::into)
}

fn needs_text(previous: Option<&koharu_scene::Authored<String>>, resume: bool) -> bool {
    match previous {
        Some(value) if value.origin == koharu_scene::Origin::User => false,
        Some(value) => !resume || value.value.trim().is_empty(),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(bytes: &'static [u8]) -> koharu_scene::AssetInput {
        koharu_scene::AssetInput::new(
            bytes,
            "image/png",
            koharu_scene::AssetMetadata {
                width: Some(1),
                height: Some(1),
                attributes: std::collections::BTreeMap::new(),
            },
        )
    }

    #[tokio::test]
    async fn resume_skips_complete_boxes_and_explicit_stages_preserve_hand_text() {
        let translator = koharu_translator::Translator::from_config(
            koharu_ml::Device::cpu(),
            koharu_config::Config::memory(koharu_translator::ProvidersConfig::default()),
        )
        .unwrap();
        let stages = Stages::new(
            &PipelineConfig::default(),
            translator,
            &koharu_ml::Device::cpu(),
        )
        .unwrap();
        let mut session = koharu_scene::Session::memory().await.unwrap();
        let mut page_id = None;
        let mut content_id = None;
        let mut layer_id = None;
        let patch = session
            .snapshot()
            .patch(|edit| {
                let page = edit.add_page(
                    koharu_scene::PageDraft::new("page", 1.0, 1.0),
                    koharu_scene::At::End,
                )?;
                page_id = Some(page);
                let content = edit.add_text_content(page, koharu_scene::At::End)?;
                content_id = Some(content);
                edit.set(
                    content,
                    &koharu_scene::SourceText {
                        text: koharu_scene::Authored::user("source".into()),
                        language: None,
                    },
                )?;
                layer_id = Some(edit.add_text_layer(
                    page,
                    koharu_scene::At::End,
                    content,
                    &koharu_scene::TextLayout {
                        origin: koharu_scene::Origin::User,
                        kind: koharu_scene::TextLayoutKind::Paragraph,
                        angle_degrees: None,
                        warp: None,
                    },
                )?);
                Ok(())
            })
            .unwrap();
        session.commit(patch).await.unwrap();
        let input = StageInput::new(
            session.snapshot(),
            page_id.unwrap(),
            None,
            None,
            Arc::default(),
            None,
        )
        .with_resume(true);
        assert!(stages.skip(Stage::Ocr, &input).unwrap());
        assert!(!stages.skip(Stage::Translation, &input).unwrap());
        let skipped = input
            .clone()
            .with_translation_hints(Arc::new(TranslationHints {
                skip: [layer_id.unwrap()].into(),
                ..Default::default()
            }));
        assert!(stages.skip(Stage::Translation, &skipped).unwrap());
        let patch = session
            .snapshot()
            .patch(|edit| {
                edit.set(
                    content_id.unwrap(),
                    &koharu_scene::Translation {
                        text: koharu_scene::Authored::user("translation".into()),
                        language: None,
                    },
                )
            })
            .unwrap();
        session.commit(patch).await.unwrap();
        let input = StageInput::new(
            session.snapshot(),
            page_id.unwrap(),
            None,
            None,
            Arc::default(),
            None,
        )
        .with_resume(true);
        assert!(stages.skip(Stage::Translation, &input).unwrap());
        let explicit = input.with_resume(false);
        assert!(stages.skip(Stage::Ocr, &explicit).unwrap());
        assert!(stages.skip(Stage::Translation, &explicit).unwrap());
    }

    #[test]
    fn missing_and_outdated_text_runs_but_hand_edits_always_win() {
        use koharu_scene::Authored;
        let generated = generation("dev.koharu.test", "test").unwrap();
        for resume in [false, true] {
            assert!(needs_text(None, resume));
            assert!(!needs_text(Some(&Authored::user("".into())), resume));
            assert!(!needs_text(Some(&Authored::user("edited".into())), resume));
            assert!(needs_text(
                Some(&Authored::generated(" ".into(), generated.clone())),
                resume,
            ));
        }
        let completed = Authored::generated("done".into(), generated);
        assert!(!needs_text(Some(&completed), true));
        assert!(needs_text(Some(&completed), false));
    }

    #[tokio::test]
    async fn registry_contains_every_stage() {
        let translator = koharu_translator::Translator::from_config(
            koharu_ml::Device::cpu(),
            koharu_config::Config::memory(koharu_translator::ProvidersConfig::default()),
        )
        .unwrap();
        let stages = Stages::new(
            &PipelineConfig::default(),
            translator,
            &koharu_ml::Device::cpu(),
        )
        .unwrap();

        assert_eq!(
            Stage::ALL.map(|stage| stages.model(stage)),
            [
                "koharu-layout-rfdetr-seg-2xl",
                "paddleocr-vl-1.6",
                "local",
                "lama",
            ]
        );
    }

    #[tokio::test]
    async fn translation_and_inpainting_compose_without_weakening_text_guards() {
        let mut session = koharu_scene::Session::memory().await.unwrap();
        let mut setup = session.snapshot().edit();
        let page = setup
            .add_page(
                koharu_scene::PageDraft::new("page", 1.0, 1.0),
                koharu_scene::At::End,
            )
            .unwrap();
        let text = setup.add_text_content(page, koharu_scene::At::End).unwrap();
        setup
            .set(
                text,
                &koharu_scene::SourceText {
                    text: koharu_scene::Authored::user("before".to_owned()),
                    language: None,
                },
            )
            .unwrap();
        setup
            .set_asset(
                page,
                &koharu_scene::AssetRole::new("source").unwrap(),
                asset(b"source"),
            )
            .unwrap();
        session.commit(setup.finish().unwrap()).await.unwrap();
        let base = session.snapshot();

        let mut text_edit = base.edit();
        text_edit.observe::<koharu_scene::SourceText>(text).unwrap();
        text_edit
            .observe::<koharu_scene::Translation>(text)
            .unwrap();
        text_edit
            .set(
                text,
                &koharu_scene::Translation {
                    text: koharu_scene::Authored::user("after".to_owned()),
                    language: None,
                },
            )
            .unwrap();
        let text_patch = text_edit.finish().unwrap();

        let mut image_edit = base.edit();
        image_edit.observe_assets(page).unwrap();
        let cleanup = image_edit
            .add_entity(page, koharu_scene::At::Start)
            .unwrap();
        image_edit
            .set(
                cleanup,
                &koharu_scene::RasterLayer {
                    origin: koharu_scene::Origin::User,
                    name: "Cleanup".to_owned(),
                    kind: koharu_scene::RasterLayerKind::Cleanup,
                },
            )
            .unwrap();
        image_edit
            .set_asset(
                cleanup,
                &koharu_scene::AssetRole::new("source").unwrap(),
                asset(b"clean"),
            )
            .unwrap();
        let image_patch = image_edit.finish().unwrap();

        let image_first = base.preview([&image_patch]).unwrap();
        assert!(text_patch.rebase_on(&image_first).is_ok());
        let text_first = base.preview([&text_patch]).unwrap();
        assert!(image_patch.rebase_on(&text_first).is_ok());

        let changed_source = base
            .patch(|edit| {
                edit.set(
                    text,
                    &koharu_scene::SourceText {
                        text: koharu_scene::Authored::user("changed".to_owned()),
                        language: None,
                    },
                )
            })
            .unwrap();
        let changed_source = base.preview([&changed_source]).unwrap();
        assert!(text_patch.rebase_on(&changed_source).is_err());
    }
}
