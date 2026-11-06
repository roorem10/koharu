use anyhow::{Context as _, Result};
use booharu_context::LetteringCategory;
use koharu_renderer::{FontPart, Renderer, TextStyles};
use koharu_scene::Typography;

/// Coverage changes the box's family, never its delivery, size, paint or slant.
pub(crate) async fn apply_font_coverage(
    renderer: &Renderer,
    styles: &TextStyles,
    category: LetteringCategory,
    typography: &mut Typography,
    text: &str,
    language: Option<&str>,
) -> Result<()> {
    if text.trim().is_empty() {
        return Ok(());
    }
    let candidates = font_candidates(styles, category, typography);
    typography.preferred_font = Some(
        renderer
            .covering_font(&candidates, text, language)
            .await?
            .context("no single category or default font covers this translation")?,
    );
    Ok(())
}

fn font_candidates(
    styles: &TextStyles,
    category: LetteringCategory,
    typography: &Typography,
) -> Vec<FontPart> {
    use LetteringCategory::*;
    let related: &[LetteringCategory] = match category {
        Shout | WeakShout | Menacing => &[Shout, WeakShout, Menacing, NormalDialogue],
        Whisper | Shy | Crying | Thoughts | Moan => {
            &[Whisper, Shy, Crying, Thoughts, Moan, NormalDialogue]
        }
        Title | Sfx => &[Title, Sfx, Shout, NormalDialogue],
        NarrationBox | PhoneText => &[NarrationBox, PhoneText, NormalDialogue],
        Floating | SmallText | NormalDialogue => &[NormalDialogue, Floating, SmallText],
    };
    let mut candidates = vec![FontPart {
        preferred_font: typography.preferred_font.clone(),
        font_weight: typography.font_weight,
        font_style: typography.font_style,
    }];
    for font in related
        .iter()
        .filter_map(|category| styles.category_style(category.as_str()).font.as_ref())
        .chain(styles.default_style.font.as_ref())
    {
        if let Some(family) = font.preferred_font.as_ref()
            && !candidates.iter().any(|candidate| {
                candidate
                    .preferred_font
                    .as_ref()
                    .is_some_and(|name| name.eq_ignore_ascii_case(family))
            })
        {
            candidates.push(FontPart {
                preferred_font: Some(family.clone()),
                font_weight: typography.font_weight,
                font_style: typography.font_style,
            });
        }
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use koharu_renderer::{CategoryStyles, ShapeStyles, TextStyle};
    use koharu_scene::{FontStyle, Origin};

    #[test]
    fn related_fonts_precede_default_without_changing_bold_or_italic() {
        let font = |family: &str| TextStyle {
            font: Some(FontPart {
                preferred_font: Some(family.into()),
                font_weight: Some(400),
                font_style: Some(FontStyle::Normal),
            }),
            ..Default::default()
        };
        let styles = TextStyles {
            default_style: font("Default"),
            shape_styles: ShapeStyles {
                spiky: font("Shout"),
                round: font("Dialogue"),
                ..Default::default()
            },
            category_styles: CategoryStyles {
                weak_shout: font("Panic"),
                menacing: font("shout"),
                ..Default::default()
            },
            ..Default::default()
        };
        let typography = Typography {
            origin: Origin::User,
            preferred_font: Some("Shout".into()),
            font_weight: Some(700),
            font_style: Some(FontStyle::Italic),
            size: None,
            auto_fit: true,
            color: None,
            stroke_color: None,
            stroke_width: None,
            alignment: None,
            writing_mode: None,
            extensions: Default::default(),
            stroke_method: None,
            stroke_position: None,
            stroke_join: None,
            outer_stroke_color: None,
            outer_stroke_width: None,
            line_spacing: None,
            letter_spacing: None,
            fill_gradient: None,
        };
        let candidates = font_candidates(&styles, LetteringCategory::Shout, &typography);
        assert_eq!(
            candidates
                .iter()
                .filter_map(|font| font.preferred_font.as_deref())
                .collect::<Vec<_>>(),
            ["Shout", "Panic", "Dialogue", "Default"]
        );
        assert!(candidates.iter().all(|font| {
            font.font_weight == Some(700) && font.font_style == Some(FontStyle::Italic)
        }));
    }
}
