use booharu_context::Flag;

use super::{QcCheck, QcFinding, fix::QcFixAction};

/// A box has two attempts, including failed requests. Layout adjustments share the
/// first attempt so a second attempt can shorten the wording when necessary.
pub(super) fn next_fix(
    findings: &[QcFinding],
    hand_edited: bool,
    attempts: u8,
    history: &[QcFixAction],
) -> Option<(QcCheck, QcFixAction)> {
    use QcFixAction::*;
    if hand_edited || attempts >= 2 {
        return None;
    }
    for check in [
        QcCheck::EmptySource,
        QcCheck::MissingTranslation,
        QcCheck::LeftoverSource,
        QcCheck::Flag,
        QcCheck::OddFont,
        QcCheck::Overflow,
        QcCheck::TooSmall,
    ] {
        if !findings.iter().any(|finding| {
            finding.check == check
                && (check != QcCheck::Flag
                    || matches!(&finding.flag, Some(Flag::Glossary { .. })))
        }) {
            continue;
        }
        let action = match check {
            QcCheck::EmptySource if history.contains(&ReadAgain) => HideEmpty,
            QcCheck::EmptySource => ReadAgain,
            QcCheck::MissingTranslation => TranslateMissing,
            QcCheck::LeftoverSource => TranslateLeftover,
            QcCheck::Flag => TranslateGlossary,
            QcCheck::OddFont if history.contains(&ApplyCategoryStyle) => continue,
            QcCheck::OddFont => ApplyCategoryStyle,
            QcCheck::Overflow | QcCheck::TooSmall if history.contains(&TightenLayout) => {
                ShortenTranslation
            }
            QcCheck::Overflow | QcCheck::TooSmall => TightenLayout,
            _ => continue,
        };
        return Some((check, action));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use koharu_scene::EntityId;

    fn finding(check: QcCheck) -> QcFinding {
        QcFinding {
            page: EntityId::new(),
            page_label: "page".into(),
            layer: EntityId::new(),
            check,
            detail: String::new(),
            translation: String::new(),
            flag: None,
            bounds: None,
        }
    }

    #[test]
    fn manual_edits_and_the_two_attempt_budget_win_over_every_finding() {
        for check in [
            QcCheck::Overflow,
            QcCheck::EmptySource,
            QcCheck::MissingTranslation,
        ] {
            assert!(next_fix(&[finding(check)], true, 0, &[]).is_none());
            assert!(next_fix(&[finding(check)], false, 2, &[]).is_none());
        }
    }

    #[test]
    fn read_before_translation_and_hide_only_after_a_completed_read() {
        let findings = [
            finding(QcCheck::MissingTranslation),
            finding(QcCheck::EmptySource),
        ];
        assert_eq!(
            next_fix(&findings, false, 0, &[]).unwrap().1,
            QcFixAction::ReadAgain,
        );
        assert_eq!(
            next_fix(&findings, false, 1, &[QcFixAction::ReadAgain]).unwrap().1,
            QcFixAction::HideEmpty,
        );
        assert_eq!(
            next_fix(&findings, false, 1, &[]).unwrap().1,
            QcFixAction::ReadAgain,
        );
    }

    #[test]
    fn shortening_follows_mechanical_layout_and_review_flags_are_not_requests() {
        let overflow = [finding(QcCheck::Overflow)];
        assert_eq!(
            next_fix(&overflow, false, 0, &[]).unwrap().1,
            QcFixAction::TightenLayout,
        );
        assert_eq!(
            next_fix(&overflow, false, 1, &[QcFixAction::TightenLayout]).unwrap().1,
            QcFixAction::ShortenTranslation,
        );
        let mut review = finding(QcCheck::Flag);
        review.flag = Some(Flag::Review {
            message: "check meaning".into(),
            page: 1,
        });
        assert!(next_fix(&[review.clone()], false, 0, &[]).is_none());
        review.flag = Some(Flag::Glossary {
            term: "name".into(),
            page: 1,
        });
        assert_eq!(
            next_fix(&[review], false, 0, &[]).unwrap().1,
            QcFixAction::TranslateGlossary,
        );
    }
}
