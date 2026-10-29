use koharu_scene::{
    FontStyle, StrokeJoin, StrokeMethod, StrokePosition, TextAlignment, Typography, WritingMode,
};
use serde::{Deserialize, Serialize};
use specta::Type;

/// App settings only. An absent group leaves the corresponding scene fields alone.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, Type)]
#[serde(default)]
pub struct TextStyle {
    pub font: Option<FontPart>,
    pub size: Option<SizePart>,
    pub fill: Option<[u8; 4]>,
    pub outline: Option<OutlinePart>,
    pub outer_outline: Option<OuterPart>,
    pub alignment: Option<AlignmentPart>,
    pub writing_mode: Option<WritingModePart>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, Type)]
pub struct FontPart {
    pub preferred_font: Option<String>,
    pub font_weight: Option<u16>,
    pub font_style: Option<FontStyle>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, Type)]
pub struct SizePart {
    pub auto_fit: bool,
    pub size: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, Type)]
pub struct OutlinePart {
    pub stroke_width: Option<f32>,
    pub stroke_color: Option<[u8; 4]>,
    pub stroke_method: Option<StrokeMethod>,
    pub stroke_position: Option<StrokePosition>,
    pub stroke_join: Option<StrokeJoin>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, Type)]
pub struct OuterPart {
    pub outer_stroke_width: Option<f32>,
    pub outer_stroke_color: Option<[u8; 4]>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, Type)]
pub struct AlignmentPart {
    pub value: Option<TextAlignment>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, Type)]
pub struct WritingModePart {
    pub value: Option<WritingMode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StyleGroup {
    Font,
    Size,
    Fill,
    Outline,
    OuterOutline,
    Alignment,
    WritingMode,
}

impl TextStyle {
    pub fn apply_to(&self, typography: &mut Typography) {
        if let Some(part) = &self.font {
            typography.preferred_font = part.preferred_font.clone();
            typography.font_weight = part.font_weight;
            typography.font_style = part.font_style;
        }
        if let Some(part) = &self.size {
            typography.auto_fit = part.auto_fit;
            typography.size = part.size;
        }
        if let Some(fill) = self.fill {
            typography.color = Some(fill);
        }
        if let Some(part) = &self.outline {
            typography.stroke_width = part.stroke_width;
            typography.stroke_color = part.stroke_color;
            typography.stroke_method = part.stroke_method;
            typography.stroke_position = part.stroke_position;
            typography.stroke_join = part.stroke_join;
        }
        if let Some(part) = &self.outer_outline {
            typography.outer_stroke_width = part.outer_stroke_width;
            typography.outer_stroke_color = part.outer_stroke_color;
        }
        if let Some(part) = &self.alignment {
            typography.alignment = part.value;
        }
        if let Some(part) = &self.writing_mode {
            typography.writing_mode = part.value;
        }
    }

    pub fn capture(typography: &Typography, groups: &[StyleGroup]) -> Self {
        Self {
            font: groups.contains(&StyleGroup::Font).then(|| FontPart {
                preferred_font: typography.preferred_font.clone(),
                font_weight: typography.font_weight,
                font_style: typography.font_style,
            }),
            size: groups.contains(&StyleGroup::Size).then_some(SizePart {
                auto_fit: typography.auto_fit,
                size: typography.size,
            }),
            // A missing fill is the renderer's black, not an undefined captured group.
            fill: groups
                .contains(&StyleGroup::Fill)
                .then_some(typography.color.unwrap_or([0, 0, 0, 255])),
            outline: groups
                .contains(&StyleGroup::Outline)
                .then_some(OutlinePart {
                    stroke_width: typography.stroke_width,
                    stroke_color: typography.stroke_color,
                    stroke_method: typography.stroke_method,
                    stroke_position: typography.stroke_position,
                    stroke_join: typography.stroke_join,
                }),
            outer_outline: groups
                .contains(&StyleGroup::OuterOutline)
                .then_some(OuterPart {
                    outer_stroke_width: typography.outer_stroke_width,
                    outer_stroke_color: typography.outer_stroke_color,
                }),
            alignment: groups
                .contains(&StyleGroup::Alignment)
                .then_some(AlignmentPart {
                    value: typography.alignment,
                }),
            writing_mode: groups
                .contains(&StyleGroup::WritingMode)
                .then_some(WritingModePart {
                    value: typography.writing_mode,
                }),
        }
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if let Some(font) = &self.font {
            anyhow::ensure!(
                !font
                    .preferred_font
                    .as_ref()
                    .is_some_and(|name| name.len() > 4096 || name.contains('\0')),
                "invalid style font name"
            );
            anyhow::ensure!(
                !font
                    .font_weight
                    .is_some_and(|weight| !(1..=1000).contains(&weight)),
                "invalid style font weight"
            );
        }
        if let Some(size) = &self.size {
            anyhow::ensure!(
                !size
                    .size
                    .is_some_and(|size| !size.is_finite() || size <= 0.0)
                    && (size.auto_fit || size.size.is_some()),
                "invalid style font size"
            );
        }
        for width in [
            self.outline.as_ref().and_then(|part| part.stroke_width),
            self.outer_outline
                .as_ref()
                .and_then(|part| part.outer_stroke_width),
        ]
        .into_iter()
        .flatten()
        {
            anyhow::ensure!(
                width.is_finite() && width >= 0.0,
                "invalid style outline width"
            );
        }
        Ok(())
    }

