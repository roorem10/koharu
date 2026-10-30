use std::sync::Arc;

use anyhow::Context;
use koharu_ml::llm::{
    ChatMessage, ChatTemplateOptions, Input, Llm, LoadOptions, MtmdOptions, media_marker,
};

mod catalog;

pub use catalog::LocalConfig;
use catalog::LocalModelDescriptor;
pub(crate) use catalog::{DEFAULT_MODEL, DEFAULT_QUANTIZATION};

use crate::{
    Device, Error, GenerationConfig, Model, ModelSelection, Provider, Quantization, Result,
    TaskRequest, TranslationRequest, prompt,
};

#[derive(Debug)]
pub struct LocalTranslator {
    descriptor: LocalModelDescriptor,
    llm: Arc<Llm>,
}

impl LocalTranslator {
    pub async fn load(device: Device, selection: &ModelSelection) -> Result<Self> {
        let model = selection
            .model
            .as_deref()
            .context("local translation requires a selected model")?;
        let descriptor = catalog::MODELS
            .iter()
            .copied()
            .find(|descriptor| descriptor.id == model)
            .with_context(|| format!("unknown local translator '{model}'"))?;
        let resolved = descriptor.resolve(selection).await?;
        let options = LoadOptions {
            mtmd: resolved.projector.map(MtmdOptions::new),
            ..LoadOptions::default()
        };
        let llm = Llm::load_with_options(device, resolved.model, options)
            .await
            .context("failed to load local translation model")?;
        if llm.capabilities().vision != descriptor.projector.is_some() {
            return Err(anyhow::anyhow!(
                "local translator vision capability does not match its catalog"
            )
            .into());
        }
        Ok(Self {
            descriptor,
            llm: Arc::new(llm),
        })
    }

    pub(crate) async fn translate(
        &self,
        request: TranslationRequest,
        generation: GenerationConfig,
    ) -> Result<Vec<String>> {
        let expected = request.segments.len();
        if expected == 0 {
            return Ok(Vec::new());
        }
        if !self
            .descriptor
            .target_languages
            .contains(request.target_language)
        {
            return Err(Error::UnsupportedLanguage {
                provider: "local",
                language: request.target_language,
            });
        }

        let task = TaskRequest::translation(&request)?;
        let output = self.run_task(task, generation).await?;
        Ok(prompt::translations("local", &output, &request.segments)?)
    }

    pub(crate) async fn run_task(
        &self,
        request: TaskRequest,
        generation: GenerationConfig,
    ) -> Result<String> {
        let payload = if request.images.is_empty() {
            request.user
        } else {
            format!(
                "{}\n{}",
                std::iter::repeat_n(media_marker(), request.images.len())
                    .collect::<Vec<_>>()
                    .join("\n"),
                request.user
            )
        };
        let prompt = self
            .llm
            .render_chat_prompt_with_options(
                &[
                    ChatMessage::system(request.system),
                    ChatMessage::user(payload),
                ],
                ChatTemplateOptions {
                    add_generation_prompt: true,
                    enable_thinking: generation.reasoning.unwrap_or(false)
                        && self.descriptor.reasoning,
                },
            )
            .context("failed to render local translation prompt")?;
        let llm = Arc::clone(&self.llm);
        let generation = self.descriptor.generation.options(generation);
        let output = tokio_rayon::spawn(move || {
            let mut input = Input::new(&prompt);
            for image in &request.images {
                input = input.with_image(image);
            }
            llm.inference_with_json_schema(&input, &generation, &request.schema)
        })
        .await?;
        Ok(output.text)
    }
}

pub(crate) fn models() -> Vec<Model> {
    catalog::MODELS
        .iter()
        .map(|descriptor| Model {
            provider: Provider::Local,
            model: Some(descriptor.id.to_owned()),
            name: descriptor.name.to_owned(),
            quantizations: descriptor
                .quantizations
                .iter()
                .map(|quantization| Quantization {
                    id: quantization.id.to_owned(),
                    name: quantization.name.to_owned(),
                    downloaded: std::iter::once(quantization.filename)
                        .chain(descriptor.projector)
                        .all(|filename| descriptor.file(filename).exists()),
                })
                .collect(),
            vision: descriptor.projector.is_some(),
            reasoning: descriptor.reasoning,
        })
        .collect()
}

pub(crate) fn supports_vision(selection: &ModelSelection) -> bool {
    selection.model.as_deref().is_some_and(|model| {
        catalog::MODELS
            .iter()
            .find(|descriptor| descriptor.id == model)
            .is_some_and(|descriptor| descriptor.projector.is_some())
    })
}
