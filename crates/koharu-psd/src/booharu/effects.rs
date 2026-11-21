//! A type layer's effects as Photoshop keeps them. A border alone is the
//! stroke effect `blocks` writes, unchanged. A gradient fill adds a Gradient
//! Overlay (`GrFl`) before the border in the same `lfx2`; a second border
//! needs the effects that hold more than one of a kind (`lmfx`), which list
//! the gradient in `gradientFillMulti` and both borders in `frameFXMulti`,
//! the first over the second, which reaches out past it. The overlay keeps
//! the gradient's stops and is aligned with the layer, at the angle and
//! scale that run it across the layer's box on the page as the renderer
//! spreads it over its ink (`fit`).

use koharu_renderer::ResolvedText;
use koharu_scene::{Color, Gradient, GradientKind, Text};
use kurbo::{Affine, Rect, Vec2};
use photocraft_psd::{
    TaggedBlock,
    descriptor::{Descriptor, UnicodeString, Value},
};

use super::word_effects;
use crate::blocks::{block, border, enumerated, opacity, rgb, stroke_effect, unit, versioned};

/// The effects of `layer`'s type layer, laid out as `text`, which its runs
/// all share, as one layer's effects are the whole layer's: the stroke
/// effect for a border alone, and Booharu's for a gradient fill or a second
/// border. `None` when the runs differ in them, or have no border and no
/// gradient spread over some ink.
pub(crate) fn effects(layer: &Text, text: &ResolvedText) -> Option<TaggedBlock> {
    if word_effects(text) {
        return None;
    }
    let run = text.runs.first()?;
    let transform = Affine::from(layer.transform);
    // An effect's size is in page pixels, where the renderer's borders grow
    // with the layer's transform.
    let scale = transform.determinant().abs().sqrt();
    let fill = run
        .fill_gradient
        .as_ref()
        .filter(|gradient| !gradient.stops.is_empty())
        .zip(run.fill_extent)
        .map(|(gradient, extent)| Value::Descriptor(gradient_fill(gradient, extent, transform)));
    // A second border is only ever outside a first.
    let Some((color, width)) = run.stroke else {
        return fill.map(|fill| layer_effects(*b"lfx2", vec![("GrFl", fill)]));
    };
    let width = f64::from(width) * scale;
    Some(match (run.outer_stroke, fill) {
        (None, None) => stroke_effect(color, width),
        (None, Some(fill)) => layer_effects(
            *b"lfx2",
            vec![
                ("GrFl", fill),
                ("FrFX", Value::Descriptor(border(color, width))),
            ],
        ),
        (Some((outer, reach)), fill) => {
            let count = 2 + i32::from(fill.is_some());
            let mut items: Vec<_> = fill
                .map(|fill| ("gradientFillMulti", Value::List(vec![fill])))
                .into_iter()
                .collect();
            let borders = vec![
                Value::Descriptor(border(color, width)),
                Value::Descriptor(border(outer, width + f64::from(reach) * scale)),
            ];
            items.push(("frameFXMulti", Value::List(borders)));
            items.push(("numModifyingFX", Value::Integer(count)));
            layer_effects(*b"lmfx", items)
        }
    })
}

/// A layer effects block, `lfx2` or `lmfx`, holding `items` after the scale
/// and the switch that Photoshop writes first.
fn layer_effects(key: [u8; 4], items: Vec<(&str, Value)>) -> TaggedBlock {
    let effects = Descriptor::new("null")
        .with("Scl ", unit(b"#Prc", 100.0))
        .with("masterFXSwitch", Value::Boolean(true));
    let effects = items
        .into_iter()
        .fold(effects, |effects, (key, value)| effects.with(key, value));
    let mut data = 0u32.to_be_bytes().to_vec();
    data.extend(versioned(effects));
    block(key, data)
}

