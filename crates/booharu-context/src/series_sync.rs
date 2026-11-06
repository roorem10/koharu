use anyhow::{Context as _, Result, ensure};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{ChapterCharacter, Character, ProjectContext, Series};

const HISTORY: &str = "series_terms";

pub struct UsedTranslation<'a> {
    pub source: &'a str,
    pub translation: &'a str,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, Type)]
pub struct TermConflict {
    pub source: String,
    pub series_target: String,
    pub chapter_target: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TermConflictChoice {
    KeepSeries,
    UseChapter,
    Leave,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Type)]
pub struct TermSyncResult {
    pub added: u32,
    pub conflicts: Vec<TermConflict>,
}

#[derive(Default, Deserialize, Serialize)]
struct TermHistory {
    series: String,
    resolved: Vec<TermConflict>,
}

impl TermHistory {
    fn load(context: &ProjectContext, series: &Series) -> Result<Self> {
        let history: Self = context
            .other
            .get(HISTORY)
            .map(|value| serde_json::from_value(value.clone()))
            .transpose()?
            .unwrap_or_default();
        Ok(if history.series == series.id {
            history
        } else {
            Self {
                series: series.id.clone(),
                ..Self::default()
            }
        })
    }
}

pub fn term_conflicts(context: &ProjectContext, series: &Series) -> Result<Vec<TermConflict>> {
    let history = TermHistory::load(context, series)?;
    let mut conflicts = Vec::new();
    for suggestion in &context.suggestions {
        let source = suggestion.source.trim();
        let target = suggestion.target.trim();
        if source.is_empty() || target.is_empty() {
            continue;
        }
        let Some(entry) = series
            .glossary
            .iter()
            .find(|entry| entry.source.trim() == source)
        else {
            continue;
        };
        if entry.target.trim() == target {
            continue;
        }
        let conflict = TermConflict {
            source: source.into(),
            series_target: entry.target.trim().into(),
            chapter_target: target.into(),
        };
        if !history.resolved.contains(&conflict) && !conflicts.contains(&conflict) {
            conflicts.push(conflict);
        }
    }
    Ok(conflicts)
}

