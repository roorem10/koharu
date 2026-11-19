//! Which model reads a chapter's pages and which explains a line (D35,
//! T12): pure rules over koharu-llm's list of the models the user can pick,
//! with the settings the callers read and pass in, as this crate can't name
//! the agent's settings.
//!
//! A model can read pages when it sees images and calls tools, a general
//! model; one the list doesn't know counts as able, as Koharu counts models
//! it doesn't know. Koharu's default translator, Index-Translate, sees
//! images but only translates. A reference to one of Koharu's own models may
//! name a quantization after a colon, which the list leaves out.
//!
//! Before a model is connected this is all that is known: after, its spec is
//! checked again ([`crate::calls::usable`]).

use anyhow::{Result, bail};
use koharu_llm::{KOHARU, Model};
use serde::{Deserialize, Serialize};

/// Why a model can't read a chapter's pages.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CantRead {
    /// It sees no images.
    NoVision,
    /// It sees images but only translates, calling no tools, as Koharu's
    /// Index-Translate does.
    OnlyTranslates,
}

impl CantRead {
    /// Why `model` can't read the pages, in a sentence's words.
    pub fn why(self, model: &str) -> String {
        match self {
            Self::NoVision => format!("{model} has no vision, so it can't see the pages"),
            Self::OnlyTranslates => {
                format!("{model} only translates, so it can't read the chapter")
            }
        }
    }
}

/// The models Read chapter and Explain this line would use, as the Context
/// tab, Settings and the line card show them.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct ReadingModel {
    /// The model a read would use: the saved Read chapter model, else the
    /// translation model when it can read; none when neither can.
    pub model: Option<String>,
    /// Whether `model` is one of Koharu's own, which reads on this computer
    /// rather than through a provider's API.
    pub local: bool,
    /// The saved Read chapter model, `[booharu-reading] model`; none for the
    /// translation model.
    pub saved: Option<String>,
    /// Why the saved model can't read pages, which makes a read refuse
    /// rather than fall back; none when it can.
    pub saved_cant_read: Option<CantRead>,
    /// The translation model, `[pipeline] translator model`.
    pub translation: String,
    /// Why the translation model can't read pages; none when it can.
    pub translation_cant_read: Option<CantRead>,
    /// The model Explain this line would use; none when none is set, which
    /// disables it.
    pub explain: Option<String>,
}

impl ReadingModel {
    /// The model a read uses, or why none can, naming the model.
    pub fn reader(&self) -> Result<&str> {
        if let Some(model) = &self.model {
            return Ok(model);
        }
        match (
            &self.saved,
            self.saved_cant_read,
            self.translation_cant_read,
        ) {
            (Some(saved), Some(why), _) => bail!(
                "{}. Choose another Read chapter model in Settings > Pipeline.",
                why.why(saved)
            ),
            (_, _, why) => bail!(
                "No model can read the chapter: {}. Choose a Read chapter model in Settings > \
                 Pipeline.",
                why.unwrap_or(CantRead::NoVision).why(&self.translation)
            ),
        }
    }
}

/// The models a read and Explain this line use, of `models`, the list
/// koharu-llm gives: `saved`, the Read chapter model, `[booharu-reading]
/// model`, or the model a read overrides it with; `translation`, the
/// translation model; and `agent`, the agent's model, `[agent] model`.
pub fn reading_model(
    models: &[Model],
    saved: Option<&str>,
    translation: &str,
    agent: Option<&str>,
) -> ReadingModel {
    let saved_cant_read = saved.and_then(|saved| can_read(models, saved).err());
    let translation_cant_read = can_read(models, translation).err();
    let model = match saved {
        Some(saved) => saved_cant_read.is_none().then(|| saved.to_owned()),
        None => translation_cant_read
            .is_none()
            .then(|| translation.to_owned()),
    };
    ReadingModel {
        local: model.as_deref().is_some_and(local),
        model,
        saved: saved.map(str::to_owned),
        saved_cant_read,
        translation: translation.to_owned(),
        translation_cant_read,
        explain: explain_model(models, saved, translation, agent),
    }
}

/// Whether the model `reference` names can read a chapter's pages, as
/// `models` lists it, or why not: it must see images and call tools. A
/// model the list doesn't have counts as able.
pub fn can_read(models: &[Model], reference: &str) -> Result<(), CantRead> {
    match listed(models, reference) {
        Some(model) if !model.images => Err(CantRead::NoVision),
        Some(model) if !model.tools => Err(CantRead::OnlyTranslates),
        _ => Ok(()),
    }
}

/// The model Explain this line uses (T12): the Read chapter model `saved`
/// when one is set; else `translation`, the translation model, when it is a
/// general model, one `models` gives tools or doesn't know; else `agent`,
/// the agent's model; else none.
pub fn explain_model(
    models: &[Model],
    saved: Option<&str>,
    translation: &str,
    agent: Option<&str>,
) -> Option<String> {
    if let Some(saved) = saved {
        return Some(saved.to_owned());
    }
    if general(models, translation) {
        return Some(translation.to_owned());
    }
    agent.map(str::to_owned)
}

/// Whether the model `reference` names is a general model: one `models`
/// gives tools, or doesn't know.
pub fn general(models: &[Model], reference: &str) -> bool {
    listed(models, reference).is_none_or(|model| model.tools)
}

