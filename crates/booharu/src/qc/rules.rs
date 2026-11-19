//! The quality check's rules: pure, over the facts the check gathers about
//! one block or text layer, so each is tested without a project, a font, or
//! a renderer.

use std::collections::BTreeMap;

use booharu_core::{context::GlossaryEntry, lettering::LetteringCategory};
use koharu_scene::LanguageIdentifier;

use super::{Majorities, Problem, measure::Measured};

/// What the checks read of one block or text layer.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Facts {
    /// Whether it's a block: a text layer gets only the size checks and
    /// flags.
    pub block: bool,
    /// Its source's wording.
    pub source: String,
    /// Its translation's wording; none when it has no translation.
    pub translation: Option<String>,
    /// Its translation's language tag; none when it has no translation.
    pub language: Option<LanguageIdentifier>,
    /// Whether its clean-up shows: a line keeping the page's own lettering
    /// hides it.
    pub cleanup_visible: bool,
    /// Whether Koharu's Detection stage made it.
    pub detected: bool,
    /// Whether Koharu's Recognition stage read it.
    pub read: bool,
    /// Whether Koharu's Translation stage gave its source back unchanged.
    pub given_back: bool,
    /// Whether Koharu's Translation stage ran on its page.
    pub translated_page: bool,
    /// Its lettering category.
    pub category: Option<LetteringCategory>,
    /// Its translation's font family; none for the default font.
    pub font: Option<String>,
    /// Whether its font was set by hand, or by the agent on purpose.
    pub hand_set: bool,
    /// Its translation, or a text layer's text, as drawn; none when it has
    /// no text to measure.
    pub measured: Option<Measured>,
    /// The glossary terms its translation leaves out, in the order they
    /// occur.
    pub slips: Vec<GlossaryEntry>,
    /// The agent's flags on it, by their messages.
    pub flags: Vec<String>,
}

/// What the rules judge a layer's facts by.
pub(crate) struct Judge<'a> {
    /// The smallest readable size on the page, in page pixels.
    pub limit: f32,
    /// The translator's target language, which a translation without a tag
    /// of its own is in.
    pub target: &'a LanguageIdentifier,
    /// Each lettering category's majority font in the chapter.
    pub majorities: &'a Majorities,
}

/// What `facts`' layer needs a look for, in the checks' order: overflow,
/// too small, a small word, a missing translation, an empty source,
/// leftover source text, an odd font, glossary slips, then the agent's
/// flags. A text layer gets only the first three and flags.
pub(crate) fn problems(facts: &Facts, judge: &Judge) -> Vec<Problem> {
    let mut problems = facts
        .measured
        .as_ref()
        .map(|measured| sizes(measured, judge.limit))
        .unwrap_or_default();
    if facts.block {
        if missing_translation(facts) {
            problems.push(Problem::MissingTranslation);
        }
        if empty_source(facts) {
            problems.push(Problem::EmptySource);
        }
        if let Some(unchanged) = leftover(facts, judge.target) {
            problems.push(Problem::LeftoverSource { unchanged });
        }
        problems.extend(odd_font(facts, judge.majorities));
        problems.extend(facts.slips.iter().map(|term| Problem::Glossary {
            source: term.source.clone(),
            target: term.target.clone(),
        }));
    }
    problems.extend(facts.flags.iter().map(|message| Problem::Flag {
        message: message.clone(),
    }));
    problems
}

/// The size checks for `measured` against `limit`: overflow when it runs
/// past its box, then too small when its smallest run and its box's size
/// are both under `limit`, or a small word when only its smallest run is,
/// told by its text. Overflow and too small can both hold, as a box that
/// shrinks to half its size and still overflows. Sizes are compared as
/// they're told, rounded to one decimal, so a finding never reads 12.0
/// under 12.0.
pub(crate) fn sizes(measured: &Measured, limit: f32) -> Vec<Problem> {
    let mut problems = Vec::new();
    if measured.overflowed {
        problems.push(Problem::Overflow);
    }
    let limit = rounded(limit);
    let smallest = measured
        .runs
        .iter()
        .min_by(|a, b| a.size.total_cmp(&b.size));
    if let Some(run) = smallest {
        let size = rounded(run.size);
        if size < limit {
            problems.push(if rounded(measured.base) < limit {
                Problem::TooSmall { size, limit }
            } else {
                Problem::SmallWord {
                    word: run.text.clone(),
                    size,
                    limit,
                }
            });
        }
    }
    problems
}