    /// Detection owns inferred colors and writing direction, even when a style defines them.
    pub fn apply_to_detected(&self, typography: &mut Typography, detect_colors: bool) {
        let mut style = self.clone();
        style.writing_mode = None;
        if detect_colors {
            style.fill = None;
            style.outline = None;
        }
        style.apply_to(typography);
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, Type)]
#[serde(default)]
pub struct TextStyles {
    pub default_style: TextStyle,
    pub shape_styles: ShapeStyles,
    /// Distinguish a deliberate Clear from a pre-migration missing fill.
    pub legacy_color_migrated: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, Type)]
#[serde(default)]
pub struct ShapeStyles {
    pub enabled: bool,
    pub rectangle: TextStyle,
    pub round: TextStyle,
    pub spiky: TextStyle,
    pub free: TextStyle,
}

impl TextStyles {
    pub fn load(
        legacy_color: impl FnOnce() -> anyhow::Result<Option<[u8; 4]>>,
    ) -> anyhow::Result<koharu_config::Config<Self>> {
        let config = koharu_config::load::<Self>("text-styles")?;
        if !config.read()?.legacy_color_migrated {
            // Read the retired setting before locking this section, and only
            // for an unmigrated profile. Other sections can have live readers.
            let color = legacy_color()?;
            let mut current = config.write()?;
            if !current.legacy_color_migrated {
                current.migrate_color(color);
                current.save()?;
            }
        }
        config.read()?.validate()?;
        Ok(config)
    }

    pub fn migrate_color(&mut self, legacy_color: Option<[u8; 4]>) {
        if self.legacy_color_migrated {
            return;
        }
        if self.default_style.fill.is_none() {
            self.default_style.fill = legacy_color;
        }
        self.legacy_color_migrated = true;
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        self.default_style.validate()?;
        for style in [
            &self.shape_styles.rectangle,
            &self.shape_styles.round,
            &self.shape_styles.spiky,
            &self.shape_styles.free,
        ] {
            style.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, Type)]
#[serde(default)]
pub struct TypesettingConfig {
    pub font_families: Vec<String>,
}

impl Default for TypesettingConfig {
    fn default() -> Self {
        Self {
            font_families: vec!["CCWildWords".to_owned(), "Adobe 黑体 Std".to_owned()],
        }
    }
}

impl TypesettingConfig {
    pub fn load() -> anyhow::Result<koharu_config::Config<Self>> {
        koharu_config::load("typesetting")
    }
}

#[cfg(test)]
mod text_style_tests {
    use super::*;

    fn typography() -> Typography {
        Typography {
            origin: koharu_scene::Origin::User,
            preferred_font: Some("Dialogue".into()),
            font_weight: Some(500),
            font_style: Some(FontStyle::Italic),
            size: Some(32.0),
            auto_fit: false,
            color: Some([1, 2, 3, 255]),
            stroke_color: Some([4, 5, 6, 255]),
            stroke_width: Some(2.0),
            stroke_method: Some(StrokeMethod::Contour),
            stroke_position: Some(StrokePosition::Outside),
            stroke_join: Some(StrokeJoin::Round),
            outer_stroke_color: None,
            outer_stroke_width: None,
            alignment: Some(TextAlignment::Center),
            writing_mode: Some(WritingMode::Horizontal),
            extensions: [("dev.booharu.test".into(), "kept".into())].into(),
        }
    }

    #[test]
    fn empty_style_keeps_every_field() {
        let original = typography();
        let mut actual = original.clone();
        TextStyle::default().apply_to(&mut actual);
        assert_eq!(actual, original);
    }