/// The model of `models` that `reference` names; for one of Koharu's own,
/// whatever quantization it names.
pub fn listed<'m>(models: &'m [Model], reference: &str) -> Option<&'m Model> {
    let reference = unquantized(reference);
    models.iter().find(|model| model.reference == reference)
}

/// Whether `reference` names one of Koharu's own models, which runs on this
/// computer.
pub fn local(reference: &str) -> bool {
    reference
        .split_once('/')
        .is_some_and(|(vendor, _)| vendor == KOHARU)
}

/// `reference` without the quantization one of Koharu's own may name after
/// a colon; another vendor's id may hold a colon of its own, as Ollama's
/// tags do.
fn unquantized(reference: &str) -> &str {
    match local(reference) {
        true => reference
            .split_once(':')
            .map_or(reference, |(model, _)| model),
        false => reference,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(reference: &str, images: bool, tools: bool) -> Model {
        let (vendor, name) = reference.split_once('/').expect("a reference");
        Model {
            reference: reference.to_owned(),
            name: name.to_owned(),
            vendor: vendor.to_owned(),
            images,
            tools,
        }
    }

    const TRANSLATE: &str = "koharu/index-translate-9b";
    const GEMMA: &str = "koharu/gemma4-12b-it";
    const TEXT: &str = "openai/text-only";
    const SEER: &str = "lmstudio/seer";

    fn models() -> Vec<Model> {
        vec![
            model(TRANSLATE, true, false),
            model(GEMMA, true, true),
            model(TEXT, false, true),
            model(SEER, true, false),
        ]
    }

    #[test]
    fn index_translate_and_a_model_without_tools_cant_read() {
        let models = models();
        assert_eq!(can_read(&models, TRANSLATE), Err(CantRead::OnlyTranslates));
        assert_eq!(
            can_read(&models, "koharu/index-translate-9b:Q8_0"),
            Err(CantRead::OnlyTranslates)
        );
        assert_eq!(can_read(&models, SEER), Err(CantRead::OnlyTranslates));
        assert_eq!(can_read(&models, TEXT), Err(CantRead::NoVision));
        assert_eq!(can_read(&models, "koharu/gemma4-12b-it:Q4_K_M"), Ok(()));
        // Only a model of Koharu's own names a quantization.
        assert_eq!(can_read(&models, "lmstudio/seer:latest"), Ok(()));
    }

    #[test]
    fn a_model_the_list_doesnt_know_counts_as_able() {
        let models = models();
        assert_eq!(can_read(&models, "anthropic/claude-new"), Ok(()));
        let reading = reading_model(&models, None, "anthropic/claude-new", None);
        assert_eq!(reading.model.as_deref(), Some("anthropic/claude-new"));
        assert!(!reading.local);
        assert_eq!(reading.reader().ok(), Some("anthropic/claude-new"));
    }

    #[test]
    fn the_read_uses_the_saved_model_else_the_translation_model_when_it_can() {
        let models = models();
        let reading = reading_model(&models, Some(GEMMA), TRANSLATE, None);
        assert_eq!(reading.model.as_deref(), Some(GEMMA));
        assert!(reading.local);
        assert_eq!(
            reading.translation_cant_read,
            Some(CantRead::OnlyTranslates)
        );
        let reading = reading_model(&models, None, TRANSLATE, None);
        assert_eq!(reading.model, None);
        let error = reading.reader().expect_err("no model");
        assert!(
            error.to_string().contains(TRANSLATE) && error.to_string().contains("only translates"),
            "{error}"
        );
    }

    #[test]
    fn a_saved_reading_model_that_cant_read_is_refused_by_name() {
        let models = models();
        // The translation model could read, but the saved model wins.
        let reading = reading_model(&models, Some(TEXT), GEMMA, None);
        assert_eq!(reading.model, None);
        assert_eq!(reading.saved_cant_read, Some(CantRead::NoVision));
        assert_eq!(reading.translation_cant_read, None);
        let error = reading.reader().expect_err("refused");
        assert!(
            error
                .to_string()
                .starts_with("openai/text-only has no vision"),
            "{error}"
        );
    }

    #[test]
    fn explain_falls_through_to_the_agents_model() {
        let models = models();
        assert_eq!(
            explain_model(&models, Some(TEXT), GEMMA, Some("openai/gpt")).as_deref(),
            Some(TEXT),
            "the Read chapter model first, though it sees no pages"
        );
        assert_eq!(
            explain_model(&models, None, GEMMA, Some("openai/gpt")).as_deref(),
            Some(GEMMA)
        );
        assert_eq!(
            explain_model(&models, None, "deepseek/unknown", None).as_deref(),
            Some("deepseek/unknown"),
            "a model the list doesn't know counts as general"
        );
        assert_eq!(
            explain_model(&models, None, TRANSLATE, Some("openai/gpt")).as_deref(),
            Some("openai/gpt")
        );
        assert_eq!(explain_model(&models, None, TRANSLATE, None), None);
        assert_eq!(
            reading_model(&models, None, TRANSLATE, Some("openai/gpt"))
                .explain
                .as_deref(),
            Some("openai/gpt")
        );
    }
}
