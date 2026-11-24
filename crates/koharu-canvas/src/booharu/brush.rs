//! The brush tools' feel on the canvas (`BrushFeel`): the brush a stroke
//! takes, Draw's at its opacity ([`felt`]).

use koharu_scene::Rgba;

use crate::{Brush, Tool, booharu::BooharuSettings};

/// `brush` as `tool`'s strokes take it under `settings`: for Draw, its
/// colour's alpha times Draw's opacity, which the preview's tint, the paint
/// on a raster and the uncovering of a mask all read, so a stroke paints as
/// one pass at that opacity, never building up within itself, as
/// Photoshop's Opacity does; for every other tool, as it is, as the Eraser
/// and Remove take no opacity.
pub(crate) fn felt(brush: Brush, tool: Tool, settings: &BooharuSettings) -> Brush {
    match tool {
        Tool::Draw => {
            let Rgba([r, g, b, a]) = brush.color;
            let opacity = settings.brush.draw_opacity as f32;
            Brush {
                color: Rgba([r, g, b, a * opacity]),
                ..brush
            }
        }
        _ => brush,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::booharu::BrushFeel;

    /// A brush 12 pixels wide in red at three quarters.
    const RED: Brush = Brush {
        diameter: 12.0,
        color: Rgba([1.0, 0.0, 0.0, 0.75]),
    };

    /// Settings whose brush feel is `feel`.
    fn feeling(feel: BrushFeel) -> BooharuSettings {
        BooharuSettings {
            brush: feel,
            ..BooharuSettings::default()
        }
    }

    #[test]
    fn draw_paints_at_its_opacity_and_no_other_tool_takes_it() {
        let half = feeling(BrushFeel {
            draw_opacity: 0.5,
            ..BrushFeel::default()
        });
        assert_eq!(
            felt(RED, Tool::Draw, &half),
            Brush {
                color: Rgba([1.0, 0.0, 0.0, 0.375]),
                ..RED
            }
        );
        for tool in [Tool::Eraser, Tool::Remove, Tool::Select, Tool::Pan] {
            assert_eq!(felt(RED, tool, &half), RED, "{tool:?}");
        }
        // At 1, as Koharu's brush is until the host pushes a feel, the
        // brush is as it was for every tool.
        let opaque = BooharuSettings::default();
        for tool in [Tool::Draw, Tool::Eraser, Tool::Remove] {
            assert_eq!(felt(RED, tool, &opaque), RED, "{tool:?}");
        }
    }

    #[test]
    fn a_stroke_takes_its_own_tools_hardness() {
        let feel = BrushFeel {
            draw_hardness: 0.25,
            eraser_hardness: 0.5,
            remove_hardness: 0.75,
            draw_opacity: 0.1,
        };
        assert_eq!(feel.hardness(Tool::Draw), 0.25);
        assert_eq!(feel.hardness(Tool::Eraser), 0.5);
        assert_eq!(feel.hardness(Tool::Remove), 0.75);
        // A tool that strokes with no brush has Koharu's hard edge, as every
        // tool has by default.
        assert_eq!(feel.hardness(Tool::Marquee), 1.0);
        let koharus = BrushFeel::default();
        for tool in [Tool::Draw, Tool::Eraser, Tool::Remove] {
            assert_eq!(koharus.hardness(tool), 1.0, "{tool:?}");
        }
        assert_eq!(koharus.draw_opacity, 1.0);
    }
}