/// A Gradient Overlay (`GrFl`) of `gradient` as the renderer spreads it over
/// `extent`, a box in the text's own coordinates that `transform` sets on
/// the page: its stops' colours and opacities at their offsets, from 0 to
/// 4096, blended straight from one to the next, as the renderer blends
/// them; aligned with the layer at the angle and scale `fit` gives.
fn gradient_fill(gradient: &Gradient, extent: Rect, transform: Affine) -> Descriptor {
    let (angle, scale) = fit(gradient, extent, transform);
    let at = |offset: f64| Value::Integer((offset.clamp(0.0, 1.0) * 4096.0).round() as i32);
    let colors = gradient.stops.iter().map(|stop| {
        Value::Descriptor(
            Descriptor::new("Clrt")
                .with("Clr ", rgb(Color::from(stop.color)))
                .with("Type", enumerated("Clry", "UsrS"))
                .with("Lctn", at(stop.offset))
                .with("Mdpn", Value::Integer(50)),
        )
    });
    let opacities = gradient.stops.iter().map(|stop| {
        Value::Descriptor(
            Descriptor::new("TrnS")
                .with("Opct", opacity(Color::from(stop.color)))
                .with("Lctn", at(stop.offset))
                .with("Mdpn", Value::Integer(50)),
        )
    });
    let stops = Descriptor {
        name: UnicodeString::new_nul("Gradient"),
        ..Descriptor::new("Grdn")
    }
    .with("Nm  ", Value::Text(UnicodeString::new_nul("Custom")))
    .with("GrdF", enumerated("GrdF", "CstS"))
    .with("Intr", Value::Double(0.0))
    .with("Clrs", Value::List(colors.collect()))
    .with("Trns", Value::List(opacities.collect()));
    let kind = match gradient.kind {
        GradientKind::Linear => "Lnr ",
        GradientKind::Radial => "Rdl ",
    };
    let offset = Descriptor::new("Pnt ")
        .with("Hrzn", unit(b"#Prc", 0.0))
        .with("Vrtc", unit(b"#Prc", 0.0));
    Descriptor::new("GrFl")
        .with("enab", Value::Boolean(true))
        .with("present", Value::Boolean(true))
        .with("showInDialog", Value::Boolean(true))
        .with("Md  ", enumerated("BlnM", "Nrml"))
        .with("Opct", unit(b"#Prc", 100.0))
        .with("Grad", Value::Descriptor(stops))
        .with("Angl", unit(b"#Ang", angle))
        .with("Type", enumerated("GrdT", kind))
        .with("Rvrs", Value::Boolean(false))
        .with("Dthr", Value::Boolean(false))
        .with("Algn", Value::Boolean(true))
        .with("Scl ", unit(b"#Prc", scale))
        .with("Ofst", Value::Descriptor(offset))
}

/// The angle, in degrees counterclockwise from the right in (-180, 180],
/// and the scale, in percent from 10 to 150, of a Gradient Overlay aligned
/// with its layer that runs as the renderer spreads `gradient` over
/// `extent`, a box in the text's own coordinates that `transform` sets on
/// the page. At scale 100 Photoshop runs a linear overlay across the
/// chord of the layer's box through its centre at its angle, and a radial
/// one out to half that chord; the box is `extent`'s on the page.
///
/// A linear gradient's lines of one colour cross the text at right angles
/// to its angle and stay parallel on the page, so it runs across them
/// along the normal the transform's inverse transpose gives, its length
/// the extent's along its angle, divided by how much that normal stretches.
/// A radial one reaches the extent's corners, as long on the page as the
/// transform scales areas, measured along the box's longer side. A
/// transform that flattens the text, which then shows nothing, keeps the
/// gradient's own angle, brought into that range, at scale 100.
pub(crate) fn fit(gradient: &Gradient, extent: Rect, transform: Affine) -> (f64, f64) {
    let (width, height) = (extent.width(), extent.height());
    let frame = transform.transform_rect_bbox(extent);
    let [a, b, c, d, ..] = transform.as_coeffs();
    let determinant = a * d - b * c;
    if !determinant.is_finite() || determinant.abs() < 1e-12 {
        return (normalized(gradient.angle), 100.0);
    }
    let (angle, length, chord) = match gradient.kind {
        GradientKind::Linear => {
            let turn = gradient.angle.to_radians();
            let along = Vec2::new(turn.cos(), -turn.sin());
            let reach = width * along.x.abs() + height * along.y.abs();
            let normal =
                Vec2::new(d * along.x - b * along.y, a * along.y - c * along.x) / determinant;
            let angle = normalized((-normal.y).atan2(normal.x).to_degrees());
            let turn = angle.to_radians();
            let chord = [(frame.width(), turn.cos()), (frame.height(), turn.sin())]
                .into_iter()
                .filter(|(_, part)| part.abs() > 1e-9)
                .map(|(side, part)| side / part.abs())
                .fold(f64::INFINITY, f64::min);
            (angle, reach / normal.hypot(), chord)
        }
        GradientKind::Radial => {
            let diameter = width.hypot(height) * determinant.abs().sqrt();
            if frame.width() >= frame.height() {
                (0.0, diameter, frame.width())
            } else {
                (90.0, diameter, frame.height())
            }
        }
    };
    let scale = 100.0 * length / chord;
    (
        angle,
        if scale.is_nan() {
            100.0
        } else {
            scale.clamp(10.0, 150.0)
        },
    )
}