    #[test]
    fn partial_style_preserves_other_groups_origin_and_extensions() {
        let mut original = typography();
        original.origin = koharu_scene::Origin::Generated(koharu_scene::Generation::new(
            koharu_scene::ProducerId::new("dev.booharu.test").unwrap(),
        ));
        let mut actual = original.clone();
        let style = TextStyle {
            fill: Some([255, 255, 255, 255]),
            outline: Some(OutlinePart {
                stroke_color: Some([0, 0, 0, 255]),
                stroke_width: Some(4.0),
                stroke_method: None,
                stroke_position: None,
                stroke_join: None,
            }),
            ..TextStyle::default()
        };
        style.apply_to(&mut actual);
        assert_eq!(
            TextStyle::capture(
                &actual,
                &[
                    StyleGroup::Font,
                    StyleGroup::Size,
                    StyleGroup::Alignment,
                    StyleGroup::WritingMode,
                    StyleGroup::OuterOutline
                ]
            ),
            TextStyle::capture(
                &original,
                &[
                    StyleGroup::Font,
                    StyleGroup::Size,
                    StyleGroup::Alignment,
                    StyleGroup::WritingMode,
                    StyleGroup::OuterOutline
                ]
            )
        );
        assert_eq!(actual.color, style.fill);
        assert_eq!(actual.stroke_width, Some(4.0));
        assert_eq!(actual.origin, original.origin);
        assert_eq!(actual.extensions, original.extensions);
    }

    #[test]
    fn defined_auto_survives_toml_roundtrip() {
        let style = TextStyle {
            alignment: Some(AlignmentPart { value: None }),
            writing_mode: Some(WritingModePart { value: None }),
            ..TextStyle::default()
        };
        let restored: TextStyle = toml::from_str(&toml::to_string(&style).unwrap()).unwrap();
        assert_eq!(restored, style);
        let mut actual = typography();
        restored.apply_to(&mut actual);
        assert_eq!(actual.alignment, None);
        assert_eq!(actual.writing_mode, None);
    }

    #[test]
    fn style_validation_rejects_invalid_values() {
        for size in [None, Some(-1.0), Some(f32::NAN)] {
            assert!(
                TextStyle {
                    size: Some(SizePart {
                        auto_fit: false,
                        size
                    }),
                    ..TextStyle::default()
                }
                .validate()
                .is_err()
            );
        }
        for width in [-1.0, f32::INFINITY, f32::NAN] {
            assert!(
                TextStyle {
                    outer_outline: Some(OuterPart {
                        outer_stroke_width: Some(width),
                        outer_stroke_color: None
                    }),
                    ..TextStyle::default()
                }
                .validate()
                .is_err()
            );
        }
        for (name, weight) in [
            ("bad\0name".into(), 400),
            ("x".repeat(4097), 400),
            ("font".into(), 1001),
        ] {
            assert!(
                TextStyle {
                    font: Some(FontPart {
                        preferred_font: Some(name),
                        font_weight: Some(weight),
                        font_style: None
                    }),
                    ..TextStyle::default()
                }
                .validate()
                .is_err()
            );
        }
    }

    #[test]
    fn empty_settings_and_captured_groups_roundtrip() {
        let empty: TextStyles = toml::from_str("").unwrap();
        assert_eq!(empty, TextStyles::default());
        let mut styles = empty;
        styles.default_style = TextStyle::capture(
            &typography(),
            &[StyleGroup::Font, StyleGroup::Outline, StyleGroup::Alignment],
        );
        let restored: TextStyles = toml::from_str(&toml::to_string(&styles).unwrap()).unwrap();
        assert_eq!(styles, restored);
        assert!(restored.default_style.size.is_none());
        assert!(restored.default_style.fill.is_none());
    }

    #[test]
    fn legacy_color_migration_is_one_time_and_preserves_an_existing_fill() {
        let legacy = Some([20, 115, 235, 180]);
        let mut styles = TextStyles::default();
        styles.migrate_color(legacy);
        assert_eq!(styles.default_style.fill, legacy);
        let mut restored: TextStyles = toml::from_str(&toml::to_string(&styles).unwrap()).unwrap();
        restored.default_style.fill = None;
        restored.migrate_color(legacy);
        assert_eq!(restored.default_style.fill, None);
        let mut existing = TextStyles::default();
        existing.default_style.fill = Some([0, 0, 0, 255]);
        existing.migrate_color(legacy);
        assert_eq!(existing.default_style.fill, Some([0, 0, 0, 255]));
    }

