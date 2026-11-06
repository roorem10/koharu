use std::collections::BTreeMap;

use anyhow::Result;
use booharu_context::LetteringCategory;
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SfxMode {
    #[default]
    Skip,
    InsideBubbles,
    TranslateAll,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, Type)]
#[serde(default)]
pub(crate) struct LetteringSettings {
    pub sfx: SfxMode,
    pub model_confidence: f32,
    /// Estimated source glyph size as a fraction of page height.
    pub small_text_threshold: f32,
    /// Overrides only: an absent entry uses the category's default, while a
    /// present entry with no base size retains the original unrestricted fit.
    pub category_sizes: BTreeMap<LetteringCategory, CategorySize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize, Type)]
pub(crate) struct CategorySize {
    pub base_size: Option<f32>,
    pub may_grow: bool,
}

impl CategorySize {
    pub(crate) fn for_category(category: LetteringCategory) -> Self {
        use LetteringCategory::*;
        // At the existing 2000px test-page height, dialogue is 32px (1.6%),
        // asides 24px (the small-text classifier's 1.2% boundary), shouts
        // 40px and titles/SFX 48px. These leave room for several dialogue
        // lines in the usual 100–200px bubble without enlarging a short reply.
        let base_size = match category {
            SmallText => 0.012,
            PhoneText | Whisper => 0.014,
            Shout => 0.020,
            WeakShout => 0.018,
            Title | Sfx => 0.024,
            _ => 0.016,
        };
        Self {
            base_size: Some(base_size),
            may_grow: matches!(category, Shout | Title | Sfx),
        }
    }

    fn validate(self) -> Result<()> {
        anyhow::ensure!(
            self.base_size
                .is_none_or(|size| size.is_finite() && size > 0.0 && size <= 1.0),
            "base size must be a page-height fraction greater than zero and at most one"
        );
        Ok(())
    }
}

impl Default for LetteringSettings {
    fn default() -> Self {
        Self {
            sfx: SfxMode::Skip,
            model_confidence: 0.6,
            // 24px on a 2000px page, between the synthetic aside and dialogue sizes.
            small_text_threshold: 0.012,
            // Config deep-merges TOML defaults. Keep the override table empty
            // so saving base_size=None cannot resurrect a preset on reopen.
            category_sizes: BTreeMap::new(),
        }
    }
}

impl LetteringSettings {
    pub(crate) fn category_size(&self, category: LetteringCategory) -> CategorySize {
        self.category_sizes
            .get(&category)
            .copied()
            .unwrap_or_else(|| CategorySize::for_category(category))
    }

    pub(crate) fn resolved(mut self) -> Self {
        for category in LetteringCategory::ALL {
            self.category_sizes
                .entry(category)
                .or_insert_with(|| CategorySize::for_category(category));
        }
        self
    }

    pub(crate) fn load() -> Result<koharu_config::Config<Self>> {
        let config = koharu_config::load::<Self>("lettering")?;
        config.read()?.validate()?;
        Ok(config)
    }

    pub(crate) fn validate(&self) -> Result<()> {
        for size in self.category_sizes.values() {
            size.validate()?;
        }
        anyhow::ensure!(
            self.model_confidence.is_finite() && (0.0..=1.0).contains(&self.model_confidence),
            "lettering confidence must be between zero and one"
        );
        anyhow::ensure!(
            self.small_text_threshold.is_finite()
                && self.small_text_threshold > 0.0
                && self.small_text_threshold <= 1.0,
            "small text threshold must be a fraction greater than zero and at most one"
        );
        Ok(())
    }
}
