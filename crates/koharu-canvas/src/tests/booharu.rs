//! The O tool's box, as the canvas answers it, on the canvas's harness.

use super::*;
use crate::booharu::{BoxModifiers, Effect as Booharu};

const NONE: Modifiers = Modifiers::empty();

/// A 400×400 page at its own size, with the O tool picked.
fn o_tool() -> Option<Fixture> {
    let mut fixture = unscaled(Vec::new())?;
    fixture.canvas.set_tool(Tool::OcrBox);
    Some(fixture)
}

/// The primary button released at `at` with `modifiers` held.
fn lift(at: (f64, f64), modifiers: Modifiers) -> PointerEvent {
    PointerEvent::Up(PointerButtonEvent {
        button: Some(PointerButton::Primary),
        pointer: mouse(1),
        state: state(at, modifiers),
    })
}

/// A drag from `from` to `to`, pressed with `pressed` held and lifted with
/// `lifted`; what the canvas answered.
fn draw(
    canvas: &mut Canvas,
    from: (f64, f64),
    to: (f64, f64),
    pressed: Modifiers,
    lifted: Modifiers,
) -> Vec<Effect> {
    let mut effects = canvas.input(down(from, pressed)).expect("input");
    effects.extend(canvas.input(drag(1, to)).expect("input"));
    effects.extend(canvas.input(lift(to, lifted)).expect("input"));
    effects
}

/// The boxes `effects` hold, which hold nothing else.
fn boxes(effects: &[Effect]) -> Vec<Booharu> {
    effects
        .iter()
        .map(|effect| match effect {
            Effect::Booharu(effect) => *effect,
            other => panic!("{other:?} isn't an O tool box"),
        })
        .collect()
}

fn page_rect(fixture: &Fixture, rect: Rect) -> Vec<Booharu> {
    vec![Booharu::Box {
        page: fixture.page,
        rect,
        modifiers: BoxModifiers::default(),
    }]
}

#[test]
fn a_drag_answers_one_box_over_its_page_rect() {
    let Some(mut fixture) = o_tool() else {
        return;
    };
    // Dragged up and to the left, as any way round.
    let effects = draw(
        &mut fixture.canvas,
        (150.0, 100.0),
        (50.0, 60.0),
        NONE,
        NONE,
    );
    assert_eq!(
        boxes(&effects),
        page_rect(&fixture, Rect::new(50.0, 60.0, 150.0, 100.0))
    );
}

#[test]
fn a_drag_under_four_screen_pixels_answers_nothing() {
    let Some(mut fixture) = o_tool() else {
        return;
    };
    let canvas = &mut fixture.canvas;
    assert!(draw(canvas, (50.0, 50.0), (53.0, 53.0), NONE, NONE).is_empty());
    // Too short either way is too short.
    assert!(draw(canvas, (50.0, 50.0), (250.0, 53.0), NONE, NONE).is_empty());
    // Zoomed out to half, three screen pixels are six of the page's, and
    // still too short; four are enough.
    canvas.set_camera(TranslateScale::scale(0.5));
    assert!(draw(canvas, (50.0, 50.0), (53.0, 53.0), NONE, NONE).is_empty());
    let effects = draw(canvas, (50.0, 50.0), (54.0, 54.0), NONE, NONE);
    assert_eq!(
        boxes(&effects),
        page_rect(&fixture, Rect::new(100.0, 100.0, 108.0, 108.0))
    );
}

#[test]
fn a_drag_running_off_the_page_is_cut_to_it() {
    let Some(mut fixture) = o_tool() else {
        return;
    };
    let effects = draw(
        &mut fixture.canvas,
        (350.0, 300.0),
        (480.0, 430.0),
        NONE,
        NONE,
    );
    assert_eq!(
        boxes(&effects),
        page_rect(&fixture, Rect::new(350.0, 300.0, 400.0, 400.0))
    );
    let effects = draw(
        &mut fixture.canvas,
        (30.0, 40.0),
        (-20.0, -10.0),
        NONE,
        NONE,
    );
    assert_eq!(
        boxes(&effects),
        page_rect(&fixture, Rect::new(0.0, 0.0, 30.0, 40.0))
    );
    // Wholly off the page, or a sliver of it, is no box.
    let off = draw(
        &mut fixture.canvas,
        (410.0, 10.0),
        (480.0, 90.0),
        NONE,
        NONE,
    );
    assert!(off.is_empty());
    let sliver = draw(
        &mut fixture.canvas,
        (398.0, 10.0),
        (480.0, 90.0),
        NONE,
        NONE,
    );
    assert!(sliver.is_empty());
}

