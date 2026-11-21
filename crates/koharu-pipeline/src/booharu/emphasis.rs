//! What a run's translation asks its model of the words to stress, and what
//! it does with the words it picks (S10-e):
//!
//! - [`asked`], from `queued`, reads `[booharu-emphasis]` into the snapshot
//!   of a run that translates, and has the run ask its model, sending
//!   [`INSTRUCTION`] after its Instructions, while the switch is on and its
//!   model isn't one of Koharu's trained only to translate.
//! - [`emphasis_schema`] and [`emphasis_answers`], in a turn that asks, are
//!   the answer's shape, each text an object of its translation and the
//!   words to stress, and the words read back from it into the turn's slot,
//!   its texts going on as Koharu's plain answer gives them.
//! - [`picked`], once the turn passed, gives each block its words, which the
//!   write hands to `translated`, which marks them.
//! - [`stressed`] and [`numbered`], in `seed`, give the page before's answer
//!   that shape for a run that asks, each block's words those the marks its
//!   run gave it still cover.

use std::collections::{BTreeSet, HashMap};

use anyhow::Context as _;
use booharu_core::{attrs::Auto, emphasis::EmphasisSettings, settings::Snapshot};
use icu_locale_core::LanguageIdentifier;
use koharu_scene::{Layer, LayerId, RichText};
use parking_lot::Mutex;
use serde_json::{Map, Value, json};

use crate::{Stage, TranslatorConfig};

/// What a run that asks its model for the words to stress sends after its
/// Instructions, as a paragraph of its own.
pub(crate) const INSTRUCTION: &str = "Each value is an object: text is the translation, plain, \
     without markup; emphasis lists exact words or phrases from text to emphasise, a first-draft \
     lettering suggestion the editor reviews. Mark a word only where a native letterer of the \
     target language would stress it: shouting, a word the source clearly stresses (bold, \
     larger, or set apart in the art), or a word like a sound effect. Most lines need none, so \
     give [] for them.";

/// `booharu` and `translator`, a run of `stages`'s as `queued` has them so
/// far, with `[booharu-emphasis]` as `read` reads it in the snapshot of a
/// run that translates, and whether that run asks its model which words to
/// stress: while the switch is on, unless its model is one of Koharu's
/// trained only to translate ([`koharu_llm::booharu::translation_only`]),
/// which keeps the plain request it was trained on. A run that asks sends
/// [`INSTRUCTION`] after its Instructions, the chapter's glossary and notes
/// among them, as a paragraph of its own, so its conversation starts again
/// whenever the switch or the model changes whether it asks; any other
/// keeps them byte for byte, so its request is Koharu's. A run that doesn't
/// translate never reads the section; one that does is refused when it
/// can't be read, naming it, as a bad stored value is an error.
pub(crate) fn asked(
    booharu: Snapshot,
    translator: TranslatorConfig,
    stages: &BTreeSet<Stage>,
    read: impl FnOnce() -> anyhow::Result<EmphasisSettings>,
) -> crate::Result<(Snapshot, TranslatorConfig)> {
    if !stages.contains(&Stage::Translate) {
        return Ok((booharu, translator));
    }
    let emphasis = read().map_err(|error| crate::Error::invalid(format!("{error:#}")))?;
    let asks = emphasis.ai && !koharu_llm::booharu::translation_only(&translator.model);
    let translator = match asks {
        true => TranslatorConfig {
            instructions: instructions(&translator.instructions),
            ..translator
        },
        false => translator,
    };
    let booharu = Snapshot {
        emphasis: Some(emphasis),
        asks_emphasis: asks,
        ..booharu
    };
    Ok((booharu, translator))
}

/// `own`, a run's Instructions, with [`INSTRUCTION`] after them, one blank
/// line between, as the chapter's glossary and notes go after them.
fn instructions(own: &str) -> String {
    match own.trim_end() {
        "" => INSTRUCTION.to_owned(),
        own => format!("{own}\n\n{INSTRUCTION}"),
    }
}

