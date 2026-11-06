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
    #[serde(default)]
    pub sfx_important: Option<bool>,
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
            // The effective category includes hand edits; a model's SFX claim must
            // not attach importance to a line the user classified differently.
            let sfx_important = self
                .category(id)
                .filter(|category| category.value == LetteringCategory::Sfx)
                .and(line.sfx_important);
            if self.lines.get(&id).and_then(|line| line.sfx_important) != sfx_important {
                // The returned IDs trigger restyling; importance alone must not
                // replace typography on a hand-classified SFX box.
                self.lines.entry(id).or_default().sfx_important = sfx_important;
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

    #[test]
    fn sfx_importance_follows_the_effective_category_and_current_reading() {
        let important = EntityId::new();
        let incidental = EntityId::new();
        let manual_dialogue = EntityId::new();
        let manual_sfx = EntityId::new();
        let valid = BTreeSet::from([important, incidental, manual_dialogue, manual_sfx]);
        let mut context = ProjectContext::default();
        context.set_user_category(manual_dialogue, Some(LetteringCategory::NormalDialogue));
        context.set_user_category(manual_sfx, Some(LetteringCategory::Sfx));
        let reading = |lines| {
            serde_json::from_value(serde_json::json!({
                "summary": "", "characters": [], "glossary": [], "lines": lines,
            }))
            .unwrap()
        };
        let changed = context.merge_reading(
            reading(serde_json::json!([
                {"id": important, "category": "sfx", "sfx_important": true},
                {"id": incidental, "category": "sfx", "sfx_important": false},
                {"id": manual_dialogue, "category": "sfx", "sfx_important": true},
                {"id": manual_sfx, "category": "shout", "sfx_important": true},
                {"id": "unknown", "category": "sfx", "sfx_important": true},
            ])),
            &valid,
            &[],
            "first".into(),
        );
        assert_eq!(
            changed.into_iter().collect::<BTreeSet<_>>(),
            BTreeSet::from([important, incidental])
        );
        assert_eq!(context.lines[&important].sfx_important, Some(true));
        assert_eq!(context.lines[&incidental].sfx_important, Some(false));
        assert_eq!(context.lines[&manual_dialogue].sfx_important, None);
        assert_eq!(context.lines[&manual_sfx].sfx_important, Some(true));
        assert_eq!(context.lines.len(), 4);
        assert_eq!(
            context.category(manual_dialogue).unwrap().value,
            LetteringCategory::NormalDialogue
        );
        assert_eq!(
            context.category(manual_sfx).unwrap().source,
            crate::LabelSource::User
        );

        context.merge_reading(
            reading(serde_json::json!([
                {"id": important, "category": "sfx", "sfx_important": null},
                {"id": incidental, "category": "title", "sfx_important": true},
            ])),
            &valid,
            &[],
            "second".into(),
        );
        assert_eq!(context.lines[&important].sfx_important, None);
        assert_eq!(context.lines[&incidental].sfx_important, None);
    }

    #[test]
    fn reading_importance_is_optional_but_must_be_boolean() {
        for value in [
            serde_json::json!({"id": "box"}),
            serde_json::json!({"id": "box", "sfx_important": null}),
        ] {
            let line: ReadLine = serde_json::from_value(value).unwrap();
            assert_eq!(line.sfx_important, None);
        }
        assert!(
            serde_json::from_value::<ReadLine>(serde_json::json!({
                "id": "box", "sfx_important": "true",
            }))
            .is_err()
        );
    }

    #[test]
    fn importance_updates_do_not_restyle_a_hand_classified_sfx() {
        let layer = EntityId::new();
        let mut context = ProjectContext::default();
        context.set_user_category(layer, Some(LetteringCategory::Sfx));
        for important in [Some(true), Some(false), None] {
            let reading = serde_json::from_value(serde_json::json!({
                "summary": "", "characters": [], "glossary": [],
                "lines": [{"id": layer, "category": "sfx", "sfx_important": important}],
            }))
            .unwrap();
            assert!(
                context
                    .merge_reading(reading, &BTreeSet::from([layer]), &[], "now".into())
                    .is_empty()
            );
            assert_eq!(context.lines[&layer].sfx_important, important);
            assert_eq!(context.category(layer).unwrap().source, crate::LabelSource::User);
        }
    }
}
