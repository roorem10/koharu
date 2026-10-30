use serde::{Deserialize, Serialize};
use specta::Type;

/// How a line is presented or delivered on the page. Each category has its own
/// font settings; the order here is the order shown in Settings.
#[derive(
    Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum LetteringCategory {
    NormalDialogue,
    Thoughts,
    Floating,
    SmallText,
    NarrationBox,
    Shout,
    WeakShout,
    Sfx,
    Moan,
    Title,
    PhoneText,
    Whisper,
    Shy,
    Crying,
    Menacing,
}

impl LetteringCategory {
    pub const ALL: [Self; 15] = [
        Self::NormalDialogue,
        Self::Thoughts,
        Self::Floating,
        Self::SmallText,
        Self::NarrationBox,
        Self::Shout,
        Self::WeakShout,
        Self::Sfx,
        Self::Moan,
        Self::Title,
        Self::PhoneText,
        Self::Whisper,
        Self::Shy,
        Self::Crying,
        Self::Menacing,
    ];

    /// The stable id used in JSON, prompts and translation keys.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NormalDialogue => "normal_dialogue",
            Self::Thoughts => "thoughts",
            Self::Floating => "floating",
            Self::SmallText => "small_text",
            Self::NarrationBox => "narration_box",
            Self::Shout => "shout",
            Self::WeakShout => "weak_shout",
            Self::Sfx => "sfx",
            Self::Moan => "moan",
            Self::Title => "title",
            Self::PhoneText => "phone_text",
            Self::Whisper => "whisper",
            Self::Shy => "shy",
            Self::Crying => "crying",
            Self::Menacing => "menacing",
        }
    }

    /// English name, for model prompts. The UI uses its own translated names.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::NormalDialogue => "Normal Dialogue",
            Self::Thoughts => "Thoughts",
            Self::Floating => "Floating",
            Self::SmallText => "Small Text",
            Self::NarrationBox => "Narration Box",
            Self::Shout => "Shout",
            Self::WeakShout => "Weak/Panic Shout",
            Self::Sfx => "SFX",
            Self::Moan => "Moan",
            Self::Title => "Title",
            Self::PhoneText => "Phone Text",
            Self::Whisper => "Whisper",
            Self::Shy => "Shy / Hesitant",
            Self::Crying => "Crying / Trembling",
            Self::Menacing => "Menacing / Sinister",
        }
    }

    /// Parses the id from [`Self::as_str`], ignoring case and surrounding space.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        Self::ALL
            .into_iter()
            .find(|category| category.as_str().eq_ignore_ascii_case(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_roundtrip_through_serde_and_parse() {
        for category in LetteringCategory::ALL {
            let json = serde_json::to_string(&category).unwrap();
            assert_eq!(json, format!("\"{}\"", category.as_str()));
            assert_eq!(
                serde_json::from_str::<LetteringCategory>(&json).unwrap(),
                category
            );
            assert_eq!(LetteringCategory::parse(category.as_str()), Some(category));
        }
        assert_eq!(
            LetteringCategory::parse(" SFX "),
            Some(LetteringCategory::Sfx)
        );
        assert_eq!(LetteringCategory::parse("tone"), None);
    }

    #[test]
    fn all_lists_every_category_once() {
        let mut ids = LetteringCategory::ALL
            .map(LetteringCategory::as_str)
            .to_vec();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 15);
    }
}