/// The answer's shape for a turn that asks for the words to stress: an
/// object with, for each of `keys`, an object of the text's translation,
/// plain, then the words of it to stress, a list of strings, as
/// [`INSTRUCTION`] tells them. Every property is required and nothing
/// bounded, as in Koharu's own schema, so rig's rewrite toward strict mode
/// leaves it as it is. The translation comes first, as serde_json keeps an
/// object's order wherever llguidance, which holds Koharu's own models to a
/// schema, is built: a model held to it writes the text before the words it
/// picks from it.
pub(crate) fn emphasis_schema(keys: &[String]) -> anyhow::Result<schemars::Schema> {
    let mut line = Map::new();
    line.insert("text".to_owned(), json!({"type": "string"}));
    line.insert(
        "emphasis".to_owned(),
        json!({"type": "array", "items": {"type": "string"}}),
    );
    let line = json!({
        "type": "object",
        "properties": line,
        "required": ["text", "emphasis"],
        "additionalProperties": false,
    });
    let properties: Map<String, Value> =
        keys.iter().map(|key| (key.clone(), line.clone())).collect();
    schemars::Schema::try_from(json!({
        "type": "object",
        "properties": properties,
        "required": keys,
        "additionalProperties": false,
    }))
    .context("the answer's schema isn't valid")
}

/// `answer`, a turn's, as Koharu's own turn reads it, which checks its
/// texts: for a turn that asks for the words to stress, whose `picks` is
/// its slot, each object's text in its place, and the words each of `keys`
/// picks, in order, in `picks`, over any an earlier answer left there, so
/// the answer a turn asks again after an echo gives the words. A text given
/// as a plain string, as Koharu's own schema asks, picks none. A missing or
/// malformed list of words picks none, and a pick that isn't a string is
/// left out, each logged; each word is trimmed, and a blank one left out.
/// An answer that isn't an object, and a text without a translation or
/// with one that isn't a string, go on as they are, so Koharu's checks
/// fail them in their own words. With no slot, `answer` is Koharu's
/// answer, as it is.
pub(crate) fn emphasis_answers(
    answer: String,
    keys: &[String],
    picks: Option<&Mutex<Vec<Vec<String>>>>,
) -> String {
    let Some(picks) = picks else {
        return answer;
    };
    let Ok(Value::Object(values)) = serde_json::from_str::<Value>(&answer) else {
        return answer;
    };
    let mut texts = Map::new();
    let mut words: HashMap<String, Vec<String>> = HashMap::new();
    for (key, value) in values {
        let text = match value {
            Value::Object(mut value) => {
                words.insert(key.clone(), self::words(&key, value.remove("emphasis")));
                value.remove("text")
            }
            text => Some(text),
        };
        if let Some(text) = text {
            texts.insert(key, text);
        }
    }
    *picks.lock() = keys
        .iter()
        .map(|key| words.remove(key).unwrap_or_default())
        .collect();
    Value::Object(texts).to_string()
}

/// The words `emphasis`, the answer's list for text `key`, picks: each
/// trimmed, a blank one left out; none for a missing or malformed list, and
/// a pick that isn't a string left out, each logged.
fn words(key: &str, emphasis: Option<Value>) -> Vec<String> {
    let picks = match emphasis {
        Some(Value::Array(picks)) => picks,
        Some(other) => {
            tracing::debug!("the answer's emphasis for text {key} isn't a list but {other}");
            return Vec::new();
        }
        None => {
            tracing::debug!("the answer gives text {key} no emphasis");
            return Vec::new();
        }
    };
    picks
        .into_iter()
        .filter_map(|pick| match pick {
            Value::String(word) => Some(word.trim().to_owned()).filter(|word| !word.is_empty()),
            other => {
                tracing::debug!("the answer's emphasis for text {key} holds {other}, no word");
                None
            }
        })
        .collect()
}

/// The words the model picked to stress in each of `translated`'s answers,
/// `picks` holding them in the same order, by block, for the write: none
/// for a run that didn't ask, whose slot stays empty.
pub(crate) fn picked(
    translated: &[(LayerId, LanguageIdentifier, String)],
    picks: Vec<Vec<String>>,
) -> HashMap<LayerId, Vec<String>> {
    translated
        .iter()
        .map(|(block, ..)| *block)
        .zip(picks)
        .filter(|(_, words)| !words.is_empty())
        .collect()
}

/// The words of `translation`, `layer`'s, that the marks its run gave it
/// cover, while its hand-edit record trusts them ([`Auto::trusted_marks`]),
/// in order: the words the run's model picked to stress there, as it
/// marked them.
pub(super) fn stressed(layer: &Layer, translation: &RichText) -> Vec<String> {
    let text = translation.text();
    let record = Auto::of(layer);
    record
        .trusted_marks(translation)
        .into_iter()
        .filter_map(|mark| text.get(mark.start..mark.end).map(str::to_owned))
        .collect()
}

