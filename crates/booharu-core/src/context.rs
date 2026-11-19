//! The series glossary and the chapter notes every translation follows.
//!
//! - A series ([`Series`]) is a glossary of names and terms, a list of
//!   characters, and notes, kept by [`SeriesStore`] as one JSON file per
//!   series in the Booharu home's `series` folder, in Booharu 0.0.4's shape,
//!   so 0.0.4's files and exports load unchanged. Chapters link to one, and
//!   deleting a chapter keeps it.
//! - A chapter's own data ([`ChapterContext`]) lives in a side file beside
//!   its project: the series it links, the brief Read chapter writes, and
//!   the terms suggested for it. Per-line facts are layer attributes, never
//!   kept there.
//! - [`Extras`] is what a translation run sends with the chapter: the
//!   glossary terms found in its text, the background, and its characters,
//!   which koharu-pipeline's snapshot hook appends to the user's
//!   Instructions in the run's snapshot ([`instructions`]), the same text for
//!   the same inputs, so Koharu's conversation goes on while nothing changed.
//! - [`instruction_terms`] reads a glossary out of the Instructions, for
//!   Import from instructions.
//! - [`agent`] keeps the project's conversation with the agent in a second
//!   side file, so the chat comes back with the project.
//!
//! Terms are told apart by their source, trimmed, ignoring case and
//! whitespace ([`same_source`]), and found in a text the same way
//! ([`occurs`]). A series file may hold one source twice, as 0.0.4's could;
//! where terms are used, the first locked one wins, else the first.

pub mod agent;
mod chapter;
mod compose;
mod instructions;
mod series;

use std::path::{Path, PathBuf};

pub use self::{
    chapter::{
        CHAPTER_VERSION, Chapter, ChapterBrief, ChapterCharacter, ChapterContext, SideFiles,
        agent_file, remove_side_files, side_file,
    },
    compose::{
        CharacterHint, Extras, MAX_TERMS, found_terms, instructions, line_terms, slips, text_known,
    },
    instructions::{InstructionTerm, instruction_terms},
    series::{Character, GlossaryEntry, Series, SeriesStore, TermKind, valid_id},
};

/// Whether `source`, a term's source, occurs in `text`, ignoring case and
/// whitespace, as Booharu 0.0.4's `term_occurs` found it, but for case. A
/// blank source occurs nowhere.
pub fn occurs(text: &str, source: &str) -> bool {
    let source = key(source);
    !source.is_empty() && key(text).contains(&source)
}

/// Whether `a` and `b`, two terms' sources, name the same term: trimmed,
/// ignoring case and whitespace.
pub fn same_source(a: &str, b: &str) -> bool {
    key(a) == key(b)
}

/// `text` as terms are compared: without whitespace, in lower case.
fn key(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>()
        .to_lowercase()
}

/// Where Booharu 0.0.4 kept its series, `PreviewData\settings\series` next
/// to the running program, when that folder is there: 0.0.4 kept its data
/// beside `Booharu.exe`, and an update to 0.0.5 in the same folder leaves it.
/// Booharu doesn't copy it (D20): the first-start note says where it is, and
/// Import in the glossary editor reads its files as they are.
pub fn old_series_folder() -> Option<PathBuf> {
    let program = std::env::current_exe().ok()?;
    old_series_in(program.parent()?)
}

/// 0.0.4's series folder beside a program in `folder`, when it's there.
fn old_series_in(folder: &Path) -> Option<PathBuf> {
    let series = folder.join("PreviewData").join("settings").join("series");
    series.is_dir().then_some(series)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_term_is_found_ignoring_case_and_whitespace() {
        assert!(occurs("ヒ ナと魔 王", "ヒナ"));
        assert!(occurs("ヒナと魔王", " 魔 王 "));
        assert!(occurs("Welcome to SCHALE!", "Schale"));
        assert!(occurs("the demon king", "Demon  King"));
        assert!(!occurs("ヒナと魔王", "リムル"));
        assert!(!occurs("anything", "  "));
        assert!(!occurs("", "ヒナ"));
        assert!(same_source(" Demon King ", "demonking"));
        assert!(!same_source("ヒナ", "ヒナタ"));
    }

    #[test]
    fn the_old_series_folder_is_found_only_where_it_is() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        assert_eq!(old_series_in(dir.path()), None);
        let series = dir
            .path()
            .join("PreviewData")
            .join("settings")
            .join("series");
        fs_err::create_dir_all(&series)?;
        assert_eq!(old_series_in(dir.path()), Some(series));
        Ok(())
    }
}
