// Ported from:
// https://github.com/koharu-rs/koharu/blob/f4ce03999ed1ae2faaec938dd52c2f41a87d03d9/crates/koharu-llm/src/providers/deepl.rs

use anyhow::Context;
use koharu_secrets::ExposeSecret;
use reqwest::Client;
use serde::Deserialize;
use url::Url;

use super::send_json;
use crate::{Error, Language, Model, Provider, Result, TranslationRequest, TranslationSegment};

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(default)]
pub struct DeepLConfig {
    pub base_url: Option<Url>,
}

pub(super) async fn models() -> Result<Vec<Model>> {
    Ok(if koharu_secrets::get("deepl")?.is_some() {
        vec![Model::service(Provider::DeepL, "DeepL")]
    } else {
        Vec::new()
    })
}

pub(super) async fn translate(
    client: &Client,
    config: &DeepLConfig,
    request: &TranslationRequest,
) -> Result<Vec<String>> {
    let api_key = koharu_secrets::get("deepl")?.context("deepl API key is not configured")?;
    let target = target(request.target_language).ok_or(Error::UnsupportedLanguage {
        provider: "deepl",
        language: request.target_language,
    })?;
    let source = request
        .source_language
        .map(|language| {
            source(language).ok_or(Error::UnsupportedSourceLanguage {
                provider: "deepl",
                language,
            })
        })
        .transpose()?;
    let root = config.base_url.as_ref().map_or_else(
        || {
            if api_key.expose_secret().trim_end().ends_with(":fx") {
                "https://api-free.deepl.com".to_owned()
            } else {
                "https://api.deepl.com".to_owned()
            }
        },
        |url| url.as_str().trim_end_matches('/').to_owned(),
    );
    let mut translated = Vec::new();
    for (segments, context) in translation_batches(request)? {
        let mut form = segments
            .iter()
            .map(|segment| ("text", segment.text.as_str()))
            .collect::<Vec<_>>();
        form.push(("target_lang", target));
        if let Some(source) = source {
            form.push(("source_lang", source));
        }
        if !context.is_empty() {
            form.push(("context", &context));
        }
        let response: Response = send_json(
            "deepl",
            client
                .post(format!("{root}/v2/translate"))
                .header(
                    "Authorization",
                    format!("DeepL-Auth-Key {}", api_key.expose_secret()),
                )
                .form(&form),
        )
        .await?;
        if response.translations.len() != segments.len() {
            return Err(Error::SegmentCount {
                provider: "deepl",
                expected: segments.len(),
                actual: response.translations.len(),
            });
        }
        translated.extend(
            response
                .translations
                .into_iter()
                .map(|translation| translation.text),
        );
    }
    Ok(translated)
}

fn translation_batches(request: &TranslationRequest) -> Result<Vec<(&[TranslationSegment], String)>> {
    // DeepL applies one reference-only context to every text in a request. Keep
    // contextual boxes separate so one character's voice cannot leak to another.
    let context = request
        .context
        .iter()
        .map(|entry| entry.source.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if request
        .segments
        .iter()
        .all(|segment| segment.context.is_empty())
    {
        return Ok(vec![(&request.segments, context)]);
    }
    request
        .segments
        .iter()
        .map(|segment| {
            let mut context = context.clone();
            if !segment.context.is_empty() {
                if !context.is_empty() {
                    context.push('\n');
                }
                context.push_str(
                    &serde_json::to_string(&segment.context).map_err(anyhow::Error::from)?,
                );
            }
            Ok((std::slice::from_ref(segment), context))
        })
        .collect()
}

#[derive(Deserialize)]
struct Response {
    translations: Vec<Translation>,
}

#[derive(Deserialize)]
struct Translation {
    text: String,
}

fn target(language: Language) -> Option<&'static str> {
    use Language::*;
    Some(match language {
        ChineseSimplified => "ZH-HANS",
        ChineseTraditional => "ZH-HANT",
        English => "EN-US",
        French => "FR",
        Portuguese => "PT-PT",
        BrazilianPortuguese => "PT-BR",
        Spanish => "ES",
        Japanese => "JA",
        Turkish => "TR",
        Russian => "RU",
        Arabic => "AR",
        Korean => "KO",
        Thai => "TH",
        Italian => "IT",
        German => "DE",
        Vietnamese => "VI",
        Malay => "MS",
        Indonesian => "ID",
        Hindi => "HI",
        Polish => "PL",
        Czech => "CS",
        Dutch => "NL",
        Gujarati => "GU",
        Urdu => "UR",
        Telugu => "TE",
        Marathi => "MR",
        Hebrew => "HE",
        Bengali => "BN",
        Bulgarian => "BG",
        Tamil => "TA",
        Ukrainian => "UK",
        Kazakh => "KK",
        Belarusian => "BE",
        Hungarian => "HU",
        Filipino | Khmer | Burmese | Persian | Tibetan | Mongolian | Uyghur | Cantonese => {
            return None;
        }
    })
}

fn source(language: Language) -> Option<&'static str> {
    use Language::*;
    Some(match language {
        ChineseSimplified | ChineseTraditional => "ZH",
        English => "EN",
        Portuguese | BrazilianPortuguese => "PT",
        language => target(language)?.split('-').next()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_free_boxes_keep_the_original_batch_and_context_bytes() {
        let request = TranslationRequest::new(["one", "two"], Language::English)
            .with_context([
                crate::TranslationContext::new("previous one", "unused"),
                crate::TranslationContext::new("previous two", "unused"),
            ]);
        let batches = translation_batches(&request).unwrap();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].0, request.segments.as_slice());
        assert_eq!(batches[0].1, "previous one\nprevious two");
    }

    #[test]
    fn different_speakers_get_separate_native_contexts_in_input_order() {
        let mut request = TranslationRequest::new(["same", "same"], Language::English)
            .with_context([crate::TranslationContext::new("previous", "unused")]);
        request.segments[0].context.speaker = Some(crate::SpeakerHint {
            name: "Mika".into(),
            original: None,
            notes: Some("She speaks formally".into()),
        });
        request.segments[1].context.note = Some("A different speaker".into());
        let batches = translation_batches(&request).unwrap();
        assert_eq!(batches.len(), 2);
        for (index, (segments, context)) in batches.iter().enumerate() {
            assert_eq!(*segments, &request.segments[index..index + 1]);
            let (previous, metadata) = context.split_once('\n').unwrap();
            assert_eq!(previous, "previous");
            let value: serde_json::Value = serde_json::from_str(metadata).unwrap();
            assert_eq!(
                value,
                serde_json::to_value(&request.segments[index].context).unwrap()
            );
        }
    }

    #[test]
    fn unsupported_languages_are_not_substituted() {
        assert_eq!(target(Language::Filipino), None);
        assert_eq!(target(Language::English), Some("EN-US"));
    }
}