/// The direction `angle` gives, in degrees, as an angle in (-180, 180],
/// where Photoshop's angles lie.
fn normalized(angle: f64) -> f64 {
    // From 0 to 360, which round-off reaches for an angle just under 0.
    let angle = angle.rem_euclid(360.0);
    if angle > 180.0 { angle - 360.0 } else { angle }
}

#[cfg(test)]
mod tests {
    use koharu_scene::{GradientStop, Rgba, StrokeMethod, Transform};
    use photocraft_psd::VersionedDescriptor;

    use super::*;
    use crate::booharu::{plain_run, resolved};

    const WHITE: Color = Color::new([1.0, 1.0, 1.0, 1.0]);
    const BLACK: Color = Color::new([0.0, 0.0, 0.0, 1.0]);
    /// A 200 by 120 box, which the tests' gradients spread over.
    const INK: Rect = Rect::new(10.0, 20.0, 210.0, 140.0);

    /// A text set on the page by `transform`.
    fn layer(transform: Affine) -> Text {
        Text {
            transform: Transform::from(transform),
            ..Text::default()
        }
    }

    /// Two runs of words, each set up by `set`.
    fn words(set: impl Fn(&mut koharu_renderer::TextRun)) -> ResolvedText {
        resolved(
            [0..5, 5..9]
                .into_iter()
                .map(|range| {
                    let mut run = plain_run(range);
                    set(&mut run);
                    run
                })
                .collect(),
        )
    }

    /// A gradient of `kind` at `angle`, from opaque red through opaque
    /// green halfway to blue that's clear.
    fn gradient(kind: GradientKind, angle: f64) -> Gradient {
        let stops = [
            (0.0, [1.0, 0.0, 0.0, 1.0]),
            (0.5, [0.0, 1.0, 0.0, 1.0]),
            (1.0, [0.0, 0.0, 1.0, 0.0]),
        ];
        Gradient {
            kind,
            angle,
            stops: stops
                .into_iter()
                .map(|(offset, color)| GradientStop {
                    offset,
                    color: Rgba(color),
                })
                .collect(),
        }
    }

    /// The effects a layer effects block holds, after its version.
    fn read(block: &TaggedBlock) -> Descriptor {
        let (effects, _) =
            VersionedDescriptor::parse_prefix(&block.data[4..]).expect("a layer effects block");
        assert_eq!(block.data[..4], [0; 4]);
        effects.descriptor
    }

    fn keys(descriptor: &Descriptor) -> Vec<String> {
        descriptor
            .items
            .iter()
            .map(|(key, _)| String::from_utf8_lossy(key.as_bytes()).into_owned())
            .collect()
    }

