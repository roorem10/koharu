use anyhow::{Context as _, Result, ensure};
use booharu_context::{
    Character, ProjectContext, Series, TermConflict, TermConflictChoice, TermSyncResult,
};
use koharu_scene::{Commit, ContextEdit, EntityId, Revision};
use serde_json::{Value, json};
use tauri::{AppHandle, Manager as _};
use tauri_runtime_cef::CefRuntime;

use super::super::{
    ChannelExt as _, Error,
    canvas::CanvasChannel,
    project::{CurrentProject, Project},
};

const TERMS: &str = "series_terms";

#[derive(Clone)]
pub(crate) struct SeriesEdit {
    before: Series,
    after: Series,
    before_terms: Option<Value>,
    after_terms: Option<Value>,
    before_suggestions: Vec<booharu_context::GlossaryEntry>,
    after_suggestions: Vec<booharu_context::GlossaryEntry>,
}

impl SeriesEdit {
    fn reversed(&self) -> Self {
        Self {
            before: self.after.clone(),
            after: self.before.clone(),
            before_terms: self.after_terms.clone(),
            after_terms: self.before_terms.clone(),
            before_suggestions: self.after_suggestions.clone(),
            after_suggestions: self.before_suggestions.clone(),
        }
    }

    fn apply(&self, project: &Project) -> Result<()> {
        let context = project.load_context()?;
        ensure!(
            context.series.as_deref() == Some(self.before.id.as_str()),
            "the linked series changed"
        );
        self.changed_context(context)?;
        let store = super::series::store()?;
        store.apply_delta(&self.before, &self.after)?;
        if let Err(error) = self.apply_context(project) {
            store.apply_delta(&self.after, &self.before)
                .context("could not restore series after context write failed")?;
            return Err(error);
        }
        Ok(())
    }

    fn changed_context(&self, mut context: ProjectContext) -> Result<ProjectContext> {
        if self.before_terms != self.after_terms {
            ensure!(
                context.other.get(TERMS) == self.before_terms.as_ref(),
                "series choices changed; undo the newer edit first"
            );
        }
        let ids: std::collections::BTreeSet<_> = self
            .before_suggestions
            .iter()
            .chain(&self.after_suggestions)
            .map(|entry| entry.id.as_str())
            .collect();
        for id in ids {
            let before = self.before_suggestions.iter().find(|entry| entry.id == id);
            let after = self.after_suggestions.iter().find(|entry| entry.id == id);
            if before == after {
                continue;
            }
            ensure!(
                context.suggestions.iter().find(|entry| entry.id == id) == before,
                "chapter suggestion changed; undo the newer edit first"
            );
            context.suggestions.retain(|entry| entry.id != id);
            if let Some(after) = after {
                ensure!(
                    !context.suggestions.iter().any(|entry| {
                        entry.source.trim() == after.source.trim()
                    }),
                    "chapter suggestion source changed"
                );
                context.suggestions.push(after.clone());
            }
        }
        if self.before_terms != self.after_terms {
            match &self.after_terms {
                Some(value) => {
                    context.other.insert(TERMS.into(), value.clone());
                }
                None => {
                    context.other.remove(TERMS);
                }
            }
        }
        Ok(context)
    }

    fn apply_context(&self, project: &Project) -> Result<()> {
        project.update_context(|context| -> Result<()> {
            *context = self.changed_context(context.clone())?;
            Ok(())
        })??;
        Ok(())
    }
}

/// Applies inverses before Scene advances. A stale entry rejects undo without
/// popping history; unrelated entries from another chapter survive the delta.
pub(crate) fn reverse_edits(project: &Project, revisions: &[Revision]) -> Result<Vec<SeriesEdit>> {
    let mut applied: Vec<SeriesEdit> = Vec::new();
    let revisions: std::collections::BTreeSet<_> = revisions.iter().copied().collect();
    for revision in revisions.iter().rev() {
        if let Some(edits) = project.series_edits.get(revision) {
            for edit in edits.iter().rev() {
                let inverse = edit.reversed();
                if let Err(error) = inverse.apply(project) {
                    restore_edits(project, &applied)?;
                    return Err(error);
                }
                applied.push(inverse);
            }
        }
    }
    Ok(applied)
}

pub(crate) fn restore_edits(project: &Project, edits: &[SeriesEdit]) -> Result<()> {
    for edit in edits.iter().rev() {
        edit.reversed().apply(project)?;
    }
    Ok(())
}