/// A page's answer in the shape a turn that asks for the words to stress
/// has its model write one, laid out as Koharu's `turn::numbered` lays out
/// a page: an object of `keys` to each of `translations` and the words of
/// it in `stressed`, a text a line, in order, the translation first, as the
/// schema lists it.
pub(super) fn numbered(
    keys: &[String],
    translations: &[String],
    stressed: &[Vec<String>],
) -> String {
    let lines = keys
        .iter()
        .zip(translations)
        .zip(stressed)
        .map(|((key, text), words)| {
            format!(
                "  {}: {{\"text\": {}, \"emphasis\": {}}}",
                json!(key),
                json!(text),
                json!(words)
            )
        })
        .collect::<Vec<_>>();
    format!("{{\n{}\n}}", lines.join(",\n"))
}

/// What `f` logs at the debug level and above, as it returns, with what it
/// returns.
#[cfg(test)]
pub(crate) fn logged<R>(f: impl FnOnce() -> R) -> (R, String) {
    #[derive(Clone, Default)]
    struct Lines(std::sync::Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Lines {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let lines = Lines::default();
    let writer = lines.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    let result = tracing::subscriber::with_default(subscriber, f);
    let text = String::from_utf8_lossy(&lines.0.lock()).into_owned();
    (result, text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stages::translate::turn::keys;

    fn snapshot() -> Snapshot {
        Snapshot {
            redo: true,
            ..Snapshot::default()
        }
    }

    fn run(stages: &[Stage]) -> BTreeSet<Stage> {
        stages.iter().copied().collect()
    }

    /// The Instructions a run of `stages` sends with the model `model` and
    /// its own `own`, the section reading `emphasis`, with what its snapshot
    /// asks.
    fn sent(
        model: &str,
        own: &str,
        stages: &[Stage],
        emphasis: EmphasisSettings,
    ) -> crate::Result<(Snapshot, TranslatorConfig)> {
        let translator = TranslatorConfig {
            model: model.to_owned(),
            instructions: own.to_owned(),
            ..TranslatorConfig::default()
        };
        asked(snapshot(), translator, &run(stages), || Ok(emphasis))
    }

    #[test]
    fn a_general_model_is_asked_after_the_instructions_and_a_translation_model_never()
    -> crate::Result<()> {
        let own = "Keep honorifics.\n\nGlossary (mandatory: use these translations and keep \
                   their spelling):\n- どうして → Why\n";
        let on = EmphasisSettings::default();
        // A general model, Koharu's own or a hosted one, is asked, the
        // instruction a paragraph of its own after the Instructions, the
        // chapter's glossary among them.
        for model in [
            "koharu/qwen3.5-9b",
            "openai/gpt-5.5",
            "lmstudio/index-translate-9b",
        ] {
            let (booharu, translator) = sent(model, own, &[Stage::Translate], on.clone())?;
            assert!(booharu.asks_emphasis, "{model}");
            assert_eq!(
                translator.instructions,
                format!("{}\n\n{INSTRUCTION}", own.trim_end()),
                "{model}"
            );
            assert_eq!(booharu.emphasis.as_ref(), Some(&on));
            // The rest of the snapshot is as `queued` had it.
            assert_eq!(
                Snapshot {
                    emphasis: None,
                    asks_emphasis: false,
                    ..booharu
                },
                snapshot()
            );
        }
        // With no Instructions, the instruction alone.
        let (_, bare) = sent("openai/gpt-5.5", " \n", &[Stage::Translate], on.clone())?;
        assert_eq!(bare.instructions, INSTRUCTION);

        // Index-Translate keeps Koharu's request: the Instructions byte for
        // byte, and the rest, at any quantization.
        for model in [
            "koharu/index-translate-9b",
            "koharu/index-translate-2b:Q8_0",
        ] {
            let (booharu, translator) = sent(model, own, &[Stage::Translate], on.clone())?;
            assert!(!booharu.asks_emphasis, "{model}");
            assert_eq!(
                translator,
                TranslatorConfig {
                    model: model.to_owned(),
                    instructions: own.to_owned(),
                    ..TranslatorConfig::default()
                }
            );
            // It still takes the section, so its translation takes the
            // marks a run left off the one it replaces.
            assert_eq!(booharu.emphasis.as_ref(), Some(&on));
        }
        // So does a run with the switch off, whatever its model.
        let off = EmphasisSettings {
            ai: false,
            ..EmphasisSettings::default()
        };
        let (booharu, translator) = sent("openai/gpt-5.5", own, &[Stage::Translate], off.clone())?;
        assert!(!booharu.asks_emphasis);
        assert_eq!(translator.instructions, own);
        assert_eq!(booharu.emphasis, Some(off));
        Ok(())
    }

    #[test]
    fn a_run_that_doesnt_translate_never_reads_the_section_and_an_unreadable_one_refuses_one_that_does()
    -> crate::Result<()> {
        let own = "Keep honorifics.";
        let translator = TranslatorConfig {
            model: "openai/gpt-5.5".to_owned(),
            instructions: own.to_owned(),
            ..TranslatorConfig::default()
        };
        let unread = || -> anyhow::Result<EmphasisSettings> { panic!("the section was read") };
        let stages = run(&[Stage::Detect, Stage::Recognize, Stage::Inpaint]);
        let (booharu, kept) = asked(snapshot(), translator.clone(), &stages, unread)?;
        assert_eq!((booharu, kept), (snapshot(), translator.clone()));

        // A settings file that isn't TOML.
        let unreadable = || {
            Err(anyhow::anyhow!("config.toml is invalid: TOML parse error")
                .context("Booharu's emphasis can't be read from [booharu-emphasis]"))
        };
        let refused = asked(
            snapshot(),
            translator,
            &run(&[Stage::Translate]),
            unreadable,
        )
        .expect_err("an unreadable section");
        assert!(
            matches!(&refused, crate::Error::Invalid(message)
                if message.contains("[booharu-emphasis]") && message.contains("TOML parse error")),
            "{refused}"
        );
        Ok(())
    }

    #[test]
    fn the_answers_shape_is_an_object_of_the_text_then_its_words_for_each_key() -> anyhow::Result<()>
    {
        let keys = keys(2);
        let schema = emphasis_schema(&keys)?;
        let line = json!({
            "type": "object",
            "properties": {
                "text": {"type": "string"},
                "emphasis": {"type": "array", "items": {"type": "string"}},
            },
            "required": ["text", "emphasis"],
            "additionalProperties": false,
        });
        assert_eq!(
            schema.as_value(),
            &json!({
                "type": "object",
                "properties": {"1": line, "2": line},
                "required": ["1", "2"],
                "additionalProperties": false,
            })
        );
        // The translation first, as a model held to the schema writes it.
        let order: Vec<&String> = schema.as_value()["properties"]["1"]["properties"]
            .as_object()
            .context("an object")?
            .keys()
            .collect();
        assert_eq!(order, ["text", "emphasis"]);
        // Strict: every object requires each of its properties and allows
        // no other, and nothing bounds a count, a length, or a number.
        fn strict(value: &Value) -> bool {
            match value {
                Value::Object(object) => {
                    let bounded = object.keys().any(|key| {
                        key.starts_with("min") || key.starts_with("max") || key.contains("imum")
                    });
                    let closed = match object.get("properties") {
                        Some(Value::Object(properties)) => {
                            let required = object.get("required").and_then(Value::as_array);
                            object.get("additionalProperties") == Some(&json!(false))
                                && required.is_some_and(|required| {
                                    properties.keys().all(|key| required.contains(&json!(key)))
                                })
                        }
                        _ => true,
                    };
                    !bounded && closed && object.values().all(strict)
                }
                Value::Array(values) => values.iter().all(strict),
                _ => true,
            }
        }
        assert!(strict(schema.as_value()));
        Ok(())
    }

    #[test]
    fn an_answer_gives_its_texts_as_koharus_and_its_words_to_the_slot() {
        let keys = keys(3);
        let picks = Mutex::new(vec![vec!["stale".to_owned()]]);
        let answer = json!({
            "1": {"text": " No way! ", "emphasis": [" No ", "", "way!", "  "]},
            // A plain string, as Koharu's own schema asks: no words.
            "2": "Why?",
            "3": {"text": "Bam", "emphasis": []},
        });
        let (plain, log) = logged(|| emphasis_answers(answer.to_string(), &keys, Some(&picks)));
        assert_eq!(
            serde_json::from_str::<Value>(&plain).ok(),
            Some(json!({"1": " No way! ", "2": "Why?", "3": "Bam"}))
        );
        // Over what an earlier answer left, in the keys' order, trimmed,
        // blank picks left out.
        assert_eq!(
            *picks.lock(),
            [
                vec!["No".to_owned(), "way!".to_owned()],
                Vec::new(),
                Vec::new()
            ]
        );
        assert!(!log.contains("emphasis"), "{log}");
    }

    #[test]
    fn a_malformed_list_or_pick_is_left_out_and_logged_naming_its_text() {
        let keys = keys(4);
        let picks = Mutex::new(Vec::new());
        let answer = json!({
            "1": {"text": "One", "emphasis": "One"},
            "2": {"text": "Two"},
            "3": {"text": "Three", "emphasis": [3, "Three", null]},
            "4": {"text": "Four", "emphasis": null},
        });
        let (plain, log) = logged(|| emphasis_answers(answer.to_string(), &keys, Some(&picks)));
        assert_eq!(
            serde_json::from_str::<Value>(&plain).ok(),
            Some(json!({"1": "One", "2": "Two", "3": "Three", "4": "Four"}))
        );
        assert_eq!(
            *picks.lock(),
            [Vec::new(), Vec::new(), vec!["Three".to_owned()], Vec::new()]
        );
        for logged in [
            "the answer's emphasis for text 1 isn't a list",
            "the answer gives text 2 no emphasis",
            "the answer's emphasis for text 3 holds 3",
            "the answer's emphasis for text 3 holds null",
            "the answer's emphasis for text 4 isn't a list but null",
        ] {
            assert!(log.contains(logged), "{logged}: {log}");
        }
    }

    #[test]
    fn a_text_that_fails_koharus_checks_reaches_them_as_it_is() {
        let keys = keys(1);
        let picks = Mutex::new(Vec::new());
        let read = |answer: &str| emphasis_answers(answer.to_owned(), &keys, Some(&picks));
        // What isn't an object goes on byte for byte.
        for answer in ["No way", "[\"No way\"]", "{\"1\": "] {
            assert_eq!(read(answer), answer);
        }
        // A text without its translation is left out, so Koharu finds it
        // missing; a translation that isn't text, or is blank, stays.
        let value = |answer: Value| serde_json::from_str::<Value>(&read(&answer.to_string())).ok();
        assert_eq!(value(json!({"1": {"emphasis": ["No"]}})), Some(json!({})));
        assert_eq!(
            value(json!({"1": {"text": 3, "emphasis": []}})),
            Some(json!({"1": 3}))
        );
        assert_eq!(
            value(json!({"1": {"text": " ", "emphasis": []}, "2": "Two"})),
            Some(json!({"1": " ", "2": "Two"}))
        );
        // Without a slot, the answer is Koharu's, as it is.
        let answer = json!({"1": {"text": "No", "emphasis": ["No"]}}).to_string();
        assert_eq!(emphasis_answers(answer.clone(), &keys, None), answer);
    }

    #[test]
    fn each_block_takes_the_words_of_its_own_answer() {
        let ids: Vec<LayerId> = (1..=3)
            .map(|n| {
                format!("01a1ee94-9e1f-7088-bee4-f54661aa2bd{n}")
                    .parse()
                    .expect("an id")
            })
            .collect();
        let translated: Vec<(LayerId, LanguageIdentifier, String)> = ids
            .iter()
            .map(|&id| (id, icu_locale_core::langid!("ja"), "text".to_owned()))
            .collect();
        let picks = vec![vec!["No".to_owned()], Vec::new(), vec!["Bam".to_owned()]];
        assert_eq!(
            picked(&translated, picks),
            HashMap::from([
                (ids[0], vec!["No".to_owned()]),
                (ids[2], vec!["Bam".to_owned()]),
            ])
        );
        // A run that didn't ask has none.
        assert!(picked(&translated, Vec::new()).is_empty());
    }

    #[test]
    fn the_page_befores_answer_is_laid_out_as_a_turn_lays_out_a_page() {
        let keys = keys(2);
        assert_eq!(
            numbered(
                &keys,
                &["No \"way\"!".to_owned(), "Sensei!".to_owned()],
                &[vec!["No".to_owned(), "way".to_owned()], Vec::new()],
            ),
            "{\n  \"1\": {\"text\": \"No \\\"way\\\"!\", \"emphasis\": [\"No\",\"way\"]},\n  \
             \"2\": {\"text\": \"Sensei!\", \"emphasis\": []}\n}"
        );
        // It reads back as an answer in the shape.
        let picks = Mutex::new(Vec::new());
        let answer = numbered(
            &keys,
            &["Hi".to_owned(), "Yo".to_owned()],
            &[vec!["Hi".to_owned()], Vec::new()],
        );
        let plain = emphasis_answers(answer, &keys, Some(&picks));
        assert_eq!(
            serde_json::from_str::<Value>(&plain).ok(),
            Some(json!({"1": "Hi", "2": "Yo"}))
        );
        assert_eq!(*picks.lock(), [vec!["Hi".to_owned()], Vec::new()]);
    }
}
