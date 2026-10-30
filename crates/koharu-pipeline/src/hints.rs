use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use koharu_scene::{EntityId, Snapshot};
use koharu_translator::{GlossaryHint, TranslationContext, TranslationRequest};

#[derive(Clone, Debug)]
pub struct GlossaryTerm {
    pub hint: GlossaryHint,
    pub locked: bool,
}

/// Per-run extras for the translation stage, filled in by the app before a run.
/// The default changes nothing, so callers that don't know about it keep today's behavior.
#[derive(Clone, Debug, Default)]
pub struct TranslationHints {
    /// Text layers the translation stage leaves untouched, such as skipped SFX.
    pub skip: BTreeSet<EntityId>,
    pub glossary: Vec<GlossaryTerm>,
    pub background: String,
    pub characters: BTreeMap<EntityId, String>,
    pub previous_pages: u8,
}

impl TranslationHints {
    pub(crate) fn apply(
        &self,
        request: &mut TranslationRequest,
        scene: &Snapshot,
        page: EntityId,
    ) -> Result<()> {
        request.glossary = page_glossary(&self.glossary, &request.segments);
        let brief = [
            self.background.as_str(),
            self.characters.get(&page).map_or("", String::as_str),
        ]
        .into_iter()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
        if !brief.is_empty() {
            request.brief = Some(brief);
        }
        request.context = previous_context(scene, page, self.previous_pages)?;
        Ok(())
    }
}

pub fn term_occurs(text: &str, source: &str) -> bool {
    let source = source.trim();
    if source.is_empty() {
        return false;
    }
    text.contains(source)
        || text
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .contains(
                &source
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>(),
            )
}

pub fn page_glossary(glossary: &[GlossaryTerm], sources: &[String]) -> Vec<GlossaryHint> {
    let mut matches: Vec<_> = glossary
        .iter()
        .filter(|term| {
            sources
                .iter()
                .any(|source| term_occurs(source, &term.hint.source))
        })
        .collect();
    matches.sort_by_key(|entry| !entry.locked);
    if matches.len() > 60 {
        tracing::warn!(matching = matches.len(), "page glossary capped at 60 terms");
    }
    let mut seen = BTreeSet::new();
    matches
        .into_iter()
        .filter(|entry| seen.insert(entry.hint.source.trim().to_owned()))
        .take(60)
        .map(|entry| entry.hint.clone())
        .collect()
}

pub fn previous_context(
    scene: &Snapshot,
    page: EntityId,
    count: u8,
) -> Result<Vec<TranslationContext>> {
    let pages: Vec<_> = scene.pages().map(|page| page.id()).collect();
    let Some(index) = pages.iter().position(|id| *id == page) else {
        return Ok(Vec::new());
    };
    let mut context = Vec::new();
    for id in &pages[index.saturating_sub(usize::from(count.min(2)))..index] {
        if let Some(group) = scene.page(*id)?.text_group()? {
            for layer in group.text_layers()? {
                let content = layer.content()?;
                if let (Some(source), Some(translation)) =
                    (content.source()?, content.translation()?)
                    && !source.text.value.trim().is_empty()
                    && !translation.text.value.trim().is_empty()
                {
                    context.push(TranslationContext::new(
                        source.text.value,
                        translation.text.value,
                    ));
                }
            }
        }
    }
    Ok(context)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn glossary_matches_trimmed_and_spaced_ocr_and_prioritizes_locked_terms() {
        let term = |source: &str, locked| GlossaryTerm {
            hint: GlossaryHint {
                source: source.into(),
                target: source.into(),
                note: None,
            },
            locked,
        };
        let glossary = vec![
            term("ヒナ", false),
            term(" 魔王 ", true),
            term("unused", true),
        ];
        let matches = page_glossary(&glossary, &["ヒ ナと魔 王".into()]);
        assert_eq!(
            matches
                .iter()
                .map(|term| term.source.as_str())
                .collect::<Vec<_>>(),
            [" 魔王 ", "ヒナ"]
        );
        assert!(!term_occurs("anything", "  "));
    }

    #[tokio::test]
    async fn empty_hints_leave_request_unchanged_and_previous_pages_are_bounded() {
        let mut session = koharu_scene::Session::memory().await.unwrap();
        let patch = session
            .snapshot()
            .patch(|edit| {
                for name in ["one", "two", "three", "four"] {
                    let page = edit.add_page(
                        koharu_scene::PageDraft::new(name, 100.0, 100.0),
                        koharu_scene::At::End,
                    )?;
                    for index in 0..2 {
                        let content = edit.add_text_content(page, koharu_scene::At::End)?;
                        edit.set(
                            content,
                            &koharu_scene::SourceText {
                                text: koharu_scene::Authored::user(format!("{name}-{index}")),
                                language: None,
                            },
                        )?;
                        if name != "two" || index == 0 {
                            edit.set(
                                content,
                                &koharu_scene::Translation {
                                    text: koharu_scene::Authored::user(format!(
                                        "translated-{name}-{index}"
                                    )),
                                    language: None,
                                },
                            )?;
                        }
                        edit.add_text_layer(
                            page,
                            koharu_scene::At::End,
                            content,
                            &koharu_scene::TextLayout {
                                origin: koharu_scene::Origin::User,
                                kind: koharu_scene::TextLayoutKind::Paragraph,
                                angle_degrees: None,
                            },
                        )?;
                    }
                }
                Ok(())
            })
            .unwrap();
        session.commit(patch).await.unwrap();
        let scene = session.snapshot();
        let page = scene.pages().last().unwrap().id();
        let mut request = TranslationRequest::new(["hello"], koharu_translator::Language::English);
        let before = request.clone();
        TranslationHints::default()
            .apply(&mut request, &scene, page)
            .unwrap();
        assert_eq!(request, before);
        assert!(previous_context(&scene, page, 0).unwrap().is_empty());
        let one = previous_context(&scene, page, 1).unwrap();
        assert_eq!(
            one.iter()
                .map(|entry| entry.source.as_str())
                .collect::<Vec<_>>(),
            ["three-0", "three-1"]
        );
        let two = previous_context(&scene, page, 2).unwrap();
        assert_eq!(
            two.iter()
                .map(|entry| entry.source.as_str())
                .collect::<Vec<_>>(),
            ["two-0", "three-0", "three-1"]
        );
        assert_eq!(previous_context(&scene, page, 255).unwrap(), two);
        assert!(
            previous_context(&scene, scene.pages().next().unwrap().id(), 2)
                .unwrap()
                .is_empty()
        );
    }
}