pub(crate) fn record_inverse(project: &mut Project, revision: Revision, edits: Vec<SeriesEdit>) {
    if !edits.is_empty() {
        project.series_edits.insert(revision, edits);
    }
}

async fn edit_series<T>(
    project: &mut Project,
    reason: &str,
    change: impl FnOnce(&mut Series, &mut ProjectContext) -> Result<T>,
) -> Result<(Option<Commit>, T)> {
    let context = project.load_context()?;
    let id = context.series.as_deref().context("pick a series first")?;
    let mut next_context = context.clone();
    let (before, after, value) = super::series::store()?.update(id, |series| {
        let before = series.clone();
        let value = change(series, &mut next_context)?;
        Ok((before, series.clone(), value))
    })?;
    let edit = SeriesEdit {
        before,
        after,
        before_terms: context.other.get(TERMS).cloned(),
        after_terms: next_context.other.get(TERMS).cloned(),
        before_suggestions: context.suggestions.clone(),
        after_suggestions: next_context.suggestions.clone(),
    };
    if edit.before == edit.after
        && edit.before_terms == edit.after_terms
        && edit.before_suggestions == edit.after_suggestions
    {
        return Ok((None, value));
    }
    let mut context_applied = false;
    let result = async {
        edit.apply_context(project)?;
        context_applied = true;
        let patch = project.snapshot().patch(|scene| {
            scene.set_project(&ContextEdit {
                token: EntityId::new().to_string(),
                reason: reason.into(),
            })
        })?;
        project.commit(patch).await
    }
    .await;
    match result {
        Ok(commit) => {
            project.series_edits.insert(commit.revision, vec![edit]);
            Ok((Some(commit), value))
        }
        Err(error) => {
            super::series::store()?.apply_delta(&edit.after, &edit.before)
                .context("could not restore series after project commit failed")?;
            if context_applied {
                edit.reversed().apply_context(project)?;
            }
            Err(error)
        }
    }
}

pub(crate) async fn save_terms(project: &mut Project) -> Result<(Option<Commit>, TermSyncResult)> {
    let mut rows = Vec::new();
    for page in project.snapshot().pages() {
        if let Some(group) = page.text_group()? {
            for layer in group.text_layers()? {
                let content = layer.content()?;
                if let (Some(source), Some(translation)) =
                    (content.source()?, content.translation()?)
                {
                    rows.push((source.text.value, translation.text.value));
                }
            }
        }
    }
    let used: Vec<_> = rows
        .iter()
        .map(|(source, translation)| booharu_context::UsedTranslation {
            source,
            translation,
        })
        .collect();
    edit_series(
        project,
        "Save chapter terms used in translations to the series",
        |series, context| {
            booharu_context::save_chapter_terms_to_series(series, context, &used)
        },
    )
    .await
}

/// Shared hook for chapter completion and the auto-run recipe. Its caller
/// records and synchronizes the returned commit just like a manual save.
pub(crate) async fn auto_save_chapter_terms_to_series(
    project: &mut Project,
) -> Result<Option<Commit>> {
    if !super::settings::load()?.auto_save_terms || project.load_context()?.series.is_none() {
        return Ok(None);
    }
    Ok(save_terms(project).await?.0)
}

pub(crate) async fn save_characters_from_reading(
    project: &mut Project,
    characters: &[booharu_context::ChapterCharacter],
) -> Result<Option<Commit>> {
    Ok(edit_series(
        project,
        "Read chapter identified new series characters",
        |series, _| Ok(booharu_context::merge_chapter_characters(series, characters)),
    )
    .await?
    .0)
}

pub(crate) async fn resolve_conflict(
    project: &mut Project,
    conflict: TermConflict,
    choice: TermConflictChoice,
) -> Result<(Option<Commit>, ())> {
    edit_series(
        project,
        "Resolve chapter and series term conflict",
        |series, context| {
            booharu_context::resolve_term_conflict(series, context, &conflict, choice)
        },
    )
    .await
}

pub(crate) async fn edit_character(
    project: &mut Project,
    expected: Option<Character>,
    character: Character,
) -> Result<(Option<Commit>, ())> {
    edit_series(project, "Edit series character", |series, _| {
        ensure!(!character.name.trim().is_empty(), "character needs a name");
        let existing = series.characters.iter().position(|entry| entry.id == character.id);
        ensure!(
            existing.map(|index| &series.characters[index]) == expected.as_ref(),
            "character changed; read it again"
        );
        ensure!(
            !series.characters.iter().any(|entry| {
                entry.id != character.id
                    && (entry.name.trim().to_lowercase() == character.name.trim().to_lowercase()
                        || (!character.original.trim().is_empty()
                            && entry.original.trim() == character.original.trim()))
            }),
            "character already exists"
        );
        match existing {
            Some(index) => series.characters[index] = character,
            None => {
                let mut character = character;
                character.id = EntityId::new().to_string();
                series.characters.push(character);
            }
        }
        Ok(())
    })
    .await
}