#[test]
fn the_modifiers_held_as_the_pointer_lifts_pass_through() {
    let Some(mut fixture) = o_tool() else {
        return;
    };
    let canvas = &mut fixture.canvas;
    let mut held = |pressed, lifted| {
        let effects = draw(canvas, (50.0, 50.0), (150.0, 120.0), pressed, lifted);
        match boxes(&effects)[..] {
            [Booharu::Box { modifiers, .. }] => modifiers,
            ref other => panic!("{other:?}"),
        }
    };
    let modifiers = |alt, shift| BoxModifiers { alt, shift };
    assert_eq!(held(NONE, Modifiers::ALT), modifiers(true, false));
    assert_eq!(held(NONE, SHIFT), modifiers(false, true));
    assert_eq!(held(NONE, Modifiers::ALT | SHIFT), modifiers(true, true));
    // What was let go of before the pointer lifted counts for nothing.
    assert_eq!(held(Modifiers::ALT | SHIFT, NONE), modifiers(false, false));
}

#[test]
fn a_cancelled_drag_draws_no_box() {
    let Some(mut fixture) = o_tool() else {
        return;
    };
    let canvas = &mut fixture.canvas;
    canvas.input(down((50.0, 50.0), NONE)).expect("input");
    canvas.input(drag(1, (150.0, 150.0))).expect("input");
    canvas.cancel();
    assert!(
        canvas
            .input(up(1, (150.0, 150.0)))
            .expect("input")
            .is_empty()
    );
}

#[test]
fn a_box_outlasts_another_change_to_its_page_and_a_selection() {
    let Some(mut fixture) = o_tool() else {
        return;
    };
    fixture
        .canvas
        .input(down((50.0, 50.0), NONE))
        .expect("input");
    fixture.canvas.input(drag(1, (100.0, 90.0))).expect("input");
    // The last box's block is added, selected and read while this one is
    // drawn.
    let page = fixture.page;
    other_writer(
        &mut fixture,
        vec![json!({"op": "add_layer", "page": page, "index": 0, "layer": {"kind": "raster"}})],
    );
    let added = layers(&fixture);
    assert!(fixture.canvas.select(added).is_empty());
    let mut effects = fixture
        .canvas
        .input(drag(1, (150.0, 120.0)))
        .expect("input");
    effects.extend(
        fixture
            .canvas
            .input(lift((150.0, 120.0), NONE))
            .expect("input"),
    );
    assert_eq!(
        boxes(&effects),
        page_rect(&fixture, Rect::new(50.0, 50.0, 150.0, 120.0))
    );
}

#[test]
fn the_box_shows_dashed_in_the_selection_colour_while_dragged() {
    let Some(mut fixture) = o_tool() else {
        return;
    };
    let canvas = &mut fixture.canvas;
    canvas.set_colors(Colors {
        selection: Rgba([1.0, 0.0, 0.0, 1.0]),
        ..Colors::default()
    });
    // On half pixels, so the box's top edge fills row 50 alone.
    canvas.input(down((50.5, 50.5), NONE)).expect("input");
    canvas.input(drag(1, (150.5, 120.5))).expect("input");
    let red = |canvas: &mut Canvas| {
        canvas.draw().expect("a draw");
        let shown = canvas.snapshot().expect("pixels");
        let inside = shown.get_pixel(100, 85).0;
        let reds = (51..150)
            .filter(|&x| {
                let [r, g, b, _] = shown.get_pixel(x, 50).0;
                r > 200 && g < 60 && b < 60
            })
            .count();
        (reds, inside)
    };
    let (reds, inside) = red(canvas);
    // Dashes, with gaps between them, and nothing filled.
    assert!(reds > 20 && reds < 90, "{reds} red pixels along the top");
    assert_eq!(inside, [255; 4]);
    canvas.input(up(1, (150.5, 120.5))).expect("input");
    assert_eq!(red(canvas).0, 0);
}