    fn get<'a>(descriptor: &'a Descriptor, key: &str) -> &'a Value {
        descriptor
            .get(key)
            .unwrap_or_else(|| panic!("no {key} in {descriptor:?}"))
    }

    fn object<'a>(descriptor: &'a Descriptor, key: &str) -> &'a Descriptor {
        match get(descriptor, key) {
            Value::Descriptor(object) => object,
            other => panic!("{key} is {other:?}"),
        }
    }

    fn list<'a>(descriptor: &'a Descriptor, key: &str) -> &'a [Value] {
        match get(descriptor, key) {
            Value::List(list) => list,
            other => panic!("{key} is {other:?}"),
        }
    }

    fn number(descriptor: &Descriptor, key: &str) -> f64 {
        match get(descriptor, key) {
            Value::UnitFloat { value, .. } | Value::Double(value) => *value,
            Value::Integer(value) => f64::from(*value),
            other => panic!("{key} is {other:?}"),
        }
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn a_border_alone_is_the_stroke_effect_byte_for_byte() {
        // Four times as wide and as tall as it's set, so twice the size.
        let text = words(|run| run.stroke = Some((WHITE, 3.0)));
        let layer = layer(Affine::scale_non_uniform(4.0, 1.0));
        assert_eq!(effects(&layer, &text), Some(stroke_effect(WHITE, 6.0)));
    }

    // Koharu's border, the default, and Contour are one Outside stroke.
    #[test]
    fn every_border_method_is_the_outside_stroke() {
        let text = words(|run| run.stroke = Some((WHITE, 3.0)));
        for method in [
            None,
            Some(StrokeMethod::Expansion),
            Some(StrokeMethod::Contour),
        ] {
            let mut layer = layer(Affine::IDENTITY);
            layer.style.stroke_method = method;
            assert_eq!(
                effects(&layer, &text),
                Some(stroke_effect(WHITE, 3.0)),
                "{method:?}"
            );
        }
    }

    #[test]
    fn a_second_border_lists_both_borders_in_lmfx() {
        let text = words(|run| {
            run.stroke = Some((WHITE, 3.0));
            run.outer_stroke = Some((BLACK, 2.0));
        });
        let block = effects(&layer(Affine::scale(2.0)), &text).expect("effects");
        assert_eq!(&block.key, b"lmfx");
        let effects = read(&block);
        assert_eq!(
            keys(&effects),
            ["Scl ", "masterFXSwitch", "frameFXMulti", "numModifyingFX"]
        );
        assert_eq!(number(&effects, "Scl "), 100.0);
        assert_eq!(get(&effects, "masterFXSwitch"), &Value::Boolean(true));
        // The first border over the second, which reaches past it, each as
        // the stroke effect writes its border.
        assert_eq!(
            list(&effects, "frameFXMulti"),
            [
                Value::Descriptor(border(WHITE, 6.0)),
                Value::Descriptor(border(BLACK, 10.0)),
            ]
        );
        assert_eq!(get(&effects, "numModifyingFX"), &Value::Integer(2));
    }

    #[test]
    fn a_gradient_is_an_overlay_aligned_with_the_layer() {
        let text = words(|run| {
            run.fill_gradient = Some(gradient(GradientKind::Linear, 30.0));
            run.fill_extent = Some(INK);
        });
        let block = effects(&layer(Affine::IDENTITY), &text).expect("effects");
        assert_eq!(&block.key, b"lfx2");
        let effects = read(&block);
        assert_eq!(keys(&effects), ["Scl ", "masterFXSwitch", "GrFl"]);
        let fill = object(&effects, "GrFl");
        assert_eq!(
            keys(fill),
            [
                "enab",
                "present",
                "showInDialog",
                "Md  ",
                "Opct",
                "Grad",
                "Angl",
                "Type",
                "Rvrs",
                "Dthr",
                "Algn",
                "Scl ",
                "Ofst"
            ]
        );
        assert_eq!(get(fill, "Md  "), &enumerated("BlnM", "Nrml"));
        assert_eq!(number(fill, "Opct"), 100.0);
        assert_eq!(get(fill, "Type"), &enumerated("GrdT", "Lnr "));
        assert_eq!(get(fill, "Algn"), &Value::Boolean(true));
        assert_eq!(get(fill, "Rvrs"), &Value::Boolean(false));
        assert!(near(number(fill, "Angl"), 30.0));
        assert!(near(number(fill, "Scl "), 100.98), "{fill:?}");
        let offset = object(fill, "Ofst");
        assert_eq!((number(offset, "Hrzn"), number(offset, "Vrtc")), (0.0, 0.0));
        // Its stops, in order, straight between them.
        let stops = object(fill, "Grad");
        assert_eq!(stops.name, UnicodeString::new_nul("Gradient"));
        assert!(stops.class_id.is("Grdn"));
        assert_eq!(keys(stops), ["Nm  ", "GrdF", "Intr", "Clrs", "Trns"]);
        assert_eq!(number(stops, "Intr"), 0.0);
        let colors: Vec<(Value, f64)> = list(stops, "Clrs")
            .iter()
            .map(|stop| {
                let Value::Descriptor(stop) = stop else {
                    panic!("a colour stop: {stop:?}");
                };
                assert_eq!(get(stop, "Type"), &enumerated("Clry", "UsrS"));
                assert_eq!(number(stop, "Mdpn"), 50.0);
                (get(stop, "Clr ").clone(), number(stop, "Lctn"))
            })
            .collect();
        let red = rgb(Color::new([1.0, 0.0, 0.0, 1.0]));
        let green = rgb(Color::new([0.0, 1.0, 0.0, 1.0]));
        let blue = rgb(Color::new([0.0, 0.0, 1.0, 1.0]));
        assert_eq!(colors, [(red, 0.0), (green, 2048.0), (blue, 4096.0)]);
        let opacities: Vec<(f64, f64)> = list(stops, "Trns")
            .iter()
            .map(|stop| {
                let Value::Descriptor(stop) = stop else {
                    panic!("an opacity stop: {stop:?}");
                };
                assert_eq!(number(stop, "Mdpn"), 50.0);
                (number(stop, "Opct"), number(stop, "Lctn"))
            })
            .collect();
        assert_eq!(opacities, [(100.0, 0.0), (100.0, 2048.0), (0.0, 4096.0)]);
    }

    #[test]
    fn a_border_and_a_gradient_share_lfx2_the_gradient_first() {
        let text = words(|run| {
            run.stroke = Some((WHITE, 3.0));
            run.fill_gradient = Some(gradient(GradientKind::Radial, 0.0));
            run.fill_extent = Some(INK);
        });
        let block = effects(&layer(Affine::scale(2.0)), &text).expect("effects");
        assert_eq!(&block.key, b"lfx2");
        let effects = read(&block);
        assert_eq!(keys(&effects), ["Scl ", "masterFXSwitch", "GrFl", "FrFX"]);
        assert_eq!(
            get(&effects, "FrFX"),
            &Value::Descriptor(border(WHITE, 6.0))
        );
        let fill = object(&effects, "GrFl");
        assert_eq!(get(fill, "Type"), &enumerated("GrdT", "Rdl "));
        assert!(near(number(fill, "Scl "), 116.62), "{fill:?}");
    }

    #[test]
    fn a_second_border_and_a_gradient_list_the_gradient_first_in_lmfx() {
        let text = words(|run| {
            run.stroke = Some((WHITE, 3.0));
            run.outer_stroke = Some((BLACK, 2.0));
            run.fill_gradient = Some(gradient(GradientKind::Linear, 0.0));
            run.fill_extent = Some(INK);
        });
        let block = effects(&layer(Affine::IDENTITY), &text).expect("effects");
        assert_eq!(&block.key, b"lmfx");
        let effects = read(&block);
        assert_eq!(
            keys(&effects),
            [
                "Scl ",
                "masterFXSwitch",
                "gradientFillMulti",
                "frameFXMulti",
                "numModifyingFX"
            ]
        );
        let fills = list(&effects, "gradientFillMulti");
        assert!(matches!(fills, [Value::Descriptor(fill)] if fill.class_id.is("GrFl")));
        assert_eq!(list(&effects, "frameFXMulti").len(), 2);
        assert_eq!(get(&effects, "numModifyingFX"), &Value::Integer(3));
    }

    #[test]
    fn runs_that_differ_or_have_nothing_to_draw_have_no_effects() {
        let identity = layer(Affine::IDENTITY);
        // Borders that differ by run.
        let mut text = words(|run| run.stroke = Some((WHITE, 3.0)));
        text.runs[1].stroke = Some((BLACK, 3.0));
        assert_eq!(effects(&identity, &text), None);
        // No border, and a gradient over no ink.
        assert_eq!(effects(&identity, &words(|_| {})), None);
        let unspread = words(|run| run.fill_gradient = Some(gradient(GradientKind::Linear, 0.0)));
        assert_eq!(effects(&identity, &unspread), None);
        assert_eq!(effects(&identity, &resolved(Vec::new())), None);
    }

    /// The angle and scale of a gradient of `kind` at `angle` over a box of
    /// `width` and `height`, set on the page by `transform`.
    fn fitted(
        kind: GradientKind,
        angle: f64,
        (width, height): (f64, f64),
        transform: Affine,
    ) -> (f64, f64) {
        fit(
            &gradient(kind, angle),
            Rect::new(5.0, 5.0, 5.0 + width, 5.0 + height),
            transform,
        )
    }

    #[test]
    fn an_overlay_runs_across_the_layer_s_box_as_the_gradient_runs_across_its_ink() {
        let linear = |angle, size, transform| fitted(GradientKind::Linear, angle, size, transform);
        let close = |(angle, scale): (f64, f64), (want_angle, want_scale): (f64, f64)| {
            near(angle, want_angle) && near(scale, want_scale)
        };
        let identity = Affine::IDENTITY;
        // Edge to edge along its angle, against the chord through the
        // centre: 233.21 against 230.94 at 30 degrees, and the same at 0
        // and 90.
        let tilted = linear(30.0, (200.0, 120.0), identity);
        assert!(close(tilted, (30.0, 100.98)), "{tilted:?}");
        assert!(close(linear(0.0, (200.0, 120.0), identity), (0.0, 100.0)));
        assert!(close(linear(90.0, (200.0, 120.0), identity), (90.0, 100.0)));
        // 309.81 against a chord of 200, 154.9, which Photoshop caps.
        assert!(close(linear(30.0, (300.0, 100.0), identity), (30.0, 150.0)));
        // A radial one reaches the corners, 116.62 from the centre, along
        // the longer side.
        let radial = |size| fitted(GradientKind::Radial, 45.0, size, identity);
        assert!(close(radial((200.0, 120.0)), (0.0, 116.62)));
        assert!(close(radial((120.0, 200.0)), (90.0, 116.62)));
        // Turned with the text, slanted by a shear, mirrored by a flip, and
        // unchanged by an even scale.
        let (cos, sin) = (30.0_f64.to_radians().cos(), 30.0_f64.to_radians().sin());
        let turned = linear(
            0.0,
            (200.0, 120.0),
            Affine::new([cos, -sin, sin, cos, 0.0, 0.0]),
        );
        assert!(near(turned.0, 30.0), "{turned:?}");
        let sheared = linear(
            0.0,
            (200.0, 120.0),
            Affine::new([1.0, 0.0, 0.5, 1.0, 0.0, 0.0]),
        );
        assert!(near(sheared.0, 0.5_f64.atan().to_degrees()), "{sheared:?}");
        let flipped = linear(0.0, (200.0, 120.0), Affine::FLIP_X);
        assert!(near(flipped.0, 180.0), "{flipped:?}");
        let scaled = linear(30.0, (200.0, 120.0), Affine::scale(3.0));
        assert!(close(scaled, tilted), "{scaled:?}");
        let radial = fitted(
            GradientKind::Radial,
            0.0,
            (200.0, 120.0),
            Affine::scale(3.0),
        );
        assert!(close(radial, (0.0, 116.62)), "{radial:?}");
    }

    #[test]
    fn an_overlay_s_angle_stays_in_photoshop_s_range_on_a_flattened_text() {
        // The scene keeps a gradient's angle as given, so on a text its
        // transform flattens to a line or a point, which shows nothing, the
        // overlay keeps that angle, brought into (-180, 180], at 100.
        let flattened = [
            Affine::new([0.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
            Affine::scale(1e-7),
        ];
        let angles = [
            (270.0, -90.0),
            (-180.0, 180.0),
            (540.0, 180.0),
            (-90.0, -90.0),
            (30.0, 30.0),
        ];
        for transform in flattened {
            for kind in [GradientKind::Linear, GradientKind::Radial] {
                for (angle, want) in angles {
                    let got = fitted(kind, angle, (200.0, 120.0), transform);
                    assert!(
                        near(got.0, want) && got.1 == 100.0,
                        "{kind:?} at {angle} under {transform:?}: {got:?}"
                    );
                }
            }
        }
        // As a text set as it is runs it, and so the file holds it.
        let upright = fitted(
            GradientKind::Linear,
            270.0,
            (200.0, 120.0),
            Affine::IDENTITY,
        );
        assert!(near(upright.0, -90.0), "{upright:?}");
        let text = words(|run| {
            run.fill_gradient = Some(gradient(GradientKind::Linear, 270.0));
            run.fill_extent = Some(INK);
        });
        let block = effects(&layer(flattened[0]), &text).expect("effects");
        let effects = read(&block);
        let fill = object(&effects, "GrFl");
        assert!(near(number(fill, "Angl"), -90.0), "{fill:?}");
        assert_eq!(number(fill, "Scl "), 100.0);
    }
}
