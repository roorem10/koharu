use std::collections::BTreeMap;

use koharu_scene::RunStyle;
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct EmphasisSettings {
    pub enabled_for_ai: bool,
    // Config deep-merges serialized defaults. Omitting this baseline table lets
    // a saved sparse preset retain explicitly cleared optional fields.
    #[serde(skip_serializing_if = "is_default_style")]
    pub default: RunStyle,
    pub categories: BTreeMap<String, RunStyle>,
}

fn is_default_style(style: &RunStyle) -> bool {
    style == &EmphasisSettings::default().default
}

impl Default for EmphasisSettings {
    fn default() -> Self {
        Self {
            enabled_for_ai: true,
            default: RunStyle {
                font_weight: Some(700),
                size_scale: Some(1.15),
                ..Default::default()
            },
            categories: BTreeMap::new(),
        }
    }
}

impl EmphasisSettings {
    pub fn load() -> anyhow::Result<koharu_config::Config<Self>> {
        let config = koharu_config::load::<Self>("emphasis")?;
        config.read()?.validate()?;
        Ok(config)
    }

    #[must_use]
    pub fn style(&self, category: &str) -> RunStyle {
        let mut style = self.default.clone();
        if let Some(overrides) = self.categories.get(category) {
            overlay_style(&mut style, overrides);
        }
        style
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        for style in std::iter::once(&self.default).chain(self.categories.values()) {
            validate_style(style)?;
        }
        Ok(())
    }
}

/// An unset category field inherits the emphasis default, never a second style mark.
pub fn overlay_style(style: &mut RunStyle, overrides: &RunStyle) {
    macro_rules! copy {
        ($($field:ident),+ $(,)?) => {$(
            if overrides.$field.is_some() {
                style.$field.clone_from(&overrides.$field);
            }
        )+};
    }
    copy!(
        font,
        font_weight,
        font_style,
        size_scale,
        color,
        fill_gradient,
        stroke_color,
        stroke_width,
        outer_stroke_color,
        outer_stroke_width
    );
}

pub fn validate_style(style: &RunStyle) -> anyhow::Result<()> {
    anyhow::ensure!(
        style
            .font
            .as_ref()
            .is_none_or(|font| !font.trim().is_empty()
                && font.len() <= 4096
                && !font.contains('\0')),
        "invalid emphasis font"
    );
    anyhow::ensure!(
        style
            .font_weight
            .is_none_or(|weight| (1..=1000).contains(&weight)),
        "emphasis weight must be between 1 and 1000"
    );
    anyhow::ensure!(
        style
            .size_scale
            .is_none_or(|scale| scale.is_finite() && (0.1..=10.0).contains(&scale)),
        "emphasis size must be between 10 and 1000 percent"
    );
    for width in [style.stroke_width, style.outer_stroke_width]
        .into_iter()
        .flatten()
    {
        anyhow::ensure!(
            width.is_finite() && (0.0..=100.0).contains(&width),
            "emphasis outline width must be between 0 and 100"
        );
    }
    if let Some(gradient) = &style.fill_gradient {
        anyhow::ensure!(
            gradient.angle_degrees.is_finite() && (2..=8).contains(&gradient.stops.len()),
            "invalid emphasis gradient"
        );
        let mut previous = 0.0;
        for stop in &gradient.stops {
            anyhow::ensure!(
                stop.offset.is_finite() && (previous..=1.0).contains(&stop.offset),
                "invalid emphasis gradient stop"
            );
            previous = stop.offset;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_defaults_roundtrip_and_category_fields_inherit() {
        let mut settings = EmphasisSettings::default();
        assert!(settings.enabled_for_ai);
        assert_eq!(settings.default.font_weight, Some(700));
        assert_eq!(settings.default.size_scale, Some(1.15));
        settings.categories.insert(
            "shout".into(),
            RunStyle {
                color: Some([255, 0, 0, 255]),
                size_scale: Some(1.3),
                ..Default::default()
            },
        );
        let saved = serde_json::to_string(&settings).unwrap();
        let restored: EmphasisSettings = serde_json::from_str(&saved).unwrap();
        assert_eq!(restored, settings);
        assert_eq!(restored.style("shout").font_weight, Some(700));
        assert_eq!(restored.style("shout").size_scale, Some(1.3));
        assert_eq!(restored.style("shout").color, Some([255, 0, 0, 255]));
        assert_eq!(restored.style("thoughts"), restored.default);
        restored.validate().unwrap();
    }

    #[test]
    fn invalid_style_values_are_rejected() {
        for style in [
            RunStyle {
                size_scale: Some(f32::NAN),
                ..Default::default()
            },
            RunStyle {
                size_scale: Some(0.0),
                ..Default::default()
            },
            RunStyle {
                font_weight: Some(0),
                ..Default::default()
            },
            RunStyle {
                stroke_width: Some(-1.0),
                ..Default::default()
            },
        ] {
            assert!(validate_style(&style).is_err());
        }
    }

    #[test]
    fn toml_preserves_cleared_default_fields_without_config_merge_refilling_them() {
        let defaults = toml::Value::try_from(EmphasisSettings::default()).unwrap();
        assert!(defaults.get("default").is_none());
        let settings = EmphasisSettings {
            default: RunStyle::default(),
            ..Default::default()
        };
        let saved = toml::to_string(&settings).unwrap();
        let restored: EmphasisSettings = toml::from_str(&saved).unwrap();
        assert_eq!(restored, settings);
        assert!(
            toml::Value::try_from(&settings)
                .unwrap()
                .get("default")
                .is_some()
        );
    }
}
