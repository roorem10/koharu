use std::{io::Cursor, sync::Arc};

use anyhow::Context;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions, Resizer};
use image::{DynamicImage, ImageEncoder, RgbImage, codecs::jpeg::JpegEncoder};

use crate::Language;

const MAX_IMAGE_DIMENSION: u32 = 2048;
const JPEG_QUALITY: u8 = 88;

#[derive(Debug, Clone, PartialEq)]
pub struct TranslationRequest {
    pub segments: Vec<String>,
    pub source_language: Option<Language>,
    pub target_language: Language,
    pub instructions: Option<String>,
    pub context: Vec<TranslationContext>,
    pub glossary: Vec<GlossaryHint>,
    pub brief: Option<String>,
    pub image: Option<Arc<DynamicImage>>,
}

impl TranslationRequest {
    #[must_use]
    pub fn new(
        segments: impl IntoIterator<Item = impl Into<String>>,
        target_language: Language,
    ) -> Self {
        Self {
            segments: segments.into_iter().map(Into::into).collect(),
            source_language: None,
            target_language,
            instructions: None,
            context: Vec::new(),
            glossary: Vec::new(),
            brief: None,
            image: None,
        }
    }

    #[cfg(test)]
    #[must_use]
    pub fn with_source_language(mut self, language: Language) -> Self {
        self.source_language = Some(language);
        self
    }

    #[must_use]
    pub fn with_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }

    #[must_use]
    pub fn with_image(mut self, image: Arc<DynamicImage>) -> Self {
        self.image = Some(image);
        self
    }

    #[must_use]
    pub fn with_context(mut self, context: impl IntoIterator<Item = TranslationContext>) -> Self {
        self.context = context.into_iter().collect();
        self
    }

    pub(crate) fn prepare_image(&mut self) -> anyhow::Result<()> {
        if let Some(image) = self.image.take() {
            self.image = Some(resize_image(image, MAX_IMAGE_DIMENSION)?);
        }
        Ok(())
    }

    pub(crate) fn remove_image(&mut self) {
        self.image = None;
    }
}

pub(crate) struct EncodedImage {
    pub(crate) data: String,
}

impl EncodedImage {
    pub(crate) fn data_url(&self) -> String {
        format!("data:image/jpeg;base64,{}", self.data)
    }
}

pub(crate) fn encode_image(image: &DynamicImage) -> anyhow::Result<EncodedImage> {
    let rgb = image.to_rgb8();
    let mut bytes = Cursor::new(Vec::new());
    JpegEncoder::new_with_quality(&mut bytes, JPEG_QUALITY)
        .write_image(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .context("failed to encode translation image")?;
    Ok(EncodedImage {
        data: STANDARD.encode(bytes.into_inner()),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TranslationContext {
    pub source: String,
    pub translation: String,
}

impl TranslationContext {
    #[must_use]
    pub fn new(source: impl Into<String>, translation: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            translation: translation.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GlossaryHint {
    pub source: String,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TaskRequest {
    pub system: String,
    pub user: String,
    pub images: Vec<Arc<DynamicImage>>,
    pub schema: serde_json::Value,
}

impl TaskRequest {
    pub(crate) fn translation(request: &TranslationRequest) -> anyhow::Result<Self> {
        let (system, user) = crate::prompt::prompts(request)?;
        Ok(Self {
            system,
            user,
            images: request.image.iter().cloned().collect(),
            schema: crate::prompt::output_schema(request.segments.len()),
        })
    }

    pub fn prepare_images(&mut self, longest: u32) -> anyhow::Result<()> {
        anyhow::ensure!(longest > 0, "image dimension must be positive");
        self.images = self
            .images
            .drain(..)
            .map(|image| resize_image(image, longest))
            .collect::<anyhow::Result<_>>()?;
        Ok(())
    }
}

fn resize_image(image: Arc<DynamicImage>, limit: u32) -> anyhow::Result<Arc<DynamicImage>> {
    let (width, height) = (image.width(), image.height());
    let longest = width.max(height);
    if longest <= limit {
        return Ok(image);
    }
    let resized_width = (u64::from(width) * u64::from(limit) / u64::from(longest)).max(1) as u32;
    let resized_height = (u64::from(height) * u64::from(limit) / u64::from(longest)).max(1) as u32;
    let source = image.to_rgb8();
    let mut resized = RgbImage::new(resized_width, resized_height);
    Resizer::new()
        .resize(
            &source,
            &mut resized,
            &ResizeOptions::new()
                .resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3))
                .use_alpha(false),
        )
        .context("failed to resize translation image")?;
    Ok(Arc::new(DynamicImage::ImageRgb8(resized)))
}
