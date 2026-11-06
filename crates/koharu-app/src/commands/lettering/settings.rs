use anyhow::Result;
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
    pub shout_signals_needed: u8,
}

impl Default for LetteringSettings {
    fn default() -> Self {
        Self {
            sfx: SfxMode::Skip,
            model_confidence: 0.6,
            // 24px on a 2000px page, between the synthetic aside and dialogue sizes.
            small_text_threshold: 0.012,
            shout_signals_needed: 2,
        }
    }
}

impl LetteringSettings {
    pub(crate) fn load() -> Result<koharu_config::Config<Self>> {
        let config = koharu_config::load::<Self>("lettering")?;
        config.read()?.validate()?;
        Ok(config)
    }

    pub(crate) fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            (1..=3).contains(&self.shout_signals_needed),
            "Shout requires between one and three signals"
        );
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
