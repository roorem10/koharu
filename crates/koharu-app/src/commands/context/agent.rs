use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
pub(crate) struct CharacterInput {
    pub id: String,
    pub name: String,
    pub original: String,
    pub notes: String,
}

impl From<CharacterInput> for booharu_context::Character {
    fn from(value: CharacterInput) -> Self {
        Self {
            id: value.id,
            name: value.name,
            original: value.original,
            notes: value.notes,
        }
    }
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct EditSeriesCharacter {
    pub expected: Option<CharacterInput>,
    pub character: CharacterInput,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct ResolveSeriesTermConflict {
    pub source: String,
    pub series_target: String,
    pub chapter_target: String,
    pub choice: ConflictChoice,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConflictChoice {
    KeepSeries,
    UseChapter,
    Leave,
}

impl From<ConflictChoice> for booharu_context::TermConflictChoice {
    fn from(value: ConflictChoice) -> Self {
        match value {
            ConflictChoice::KeepSeries => Self::KeepSeries,
            ConflictChoice::UseChapter => Self::UseChapter,
            ConflictChoice::Leave => Self::Leave,
        }
    }
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct UpdateSeriesTerm {
    pub series_id: String,
    pub source: String,
    pub expected_target: String,
    pub target: String,
}
