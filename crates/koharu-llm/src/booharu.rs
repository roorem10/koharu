//! Booharu's reads of Koharu's models, declared with one marked line in
//! `lib.rs`: what a run knows of its translation model before it connects
//! one, as Booharu's pipeline hooks decide what a run asks while it's queued.

use crate::{KOHARU, TRANSLATION, local::catalog};

/// Whether `reference`, a `vendor/model` reference as the translator's
/// setting holds one, names one of Koharu's own models trained only to
/// translate, such as Index-Translate: its catalog entry's kind is
/// [`TRANSLATION`], as its spec's `details.kind` is once it connects. Such a
/// model keeps the plain request it was trained on. A quantization after a
/// colon is read as [`crate::connect`] reads one, and left out. False for
/// another vendor's model, whose kind isn't known before it connects, as an
/// LM Studio, Ollama, or OpenAI-compatible server lists none, and for an id
/// Koharu's catalog doesn't have.
pub fn translation_only(reference: &str) -> bool {
    let Some((KOHARU, id)) = reference.split_once('/') else {
        return false;
    };
    let id = id.split_once(':').map_or(id, |(id, _)| id);
    catalog::entry(id).is_ok_and(|entry| entry.kind == Some(TRANSLATION))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_koharus_translation_models_are_translation_only() {
        for id in [
            "index-translate-2b",
            "index-translate-9b",
            "index-translate-35b-a3b-preview",
        ] {
            assert!(translation_only(&format!("koharu/{id}")), "{id}");
            // At a quantization, as `connect` reads one.
            assert!(translation_only(&format!("koharu/{id}:Q8_0")), "{id}");
        }
        // A general model of Koharu's own, at its default and at a
        // quantization.
        assert!(!translation_only("koharu/qwen3.5-9b"));
        assert!(!translation_only("koharu/gemma4-e4b-it:Q4_K_M"));
        // Another vendor's, whatever it's called, and an id the catalog
        // doesn't have.
        for reference in [
            "openai/gpt-5.5",
            "lmstudio/index-translate-9b",
            "koharu/index-translate-1b",
            "index-translate-9b",
            "",
        ] {
            assert!(!translation_only(reference), "{reference}");
        }
        // The kind the catalog gives a model is the one its spec carries.
        let specs = crate::local::specs();
        let kind = |id: &str| {
            let spec = specs.iter().find(|spec| spec.id == id).expect("listed");
            spec.details.kind.clone()
        };
        assert_eq!(kind("index-translate-9b").as_deref(), Some(TRANSLATION));
        assert_eq!(kind("qwen3.5-9b"), None);
    }
}