fn known_target(series: &Series, source: &str, expected: &str) -> bool {
    series.glossary.iter().any(|entry| {
        entry.source.trim() == source && entry.target.trim() == expected
    }) || series.characters.iter().any(|entry| {
        !entry.original.trim().is_empty()
            && entry.original.trim() == source
            && entry.name.trim() == expected
    })
}

pub(crate) async fn update_term(
    project: &mut Project,
    series_id: String,
    source: String,
    expected_target: String,
    target: String,
) -> Result<(Option<Commit>, ())> {
    edit_series(project, "Use hand-edited term spelling throughout the series", |series, _| {
        ensure!(series.id == series_id, "the linked series changed");
        ensure!(
            !source.trim().is_empty() && !target.trim().is_empty(),
            "term needs source and target"
        );
        ensure!(
            known_target(series, source.trim(), expected_target.trim()),
            "term changed; read it again"
        );
        if let Some(term) = series
            .glossary
            .iter_mut()
            .find(|entry| entry.source.trim() == source.trim())
        {
            ensure!(term.target.trim() == expected_target.trim(), "term changed; read it again");
            term.target = target.trim().into();
        } else {
            series.glossary.push(booharu_context::GlossaryEntry {
                id: EntityId::new().to_string(),
                source: source.trim().into(),
                target: target.trim().into(),
                kind: booharu_context::TermKind::Name,
                ..Default::default()
            });
        }
        for character in &mut series.characters {
            if character.original.trim() == source.trim()
                && character.name.trim() == expected_target.trim()
            {
                character.name = target.trim().into();
            }
        }
        Ok(())
    })
    .await
}

