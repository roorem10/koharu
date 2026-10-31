use anyhow::Result;
use async_trait::async_trait;
use koharu_scene::{Authored, LanguageTag, Origin, SourceText, Translation};
use koharu_translator::{TranslationRequest, Translator};

use crate::TranslationConfig;

use super::{StageInput, StageProcessor, finish, generation};

const PRODUCER: &str = "dev.koharu.pipeline.translation";

pub(super) struct Processor {
    config: TranslationConfig,
    translator: Translator,
}

impl Processor {
    pub(super) fn new(config: TranslationConfig, translator: Translator) -> Self {
        Self { config, translator }
    }
}

#[async_trait]
impl StageProcessor for Processor {
    fn model(&self) -> &'static str {
        Translator::model(&self.config.model)
    }

    fn skip(&self, input: &StageInput) -> Result<bool> {
        if !input.resume {
            return Ok(false);
        }
        if let Some(group) = input.scene.page(input.page)?.text_group()? {
            for layer in group.text_layers()? {
                if input.contains_entity(layer.id())?
                    && !input.translation_hints.skip.contains(&layer.id())
                    && !layer.content()?.translation()?.is_some_and(|translation| {
                        !translation.text.value.trim().is_empty()
                            || matches!(translation.text.origin, Origin::User)
                    })
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    fn unload(&self) -> bool {
        self.translator.unload()
    }

    async fn load(&self) -> Result<()> {
        self.translator.load_model(&self.config.model).await
    }

    async fn process(&self, input: StageInput) -> Result<koharu_scene::Patch> {
        let mut targets = Vec::new();
        if let Some(group) = input.scene.page(input.page)?.text_group()? {
            for layer in group.text_layers()? {
                if !input.contains_entity(layer.id())?
                    || input.translation_hints.skip.contains(&layer.id())
                {
                    continue;
                }
                let content = layer.content()?;
                if content.translation()?.is_some_and(|translation| {
                    matches!(translation.text.origin, Origin::User)
                        || (input.resume && !translation.text.value.trim().is_empty())
                }) {
                    continue;
                }
                let Some(source) = content.source()? else {
                    continue;
                };
                if !source.text.value.trim().is_empty() {
                    targets.push((layer.id(), content.id(), source.text.value));
                }
            }
        }
        let mut request = TranslationRequest::new(
            targets.iter().map(|(_, _, source)| source.clone()),
            self.config.target_language,
        );
        input
            .translation_hints
            .apply(&mut request, &input.scene, input.page)?;
        if let Some(instructions) = self.config.instructions.as_deref() {
            request = request.with_instructions(instructions);
        }
        if Translator::supports_vision(&self.config.model, &self.config.generation)
            && let Some(image) = input.images.get(&input.scene, input.page, "source").await?
        {
            request = request.with_image(image);
        }
        let (provider, translated) = self
            .translator
            .translate(&self.config.model, self.config.generation, request)
            .await?;
        let language = LanguageTag::new(self.config.target_language.tag())?;
        let generated = generation(PRODUCER, provider)?;
        let mut edit = input.scene.edit_as(generated.clone());
        for (_, entity, _) in &targets {
            edit.observe::<SourceText>(*entity)?;
            edit.observe::<Translation>(*entity)?;
        }
        for ((layer, entity, source), segment) in targets.into_iter().zip(translated) {
            if input
                .scene
                .component::<Translation>(entity)?
                .is_some_and(|value| matches!(value.text.origin, Origin::User))
            {
                continue;
            }
            write_translation(
                &mut edit, layer, entity, &source, segment, &language, &generated,
            )?;
        }
        finish(edit)
    }
}

fn write_translation(
    edit: &mut koharu_scene::Edit,
    layer: koharu_scene::EntityId,
    content: koharu_scene::EntityId,
    source: &str,
    mut segment: koharu_translator::TranslatedSegment,
    language: &LanguageTag,
    generated: &koharu_scene::Generation,
) -> koharu_scene::Result<()> {
    if source.trim() == "\u{2026}" {
        segment.text = "\u{2026}".to_owned();
        segment.emphasis.clear();
    }
    edit.set(
        content,
        &Translation {
            text: Authored::generated(segment.text, generated.clone()),
            language: Some(language.clone()),
        },
    )?;
    edit.set(
        layer,
        &crate::EmphasisHints {
            origin: Origin::Generated(generated.clone()),
            words: segment.emphasis,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use koharu_scene::{At, PageDraft, Session, TextLayout, TextLayoutKind};

    #[tokio::test]
    async fn translation_and_hints_commit_together_and_empty_hints_replace_old_ones() {
        let mut session = Session::memory().await.unwrap();
        let mut ids = None;
        let patch = session
            .snapshot()
            .patch(|edit| {
                let page = edit.add_page(PageDraft::new("page", 100.0, 100.0), At::End)?;
                let content = edit.add_text_content(page, At::End)?;
                edit.set(
                    content,
                    &SourceText {
                        text: Authored::user("嫌".to_owned()),
                        language: None,
                    },
                )?;
                let layer = edit.add_text_layer(
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
                ids = Some((layer, content));
                Ok(())
            })
            .unwrap();
        session.commit(patch).await.unwrap();
        let (layer, content) = ids.unwrap();
        let generation = generation(PRODUCER, "test").unwrap();
        let language = LanguageTag::new("en-US").unwrap();
        for words in [vec!["NO".to_owned()], Vec::new()] {
            let mut edit = session.snapshot().edit_as(generation.clone());
            write_translation(
                &mut edit,
                layer,
                content,
                "嫌",
                koharu_translator::TranslatedSegment {
                    text: "NO!".into(),
                    emphasis: words.clone(),
                },
                &language,
                &generation,
            )
            .unwrap();
            session.commit(edit.finish().unwrap()).await.unwrap();
            let snapshot = session.snapshot();
            let hints = snapshot
                .component::<crate::EmphasisHints>(layer)
                .unwrap()
                .unwrap();
            assert_eq!(hints.words, words);
            assert_eq!(hints.origin, Origin::Generated(generation.clone()));
            assert_eq!(
                snapshot
                    .component::<Translation>(content)
                    .unwrap()
                    .unwrap()
                    .text
                    .value,
                "NO!"
            );
            assert!(
                snapshot
                    .component::<crate::EmphasisHints>(content)
                    .unwrap()
                    .is_none()
            );
        }
    }
}