/// `size` rounded to one decimal, as a finding tells it.
fn rounded(size: f32) -> f32 {
    (size * 10.0).round() / 10.0
}

/// Whether the block has a source but no translation, on a page Koharu's
/// Translation stage ran on, and shows its clean-up: a line keeping the
/// page's lettering, given back unchanged or with its clean-up hidden by
/// hand, isn't missing one.
fn missing_translation(facts: &Facts) -> bool {
    !facts.source.trim().is_empty()
        && written(facts).is_none()
        && facts.cleanup_visible
        && facts.translated_page
}

/// Whether Koharu's stages found the block and read it, and read nothing.
fn empty_source(facts: &Facts) -> bool {
    facts.detected && facts.read && facts.source.trim().is_empty()
}

/// Whether the block shows source text a translation into its language
/// keeps only by mistake: some in its translation, unless the translation
/// is its source, trimmed (`false`), or, on a line given back unchanged,
/// some in its source, which the page still shows (`true`). Its language is
/// its translation's tag, or `target`'s with no translation or an
/// undetermined tag.
fn leftover(facts: &Facts, target: &LanguageIdentifier) -> Option<bool> {
    let target = facts
        .language
        .as_ref()
        .filter(|tag| facts.translation.is_some() && !tag.language.is_unknown())
        .unwrap_or(target);
    let kept = |text: &str| {
        text.chars()
            .any(|character| leftover_character(target, character))
    };
    match written(facts) {
        Some(translation) => {
            (translation.trim() != facts.source.trim() && kept(translation)).then_some(false)
        }
        None => (facts.given_back && kept(&facts.source)).then_some(true),
    }
}

/// The block's translation's wording, when it isn't blank.
fn written(facts: &Facts) -> Option<&str> {
    facts
        .translation
        .as_deref()
        .filter(|translation| !translation.trim().is_empty())
}

/// A script source text is in that only some languages write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Script {
    /// Hiragana, katakana, their phonetic extensions, and half-width
    /// katakana.
    Kana,
    /// CJK ideographs, with extension A and the supplementary planes'.
    Han,
    /// Hangul syllables, jamo, and compatibility jamo.
    Hangul,
}

/// The script `character` is in, of the three source scripts.
fn script(character: char) -> Option<Script> {
    match u32::from(character) {
        0x3040..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF9F => Some(Script::Kana),
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0x2_0000..=0x2_FFFF | 0x3_0000..=0x3_23AF => {
            Some(Script::Han)
        }
        0xAC00..=0xD7AF | 0x1100..=0x11FF | 0x3130..=0x318F => Some(Script::Hangul),
        _ => None,
    }
}

/// The Chinese languages, each written in Han: the macrolanguage `zh` and
/// the languages ISO 639-3 counts in it, Cantonese, `yue`, which Koharu
/// offers as a target, among them.
const CHINESE: [&str; 17] = [
    "zh", "cdo", "cjy", "cmn", "cnp", "cpx", "csp", "czh", "czo", "gan", "hak", "hsn", "lzh",
    "mnp", "nan", "wuu", "yue",
];

/// Whether a translation into `target` writes `character` only as leftover
/// source text: Japanese writes kana and Han, the Chinese languages Han,
/// and Korean Hangul; any other language none of them.
fn leftover_character(target: &LanguageIdentifier, character: char) -> bool {
    let written: &[Script] = match target.language.as_str() {
        "ja" => &[Script::Kana, Script::Han],
        "ko" => &[Script::Hangul],
        language if CHINESE.contains(&language) => &[Script::Han],
        _ => &[],
    };
    script(character).is_some_and(|script| !written.contains(&script))
}

/// The odd font check: the block's font isn't its category's majority font,
/// unless it was set by hand or by the agent on purpose. A block with no
/// translation to letter has no font to judge.
fn odd_font(facts: &Facts, majorities: &Majorities) -> Option<Problem> {
    if facts.hand_set || written(facts).is_none() {
        return None;
    }
    let expected = majorities.get(facts.category.as_ref()?)?;
    (*expected != facts.font).then(|| Problem::OddFont {
        expected: shown(expected.as_deref()),
        font: shown(facts.font.as_deref()),
    })
}