/// Only published translations are evidence of use. A model suggestion on its
/// own must not establish the next chapter's spelling.
pub fn save_chapter_terms_to_series(
    series: &mut Series,
    context: &mut ProjectContext,
    rows: &[UsedTranslation<'_>],
) -> Result<TermSyncResult> {
    ensure!(
        context.series.as_deref() == Some(series.id.as_str()),
        "chapter series changed"
    );
    let conflicts = term_conflicts(context, series)?;
    let mut saved = Vec::new();
    let mut added = 0;
    for entry in &context.suggestions {
        let source = entry.source.trim();
        let target = entry.target.trim();
        if !rows
            .iter()
            .any(|row| term_used(row.source, source) && term_used(row.translation, target))
        {
            continue;
        }
        if let Some(existing) = series
            .glossary
            .iter()
            .find(|term| term.source.trim() == source)
        {
            if existing.target.trim() == target {
                saved.push(entry.id.clone());
            }
            continue;
        }
        let mut entry = entry.clone();
        entry.source = source.into();
        entry.target = target.into();
        entry.locked = false;
        ensure!(
            !series.glossary.iter().any(|term| term.id == entry.id),
            "glossary entry id already exists"
        );
        saved.push(entry.id.clone());
        series.glossary.push(entry);
        added += 1;
    }
    context.suggestions.retain(|entry| !saved.contains(&entry.id));
    Ok(TermSyncResult { added, conflicts })
}

pub fn resolve_term_conflict(
    series: &mut Series,
    context: &mut ProjectContext,
    conflict: &TermConflict,
    choice: TermConflictChoice,
) -> Result<()> {
    ensure!(
        context.series.as_deref() == Some(series.id.as_str()),
        "chapter series changed"
    );
    let mut history = TermHistory::load(context, series)?;
    ensure!(
        term_conflicts(context, series)?.contains(conflict),
        "term conflict changed; refresh the chapter context"
    );
    if matches!(choice, TermConflictChoice::UseChapter) {
        let entry = series
            .glossary
            .iter_mut()
            .find(|entry| entry.source.trim() == conflict.source)
            .context("series term no longer exists")?;
        // An explicit choice may change a locked spelling, but never its lock.
        entry.target = conflict.chapter_target.clone();
    }
    if !matches!(choice, TermConflictChoice::Leave) {
        context.suggestions.retain(|entry| {
            entry.source.trim() != conflict.source || entry.target.trim() != conflict.chapter_target
        });
    }
    if !history.resolved.contains(conflict) {
        history.resolved.push(conflict.clone());
    }
    context
        .other
        .insert(HISTORY.into(), serde_json::to_value(history)?);
    Ok(())
}

pub fn merge_chapter_characters(series: &mut Series, characters: &[ChapterCharacter]) -> u32 {
    let mut added = 0;
    for character in characters {
        let name = character.name.trim();
        let original = character.original.trim();
        if name.is_empty()
            || series.characters.iter().any(|existing| {
                (!original.is_empty() && existing.original.trim() == original)
                    || existing.name.trim().to_lowercase() == name.to_lowercase()
            })
        {
            continue;
        }
        series.characters.push(Character {
            id: koharu_scene::EntityId::new().to_string(),
            name: name.into(),
            original: original.into(),
            notes: if character.notes_confidence.is_some_and(|confidence| {
                confidence.is_finite() && (0.95..=1.0).contains(&confidence)
            }) {
                character.notes.trim().into()
            } else {
                String::new()
            },
        });
        added += 1;
    }
    added
}

fn unspaced(character: char) -> bool {
    matches!(character as u32,
        0x0E00..=0x0EFF | 0x1000..=0x109F | 0x1780..=0x17FF |
        0x3040..=0x30FF | 0x3400..=0x9FFF | 0xAC00..=0xD7AF |
        0xF900..=0xFAFF | 0x20000..=0x3134F)
}

fn word(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn term_used(text: &str, term: &str) -> bool {
    let term = term.trim();
    if term.is_empty() {
        return false;
    }
    let compact = term
        .chars()
        .filter(|character| !character.is_whitespace())
        .all(unspaced);
    let normalize = |text: &str| {
        text.chars()
            .filter(|character| !compact || !character.is_whitespace())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    let text = normalize(text);
    let term = normalize(term);
    let first = term.chars().next().expect("nonempty term");
    let last = term.chars().next_back().expect("nonempty term");
    text.match_indices(&term).any(|(start, _)| {
        let before = text[..start].chars().next_back();
        let after = text[start + term.len()..].chars().next();
        (!word(first) || unspaced(first) || before.is_none_or(|character| !word(character)))
            && (!word(last) || unspaced(last) || after.is_none_or(|character| !word(character)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GlossaryEntry;

    fn term(id: &str, source: &str, target: &str) -> GlossaryEntry {
        GlossaryEntry {
            id: id.into(), source: source.into(), target: target.into(),
            ..Default::default()
        }
    }

    fn linked() -> (Series, ProjectContext) {
        let series = Series::new("Series".into()).unwrap();
        let context = ProjectContext { series: Some(series.id.clone()), ..Default::default() };
        (series, context)
    }

    #[test]
    fn only_used_terms_are_saved_unlocked_and_repeating_is_empty() {
        let (mut series, mut context) = linked();
        let mut used = term("one", "魔王", "Demon King");
        used.locked = true;
        context.suggestions = vec![used, term("two", "Ann", "Anna"), term("three", "村", "Village")];
        let rows = [UsedTranslation { source: "魔\n王だ。 Annette", translation: "Demon King and Anna" }];
        let result = save_chapter_terms_to_series(&mut series, &mut context, &rows).unwrap();
        assert_eq!(result.added, 1);
        assert!(!series.glossary[0].locked);
        assert_eq!(context.suggestions.len(), 2);
        assert_eq!(save_chapter_terms_to_series(&mut series, &mut context, &rows).unwrap().added, 0);
        assert!(!term_used("Annabelle", "Anna"));
        assert!(!term_used("Bob", ""));
    }

    #[test]
    fn conflicts_are_visible_before_saving_and_leave_is_offered_only_once() {
        let (mut series, mut context) = linked();
        series.glossary.push(term("old", "ヒナ", "Hina"));
        context.suggestions.push(term("new", "ヒナ", "Hena"));
        let rows = [UsedTranslation { source: "ヒナ！", translation: "Hena!" }];
        let conflicts = term_conflicts(&context, &series).unwrap();
        let conflict = &conflicts[0];
        let mut context = serde_json::from_value(serde_json::to_value(&context).unwrap()).unwrap();
        resolve_term_conflict(&mut series, &mut context, conflict, TermConflictChoice::Leave).unwrap();
        assert!(save_chapter_terms_to_series(&mut series, &mut context, &rows).unwrap().conflicts.is_empty());
        assert_eq!(series.glossary[0].target, "Hina");
        assert_eq!(context.suggestions.len(), 1);
    }

    #[test]
    fn explicit_conflict_choice_preserves_lock_and_rejects_stale_target() {
        let (mut series, mut context) = linked();
        let mut entry = term("old", "ヒナ", "Hina");
        entry.locked = true;
        series.glossary.push(entry);
        context.suggestions.push(term("new", "ヒナ", "Hena"));
        let rows = [UsedTranslation { source: "ヒナ", translation: "Hena" }];
        let conflict = save_chapter_terms_to_series(&mut series, &mut context, &rows).unwrap().conflicts.remove(0);
        let mut changed = series.clone();
        changed.glossary[0].target = "Other".into();
        assert!(resolve_term_conflict(&mut changed, &mut context, &conflict, TermConflictChoice::UseChapter).is_err());
        resolve_term_conflict(&mut series, &mut context, &conflict, TermConflictChoice::UseChapter).unwrap();
        assert_eq!(series.glossary[0].target, "Hena");
        assert!(series.glossary[0].locked);
        assert!(context.suggestions.is_empty());
    }

    #[test]
    fn characters_preserve_hand_edits_and_only_publish_confident_notes() {
        let (mut series, _) = linked();
        series.characters.push(Character { id: "hand".into(), name: "Hina".into(), original: "ヒナ".into(), notes: "Hand note".into() });
        let characters = vec![
            ChapterCharacter { name: "Hena".into(), original: "ヒナ".into(), notes: "Wrong".into(), notes_confidence: Some(1.0), ..Default::default() },
            ChapterCharacter { name: "Jun".into(), notes: "They/them".into(), notes_confidence: Some(0.99), ..Default::default() },
            ChapterCharacter { name: "Rei".into(), notes: "Guessed".into(), notes_confidence: Some(0.7), ..Default::default() },
        ];
        assert_eq!(merge_chapter_characters(&mut series, &characters), 2);
        assert_eq!(series.characters[0].notes, "Hand note");
        assert_eq!(series.characters[1].notes, "They/them");
        assert!(series.characters[2].notes.is_empty());
        assert_eq!(merge_chapter_characters(&mut series, &characters), 0);
    }
}