    #[test]
    fn missing_settings_file_and_persisted_styles_roundtrip() {
        // Executable-relative preview settings let subprocesses test real files
        // without ever opening a developer's normal profile.
        if !koharu_config::preview::enabled() {
            return;
        }
        const CHILD: &str = "BOOHARU_TEXT_STYLES_FILE_TEST";
        let executable = std::env::current_exe().unwrap();
        if let Ok(phase) = std::env::var(CHILD) {
            let path = koharu_config::path().unwrap();
            assert_eq!(
                path,
                executable
                    .parent()
                    .unwrap()
                    .join("PreviewData/settings/config.toml")
            );
            let expected = TextStyles {
                default_style: TextStyle {
                    alignment: Some(AlignmentPart { value: None }),
                    fill: Some([9, 8, 7, 255]),
                    ..TextStyle::capture(&typography(), &[StyleGroup::Font, StyleGroup::Outline])
                },
                shape_styles: ShapeStyles {
                    enabled: true,
                    round: TextStyle::capture(&typography(), &[StyleGroup::Outline]),
                    spiky: TextStyle::capture(&typography(), &[StyleGroup::Font]),
                    ..Default::default()
                },
                legacy_color_migrated: true,
            };
            let legacy = [20, 115, 235, 180];
            match phase.as_str() {
                "empty" => {
                    assert!(!path.exists());
                    let config = TextStyles::load(|| Ok(None)).unwrap();
                    assert_eq!(config.read().unwrap().default_style, TextStyle::default());
                    assert_eq!(config.read().unwrap().shape_styles, ShapeStyles::default());
                    *config.write().unwrap() = expected;
                    config.save().unwrap();
                }
                "reload" => {
                    let config =
                        TextStyles::load(|| panic!("legacy color read after migration")).unwrap();
                    assert_eq!(*config.read().unwrap(), expected);
                }
                "legacy" => {
                    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                    std::fs::write(&path, "[pipeline]\ndetect_text_color = false\ndefault_text_color = [20, 115, 235, 180]\n").unwrap();
                    let document: toml::Table =
                        toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
                    let color = document["pipeline"]["default_text_color"]
                        .clone()
                        .try_into::<[u8; 4]>()
                        .unwrap();
                    let config = TextStyles::load(|| Ok(Some(color))).unwrap();
                    assert_eq!(config.read().unwrap().default_style.fill, Some(legacy));
                    let document: toml::Table =
                        toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
                    assert_eq!(
                        document["pipeline"]["default_text_color"]
                            .clone()
                            .try_into::<[u8; 4]>()
                            .unwrap(),
                        legacy
                    );
                    config.write().unwrap().default_style.fill = None;
                    config.save().unwrap();
                }
                "legacy-reload" => {
                    let config =
                        TextStyles::load(|| panic!("legacy color read after clearing")).unwrap();
                    assert_eq!(config.read().unwrap().default_style.fill, None);
                    assert!(config.read().unwrap().legacy_color_migrated);
                }
                _ => panic!("unknown test phase"),
            }
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        for (name, phases) in [
            ("empty", ["empty", "reload"]),
            ("legacy", ["legacy", "legacy-reload"]),
        ] {
            let folder = directory.path().join(name);
            std::fs::create_dir_all(&folder).unwrap();
            let copy = folder.join(executable.file_name().unwrap());
            std::fs::copy(&executable, &copy).unwrap();
            for phase in phases {
                let output = std::process::Command::new(&copy)
                    .args(["--exact", "config::text_style_tests::missing_settings_file_and_persisted_styles_roundtrip"])
                    .env(CHILD, phase).output().unwrap();
                assert!(
                    output.status.success(),
                    "{phase}: {}\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }

    #[test]
    fn shape_styles_roundtrip_validate_and_ignore_unknown_settings() {
        let styles = TextStyles {
            shape_styles: ShapeStyles {
                enabled: true,
                round: TextStyle::capture(&typography(), &[StyleGroup::Font]),
                free: TextStyle::capture(&typography(), &[StyleGroup::Fill]),
                ..Default::default()
            },
            ..Default::default()
        };
        styles.validate().unwrap();
        let restored: TextStyles = toml::from_str(&toml::to_string(&styles).unwrap()).unwrap();
        assert_eq!(restored, styles);
        assert!(restored.shape_styles.round.size.is_none());
        // Discarded test-build fields are unknown settings, not a migration source.
        let unknown: TextStyles = toml::from_str("presets = [{ id = 'old', name = 'Old' }]\n[shape_rules]\nenabled = true\nround = 'old'\n").unwrap();
        assert_eq!(unknown, TextStyles::default());
        for shape in 0..4 {
            let mut invalid = styles.clone();
            let target = match shape {
                0 => &mut invalid.shape_styles.rectangle,
                1 => &mut invalid.shape_styles.round,
                2 => &mut invalid.shape_styles.spiky,
                _ => &mut invalid.shape_styles.free,
            };
            target.font = Some(FontPart {
                preferred_font: None,
                font_weight: Some(0),
                font_style: None,
            });
            assert!(invalid.validate().is_err());
        }
    }
}
