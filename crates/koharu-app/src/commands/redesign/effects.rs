use koharu_renderer::TextStyles;
use koharu_scene::{StrokeJoin, StrokeMethod, StrokePosition, TextWarp, Typography};

pub(super) fn automatic_effects(
    current: &Typography,
    styles: &TextStyles,
    category: Option<&str>,
    detect_colors: bool,
) -> Typography {
    let mut expected = current.clone();
    // Detection never adds a second outline or a gradient. An absent style is inheritance,
    // so these need the detector's neutral baseline rather than the edited effects.
    expected.outer_stroke_width = None;
    expected.outer_stroke_color = None;
    expected.fill_gradient = None;
    expected.stroke_method = Some(StrokeMethod::Contour);
    expected.stroke_position = None;
    expected.stroke_join = None;
    if !detect_colors {
        expected.stroke_width = None;
        expected.stroke_color = None;
    }
    styles
        .default_style
        .apply_to_detected(&mut expected, detect_colors);
    if styles.shape_styles.enabled
        && let Some(category) = category
    {
        styles
            .category_style(category)
            .apply_to_detected(&mut expected, detect_colors);
    }
    expected
}

pub(super) fn effects_differ(
    actual: &Typography,
    expected: &Typography,
    warp: Option<&TextWarp>,
) -> bool {
    warp.is_some()
        || (actual.stroke_width.unwrap_or(0.0) - expected.stroke_width.unwrap_or(0.0)).abs() >= 0.01
        || actual.stroke_color.unwrap_or([255; 4]) != expected.stroke_color.unwrap_or([255; 4])
        || actual.stroke_method.unwrap_or(StrokeMethod::Contour)
            != expected.stroke_method.unwrap_or(StrokeMethod::Contour)
        || actual.stroke_position.unwrap_or(StrokePosition::Outside)
            != expected.stroke_position.unwrap_or(StrokePosition::Outside)
        || actual.stroke_join.unwrap_or(StrokeJoin::Round)
            != expected.stroke_join.unwrap_or(StrokeJoin::Round)
        || (actual.outer_stroke_width.unwrap_or(0.0) - expected.outer_stroke_width.unwrap_or(0.0))
            .abs()
            >= 0.01
        || actual.outer_stroke_color.unwrap_or([0, 0, 0, 255])
            != expected.outer_stroke_color.unwrap_or([0, 0, 0, 255])
        || actual.fill_gradient != expected.fill_gradient
}

pub(super) fn copy_effects(target: &mut Typography, expected: &Typography) {
    target.stroke_width = expected.stroke_width;
    target.stroke_color = expected.stroke_color;
    target.stroke_method = expected.stroke_method;
    target.stroke_position = expected.stroke_position;
    target.stroke_join = expected.stroke_join;
    target.outer_stroke_width = expected.outer_stroke_width;
    target.outer_stroke_color = expected.outer_stroke_color;
    target.fill_gradient = expected.fill_gradient.clone();
}
