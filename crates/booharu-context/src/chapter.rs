use std::collections::BTreeSet;

use koharu_scene::EntityId;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{LetteringCategory, ProjectContext};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TermKind {
    Name,
    Place,
    Term,
    Honorific,
    #[default]
    Other,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, Type)]
#[serde(default)]
pub struct GlossaryEntry {
    pub id: String,
    pub source: String,
    pub target: String,
    pub note: String,
    pub kind: TermKind,
    pub locked: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, Type)]
#[serde(default)]
pub struct Character {
    pub id: String,
    pub name: String,
    pub original: String,
    pub notes: String,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, Type)]
#[serde(default)]
pub struct ChapterCharacter {
    pub name: String,
    pub original: String,
    pub pages: Vec<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, Type)]
#[serde(default)]
pub struct ChapterBrief {
    pub summary: String,
    pub characters: Vec<ChapterCharacter>,
    pub edited: bool,
    pub read_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Flag {
    Glossary { term: String, page: u32 },
    Review { message: String, page: u32 },
}

#[derive(Clone, Debug, Deserialize)]
pub struct ChapterReading {
    pub summary: String,
    pub characters: Vec<ChapterCharacter>,
    pub glossary: Vec<SuggestedTerm>,
    pub lines: Vec<ReadLine>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SuggestedTerm {
    pub source: String,
    pub target: String,
    pub kind: TermKind,
    pub note: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ReadLine {
    // Keep IDs and categories as strings so a hallucinated ID cannot fail the whole read.
    pub id: String,
    pub category: Option<String>,
    pub confidence: Option<f32>,
    pub note: Option<String>,
}

impl ProjectContext {
    pub fn merge_reading(
        &mut self,
        reading: ChapterReading,
        valid_layers: &BTreeSet<EntityId>,
        series: &[GlossaryEntry],
        read_at: String,
    ) -> Vec<EntityId> {
        let brief = self.brief.get_or_insert_with(ChapterBrief::default);
        if !brief.edited {
            brief.summary = reading.summary;
        }
        brief.characters = reading.characters;
        brief.read_at = Some(read_at);
        for term in reading.glossary {
            let source = term.source.trim();
            if source.is_empty()
                || term.target.trim().is_empty()
                || series
                    .iter()
                    .chain(&self.suggestions)
                    .any(|entry| entry.source.trim() == source)
            {
                continue;
            }
            self.suggestions.push(GlossaryEntry {
                id: EntityId::new().to_string(),
                source: source.into(),
                target: term.target.trim().into(),
                kind: term.kind,
                note: term.note,
                locked: false,
            });
        }
        let mut changed = BTreeSet::new();
        for line in reading.lines {
            let Some(id) = valid_layers
                .iter()
                .copied()
                .find(|id| id.to_string() == line.id)
            else {
                continue;
            };
            if let Some(category) = line
                .category
                .as_deref()
                .and_then(LetteringCategory::parse)
                .filter(|category| {
                    !matches!(
                        category,
                        LetteringCategory::NormalDialogue
                            | LetteringCategory::Floating
                            | LetteringCategory::SmallText
                            | LetteringCategory::NarrationBox
                    )
                })
                && line
                    .confidence
                    .is_none_or(|value| value.is_finite() && (0.0..=1.0).contains(&value))
                && self.set_model_category(id, category, line.confidence)
            {
                changed.insert(id);
            }
            if let Some(note) = line.note.filter(|text| !text.trim().is_empty()) {
                self.lines.entry(id).or_default().note = Some(note);
            }
        }
        changed.into_iter().collect()
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, Type)]
pub struct InstructionTerm {
    pub line: u32,
    pub source: String,
    pub target: String,
}

pub fn instruction_terms(instructions: &str) -> Vec<InstructionTerm> {
    let mut in_glossary = false;
    let mut terms = Vec::new();
    for (index, raw) in instructions.lines().enumerate() {
        let text = raw.trim().trim_start_matches(['-', '*']).trim();
        if text.to_lowercase().contains("glossary") {
            in_glossary = true;
            continue;
        }
        let pair = text
            .split_once('→')
            .or_else(|| text.split_once('='))
            .or_else(|| {
                text.split_once(':')
                    .filter(|(source, _)| in_glossary || !source.is_ascii())
            });
        if let Some((source, target)) = pair {
            let (source, target) = (source.trim(), target.trim());
            if !source.is_empty() && !target.is_empty() && !source.contains(['=', '→']) {
                terms.push(InstructionTerm {
                    line: index as u32,
                    source: source.into(),
                    target: target.into(),
                });
            }
        }
    }
    terms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_owner_instructions_and_keeps_line_numbers() {
        let terms = instruction_terms(
            "Use natural English.\nGlossary:\nヒナ=Hina\n魔王 = Demon King\n\n村 → Village\n町: Town",
        );
        assert_eq!(terms.len(), 4);
        assert_eq!(
            (
                terms[0].line,
                terms[0].source.as_str(),
                terms[0].target.as_str()
            ),
            (2, "ヒナ", "Hina")
        );
        assert_eq!(terms[1].target, "Demon King");
        assert_eq!(instruction_terms("ヒナ: Hina")[0].target, "Hina");
        assert!(instruction_terms("Tone: informal").is_empty());
    }

    #[test]
    fn merge_preserves_user_labels_locked_terms_and_edited_summary() {
        let layer = EntityId::new();
        let mut context = ProjectContext::default();
        context.set_user_category(layer, Some(LetteringCategory::Thoughts));
        context.brief = Some(ChapterBrief {
            summary: "User summary".into(),
            edited: true,
            ..Default::default()
        });
        let series = vec![GlossaryEntry {
            source: "ヒナ".into(),
            target: "Hina".into(),
            locked: true,
            ..Default::default()
        }];
        let reading = serde_json::from_value(serde_json::json!({
            "summary":"Model summary", "characters":[],
            "glossary":[{"source":"ヒナ","target":"Wrong","kind":"name","note":""},
                {"source":"魔王","target":"Demon King","kind":"term","note":""},
                {"source":"魔王","target":"King","kind":"term","note":""}],
            "lines":[{"id":layer.to_string(),"category":"shout","confidence":0.9,"note":"pun"},
                {"id":"unknown","category":"sfx","confidence":0.8,"note":"discard"}]
        }))
        .unwrap();
        assert!(
            context
                .merge_reading(reading, &BTreeSet::from([layer]), &series, "now".into())
                .is_empty()
        );
        assert_eq!(
            context.category(layer).unwrap().value,
            LetteringCategory::Thoughts
        );
        assert_eq!(context.brief.unwrap().summary, "User summary");
        assert_eq!(context.suggestions.len(), 1);
        assert_eq!(context.lines.len(), 1);
        assert_eq!(series[0].target, "Hina");
    }
}