/// The font more than half of `fonts`, one category's blocks' fonts, are
/// set in, none counting as a font of its own, when there are at least 3;
/// none on a tie or a split.
pub(crate) fn majority(fonts: &[Option<String>]) -> Option<Option<String>> {
    if fonts.len() < 3 {
        return None;
    }
    let mut counts: BTreeMap<&Option<String>, usize> = BTreeMap::new();
    for font in fonts {
        *counts.entry(font).or_default() += 1;
    }
    counts
        .into_iter()
        .find(|(_, count)| *count > fonts.len() / 2)
        .map(|(font, _)| font.clone())
}

/// The name `family` is shown by: its first name, as a list of families is
/// drawn in its first; none for none, the default font.
fn shown(family: Option<&str>) -> Option<String> {
    let name = family?.split(',').next()?;
    let name = name.trim().trim_matches(['"', '\'']).trim();
    (!name.is_empty()).then(|| name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{super::measure::Sized, *};

    fn english() -> LanguageIdentifier {
        "en".parse().expect("a language")
    }

    fn judge<'a>(target: &'a LanguageIdentifier, majorities: &'a Majorities) -> Judge<'a> {
        Judge {
            limit: 12.0,
            target,
            majorities,
        }
    }

    /// A translated block, its source `source` and its translation
    /// `translation`, tagged with the language `language`.
    fn block(source: &str, translation: &str, language: Option<&str>) -> Facts {
        Facts {
            block: true,
            source: source.to_owned(),
            translation: Some(translation.to_owned()),
            language: language.map(|language| language.parse().expect("a language")),
            cleanup_visible: true,
            detected: true,
            read: true,
            translated_page: true,
            ..Facts::default()
        }
    }

    fn measured(smallest: f32, base: f32) -> Measured {
        Measured {
            overflowed: false,
            base,
            runs: vec![
                Sized {
                    text: "big".to_owned(),
                    size: base.max(smallest),
                },
                Sized {
                    text: "tiny".to_owned(),
                    size: smallest,
                },
            ],
        }
    }

    #[test]
    fn small_text_is_too_small_or_one_small_word() {
        // 0.0.4's cases at its limit of 12 pixels.
        assert_eq!(
            sizes(&measured(9.0, 24.0), 12.0),
            [Problem::SmallWord {
                word: "tiny".to_owned(),
                size: 9.0,
                limit: 12.0
            }]
        );
        assert_eq!(
            sizes(&measured(9.0, 10.0), 12.0),
            [Problem::TooSmall {
                size: 9.0,
                limit: 12.0
            }]
        );
        assert_eq!(sizes(&measured(14.0, 24.0), 12.0), []);
        // Told and compared rounded to one decimal.
        assert_eq!(sizes(&measured(11.96, 24.0), 12.0), []);
        assert_eq!(
            sizes(&measured(9.04, 9.04), 12.04),
            [Problem::TooSmall {
                size: 9.0,
                limit: 12.0
            }]
        );
        // Overflow goes with too small when both hold.
        let both = Measured {
            overflowed: true,
            ..measured(6.0, 6.0)
        };
        assert_eq!(
            sizes(&both, 12.0),
            [
                Problem::Overflow,
                Problem::TooSmall {
                    size: 6.0,
                    limit: 12.0
                }
            ]
        );
        // Nothing to measure but whether it overflows.
        let blank = Measured {
            overflowed: true,
            base: 2.0,
            runs: Vec::new(),
        };
        assert_eq!(sizes(&blank, 12.0), [Problem::Overflow]);
    }

    #[test]
    fn leftover_source_text_is_a_script_the_target_doesnt_write() {
        let en = english();
        let none = Majorities::new();
        let leftover = |translation: &str, language: Option<&str>| {
            problems(&block("原文", translation, language), &judge(&en, &none))
                .contains(&Problem::LeftoverSource { unchanged: false })
        };
        // 0.0.4's cases, the target English.
        for flagged in [
            "Hello あ",
            "Hello ア",
            "Hello 漢",
            "Hello ｱ",
            "Hello 㐀",
            "Hello 𠮷",
        ] {
            assert!(leftover(flagged, Some("en-US")), "{flagged}");
        }
        for clean in ["Hello", "  原文  ", "123 !"] {
            assert!(!leftover(clean, None), "{clean}");
        }
        assert!(!leftover("你好", Some("zh-CN")));
        assert!(leftover("你好あ", Some("zh-TW")));
        assert!(
            !problems(
                &block("Hello", "こんにちは", Some("ja-JP")),
                &judge(&en, &none)
            )
            .contains(&Problem::LeftoverSource { unchanged: false })
        );
        // Hangul in English is leftover now, and Han in Chinese isn't.
        assert!(leftover("Hello 한국어", Some("en")));
        assert!(!leftover("漢字", Some("zh")));
        assert!(leftover("Hello 漢", Some("ko")));
        assert!(!leftover("안녕", Some("ko")));
        // Cantonese, and Chinese's other languages, write Han too.
        assert!(!leftover("你係邊個？", Some("yue")));
        assert!(leftover("你好あ", Some("yue")));
        assert!(!leftover("侬好", Some("wuu")));
        assert!(leftover("你好あ", Some("cmn")));
    }

    #[test]
    fn an_undetermined_tag_reads_as_the_target_language() {
        let none = Majorities::new();
        let found = |target: &str| {
            let target: LanguageIdentifier = target.parse().expect("a language");
            problems(
                &block("原文", "Hello あ", Some("und")),
                &judge(&target, &none),
            )
            .contains(&Problem::LeftoverSource { unchanged: false })
        };
        assert!(found("en"));
        assert!(!found("ja"));
        // No translation, no tag of its own.
        let given_back = Facts {
            translation: None,
            language: Some("ja".parse().expect("a language")),
            cleanup_visible: false,
            given_back: true,
            ..block("原文", "", None)
        };
        let en = english();
        assert!(
            problems(&given_back, &judge(&en, &none))
                .contains(&Problem::LeftoverSource { unchanged: true })
        );
    }

    #[test]
    fn a_line_given_back_shows_its_source_as_leftover() {
        let en = english();
        let none = Majorities::new();
        let given_back = Facts {
            translation: None,
            cleanup_visible: false,
            given_back: true,
            ..block("魔王です", "", None)
        };
        assert_eq!(
            problems(&given_back, &judge(&en, &none)),
            [Problem::LeftoverSource { unchanged: true }]
        );
        // A source in the target's script, or a line no stage gave back,
        // shows nothing left over.
        let ja: LanguageIdentifier = "ja".parse().expect("a language");
        assert_eq!(problems(&given_back, &judge(&ja, &none)), []);
        let yue: LanguageIdentifier = "yue".parse().expect("a language");
        let chinese = Facts {
            source: "你好".to_owned(),
            ..given_back.clone()
        };
        assert_eq!(problems(&chinese, &judge(&yue, &none)), []);
        let kept = Facts {
            given_back: false,
            ..given_back
        };
        assert_eq!(problems(&kept, &judge(&en, &none)), []);
    }

    #[test]
    fn a_missing_translation_needs_a_source_a_clean_up_and_a_translated_page() {
        let en = english();
        let none = Majorities::new();
        let untranslated = Facts {
            translation: None,
            ..block("原文", "", None)
        };
        assert_eq!(
            problems(&untranslated, &judge(&en, &none)),
            [Problem::MissingTranslation]
        );
        let blank = Facts {
            translation: Some("  ".to_owned()),
            ..untranslated.clone()
        };
        assert_eq!(
            problems(&blank, &judge(&en, &none)),
            [Problem::MissingTranslation]
        );
        for skipped in [
            Facts {
                translated_page: false,
                ..untranslated.clone()
            },
            Facts {
                cleanup_visible: false,
                ..untranslated.clone()
            },
            Facts {
                source: " ".to_owned(),
                read: false,
                ..untranslated.clone()
            },
        ] {
            assert_eq!(problems(&skipped, &judge(&en, &none)), [], "{skipped:?}");
        }
    }

    #[test]
    fn an_empty_source_was_found_and_read_as_nothing() {
        let en = english();
        let none = Majorities::new();
        let empty = Facts {
            translation: None,
            ..block("  ", "", None)
        };
        assert_eq!(problems(&empty, &judge(&en, &none)), [Problem::EmptySource]);
        for skipped in [
            Facts {
                read: false,
                ..empty.clone()
            },
            Facts {
                detected: false,
                ..empty.clone()
            },
        ] {
            assert_eq!(problems(&skipped, &judge(&en, &none)), [], "{skipped:?}");
        }
    }

    #[test]
    fn a_category_has_a_majority_font_of_at_least_three_and_more_than_half() {
        let a = Some("A".to_owned());
        let b = Some("B".to_owned());
        let c = Some("C".to_owned());
        assert_eq!(majority(&[a.clone(), b.clone()]), None);
        assert_eq!(majority(&[a.clone(), b.clone(), c]), None);
        assert_eq!(
            majority(&[a.clone(), a.clone(), b.clone(), b.clone()]),
            None
        );
        assert_eq!(
            majority(&[a.clone(), a.clone(), b.clone()]),
            Some(a.clone())
        );
        // The default font counts as a font of its own.
        assert_eq!(majority(&[None, None, b]), Some(None));
    }

    #[test]
    fn a_font_off_its_categorys_majority_is_odd_unless_set_by_hand() {
        let en = english();
        let majorities = Majorities::from([
            (
                LetteringCategory::NormalDialogue,
                Some("A, Arial".to_owned()),
            ),
            (LetteringCategory::Sfx, None),
        ]);
        let odd = Facts {
            category: Some(LetteringCategory::NormalDialogue),
            font: Some("'B' , Arial".to_owned()),
            ..block("原文", "Hello", Some("en"))
        };
        assert_eq!(
            problems(&odd, &judge(&en, &majorities)),
            [Problem::OddFont {
                expected: Some("A".to_owned()),
                font: Some("B".to_owned())
            }]
        );
        let hand_set = Facts {
            hand_set: true,
            ..odd.clone()
        };
        assert_eq!(problems(&hand_set, &judge(&en, &majorities)), []);
        // A category without a majority, or no translation to letter.
        let shout = Facts {
            category: Some(LetteringCategory::Shout),
            ..odd.clone()
        };
        assert_eq!(problems(&shout, &judge(&en, &majorities)), []);
        let untranslated = Facts {
            translation: None,
            cleanup_visible: false,
            ..odd.clone()
        };
        assert_eq!(problems(&untranslated, &judge(&en, &majorities)), []);
        // The default font, expected or found, is told as none.
        let sfx = Facts {
            category: Some(LetteringCategory::Sfx),
            ..odd
        };
        assert_eq!(
            problems(&sfx, &judge(&en, &majorities)),
            [Problem::OddFont {
                expected: None,
                font: Some("B".to_owned())
            }]
        );
    }

    #[test]
    fn a_text_layer_gets_only_the_size_checks_and_flags() {
        let en = english();
        let none = Majorities::new();
        let text = Facts {
            block: false,
            translation: None,
            measured: Some(measured(6.0, 6.0)),
            flags: vec!["Check the joke".to_owned()],
            slips: vec![GlossaryEntry::new("魔王", "Demon King")],
            ..block("", "", None)
        };
        assert_eq!(
            problems(&text, &judge(&en, &none)),
            [
                Problem::TooSmall {
                    size: 6.0,
                    limit: 12.0
                },
                Problem::Flag {
                    message: "Check the joke".to_owned()
                }
            ]
        );
    }

    #[test]
    fn a_blocks_problems_come_in_the_checks_order() {
        let en = english();
        let majorities = Majorities::from([(LetteringCategory::Floating, Some("A".to_owned()))]);
        let facts = Facts {
            measured: Some(Measured {
                overflowed: true,
                ..measured(9.0, 24.0)
            }),
            category: Some(LetteringCategory::Floating),
            font: Some("B".to_owned()),
            slips: vec![GlossaryEntry::new("勇者", "Hero")],
            flags: vec!["Too long".to_owned()],
            ..block("勇者です", "The braveあ", Some("en"))
        };
        let found: Vec<String> = problems(&facts, &judge(&en, &majorities))
            .iter()
            .map(Problem::check)
            .collect();
        assert_eq!(
            found,
            [
                "overflow",
                "small_word",
                "leftover_source",
                "odd_font",
                "glossary:勇者",
                "flag"
            ]
        );
    }
}