async fn synchronize(
    handle: &AppHandle<CefRuntime>,
    commit: Option<Commit>,
    page: Option<EntityId>,
) -> Result<()> {
    if let Some(commit) = commit {
        let desktop = handle.state::<koharu_desktop::Desktop>();
        desktop.synchronize(&commit.snapshot, page, &commit).await?;
        handle.state::<CanvasChannel>().channel.publish(desktop.canvas_state());
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn save_chapter_terms_to_series(
    handle: AppHandle<CefRuntime>,
) -> std::result::Result<TermSyncResult, Error> {
    let (commit, page, value) = {
        let projects = handle.state::<CurrentProject>();
        let mut projects = projects.project.lock().await;
        let project = projects.as_mut().context("no project is open")?;
        let (commit, value) = save_terms(project).await?;
        if let Some(commit) = &commit {
            project.record_commit(commit);
        }
        (commit, project.active_page(), value)
    };
    synchronize(&handle, commit, page).await?;
    Ok(value)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn resolve_series_term_conflict(
    conflict: TermConflict,
    choice: TermConflictChoice,
    handle: AppHandle<CefRuntime>,
) -> std::result::Result<(), Error> {
    let (commit, page) = {
        let projects = handle.state::<CurrentProject>();
        let mut projects = projects.project.lock().await;
        let project = projects.as_mut().context("no project is open")?;
        let (commit, ()) = resolve_conflict(project, conflict, choice).await?;
        if let Some(commit) = &commit {
            project.record_commit(commit);
        }
        (commit, project.active_page())
    };
    Ok(synchronize(&handle, commit, page).await?)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn update_series_term(
    series_id: String,
    source: String,
    expected_target: String,
    target: String,
    project_name: String,
    handle: AppHandle<CefRuntime>,
) -> std::result::Result<(), Error> {
    let (commit, page) = {
        let projects = handle.state::<CurrentProject>();
        let mut projects = projects.project.lock().await;
        let project = projects.as_mut().context("no project is open")?;
        if project.name != project_name {
            return Err(anyhow::anyhow!("the open project changed").into());
        }
        let (commit, ()) = update_term(project, series_id, source, expected_target, target).await?;
        if let Some(commit) = &commit {
            project.record_commit(commit);
        }
        (commit, project.active_page())
    };
    Ok(synchronize(&handle, commit, page).await?)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn claim_term_offer(
    series_id: String,
    source: String,
    expected_target: String,
    target: String,
    project_name: String,
    project: tauri::State<'_, CurrentProject>,
) -> std::result::Result<bool, Error> {
    let projects = project.project.lock().await;
    let project = projects.as_ref().context("no project is open")?;
    if project.name != project_name {
        return Err(anyhow::anyhow!("the open project changed").into());
    }
    let context = project.load_context()?;
    if context.series.as_deref() != Some(series_id.as_str()) {
        return Err(anyhow::anyhow!("the linked series changed").into());
    }
    let series = super::series::store()?.get(&series_id)?;
    if !known_target(&series, source.trim(), expected_target.trim()) {
        return Err(anyhow::anyhow!("term changed; read it again").into());
    }
    let key = json!([series_id, source.trim(), expected_target.trim(), target.trim()]);
    Ok(project.update_context(|context| -> Result<bool> {
        let offers = context
            .other
            .entry("series_term_offers")
            .or_insert_with(|| json!([]));
        let offers = offers.as_array_mut().context("invalid saved term offers")?;
        if offers.contains(&key) {
            return Ok(false);
        }
        offers.push(key);
        Ok(true)
    })??)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn series_edit_undo_preserves_other_chapters_and_rejects_stale_entries() {
        let directory = tempfile::tempdir().unwrap();
        let mut project = Project::new(
            koharu_scene::Session::memory().await.unwrap(),
            "chapter".into(),
        );
        project.directory = Some(directory.path().to_path_buf());
        let store = super::super::series::store().unwrap();
        let mut series = Series::new("Series undo test".into()).unwrap();
        series.glossary.push(booharu_context::GlossaryEntry {
            id: "name".into(),
            source: "ヒナ".into(),
            target: "Hina".into(),
            ..Default::default()
        });
        store.save(&series).unwrap();
        project
            .update_context(|context| context.series = Some(series.id.clone()))
            .unwrap();
        let (commit, ()) = update_term(
            &mut project,
            series.id.clone(),
            "ヒナ".into(),
            "Hina".into(),
            "Hinna".into(),
        )
        .await
        .unwrap();
        let commit = commit.unwrap();
        project.record_commit(&commit);
        let revision = project.revision();
        assert_eq!(project.undo.len(), 1);

        store
            .update(&series.id, |series| {
                series.glossary[0].target = "A later hand edit".into();
                series.glossary.push(booharu_context::GlossaryEntry {
                    id: "other".into(),
                    source: "村".into(),
                    target: "Village".into(),
                    ..Default::default()
                });
                Ok(())
            })
            .unwrap();
        assert!(project.undo().await.is_err());
        assert_eq!(project.revision(), revision);
        assert_eq!(project.undo.len(), 1);
        assert!(project.redo.is_empty());

        store
            .update(&series.id, |series| {
                series.glossary[0].target = "Hinna".into();
                Ok(())
            })
            .unwrap();
        project.undo().await.unwrap();
        let undone = store.get(&series.id).unwrap();
        assert_eq!(undone.glossary[0].target, "Hina");
        assert!(undone.glossary.iter().any(|term| term.id == "other"));
        project.redo().await.unwrap();
        assert_eq!(store.get(&series.id).unwrap().glossary[0].target, "Hinna");
        store.delete(&series.id).unwrap();
    }

    #[test]
    fn context_inverse_changes_only_its_suggestions_and_choices() {
        let series = Series::new("Context inverse".into()).unwrap();
        let suggestion = booharu_context::GlossaryEntry {
            id: "saved".into(),
            source: "a".into(),
            target: "A".into(),
            ..Default::default()
        };
        let unrelated = booharu_context::GlossaryEntry {
            id: "other".into(),
            source: "b".into(),
            target: "B".into(),
            ..Default::default()
        };
        let edit = SeriesEdit {
            before: series.clone(),
            after: series,
            before_terms: None,
            after_terms: None,
            before_suggestions: vec![suggestion.clone()],
            after_suggestions: vec![],
        };
        let mut context = ProjectContext::default();
        context.suggestions.push(unrelated.clone());
        context.other.insert("unrelated".into(), json!("kept"));
        context.other.insert(TERMS.into(), json!({"a later decision":"kept"}));
        let restored = edit.reversed().changed_context(context).unwrap();
        assert!(restored.suggestions.contains(&suggestion));
        assert!(restored.suggestions.contains(&unrelated));
        assert_eq!(restored.other["unrelated"], "kept");
        assert_eq!(restored.other[TERMS], json!({"a later decision":"kept"}));
    }
}
