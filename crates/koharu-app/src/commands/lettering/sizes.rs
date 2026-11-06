use anyhow::{Context as _, Result, ensure};
use booharu_context::{LetteringCategory, ProjectContext};
use koharu_pipeline::bubble_shape::text_bubble_shape;
use koharu_renderer::{AutoFitSizeLimit, auto_fit_size_limit, set_auto_fit_size_limit};
use koharu_scene::{EntityId, Origin, Snapshot, TextRuns, Typography};

use super::{LetteringSettings, resolve_category, settings::CategorySize};
use crate::commands::qc::QcSettings;

pub(crate) fn has_manual_size(typography: Option<&Typography>, runs: Option<&TextRuns>) -> bool {
    // Typography has one owner for all fields. Protect user typography
    // conservatively rather than guessing which earlier edit changed size.
    typography.is_some_and(|value| !value.auto_fit || value.origin == Origin::User)
        || runs.is_some_and(|value| {
            value.origin == Origin::User
                && value.runs.iter().any(|run| run.style.size_scale.is_some())
        })
}

pub(crate) struct SizePolicy {
    pub(crate) settings: LetteringSettings,
    pub(crate) minimum: f32,
}

impl SizePolicy {
    pub(crate) fn load() -> Result<Self> {
        Self::new(
            LetteringSettings::load()?.read()?.clone(),
            koharu_config::load::<QcSettings>("qc")?.read()?.min_font_size,
        )
    }

    pub(crate) fn new(settings: LetteringSettings, minimum: f32) -> Result<Self> {
        settings.validate()?;
        ensure!(
            minimum.is_finite() && minimum > 0.0,
            "invalid QC minimum font size"
        );
        Ok(Self { settings, minimum })
    }

    pub(crate) fn limit(&self, category: LetteringCategory, height: f32) -> Option<AutoFitSizeLimit> {
        let size = self.settings.category_size(category);
        let base = size.base_size? * height;
        Some(AutoFitSizeLimit {
            maximum: if size.may_grow {
                f32::MAX
            } else {
                base.max(self.minimum)
            },
            minimum: self.minimum,
        })
    }

    pub(crate) fn apply(
        &self,
        context: &ProjectContext,
        snapshot: &Snapshot,
        layer: EntityId,
        typography: &mut Typography,
    ) -> Result<()> {
        let original = snapshot.component::<Typography>(layer)?;
        let runs = snapshot.component::<TextRuns>(layer)?;
        if has_manual_size(original.as_ref(), runs.as_ref()) {
            if let Some(original) = original {
                typography.auto_fit = original.auto_fit;
                typography.size = original.size;
                set_auto_fit_size_limit(typography, auto_fit_size_limit(&original)?)?;
            }
            return Ok(());
        }
        if !typography.auto_fit {
            set_auto_fit_size_limit(typography, None)?;
            return Ok(());
        }
        let info = text_bubble_shape(snapshot, layer)?;
        let category = resolve_category(context, snapshot, layer, &info, &self.settings)?.category;
        let mut page = layer;
        while let Some(parent) = snapshot.parent(page)? {
            page = parent;
        }
        let height = snapshot.page(page)?.page()?.height as f32;
        set_auto_fit_size_limit(typography, self.limit(category, height))?;
        Ok(())
    }
}

pub(crate) fn save_category_size(
    category: LetteringCategory,
    base_size: Option<f32>,
    may_grow: bool,
) -> Result<LetteringSettings> {
    let config = LetteringSettings::load()?;
    let mut current = config.write()?;
    let mut settings = current.clone();
    settings.category_sizes.insert(
        category,
        CategorySize {
            base_size,
            may_grow,
        },
    );
    settings.validate().context("invalid category size")?;
    *current = settings.clone();
    current.save()?;
    Ok(settings.resolved())
}

#[cfg(test)]
mod tests {
    use super::*;
    use koharu_scene::{Generation, ProducerId, RunStyle, StyleRun};

    #[test]
    fn category_limits_scale_with_scan_and_respect_clear_grow_and_qc_floor() {
        let mut policy = SizePolicy::new(LetteringSettings::default(), 12.0).unwrap();
        let category = LetteringCategory::NormalDialogue;
        assert_eq!(policy.limit(category, 2000.0).unwrap().maximum, 32.0);
        assert_eq!(policy.limit(category, 4000.0).unwrap().maximum, 64.0);
        assert_eq!(policy.limit(category, 100.0).unwrap().maximum, 12.0);
        assert_eq!(
            policy.limit(LetteringCategory::Shout, 2000.0).unwrap().maximum,
            f32::MAX
        );
        policy.settings.category_sizes.insert(
            category,
            CategorySize {
                base_size: None,
                may_grow: false,
            },
        );
        assert!(policy.limit(category, 2000.0).is_none());
        for size in [0.0, -0.1, f32::NAN, f32::INFINITY, 1.1] {
            policy.settings.category_sizes.insert(
                category,
                CategorySize {
                    base_size: Some(size),
                    may_grow: false,
                },
            );
            assert!(policy.settings.validate().is_err());
        }
    }

    #[test]
    fn manual_word_sizes_are_protected_but_generated_emphasis_is_not() {
        let mut runs = TextRuns {
            origin: Origin::User,
            runs: vec![StyleRun {
                start: 0,
                end: 2,
                style: RunStyle {
                    size_scale: Some(1.15),
                    ..Default::default()
                },
            }],
        };
        assert!(has_manual_size(None, Some(&runs)));
        runs.origin = Origin::Generated(Generation::new(ProducerId::new("dev.booharu.test").unwrap()));
        assert!(!has_manual_size(None, Some(&runs)));
        runs.origin = Origin::User;
        runs.runs[0].style.size_scale = None;
        runs.runs[0].style.font_weight = Some(700);
        assert!(!has_manual_size(None, Some(&runs)));
    }

    #[test]
    fn clearing_a_base_survives_settings_serialization() {
        let mut settings = LetteringSettings::default().resolved();
        settings
            .category_sizes
            .get_mut(&LetteringCategory::NormalDialogue)
            .unwrap()
            .base_size = None;
        let json = serde_json::to_string(&settings).unwrap();
        let restored: LetteringSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(
            restored
                .category_size(LetteringCategory::NormalDialogue)
                .base_size,
            None
        );
        // TOML omits None, retaining the category table through may_grow.
        let omitted: LetteringSettings = serde_json::from_value(serde_json::json!({
            "category_sizes": { "normal_dialogue": { "may_grow": false } }
        }))
        .unwrap();
        assert_eq!(
            omitted
                .resolved()
                .category_size(LetteringCategory::NormalDialogue)
                .base_size,
            None
        );
        // The default must not contribute a base to the config loader's deep merge.
        assert!(LetteringSettings::default().category_sizes.is_empty());
    }
}
