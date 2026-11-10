use std::collections::BTreeMap;

use booharu_context::{LetteringCategory, LineContext};

use super::{QcCheck, Row};

/// Flags source-language script left in a translation.
///
/// The target language decides what counts as left over: Japanese targets are
/// never flagged, and Chinese targets are flagged for kana only, because Han
/// characters are the translation there.
pub(super) fn has_leftover_source(source: &str, translation: &str, language: Option<&str>) -> bool {
    let language = language.unwrap_or_default().to_ascii_lowercase();
    if language == "ja" || language.starts_with("ja-") {
        return false;
    }
    let han_is_target = language == "zh" || language.starts_with("zh-");
    let translation = translation.trim();
    translation != source.trim()
        && translation.chars().any(|character| {
            let kana = matches!(character as u32,
                0x3040..=0x309f | 0x30a0..=0x30ff | 0x31f0..=0x31ff | 0xff66..=0xff9f
            );
            let han = matches!(character as u32,
                0x3400..=0x4dbf | 0x4e00..=0x9fff | 0x20000..=0x2ffff | 0x30000..=0x323af
            );
            kana || (han && !han_is_target)
        })
}

pub(super) fn font_majorities(rows: &[Row]) -> BTreeMap<LetteringCategory, Option<String>> {
    let mut counts: BTreeMap<LetteringCategory, BTreeMap<Option<String>, usize>> = BTreeMap::new();
    for row in rows {
        if let Some(category) = row.category {
            *counts
                .entry(category)
                .or_default()
                .entry(row.font.clone())
                .or_default() += 1;
        }
    }
    counts
        .into_iter()
        .filter_map(|(category, fonts)| {
            let total: usize = fonts.values().sum();
            if total < 3 {
                return None;
            }
            // Avoid treating a tied or fragmented category as having an established style.
            fonts
                .into_iter()
                .find(|(_, count)| *count > total / 2)
                .map(|(font, _)| (category, font))
        })
        .collect()
}

pub(super) fn is_hidden(line: Option<&LineContext>, check: QcCheck) -> bool {
    check != QcCheck::Flag && line.is_some_and(|line| line.qc_ok.iter().any(|id| id == check.id()))
}

#[cfg(test)]
mod tests {
    use super::super::text_checks;
    use super::*;
    use koharu_scene::EntityId;
    use std::collections::BTreeSet;

    fn row(font: &str) -> Row {
        Row {
            page: EntityId::new(),
            page_label: "1.png".into(),
            layer: EntityId::new(),
            source: "原文".into(),
            translation: "Translation".into(),
            language: None,
            detected: true,
            ocr_checked: true,
            category: Some(LetteringCategory::NormalDialogue),
            font: Some(font.into()),
            user_font: false,
        }
    }

    #[test]
    fn japanese_scripts_and_deliberately_untranslated_lines() {
        for text in [
            "Hello あ",
            "Hello ア",
            "Hello 漢",
            "Hello ｱ",
            "Hello 㐀",
            "Hello 𠮷",
        ] {
            assert!(has_leftover_source("原文", text, Some("en-US")), "{text}");
        }
        for text in ["Hello", "", "  原文  ", "한국어", "123 !"] {
            assert!(!has_leftover_source("原文", text, None), "{text}");
        }
        // Han characters are the translation for a Chinese target; kana is not.
        assert!(!has_leftover_source("原文", "你好", Some("zh-CN")));
        assert!(has_leftover_source("原文", "你好あ", Some("zh-TW")));
        assert!(!has_leftover_source("Hello", "こんにちは", Some("ja-JP")));
    }

    #[test]
    fn majority_requires_three_boxes_and_a_strict_majority() {
        assert!(font_majorities(&[row("A"), row("B")]).is_empty());
        assert!(font_majorities(&[row("A"), row("B"), row("C")]).is_empty());
        assert!(font_majorities(&[row("A"), row("A"), row("B"), row("B")]).is_empty());
        let rows = [row("A"), row("A"), row("B")];
        let majorities = font_majorities(&rows);
        assert_eq!(
            majorities[&LetteringCategory::NormalDialogue],
            Some("A".into())
        );
        let mut odd = row("B");
        let mut checks = Vec::new();
        text_checks(&odd, &BTreeSet::new(), &majorities, &mut checks);
        assert_eq!(checks, [(QcCheck::OddFont, "A".into())]);
        odd.user_font = true;
        checks.clear();
        text_checks(&odd, &BTreeSet::new(), &majorities, &mut checks);
        assert!(checks.is_empty());
        odd.user_font = false;
        odd.category = Some(LetteringCategory::Shout);
        text_checks(&odd, &BTreeSet::new(), &majorities, &mut checks);
        assert!(checks.is_empty());
    }

    #[test]
    fn acknowledgements_do_not_hide_other_checks_or_review_flags() {
        let line = LineContext {
            qc_ok: vec!["overflow".into(), "flag".into()],
            ..Default::default()
        };
        assert!(is_hidden(Some(&line), QcCheck::Overflow));
        assert!(!is_hidden(Some(&line), QcCheck::TooSmall));
        assert!(!is_hidden(Some(&line), QcCheck::Flag));
        assert!(!is_hidden(None, QcCheck::Overflow));
    }

    #[test]
    fn skipped_sfx_and_boxes_not_yet_ocrd_are_not_missing_work() {
        let mut line = row("A");
        line.translation.clear();
        let mut checks = Vec::new();
        text_checks(
            &line,
            &BTreeSet::from([line.layer]),
            &BTreeMap::new(),
            &mut checks,
        );
        assert!(checks.is_empty());
        text_checks(&line, &BTreeSet::new(), &BTreeMap::new(), &mut checks);
        assert_eq!(checks[0].0, QcCheck::MissingTranslation);
        line.source.clear();
        line.ocr_checked = false;
        checks.clear();
        text_checks(&line, &BTreeSet::new(), &BTreeMap::new(), &mut checks);
        assert!(checks.is_empty());
        line.ocr_checked = true;
        text_checks(&line, &BTreeSet::new(), &BTreeMap::new(), &mut checks);
        assert_eq!(checks[0].0, QcCheck::EmptySource);
        line.detected = false;
        checks.clear();
        text_checks(&line, &BTreeSet::new(), &BTreeMap::new(), &mut checks);
        assert!(checks.is_empty());
    }
}
