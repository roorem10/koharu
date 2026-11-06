use booharu_context::{ProjectContext, Series};
use koharu_scene::EntityId;

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct HardLine {
    pub pun: bool,
    pub unknown_terms: Vec<String>,
    pub qc: bool,
}

impl HardLine {
    pub fn is_hard(&self) -> bool {
        self.pun || self.qc || !self.unknown_terms.is_empty()
    }
}

/// Reader notes contain likely wordplay. Names and terms come from the reader or
/// series characters, never guesses based on capitalization or a language heuristic.
pub(super) fn classify(
    layer: EntityId,
    source: &str,
    context: &ProjectContext,
    series: Option<&Series>,
    qc_flag: bool,
) -> HardLine {
    if source.trim().is_empty() {
        return HardLine::default();
    }
    let line = context.lines.get(&layer);
    let mut result = HardLine {
        pun: line
            .and_then(|line| line.note.as_deref())
            .is_some_and(|note| !note.trim().is_empty()),
        qc: qc_flag
            || line.is_some_and(|line| {
                line.flags.iter().any(|flag| {
                    !line
                        .qc_ok
                        .contains(&super::super::lines::flag_acknowledgement(flag))
                })
            }),
        ..Default::default()
    };
    let characters = context
        .brief
        .iter()
        .flat_map(|brief| &brief.characters)
        .map(|character| {
            if character.original.trim().is_empty() {
                character.name.as_str()
            } else {
                character.original.as_str()
            }
        })
        .chain(
            series
                .into_iter()
                .flat_map(|series| &series.characters)
                .map(|character| {
                    if character.original.trim().is_empty() {
                        character.name.as_str()
                    } else {
                        character.original.as_str()
                    }
                }),
        );
    let terms = context
        .suggestions
        .iter()
        .map(|entry| entry.source.as_str())
        .chain(characters);
    for term in terms {
        let term = term.trim();
        let known = series
            .into_iter()
            .flat_map(|series| &series.glossary)
            .any(|entry| {
                normalize(&entry.source) == normalize(term) && !entry.target.trim().is_empty()
            });
        if !known
            && koharu_pipeline::term_occurs(source, term)
            && !result
                .unknown_terms
                .iter()
                .any(|entry| normalize(entry) == normalize(term))
        {
            result.unknown_terms.push(term.to_owned());
        }
    }
    result
}

fn normalize(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use booharu_context::{ChapterBrief, ChapterCharacter, Flag, GlossaryEntry};

    #[test]
    fn notes_unknown_names_terms_and_unacknowledged_qc_are_independent_signals() {
        let layer = EntityId::new();
        let mut context = ProjectContext::default();
        assert!(!classify(layer, "hello", &context, None, false).is_hard());
        context.lines.entry(layer).or_default().note = Some("wordplay".into());
        assert!(classify(layer, "hello", &context, None, false).pun);
        context.lines.get_mut(&layer).unwrap().note = None;
        context.brief = Some(ChapterBrief {
            characters: vec![ChapterCharacter {
                original: "リムル".into(),
                ..Default::default()
            }],
            ..Default::default()
        });
        context.suggestions.push(GlossaryEntry {
            source: "魔王".into(),
            target: "Demon King".into(),
            ..Default::default()
        });
        assert_eq!(
            classify(layer, "魔王リムル", &context, None, false).unknown_terms,
            ["魔王", "リムル"]
        );
        let mut series = Series::new("fixture".into()).unwrap();
        series.glossary = vec![
            GlossaryEntry {
                source: "魔王".into(),
                target: "Demon King".into(),
                ..Default::default()
            },
            GlossaryEntry {
                source: "リ ム ル".into(),
                target: "Rimuru".into(),
                ..Default::default()
            },
        ];
        assert!(!classify(layer, "魔王リムル", &context, Some(&series), false).is_hard());
        assert!(classify(layer, "hello", &context, Some(&series), true).qc);
        let flag = Flag::Review {
            message: "ambiguous pronoun".into(),
            page: 1,
        };
        context.lines.get_mut(&layer).unwrap().flags.push(flag.clone());
        assert!(classify(layer, "hello", &context, None, false).qc);
        context
            .lines
            .get_mut(&layer)
            .unwrap()
            .qc_ok
            .push(super::super::super::lines::flag_acknowledgement(&flag));
        assert!(!classify(layer, "hello", &context, None, false).qc);
        assert!(!classify(layer, " ", &context, None, true).is_hard());
    }
}
