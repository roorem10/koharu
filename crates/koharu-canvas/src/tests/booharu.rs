//! The O tool's box, as the canvas answers it, the default look of the text
//! the T tool adds and of a translation typed into a block with none, the
//! clean-up such a translation shows again on a sound effect kept as art,
//! and, while typing, Escape keeping what's typed, marks taken off, size
//! steps, the selected text the type bar quotes and the empty line a line
//! break typed at the end starts, a right-click's box menu, a box styled
//! before it's translated, which the canvas types into, outlines, resizes,
//! moves and turns as a block with none, and the brush's feel, on the
//! canvas's harness. The brush's tests ask for no GPU (`gpu_free`), so they
//! run without one.

use std::collections::BTreeMap;

use booharu_core::{
    attrs::{self, Auto},
    lettering::{CategoryFont, LetteringCategory},
    settings::{DefaultTextStyle, LetteringSettings},
    sfx, status,
};
use koharu_scene::{CharStyle, Fit, RichText};

use super::*;
use crate::booharu::{
    BooharuSettings, BoxModifiers, BrushFeel, Effect as Booharu, TextDefaults, Unmark,
};

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

const RED: Rgba = Rgba([1.0, 0.0, 0.0, 1.0]);
const WHITE: Rgba = Rgba([1.0; 4]);
const BLUE: Rgba = Rgba([0.0, 0.0, 1.0, 1.0]);

/// A family no machine has, so the renderer neither finds nor downloads it,
/// and sets the text in its own fallback.
const FONT: &str = "Booharu Test";

/// Booharu's settings as the host pushes them: the default font, red text
/// outlined 2 pixels wide in `stroke`, colour detection as given, and boxes
/// at Best Fit.
fn pushed(detect_colors: bool, stroke: Option<Rgba>) -> BooharuSettings {
    BooharuSettings {
        text: TextDefaults {
            lettering: LetteringSettings {
                default: DefaultTextStyle {
                    fill: Some(RED),
                    stroke,
                    stroke_width: 2.0,
                },
                detect_colors,
                ..LetteringSettings::default()
            },
            family: Some(FONT.to_owned()),
            fit: Fit::Best,
        },
        ..BooharuSettings::default()
    }
}

/// Types "Hi" with the T tool from a press at `from` dragged to `to`, and
/// commits it: the text added, as JSON.
fn type_new(fixture: &mut Fixture, from: (f64, f64), to: (f64, f64)) -> Value {
    fixture.canvas.set_tool(Tool::Text);
    let effects = gesture(&mut fixture.canvas, from, to);
    assert!(apply(fixture, effects).is_empty());
    type_text(&mut fixture.canvas, "Hi");
    let effects = press_key(&mut fixture.canvas, NamedKey::Enter, Code::Enter, CONTROL);
    assert_eq!(apply(fixture, effects), ["add_text"]);
    let added = *layers(fixture).last().expect("the text");
    layer(fixture, added)
}

fn chars(value: &Value) -> CharStyle {
    serde_json::from_value(value["style"]["chars"].clone()).expect("a style")
}

#[test]
fn the_t_tools_text_takes_the_pushed_look_and_a_box_best_fit() {
    let Some(mut fixture) = unscaled(Vec::new()) else {
        return;
    };
    // Before the host pushes any settings, Koharu's own.
    let boxed = type_new(&mut fixture, (10.0, 10.0), (110.0, 60.0));
    assert_eq!(chars(&boxed), CharStyle::default());
    assert_eq!(boxed["frame"]["box"]["fit"], json!("shrink"));

    fixture.canvas.set_booharu(pushed(true, Some(WHITE)));
    let look = CharStyle {
        family: Some(FONT.to_owned()),
        fill: Some(RED),
        stroke: Some(WHITE),
        stroke_width: Some(2.0),
        ..CharStyle::default()
    };
    let boxed = type_new(&mut fixture, (10.0, 210.0), (110.0, 260.0));
    assert_eq!(chars(&boxed), look);
    assert_eq!(boxed["frame"]["box"]["fit"], json!("best"));
    let point = type_new(&mut fixture, (200.0, 100.0), (200.0, 100.0));
    assert_eq!(chars(&point), look);
    assert_eq!(point["frame"], json!("point"));
}

/// A page of two blocks without a translation, dialogue and a sound
/// effect, each lettered in blue outlined 3 pixels wide in white at size 32
/// in a box of its own: the fixture and the blocks.
fn untranslated() -> Option<(Fixture, LayerId, LayerId)> {
    let block = |role: &str, x: f64| {
        json!({"kind": "block", "role": role, "source": {
            "frame": {"box": {"bounds": {"x": x, "y": 50.0, "width": 100.0, "height": 100.0}}},
            "content": "やあ",
            "style": {"chars": {"size": 32.0, "fill": BLUE, "stroke": WHITE, "stroke_width": 3.0}},
        }})
    };
    let fixture = unscaled(vec![
        json!({"op": "add_layer", "page": "$page", "index": 0, "layer": block("text", 50.0)}),
        json!({"op": "add_layer", "page": "$page", "index": 1, "layer": block("sfx", 250.0)}),
    ])?;
    let [talk, boom] = layers(&fixture)[..] else {
        panic!("two blocks");
    };
    Some((fixture, talk, boom))
}

/// Types "Hi" with the T tool into the block pressed at `at`, and commits
/// it: what the commit answered, not yet applied.
fn commit_translation(fixture: &mut Fixture, block: LayerId, at: (f64, f64)) -> Vec<Effect> {
    fixture.canvas.set_tool(Tool::Text);
    let effects = gesture(&mut fixture.canvas, at, at);
    assert_eq!(selections(&effects), [vec![block]]);
    type_text(&mut fixture.canvas, "Hi");
    press_key(&mut fixture.canvas, NamedKey::Enter, Code::Enter, CONTROL)
}

/// Types "Hi" with the T tool into the block pressed at `at`, and commits
/// it: the translation's character values.
fn type_translation(fixture: &mut Fixture, block: LayerId, at: (f64, f64)) -> CharStyle {
    let effects = commit_translation(fixture, block, at);
    assert_eq!(apply(fixture, effects), ["set_text"]);
    chars(&layer(fixture, block)["translation"])
}

#[test]
fn a_translation_typed_into_a_block_with_none_takes_a_new_translations_look() {
    let Some((mut fixture, talk, boom)) = untranslated() else {
        return;
    };
    let source = CharStyle {
        size: Some(32.0),
        fill: Some(BLUE),
        stroke: Some(WHITE),
        stroke_width: Some(3.0),
        ..CharStyle::default()
    };
    // With detection on, the colours copied from the source stay, and the
    // default font comes in.
    fixture.canvas.set_booharu(pushed(true, Some(WHITE)));
    assert_eq!(
        type_translation(&mut fixture, talk, (100.0, 100.0)),
        CharStyle {
            family: Some(FONT.to_owned()),
            ..source.clone()
        }
    );
    // With it off, the default colours; a sound effect keeps a white
    // outline about red, an em's sixteenth wide, as the default has none.
    fixture.canvas.set_booharu(pushed(false, None));
    assert_eq!(
        type_translation(&mut fixture, boom, (300.0, 100.0)),
        CharStyle {
            family: Some(FONT.to_owned()),
            fill: Some(RED),
            stroke: Some(WHITE),
            stroke_width: Some(2.0),
            ..source
        }
    );
}

#[test]
fn a_translation_typed_into_a_block_takes_its_categorys_font_while_the_switch_is_on() {
    // A block the user labelled Shout, and one floating outside any
    // balloon, as `untranslated`'s are lettered.
    let block = |x: f64, attributes: Value| {
        json!({"kind": "block", "role": "text", "attributes": attributes, "source": {
            "frame": {"box": {"bounds": {"x": x, "y": 50.0, "width": 100.0, "height": 100.0}}},
            "content": "やあ",
            "style": {"chars": {"size": 32.0, "fill": BLUE, "stroke": WHITE, "stroke_width": 3.0}},
        }})
    };
    let shout = json!({"booharu": {"lettering": {"category": "shout", "source": "user"}}});
    let Some(mut fixture) = unscaled(vec![
        json!({"op": "add_layer", "page": "$page", "index": 0, "layer": block(50.0, shout)}),
        json!({"op": "add_layer", "page": "$page", "index": 1, "layer": block(250.0, json!({}))}),
    ]) else {
        return;
    };
    let [labelled, floating] = layers(&fixture)[..] else {
        panic!("two blocks");
    };
    // Pick fonts automatically on, Shout in a bold Booharu Shout, and no
    // font for Normal Dialogue, so the floating block's category has none.
    let mut settings = pushed(true, Some(WHITE));
    settings.text.lettering.auto = true;
    settings.text.lettering.categories = BTreeMap::from([(
        LetteringCategory::Shout,
        CategoryFont {
            family: Some("Booharu Shout".to_owned()),
            bold: true,
            italic: false,
        },
    )]);
    fixture.canvas.set_booharu(settings);
    let font = |chars: CharStyle| (chars.family, chars.weight, chars.italic);

    // The category comes from the page the canvas holds.
    assert_eq!(
        font(type_translation(&mut fixture, labelled, (100.0, 100.0))),
        (Some("Booharu Shout".to_owned()), Some(700), None)
    );
    assert_eq!(
        font(type_translation(&mut fixture, floating, (300.0, 100.0))),
        (Some(FONT.to_owned()), None, None)
    );
    // The T tool's own text has no category (T17), only the default look.
    let text = type_new(&mut fixture, (10.0, 210.0), (110.0, 260.0));
    assert_eq!(font(chars(&text)), (Some(FONT.to_owned()), None, None));
}

/// Keeps the block `block` as art, as a run skipping a sound effect does:
/// its clean-up hidden, with the kept mark.
fn keep_as_art(fixture: &mut Fixture, block: LayerId) {
    let kept = {
        let project = fixture.session.lock().project();
        sfx::kept_original(project.layer(block).expect("the block"))
    };
    assert_eq!(kept.len(), 2, "{kept:?}");
    let ops = kept
        .iter()
        .map(|op| serde_json::to_value(op).expect("an op"))
        .collect();
    other_writer(fixture, ops);
}

/// The ops of the one edit among `effects`, which is labelled `set_text`.
fn set_text(effects: &[Effect]) -> Vec<Op> {
    edit_ops(effects, "set_text")
}

/// The ops of the one edit among `effects`, which is labelled `label`.
fn edit_ops(effects: &[Effect], label: &str) -> Vec<Op> {
    let edits: Vec<&Edit> = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Edit(edit) => Some(edit),
            _ => None,
        })
        .collect();
    match edits[..] {
        [edit] if edit.label == label => edit.ops.clone(),
        ref other => panic!("{other:?} isn't one {label} edit"),
    }
}

fn json_of(ops: &[Op]) -> Value {
    serde_json::to_value(ops).expect("JSON")
}

/// The ops Koharu's typing gives `block`, read from `fixture` before the
/// commit, with nothing pushed: the translation in its source's place and
/// style, and its words, "Hi", as `ops`, the commit's, hold them second.
fn koharus(fixture: &Fixture, block: LayerId, ops: &[Op]) -> Value {
    let mut placed = layer(fixture, block)["source"].clone();
    placed.as_object_mut().expect("a text").remove("content");
    let Some(Op::SetRichText { spans, .. }) = ops.get(1) else {
        panic!("{ops:?} hold no words second");
    };
    let words: String = spans.iter().map(|span| span.insert.as_str()).collect();
    assert_eq!(words, "Hi");
    json_of(&[
        Op::UpdateLayer {
            layer: block.into(),
            patch: json!({ "translation": placed }),
        },
        Op::SetRichText {
            layer: block.into(),
            spans: spans.clone(),
        },
    ])
}

#[test]
fn a_translation_typed_into_an_effect_kept_as_art_shows_its_clean_up_in_the_same_step() {
    let Some((mut fixture, _, boom)) = untranslated() else {
        return;
    };
    keep_as_art(&mut fixture, boom);
    let effects = commit_translation(&mut fixture, boom, (300.0, 100.0));
    let ops = set_text(&effects);
    // Koharu's two ops, then the clean-up shown and the record cleared, as
    // it held only the mark.
    assert_eq!(ops.len(), 4, "{ops:?}");
    assert_eq!(json_of(&ops[..2]), koharus(&fixture, boom, &ops));
    let shown = [
        Op::UpdateLayer {
            layer: boom.into(),
            patch: json!({"cleanup": {"visible": true}}),
        },
        attrs::clear::<Auto>(boom),
    ];
    assert_eq!(json_of(&ops[2..]), json_of(&shown));

    assert_eq!(apply(&mut fixture, effects), ["set_text"]);
    let typed = layer(&fixture, boom);
    assert!(typed["translation"].is_object(), "{typed}");
    assert_eq!(typed["cleanup"]["visible"], json!(true));
    assert!(typed["attributes"]["booharu"]["auto"].is_null(), "{typed}");
    // One step: undone, the effect is kept as art again.
    fixture
        .session
        .lock()
        .undo()
        .expect("the undo")
        .expect("a step");
    fixture.render();
    let undone = layer(&fixture, boom);
    assert!(undone["translation"].is_null(), "{undone}");
    assert_eq!(undone["cleanup"]["visible"], json!(false));
    assert_eq!(undone["attributes"]["booharu"]["auto"]["kept"], json!(true));
}

#[test]
fn a_translation_typed_into_a_block_without_the_mark_is_koharus_alone() {
    let Some((mut fixture, talk, boom)) = untranslated() else {
        return;
    };
    // A sound effect whose clean-up was hidden by hand keeps it hidden.
    other_writer(
        &mut fixture,
        vec![json!({"op": "update_layer", "layer": boom,
            "patch": {"cleanup": {"visible": false}}})],
    );
    let effects = commit_translation(&mut fixture, boom, (300.0, 100.0));
    let ops = set_text(&effects);
    assert_eq!(json_of(&ops), koharus(&fixture, boom, &ops));
    assert_eq!(apply(&mut fixture, effects), ["set_text"]);
    let typed = layer(&fixture, boom);
    assert!(typed["translation"].is_object(), "{typed}");
    assert_eq!(typed["cleanup"]["visible"], json!(false));
    assert!(typed["attributes"]["booharu"].is_null(), "{typed}");

    // Dialogue, its clean-up showing, alike.
    let effects = commit_translation(&mut fixture, talk, (100.0, 100.0));
    let ops = set_text(&effects);
    assert_eq!(json_of(&ops), koharus(&fixture, talk, &ops));
}

#[test]
fn pushing_settings_redraws_nothing() {
    let Some(mut fixture) = unscaled(red_square()) else {
        return;
    };
    let canvas = &mut fixture.canvas;
    assert!(canvas.draw().expect("a draw").is_some());
    canvas.set_booharu(pushed(false, Some(WHITE)));
    assert!(canvas.draw().expect("a draw").is_none());
    // The same ones again, as each save pushes them.
    canvas.set_booharu(pushed(false, Some(WHITE)));
    assert!(canvas.draw().expect("a draw").is_none());
}

/// How typing is left: Escape, or Ctrl+Enter.
#[derive(Clone, Copy)]
enum Leave {
    Escape,
    Commit,
}

/// Leaves typing `how`: what typing answered.
fn leave(canvas: &mut Canvas, how: Leave) -> Vec<Effect> {
    match how {
        Leave::Escape => press_key(canvas, NamedKey::Escape, Code::Escape, NONE),
        Leave::Commit => press_key(canvas, NamedKey::Enter, Code::Enter, CONTROL),
    }
}

fn steps(fixture: &Fixture) -> usize {
    fixture.session.lock().history().steps.len()
}

#[test]
fn escape_keeps_a_new_text_as_ctrl_enter_does() {
    // The edits it applied, the project's steps, and the text added.
    let kept = |how| -> Option<(Vec<String>, usize, Value)> {
        let mut fixture = unscaled(Vec::new())?;
        fixture.canvas.set_tool(Tool::Text);
        gesture(&mut fixture.canvas, (30.0, 40.0), (30.0, 40.0));
        type_text(&mut fixture.canvas, "Kept");
        let effects = leave(&mut fixture.canvas, how);
        let selected = selections(&effects);
        let labels = apply(&mut fixture, effects);
        assert!(fixture.canvas.typing().is_none());
        let [added] = layers(&fixture)[..] else {
            panic!("one layer");
        };
        assert_eq!(selected, [vec![added]]);
        Some((
            labels,
            steps(&fixture),
            layer(&fixture, added)["content"].clone(),
        ))
    };
    let Some(escaped) = kept(Leave::Escape) else {
        return;
    };
    assert_eq!(escaped.0, ["add_text"]);
    assert_eq!(escaped.2, json!([{"insert": "Kept"}]));
    assert_eq!(Some(escaped), kept(Leave::Commit));
}

#[test]
fn escape_keeps_an_edit_to_a_text_as_ctrl_enter_does() {
    let edited = |how| -> Option<(Vec<String>, usize, Value)> {
        let mut fixture = unscaled(typed_text(json!("Hello")))?;
        let text = layers(&fixture)[0];
        fixture.canvas.set_tool(Tool::Text);
        gesture(&mut fixture.canvas, (101.0, 220.0), (101.0, 220.0));
        press_key(&mut fixture.canvas, NamedKey::End, Code::End, NONE);
        type_text(&mut fixture.canvas, "!");
        let effects = leave(&mut fixture.canvas, how);
        let labels = apply(&mut fixture, effects);
        assert!(fixture.canvas.typing().is_none());
        // The text stays selected, so the type bar styles it whole next.
        assert_eq!(fixture.canvas.selection(), [text]);
        Some((
            labels,
            steps(&fixture),
            layer(&fixture, text)["content"].clone(),
        ))
    };
    let Some(escaped) = edited(Leave::Escape) else {
        return;
    };
    assert_eq!(escaped.0, ["set_text"]);
    assert_eq!(escaped.2, json!([{"insert": "Hello!"}]));
    assert_eq!(Some(escaped), edited(Leave::Commit));
}

#[test]
fn escape_keeps_a_first_translation_with_its_look_as_ctrl_enter_does() {
    let typed = |how| -> Option<(Vec<String>, usize, Value)> {
        let (mut fixture, talk, _) = untranslated()?;
        fixture.canvas.set_booharu(pushed(false, None));
        fixture.canvas.set_tool(Tool::Text);
        let effects = gesture(&mut fixture.canvas, (100.0, 100.0), (100.0, 100.0));
        assert_eq!(selections(&effects), [vec![talk]]);
        type_text(&mut fixture.canvas, "Hi");
        let effects = leave(&mut fixture.canvas, how);
        let labels = apply(&mut fixture, effects);
        Some((
            labels,
            steps(&fixture),
            layer(&fixture, talk)["translation"].clone(),
        ))
    };
    let Some(escaped) = typed(Leave::Escape) else {
        return;
    };
    assert_eq!(escaped.0, ["set_text"]);
    // The look a new translation gets: the default font and colours.
    let look = chars(&escaped.2);
    assert_eq!(look.family.as_deref(), Some(FONT));
    assert_eq!(look.fill, Some(RED));
    assert_eq!(Some(escaped), typed(Leave::Commit));
}

#[test]
fn escape_during_a_composition_drops_only_the_composition() {
    let Some(mut fixture) = unscaled(Vec::new()) else {
        return;
    };
    fixture.canvas.set_tool(Tool::Text);
    gesture(&mut fixture.canvas, (30.0, 40.0), (30.0, 40.0));
    type_text(&mut fixture.canvas, "a");
    fixture
        .canvas
        .compose("ni", Some((2, 2)))
        .expect("a composition");
    assert!(leave(&mut fixture.canvas, Leave::Escape).is_empty());
    // Typing goes on, the composition dropped rather than typed.
    assert!(fixture.canvas.typing().is_some());
    type_text(&mut fixture.canvas, "b");
    let effects = leave(&mut fixture.canvas, Leave::Escape);
    assert_eq!(apply(&mut fixture, effects), ["add_text"]);
    let [added] = layers(&fixture)[..] else {
        panic!("one layer");
    };
    assert_eq!(layer(&fixture, added)["content"], json!([{"insert": "ab"}]));
}

/// A point text of `content` in the character values `chars`, its origin
/// at (100, 200), typed into with the T tool, the caret at its start: the
/// fixture and the text.
fn typing_into(content: Value, chars: Value) -> Option<(Fixture, LayerId)> {
    let mut fixture = unscaled(vec![
        json!({"op": "add_layer", "page": "$page", "index": 0, "layer": {
            "kind": "text", "content": content, "style": {"chars": chars},
            "transform": [[1.0, 0.0, 100.0], [0.0, 1.0, 200.0]],
        }}),
    ])?;
    let text = layers(&fixture)[0];
    fixture.canvas.set_tool(Tool::Text);
    let effects = gesture(&mut fixture.canvas, (101.0, 220.0), (101.0, 220.0));
    assert_eq!(selections(&effects), [vec![text]]);
    press_key(&mut fixture.canvas, NamedKey::Home, Code::Home, NONE);
    Some((fixture, text))
}

/// Selects `count` graphemes after the first `skip`.
fn select(canvas: &mut Canvas, skip: usize, count: usize) {
    press_key(canvas, NamedKey::Home, Code::Home, NONE);
    for _ in 0..skip {
        press_key(canvas, NamedKey::ArrowRight, Code::ArrowRight, NONE);
    }
    for _ in 0..count {
        press_key(canvas, NamedKey::ArrowRight, Code::ArrowRight, SHIFT);
    }
}

fn select_all(canvas: &mut Canvas) {
    let all = key_down(Key::Character("a".to_owned()), Code::KeyA, CONTROL);
    canvas.key(&all).expect("a key");
}

/// The words of a content, as a layer's JSON holds it, each with its marks.
fn spans(content: &Value) -> Vec<(String, CharStyle)> {
    let content: RichText = serde_json::from_value(content.clone()).expect("spans");
    content
        .0
        .into_iter()
        .map(|span| (span.insert, span.attributes))
        .collect()
}

/// Commits the typing in one step: the text's words, each with its marks.
fn committed(fixture: &mut Fixture, text: LayerId) -> Vec<(String, CharStyle)> {
    let effects = leave(&mut fixture.canvas, Leave::Commit);
    assert_eq!(apply(fixture, effects), ["set_text"]);
    spans(&layer(fixture, text)["content"])
}

fn marks(json: Value) -> CharStyle {
    serde_json::from_value(json).expect("marks")
}

fn words(spans: &[(&str, Value)]) -> Vec<(String, CharStyle)> {
    spans
        .iter()
        .map(|(insert, json)| ((*insert).to_owned(), marks(json.clone())))
        .collect()
}

#[test]
fn clear_takes_every_mark_off_the_selection_only() {
    let content = json!([
        {"insert": "Hello", "attributes": {"weight": 700, "size": 30}},
        {"insert": " world", "attributes": {"italic": true}},
    ]);
    let Some((mut fixture, text)) = typing_into(content, json!({"size": 40})) else {
        return;
    };
    // "lo wo".
    select(&mut fixture.canvas, 3, 5);
    fixture.canvas.unmark_typing(Unmark::All).expect("Clear");
    let typing = fixture.canvas.typing().expect("typing");
    assert_eq!(typing.chars.size, Some(40.0));
    assert!(typing.mixed.is_empty(), "{:?}", typing.mixed);
    assert_eq!(
        committed(&mut fixture, text),
        words(&[
            ("Hel", json!({"weight": 700, "size": 30})),
            ("lo wo", json!({})),
            ("rld", json!({"italic": true})),
        ])
    );
}

#[test]
fn clear_at_a_caret_types_the_next_characters_without_marks() {
    let content = json!([{"insert": "Hello", "attributes": {"weight": 700}}]);
    let Some((mut fixture, text)) = typing_into(content, json!({"size": 40})) else {
        return;
    };
    press_key(&mut fixture.canvas, NamedKey::End, Code::End, NONE);
    fixture.canvas.unmark_typing(Unmark::All).expect("Clear");
    // The next character's style is the text's.
    let typing = fixture.canvas.typing().expect("typing");
    assert_eq!((typing.chars.weight, typing.chars.size), (None, Some(40.0)));
    type_text(&mut fixture.canvas, "!");
    assert_eq!(
        committed(&mut fixture, text),
        words(&[("Hello", json!({"weight": 700})), ("!", json!({}))])
    );
}

#[test]
fn bold_off_takes_the_weight_off_a_regular_text_and_writes_regular_over_a_bold_one() {
    let content = json!([
        {"insert": "Hello", "attributes": {"weight": 700, "size": 30}},
        {"insert": " world"},
    ]);
    let Some((mut fixture, text)) = typing_into(content, json!({"size": 40})) else {
        return;
    };
    select(&mut fixture.canvas, 0, 5);
    fixture.canvas.unmark_typing(Unmark::Bold).expect("B off");
    // The weight goes and the size stays.
    assert_eq!(
        committed(&mut fixture, text),
        words(&[("Hello", json!({"size": 30})), (" world", json!({}))])
    );

    let bold = json!({"size": 40, "weight": 700});
    let Some((mut fixture, text)) = typing_into(json!("Hello world"), bold) else {
        return;
    };
    select(&mut fixture.canvas, 0, 5);
    fixture.canvas.unmark_typing(Unmark::Bold).expect("B off");
    let typing = fixture.canvas.typing().expect("typing");
    assert_eq!(typing.chars.weight, Some(400));
    assert_eq!(
        committed(&mut fixture, text),
        words(&[("Hello", json!({"weight": 400})), (" world", json!({}))])
    );
}

#[test]
fn italic_off_takes_the_mark_off_or_writes_upright_over_an_italic_text() {
    let content =
        json!([{"insert": "Hello", "attributes": {"italic": true}}, {"insert": " world"}]);
    let Some((mut fixture, text)) = typing_into(content, json!({"size": 40})) else {
        return;
    };
    select(&mut fixture.canvas, 0, 5);
    fixture.canvas.unmark_typing(Unmark::Italic).expect("I off");
    assert_eq!(
        committed(&mut fixture, text),
        words(&[("Hello world", json!({}))])
    );

    let italic = json!({"size": 40, "italic": true});
    let Some((mut fixture, text)) = typing_into(json!("Hello world"), italic) else {
        return;
    };
    select(&mut fixture.canvas, 0, 5);
    fixture.canvas.unmark_typing(Unmark::Italic).expect("I off");
    assert_eq!(
        committed(&mut fixture, text),
        words(&[("Hello", json!({"italic": false})), (" world", json!({}))])
    );
}

#[test]
fn marks_taken_off_back_to_the_text_as_it_was_commit_nothing() {
    let Some((mut fixture, _)) = typing_into(json!("Hello world"), json!({"size": 40})) else {
        return;
    };
    select(&mut fixture.canvas, 0, 5);
    let bold = CharStyle {
        weight: Some(700),
        italic: Some(true),
        ..CharStyle::default()
    };
    fixture.canvas.style_typing(&bold).expect("a style");
    fixture.canvas.unmark_typing(Unmark::Bold).expect("B off");
    fixture.canvas.unmark_typing(Unmark::Italic).expect("I off");
    let before = steps(&fixture);
    let effects = leave(&mut fixture.canvas, Leave::Commit);
    assert!(apply(&mut fixture, effects).is_empty());
    assert!(fixture.canvas.typing().is_none());
    assert_eq!(steps(&fixture), before);
}

#[test]
fn an_untranslated_blocks_marks_come_off_over_its_source() {
    // A bold source: B off writes regular, as over a bold text.
    let bold = json!({"kind": "block", "role": "text", "source": {
        "frame": {"box": {"bounds": {"x": 50.0, "y": 50.0, "width": 100.0, "height": 100.0}}},
        "content": "やあ",
        "style": {"chars": {"size": 32.0, "weight": 700}},
    }});
    let Some(mut fixture) = unscaled(vec![
        json!({"op": "add_layer", "page": "$page", "index": 0, "layer": bold}),
    ]) else {
        return;
    };
    let block = layers(&fixture)[0];
    fixture.canvas.set_tool(Tool::Text);
    gesture(&mut fixture.canvas, (100.0, 100.0), (100.0, 100.0));
    type_text(&mut fixture.canvas, "Hi");
    select_all(&mut fixture.canvas);
    fixture.canvas.unmark_typing(Unmark::Bold).expect("B off");
    let effects = leave(&mut fixture.canvas, Leave::Commit);
    assert_eq!(apply(&mut fixture, effects), ["set_text"]);
    assert_eq!(
        spans(&layer(&fixture, block)["translation"]["content"]),
        words(&[("Hi", json!({"weight": 400}))])
    );

    // A regular source: B off takes the weight off again.
    let Some((mut fixture, talk, _)) = untranslated() else {
        return;
    };
    fixture.canvas.set_tool(Tool::Text);
    gesture(&mut fixture.canvas, (100.0, 100.0), (100.0, 100.0));
    type_text(&mut fixture.canvas, "Hi");
    select_all(&mut fixture.canvas);
    let bold = CharStyle {
        weight: Some(700),
        ..CharStyle::default()
    };
    fixture.canvas.style_typing(&bold).expect("a style");
    fixture.canvas.unmark_typing(Unmark::Bold).expect("B off");
    let effects = leave(&mut fixture.canvas, Leave::Commit);
    assert_eq!(apply(&mut fixture, effects), ["set_text"]);
    assert_eq!(
        spans(&layer(&fixture, talk)["translation"]["content"]),
        words(&[("Hi", json!({}))])
    );
}

#[test]
fn a_size_step_steps_each_selected_character_from_its_own_size() {
    let content = json!([{"insert": "Big", "attributes": {"size": 50}}, {"insert": " and small"}]);
    let Some((mut fixture, text)) = typing_into(content, json!({"size": 40})) else {
        return;
    };
    // "Big and": 50 and the text's 40.
    select(&mut fixture.canvas, 0, 7);
    fixture.canvas.step_typing_size(true).expect("A+");
    let typing = fixture.canvas.typing().expect("typing");
    assert!(typing.mixed.contains(&CharKey::Size));
    // Stepped back, the text is as it was.
    fixture.canvas.step_typing_size(false).expect("A-");
    let unchanged = leave(&mut fixture.canvas, Leave::Commit);
    assert!(apply(&mut fixture, unchanged).is_empty());

    gesture(&mut fixture.canvas, (101.0, 220.0), (101.0, 220.0));
    select(&mut fixture.canvas, 0, 7);
    fixture.canvas.step_typing_size(true).expect("A+");
    assert_eq!(
        committed(&mut fixture, text),
        words(&[
            ("Big", json!({"size": 52.5})),
            (" and", json!({"size": 42})),
            (" small", json!({})),
        ])
    );
}

#[test]
fn a_size_step_at_a_caret_holds_for_what_is_typed_next() {
    let Some((mut fixture, text)) = typing_into(json!("Hello"), json!({"size": 40})) else {
        return;
    };
    press_key(&mut fixture.canvas, NamedKey::End, Code::End, NONE);
    fixture.canvas.step_typing_size(true).expect("A+");
    let typing = fixture.canvas.typing().expect("typing");
    assert_eq!(typing.chars.size, Some(42.0));
    type_text(&mut fixture.canvas, "!");
    assert_eq!(
        committed(&mut fixture, text),
        words(&[("Hello", json!({})), ("!", json!({"size": 42}))])
    );
}

fn quoted(canvas: &Canvas) -> Option<String> {
    canvas.typing().expect("typing").selected
}

#[test]
fn the_selection_is_quoted_on_one_line_and_none_at_a_caret() {
    let Some((mut fixture, _)) = typing_into(json!("Hello\nworld"), json!({"size": 40})) else {
        return;
    };
    assert_eq!(quoted(&fixture.canvas), None);
    select(&mut fixture.canvas, 0, 3);
    assert_eq!(quoted(&fixture.canvas).as_deref(), Some("Hel"));
    select_all(&mut fixture.canvas);
    assert_eq!(quoted(&fixture.canvas).as_deref(), Some("Hello world"));
    // A caret again quotes nothing.
    press_key(&mut fixture.canvas, NamedKey::End, Code::End, NONE);
    assert_eq!(quoted(&fixture.canvas), None);
}

#[test]
fn a_long_selection_is_cut_at_eighty_grapheme_clusters() {
    let quote = |content: String| -> Option<Option<String>> {
        let (mut fixture, _) = typing_into(json!(content), json!({"size": 40}))?;
        select_all(&mut fixture.canvas);
        Some(quoted(&fixture.canvas))
    };
    let Some(cjk) = quote("魔".repeat(100)) else {
        return;
    };
    assert_eq!(cjk, Some(format!("{}…", "魔".repeat(80))));
    // A thumbs-up with its skin tone is one cluster of two characters.
    let thumbs = "\u{1F44D}\u{1F3FD}";
    assert_eq!(
        quote(thumbs.repeat(81)),
        Some(Some(format!("{}…", thumbs.repeat(80))))
    );
    assert_eq!(quote(thumbs.repeat(80)), Some(Some(thumbs.repeat(80))));
}

#[test]
fn each_line_break_is_quoted_as_one_space() {
    // A CRLF pair, as stored text can hold one, then each of Unicode's
    // other mandatory breaks.
    let content = "a\r\nb\rc\u{000B}d\u{000C}e\u{0085}f\u{2028}g\u{2029}h\ni";
    let Some((mut fixture, _)) = typing_into(json!(content), json!({"size": 40})) else {
        return;
    };
    select_all(&mut fixture.canvas);
    assert_eq!(
        quoted(&fixture.canvas).as_deref(),
        Some("a b c d e f g h i")
    );
}

#[test]
fn a_drag_over_a_composition_quotes_no_characters_the_text_lacks() {
    let Some((mut fixture, _)) = typing_into(json!("ab"), json!({"size": 40})) else {
        return;
    };
    // A press held between "a" and "b", where the input method composes.
    press_key(
        &mut fixture.canvas,
        NamedKey::ArrowRight,
        Code::ArrowRight,
        NONE,
    );
    let between = caret(&fixture.canvas).center();
    let at = (between.x, between.y);
    fixture.canvas.input(down(at, NONE)).expect("input");
    fixture
        .canvas
        .compose("nihon", None)
        .expect("a composition");
    // The pointer runs past the composition, while the text is still "ab":
    // Koharu's drag places the caret by the text shown, past its end.
    let past = (at.0 + 200.0, at.1);
    fixture.canvas.input(drag(1, past)).expect("input");
    assert_eq!(quoted(&fixture.canvas), None);
    fixture.canvas.input(up(1, past)).expect("input");
    assert_eq!(quoted(&fixture.canvas), None);
    assert!(fixture.canvas.draw().is_ok());
}

/// Where the caret shows while typing, in the window's pixels, which are
/// the page's on an unscaled fixture.
fn caret(canvas: &Canvas) -> Rect {
    canvas.caret_area().expect("typing shows a caret")
}

#[test]
fn a_line_break_typed_at_the_end_puts_the_caret_on_a_new_empty_line() {
    let Some(mut fixture) = unscaled(Vec::new()) else {
        return;
    };
    fixture.canvas.set_tool(Tool::Text);
    gesture(&mut fixture.canvas, (30.0, 40.0), (30.0, 40.0));
    type_text(&mut fixture.canvas, "ab");
    press_key(&mut fixture.canvas, NamedKey::Home, Code::Home, NONE);
    let start = caret(&fixture.canvas);
    press_key(&mut fixture.canvas, NamedKey::End, Code::End, NONE);
    let end = caret(&fixture.canvas);
    press_key(&mut fixture.canvas, NamedKey::Enter, Code::Enter, NONE);
    // The caret starts the empty line under the first, as Photoshop shows
    // it (GitHub #89), rather than staying at the end of the first.
    let below = caret(&fixture.canvas);
    assert!(
        (below.x0 - start.x0).abs() < 1e-3,
        "{below:?} from {start:?}"
    );
    assert!(below.y0 > end.y1 - 1e-3, "{below:?} under {end:?}");
    // Up goes to the first line's start, and a press on the empty line
    // puts the caret back on it, still typing, so what's typed next goes
    // there.
    press_key(&mut fixture.canvas, NamedKey::ArrowUp, Code::ArrowUp, NONE);
    let up = caret(&fixture.canvas);
    assert!((up.x0 - start.x0).abs() < 1e-3, "{up:?} from {start:?}");
    assert!(up.y1 < below.y0 + 1e-3, "{up:?} over {below:?}");
    let on = below.center();
    gesture(&mut fixture.canvas, (on.x, on.y), (on.x, on.y));
    assert_eq!(caret(&fixture.canvas), below);
    type_text(&mut fixture.canvas, "c");
    let effects = leave(&mut fixture.canvas, Leave::Commit);
    assert_eq!(apply(&mut fixture, effects), ["add_text"]);
    let [added] = layers(&fixture)[..] else {
        panic!("one layer");
    };
    assert_eq!(
        layer(&fixture, added)["content"],
        json!([{"insert": "ab\nc"}])
    );
}

/// What a right-click answers, which is nothing but selections and
/// Booharu's effects.
#[derive(Debug, PartialEq)]
enum Answer {
    Selection(Vec<LayerId>),
    Booharu(Booharu),
}

/// The secondary button pressed at `at`, in window pixels, and released
/// there: what the press answered. It starts no gesture, so its release
/// answers nothing.
fn right_click(canvas: &mut Canvas, at: (f64, f64)) -> Vec<Answer> {
    let event = |held: PointerState| PointerButtonEvent {
        button: Some(PointerButton::Secondary),
        pointer: mouse(1),
        state: held,
    };
    let effects = canvas
        .input(PointerEvent::Down(event(state(at, NONE))))
        .expect("input");
    let released = canvas
        .input(PointerEvent::Up(event(state(at, NONE))))
        .expect("input");
    assert!(released.is_empty(), "{released:?}");
    effects
        .into_iter()
        .map(|effect| match effect {
            Effect::Selection(layers) => Answer::Selection(layers),
            Effect::Booharu(effect) => Answer::Booharu(effect),
            other => panic!("{other:?} isn't a selection or a menu"),
        })
        .collect()
}

/// A box text 100 by 40 at `(x, y)` on the page, with its layer values
/// merged in.
fn boxed_text(index: usize, at: (f64, f64), extra: Value) -> Value {
    let mut layer = json!({
        "kind": "text", "content": "Hello",
        "frame": {"box": {"bounds": {"x": 0, "y": 0, "width": 100, "height": 40}}},
        "transform": [[1.0, 0.0, at.0], [0.0, 1.0, at.1]],
    });
    if let (Some(layer), Some(extra)) = (layer.as_object_mut(), extra.as_object()) {
        layer.extend(extra.clone());
    }
    json!({"op": "add_layer", "page": "$page", "index": index, "layer": layer})
}

/// A 400×400 page holding a text over (40, 40) to (140, 80), a block
/// translated over (200, 40) to (300, 80), a locked text over (40, 200) to
/// (140, 240) and a 10×10 image at (300, 300), shown at scale 2 in a
/// viewport 100 by 60 window pixels in from the window's corner, the camera
/// moving the page on by 40 by 20 of its pixels: the page point `p` shows
/// at the window pixel `2p + (140, 80)`, which is the CSS pixel
/// `p + (70, 40)`. The fixture, and the text's, the block's, the locked
/// text's and the image's ids.
fn menu_page() -> Option<(Fixture, [LayerId; 4])> {
    let (bytes, hash) = png();
    let mut fixture = fixture(
        (400, 400),
        vec![
            boxed_text(0, (40.0, 40.0), json!({})),
            json!({"op": "add_layer", "page": "$page", "index": 1, "layer": {
                "kind": "block", "source": {"content": "やあ"}, "translation": {
                    "content": "Hi",
                    "frame": {"box": {"bounds": {"x": 0, "y": 0, "width": 100, "height": 40}}},
                    "transform": [[1.0, 0.0, 200.0], [0.0, 1.0, 40.0]],
                },
            }}),
            boxed_text(2, (40.0, 200.0), json!({"locked": true})),
            json!({"op": "add_file", "bytes": bytes}),
            json!({"op": "add_layer", "page": "$page", "index": 3, "layer": {
                "kind": "image", "file": hash, "transform": [[1.0, 0.0, 300.0], [0.0, 1.0, 300.0]],
            }}),
        ],
    )?;
    let canvas = &mut fixture.canvas;
    canvas.set_viewport(Rect::new(100.0, 60.0, 900.0, 860.0), 2.0);
    canvas.set_camera(TranslateScale::new(Vec2::new(40.0, 20.0), 2.0));
    let [text, block, locked, image] = layers(&fixture)[..] else {
        panic!("four layers");
    };
    Some((fixture, [text, block, locked, image]))
}

/// The window pixel `menu_page` shows the page point `p` at.
fn window(p: (f64, f64)) -> (f64, f64) {
    (2.0 * p.0 + 140.0, 2.0 * p.1 + 80.0)
}

#[test]
fn a_right_click_on_a_text_or_block_selects_it_and_opens_its_menu_at_the_pointer() {
    let Some((mut fixture, [text, block, ..])) = menu_page() else {
        return;
    };
    let page = fixture.page;
    for tool in [Tool::Select, Tool::DirectSelect, Tool::Text] {
        let canvas = &mut fixture.canvas;
        canvas.set_tool(tool);
        canvas.select(Vec::new());
        // At the CSS pixel the window shows the press at.
        assert_eq!(
            right_click(canvas, window((90.0, 61.0))),
            [
                Answer::Selection(vec![text]),
                Answer::Booharu(Booharu::Menu {
                    page,
                    layer: text,
                    at: Point::new(160.0, 101.0),
                }),
            ],
            "{tool:?}"
        );
        assert_eq!(
            right_click(canvas, window((250.5, 70.0))),
            [
                Answer::Selection(vec![block]),
                Answer::Booharu(Booharu::Menu {
                    page,
                    layer: block,
                    at: Point::new(320.5, 110.0),
                }),
            ],
            "{tool:?}"
        );
    }
}

#[test]
fn a_right_click_inside_the_selection_keeps_it() {
    let Some((mut fixture, [text, block, ..])) = menu_page() else {
        return;
    };
    let page = fixture.page;
    let canvas = &mut fixture.canvas;
    canvas.select(vec![text, block]);
    assert_eq!(
        right_click(canvas, window((90.0, 60.0))),
        [Answer::Booharu(Booharu::Menu {
            page,
            layer: text,
            at: Point::new(160.0, 100.0),
        })]
    );
    // A selected box alone is still told, without a selection.
    canvas.select(vec![block]);
    assert_eq!(right_click(canvas, window((250.0, 60.0))).len(), 1);
}

#[test]
fn a_right_click_off_a_box_or_on_a_locked_text_or_an_image_answers_nothing() {
    let Some((mut fixture, _)) = menu_page() else {
        return;
    };
    let canvas = &mut fixture.canvas;
    for tool in [Tool::Select, Tool::DirectSelect, Tool::Text] {
        canvas.set_tool(tool);
        for at in [(200.0, 150.0), (90.0, 220.0), (305.0, 305.0)] {
            assert_eq!(right_click(canvas, window(at)), [], "{tool:?} at {at:?}");
        }
    }
}

#[test]
fn a_right_click_while_typing_or_with_another_tool_answers_nothing() {
    let Some((mut fixture, [text, ..])) = menu_page() else {
        return;
    };
    let on = window((90.0, 60.0));
    let canvas = &mut fixture.canvas;
    for tool in [
        Tool::Marquee,
        Tool::Lasso,
        Tool::Draw,
        Tool::Eraser,
        Tool::ColorPicker,
        Tool::Remove,
        Tool::Pan,
        Tool::OcrBox,
    ] {
        canvas.set_tool(tool);
        assert_eq!(right_click(canvas, on), [], "{tool:?}");
    }
    // Typing into the text, a right-click on it leaves it typing.
    canvas.set_tool(Tool::Text);
    let effects = gesture(canvas, on, on);
    assert_eq!(selections(&effects), [vec![text]]);
    assert!(canvas.typing().is_some());
    assert_eq!(right_click(canvas, on), []);
    assert_eq!(right_click(canvas, window((250.0, 60.0))), []);
    assert!(canvas.typing().is_some());
}

#[test]
fn a_primary_press_on_a_text_opens_no_menu() {
    let Some((mut fixture, [text, ..])) = menu_page() else {
        return;
    };
    let on = window((90.0, 60.0));
    let effects = gesture(&mut fixture.canvas, on, on);
    assert_eq!(selections(&effects), [vec![text]]);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Booharu(_))),
        "{effects:?}"
    );
}

/// A colour set by hand, as the type bar's.
const GREEN: Rgba = Rgba([0.0, 0.5, 0.0, 1.0]);

/// Whether `block`'s translation is only a placeholder, by Booharu's rule.
fn placeholder(fixture: &Fixture, block: LayerId) -> bool {
    let project = fixture.session.lock().project();
    let layer = project.layer(block).expect("the block");
    let koharu_scene::Content::Block(found) = &layer.content else {
        panic!("{block} isn't a block");
    };
    status::placeholder(layer, found)
}

/// Styles `block` before it's translated, as the type bar does, writing
/// `translation` into the translation the block lacks, which the scene
/// starts from empty point text at the page's corner: a placeholder.
fn style_before_translating(fixture: &mut Fixture, block: LayerId, translation: Value) {
    other_writer(
        fixture,
        vec![json!({"op": "update_layer", "layer": block,
            "patch": {"translation": translation}})],
    );
    assert!(placeholder(fixture, block), "{}", layer(fixture, block));
}

/// Whether `inner` lies wholly in `outer`.
fn within(outer: Rect, inner: Rect) -> bool {
    outer.union(inner) == outer
}

#[test]
fn a_box_styled_before_its_translated_types_a_first_translation_in_its_place() {
    let Some((mut fixture, talk, boom)) = untranslated() else {
        return;
    };
    // The type bar's font and colour on the dialogue, and only a colour on
    // the sound effect, each set before it's translated.
    style_before_translating(
        &mut fixture,
        talk,
        json!({"style": {"chars": {"family": "Comic Neue", "fill": GREEN}}}),
    );
    style_before_translating(
        &mut fixture,
        boom,
        json!({"style": {"chars": {"fill": GREEN}}}),
    );

    // A press types into the dialogue as into a block with none, its caret
    // in the block's box rather than at the corner its placeholder lies
    // at, and the type bar showing the style set by hand.
    fixture.canvas.set_booharu(pushed(false, Some(WHITE)));
    fixture.canvas.set_tool(Tool::Text);
    let effects = gesture(&mut fixture.canvas, (100.0, 100.0), (100.0, 100.0));
    assert_eq!(selections(&effects), [vec![talk]]);
    let at = caret(&fixture.canvas);
    assert!(within(Rect::new(49.0, 49.0, 151.0, 151.0), at), "{at:?}");
    let typed = fixture.canvas.typing().expect("typing");
    assert_eq!(typed.layer, Some(talk));
    assert_eq!(
        (typed.chars.family.as_deref(), typed.chars.fill),
        (Some("Comic Neue"), Some(GREEN))
    );
    type_text(&mut fixture.canvas, "Hi");
    let effects = leave(&mut fixture.canvas, Leave::Commit);
    // Koharu's first translation: its place and style, then its words.
    let ops = set_text(&effects);
    assert!(
        matches!(ops[..], [Op::UpdateLayer { .. }, Op::SetRichText { .. }]),
        "{ops:?}"
    );
    assert_eq!(apply(&mut fixture, effects), ["set_text"]);
    let typed = layer(&fixture, talk);
    let translation = &typed["translation"];
    assert_eq!(translation["content"], json!([{"insert": "Hi"}]));
    assert_eq!(translation["frame"], typed["source"]["frame"]);
    assert_eq!(translation["transform"], typed["source"]["transform"]);
    // With colour detection off, the font and colour set by hand stay, and
    // the default look fills in the rest: its outline in place of the
    // source's.
    assert_eq!(
        chars(translation),
        CharStyle {
            family: Some("Comic Neue".to_owned()),
            size: Some(32.0),
            fill: Some(GREEN),
            stroke: Some(WHITE),
            stroke_width: Some(2.0),
            ..CharStyle::default()
        }
    );
    assert!(!placeholder(&fixture, talk));

    // The sound effect, with no font set, gets the default one; with
    // detection on, its colour set by hand and its source's outline stay.
    fixture.canvas.set_booharu(pushed(true, Some(WHITE)));
    assert_eq!(
        type_translation(&mut fixture, boom, (300.0, 100.0)),
        CharStyle {
            family: Some(FONT.to_owned()),
            size: Some(32.0),
            fill: Some(GREEN),
            stroke: Some(WHITE),
            stroke_width: Some(3.0),
            ..CharStyle::default()
        }
    );
}

#[test]
fn a_first_translation_keeps_the_paragraph_keys_and_a_vertical_direction_set_before() {
    let Some((mut fixture, talk, boom)) = untranslated() else {
        return;
    };
    // Every paragraph key on the dialogue, set vertical; on the sound
    // effect, whose source runs vertically, only an alignment, so its
    // placeholder holds the scene's horizontal, which no choice sets apart.
    other_writer(
        &mut fixture,
        vec![json!({"op": "update_layer", "layer": boom,
            "patch": {"source": {"direction": "vertical"}}})],
    );
    style_before_translating(
        &mut fixture,
        talk,
        json!({"direction": "vertical", "style":
            {"align": "center", "line_height": 1.5, "stroke_method": "contour"}}),
    );
    style_before_translating(&mut fixture, boom, json!({"style": {"align": "end"}}));
    for (block, at) in [(talk, (100.0, 100.0)), (boom, (300.0, 100.0))] {
        let effects = commit_translation(&mut fixture, block, at);
        assert_eq!(apply(&mut fixture, effects), ["set_text"]);
    }
    let typed = layer(&fixture, talk)["translation"].clone();
    assert_eq!(
        typed["style"],
        json!({"chars": typed["style"]["chars"], "align": "center", "line_height": 1.5,
            "stroke_method": "contour"})
    );
    assert_eq!(typed["direction"], json!("vertical"));
    let typed = layer(&fixture, boom)["translation"].clone();
    assert_eq!(typed["style"]["align"], json!("end"));
    assert_eq!(typed["direction"], json!("vertical"));
}

#[test]
fn a_whole_box_style_written_while_typing_into_a_block_with_none_keeps_the_caret() {
    let Some((mut fixture, talk, _)) = untranslated() else {
        return;
    };
    fixture.canvas.set_tool(Tool::Text);
    let effects = gesture(&mut fixture.canvas, (100.0, 100.0), (100.0, 100.0));
    assert_eq!(selections(&effects), [vec![talk]]);
    type_text(&mut fixture.canvas, "Hi");
    let before = caret(&fixture.canvas);
    // The type bar's border method, which styles the whole box, written
    // into the translation the block lacks while it's typed into: typing
    // takes the change on in the block's place.
    style_before_translating(
        &mut fixture,
        talk,
        json!({"style": {"stroke_method": "contour"}}),
    );
    assert!(fixture.canvas.is_typing());
    assert_eq!(caret(&fixture.canvas), before);
    let effects = leave(&mut fixture.canvas, Leave::Commit);
    assert_eq!(apply(&mut fixture, effects), ["set_text"]);
    let typed = layer(&fixture, talk);
    assert_eq!(typed["translation"]["content"], json!([{"insert": "Hi"}]));
    assert_eq!(typed["translation"]["frame"], typed["source"]["frame"]);
    assert_eq!(
        typed["translation"]["style"]["stroke_method"],
        json!("contour")
    );
}

#[test]
fn a_box_styled_before_its_translated_keeps_its_sources_control() {
    let Some((mut fixture, talk, _)) = untranslated() else {
        return;
    };
    style_before_translating(
        &mut fixture,
        talk,
        json!({"style": {"chars": {"fill": GREEN}}}),
    );
    // A click in its source's box picks it, and the selection's bounds,
    // which the type bar goes beside, hold that box.
    let canvas = &mut fixture.canvas;
    let mut effects = canvas.input(down((100.0, 100.0), NONE)).expect("input");
    effects.extend(canvas.input(up(1, (100.0, 100.0))).expect("input"));
    assert_eq!(selections(&effects), [vec![talk]]);
    assert_eq!(fixture.canvas.selection(), [talk]);
    let bounds = fixture.canvas.selection_bounds().expect("bounds");
    assert!(
        within(bounds, Rect::new(50.0, 50.0, 150.0, 150.0)),
        "{bounds:?}"
    );
}

#[test]
fn a_box_styled_before_its_translated_resizes_and_reshapes_its_source() {
    let Some((mut fixture, talk, _)) = untranslated() else {
        return;
    };
    style_before_translating(
        &mut fixture,
        talk,
        json!({"style": {"chars": {"fill": GREEN}}}),
    );
    let styled = layer(&fixture, talk)["translation"].clone();
    fixture.canvas.select(vec![talk]);
    // Its right edge, from 150 to 170: its source's box widens, and its
    // placeholder stays as it was, unplaced, so still a placeholder.
    let effects = gesture(&mut fixture.canvas, (150.0, 100.0), (170.0, 100.0));
    assert_eq!(apply(&mut fixture, effects), ["resize"]);
    let resized = layer(&fixture, talk);
    assert_eq!(
        resized["source"]["frame"]["box"]["bounds"],
        json!({"x": 50.0, "y": 50.0, "width": 120.0, "height": 100.0})
    );
    assert_eq!(resized["translation"], styled);
    assert!(placeholder(&fixture, talk));

    // Its corner, dragged with the direct selection tool, reshapes its
    // source's frame alike.
    fixture.canvas.set_tool(Tool::DirectSelect);
    let effects = gesture(&mut fixture.canvas, (170.0, 150.0), (190.0, 170.0));
    assert_eq!(apply(&mut fixture, effects), ["edit_frame"]);
    let reshaped = layer(&fixture, talk);
    let path = reshaped["source"]["frame"]["path"]["path"]
        .as_str()
        .expect("a path");
    let anchors: Vec<Point> = BezPath::from_svg(path)
        .expect("SVG")
        .elements()
        .iter()
        .filter_map(|element| match *element {
            kurbo::PathEl::MoveTo(point) | kurbo::PathEl::LineTo(point) => Some(point),
            _ => None,
        })
        .collect();
    assert!(anchors.contains(&Point::new(190.0, 170.0)), "{anchors:?}");
    assert_eq!(reshaped["translation"], styled);
    assert!(placeholder(&fixture, talk));
}

#[test]
fn a_box_styled_before_its_translated_moves_and_turns_its_source() {
    let Some((mut fixture, talk, _)) = untranslated() else {
        return;
    };
    style_before_translating(
        &mut fixture,
        talk,
        json!({"style": {"chars": {"fill": GREEN}}}),
    );
    let styled = layer(&fixture, talk)["translation"].clone();
    // A drag on it moves its source, an update of the source's transform
    // where Koharu would move the placeholder's.
    let effects = gesture(&mut fixture.canvas, (100.0, 100.0), (130.0, 120.0));
    assert_eq!(
        json_of(&edit_ops(&effects, "move")),
        json_of(&[Op::UpdateLayer {
            layer: talk.into(),
            patch: json!({"source": {"transform": [[1.0, 0.0, 30.0], [0.0, 1.0, 20.0]]}}),
        }])
    );
    assert_eq!(apply(&mut fixture, effects), ["move"]);
    assert_eq!(layer(&fixture, talk)["translation"], styled);
    assert!(placeholder(&fixture, talk));

    // Its rotate handle, 16 pixels above its top's middle, at (130, 54),
    // turned a quarter about its centre, (130, 120), after the move.
    let effects = gesture(&mut fixture.canvas, (130.0, 54.0), (196.0, 120.0));
    assert_eq!(apply(&mut fixture, effects), ["rotate"]);
    let turned = layer(&fixture, talk);
    let transform = affine(&turned["source"]["transform"]);
    assert!(close(
        transform * Point::new(100.0, 100.0),
        Point::new(130.0, 120.0)
    ));
    let across = transform * Point::new(101.0, 100.0) - transform * Point::new(100.0, 100.0);
    assert!(close(across.to_point(), Point::new(0.0, 1.0)), "{across:?}");
    assert_eq!(turned["translation"], styled);
    assert!(placeholder(&fixture, talk));

    // Typed into, still selected, its first translation goes where its
    // source went.
    fixture.canvas.set_tool(Tool::Text);
    gesture(&mut fixture.canvas, (130.0, 120.0), (130.0, 120.0));
    let typed = fixture.canvas.typing().expect("typing");
    assert_eq!(typed.layer, Some(talk));
    type_text(&mut fixture.canvas, "Hi");
    let effects = leave(&mut fixture.canvas, Leave::Commit);
    assert_eq!(apply(&mut fixture, effects), ["set_text"]);
    let typed = layer(&fixture, talk);
    assert_eq!(
        typed["translation"]["transform"],
        typed["source"]["transform"]
    );
}

#[test]
fn an_arrow_key_moves_a_box_styled_before_its_translated_by_its_source() {
    let Some((mut fixture, talk, _)) = untranslated() else {
        return;
    };
    style_before_translating(
        &mut fixture,
        talk,
        json!({"style": {"chars": {"fill": GREEN}}}),
    );
    let styled = layer(&fixture, talk)["translation"].clone();
    fixture.canvas.select(vec![talk]);
    let (before, start) = (steps(&fixture), fixture.canvas.selection_bounds());
    // An arrow's pixel to the right moves its source, an update of the
    // source's transform where Koharu's nudge would move the placeholder's,
    // in one move step.
    let effects = fixture.canvas.nudge(Vec2::new(1.0, 0.0));
    assert_eq!(
        json_of(&edit_ops(&effects, "move")),
        json_of(&[Op::UpdateLayer {
            layer: talk.into(),
            patch: json!({"source": {"transform": [[1.0, 0.0, 1.0], [0.0, 1.0, 0.0]]}}),
        }])
    );
    assert_eq!(apply(&mut fixture, effects), ["move"]);
    assert_eq!(steps(&fixture), before + 1);
    assert_eq!(
        fixture.session.lock().history().undo(),
        Some("move"),
        "the step undo takes back"
    );
    // Shift's ten pixels down goes after what the source had.
    let effects = fixture.canvas.nudge(Vec2::new(0.0, 10.0));
    assert_eq!(apply(&mut fixture, effects), ["move"]);
    assert_eq!(steps(&fixture), before + 2);
    let moved = layer(&fixture, talk);
    assert!(close(
        affine(&moved["source"]["transform"]) * Point::ZERO,
        Point::new(1.0, 10.0)
    ));
    assert_eq!(moved["translation"], styled);
    assert!(placeholder(&fixture, talk));
    // Its control, its source's, went with it, handles and all.
    let (start, bounds) = (
        start.expect("bounds"),
        fixture.canvas.selection_bounds().expect("bounds"),
    );
    let shifted = start + Vec2::new(1.0, 10.0);
    assert!(
        close(bounds.origin(), shifted.origin())
            && close(
                Point::new(bounds.x1, bounds.y1),
                Point::new(shifted.x1, shifted.y1)
            ),
        "{bounds:?} isn't {shifted:?}"
    );
}

#[test]
fn a_translated_box_types_resizes_and_moves_its_translation_as_before() {
    let Some((mut fixture, talk, boom)) = untranslated() else {
        return;
    };
    // The dialogue's translation typed by hand and styled since, and the
    // sound effect's written by a run, which records it.
    let effects = commit_translation(&mut fixture, talk, (100.0, 100.0));
    assert_eq!(apply(&mut fixture, effects), ["set_text"]);
    other_writer(
        &mut fixture,
        vec![
            json!({"op": "update_layer", "layer": talk,
                "patch": {"translation": {"style": {"chars": {"fill": GREEN}}}}}),
            json!({"op": "update_layer", "layer": boom, "patch": {"translation": {"frame":
                {"box": {"bounds": {"x": 250.0, "y": 50.0, "width": 100.0, "height": 100.0}}}}}}),
            json!({"op": "set_text", "layer": boom, "text": "Bang"}),
            json!({"op": "set_attribute", "layer": boom, "namespace": "c2pa",
                "key": "translation", "value": {"action": "c2pa.created"}}),
        ],
    );
    for (block, at) in [(talk, (100.0, 100.0)), (boom, (300.0, 100.0))] {
        assert!(!placeholder(&fixture, block));
        let before = layer(&fixture, block);
        // Typing goes into its translation, whose commit writes its words
        // alone.
        fixture.canvas.set_tool(Tool::Text);
        gesture(&mut fixture.canvas, at, at);
        let typed = fixture.canvas.typing().expect("typing");
        assert_eq!(typed.layer, Some(block));
        type_text(&mut fixture.canvas, "!");
        let effects = leave(&mut fixture.canvas, Leave::Commit);
        let ops = set_text(&effects);
        assert!(matches!(ops[..], [Op::SetRichText { .. }]), "{ops:?}");
        assert_eq!(apply(&mut fixture, effects), ["set_text"]);
        // A resize writes its translation's frame, and a move is Koharu's
        // transform, which the scene puts on its translation.
        fixture.canvas.set_tool(Tool::Select);
        fixture.canvas.select(vec![block]);
        let edge = at.0 + 50.0;
        let effects = gesture(&mut fixture.canvas, (edge, 100.0), (edge + 20.0, 100.0));
        assert_eq!(apply(&mut fixture, effects), ["resize"]);
        let effects = gesture(&mut fixture.canvas, at, (at.0 + 10.0, at.1 + 10.0));
        let ops = edit_ops(&effects, "move");
        assert!(matches!(ops[..], [Op::TransformLayer { .. }]), "{ops:?}");
        assert_eq!(apply(&mut fixture, effects), ["move"]);
        let after = layer(&fixture, block);
        assert_eq!(after["source"], before["source"]);
        assert_eq!(
            after["translation"]["frame"]["box"]["bounds"]["width"],
            json!(120.0)
        );
        assert!(close(
            affine(&after["translation"]["transform"]) * Point::ZERO,
            Point::new(10.0, 10.0)
        ));
    }
}

/// A project of one page of `size` made by `ops` after `add_page` (labelled
/// `$page`), shown on a canvas at its own size in a viewport of that size,
/// as `unscaled_of` shows one, but asking for no GPU, so a test that never
/// draws runs without one: since Koharu 0.89.0 neither a canvas nor its
/// page's frame needs one, only a draw, a snapshot of the canvas and the
/// colour picker do.
fn gpu_free(size: (u32, u32), ops: Vec<Value>) -> Fixture {
    let dir = tempfile::tempdir().expect("a folder");
    let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
    let page = json!({"op": "add_page", "id": "$page", "index": 0,
        "size": {"width": size.0, "height": size.1}});
    let ops: Vec<Op> = std::iter::once(page)
        .chain(ops)
        .map(|op| serde_json::from_value(op).expect("an op"))
        .collect();
    let change = session
        .apply(Edit {
            label: "test".into(),
            ops,
        })
        .expect("an edit");
    let session = Arc::new(Mutex::new(session));
    let mut fixture = Fixture {
        _dir: dir,
        session: Arc::clone(&session),
        renderer: RENDERER.clone(),
        canvas: Canvas::new(session, RENDERER.clone()),
        page: change.pages.added[0],
    };
    fixture.render();
    let (width, height) = (f64::from(size.0), f64::from(size.1));
    fixture
        .canvas
        .set_viewport(Rect::new(0.0, 0.0, width, height), 1.0);
    fixture.canvas.set_camera(TranslateScale::default());
    fixture
}

/// Booharu's settings with Draw at half opacity and every edge hard.
fn half_opacity() -> BooharuSettings {
    BooharuSettings {
        brush: BrushFeel {
            draw_opacity: 0.5,
            ..BrushFeel::default()
        },
        ..BooharuSettings::default()
    }
}

#[test]
fn draw_at_half_opacity_paints_half_alpha_and_uncovers_a_mask_by_half() {
    // A new raster, under a stroke at full coverage.
    let mut fixture = gpu_free((100, 100), Vec::new());
    fixture.canvas.set_booharu(half_opacity());
    fixture.canvas.set_tool(Tool::Draw);
    fixture.canvas.set_brush(brush(10.0, [1.0, 0.0, 0.0, 1.0]));
    let effects = gesture(&mut fixture.canvas, (20.0, 50.0), (80.0, 50.0));
    assert_eq!(apply(&mut fixture, effects), ["paint"]);
    let [raster] = layers(&fixture)[..] else {
        panic!("one layer");
    };
    assert_eq!(pixel(&fixture, raster, (50, 50)), [255, 0, 0, 128]);
    assert_eq!(pixel(&fixture, raster, (50, 70))[3], 0);

    // A block's mask, whose clean-up it hides wholly to start with: the
    // stroke uncovers it halfway, whatever its colour.
    let mut fixture = gpu_free(
        (400, 400),
        vec![
            json!({"op": "add_layer", "page": "$page", "index": 0, "layer": {
                "kind": "block", "source": {"content": "やあ"}, "translation": {
                    "content": "Hi",
                    "frame": {"box": {"bounds": {"x": 0, "y": 0, "width": 60, "height": 30}}},
                    "transform": [[1.0, 0.0, 300.0], [0.0, 1.0, 300.0]],
                },
            }}),
        ],
    );
    let block = layers(&fixture)[0];
    let mask: LayerId =
        serde_json::from_value(layer(&fixture, block)["mask"]["id"].clone()).expect("an id");
    fixture.canvas.set_booharu(half_opacity());
    fixture.canvas.select(vec![mask]);
    fixture.canvas.set_tool(Tool::Draw);
    fixture.canvas.set_brush(brush(6.0, [0.0, 0.0, 0.0, 1.0]));
    assert_eq!(grey(&fixture, mask, (20, 20)), 0);
    let effects = gesture(&mut fixture.canvas, (20.0, 20.0), (20.0, 20.0));
    assert_eq!(apply(&mut fixture, effects), ["paint"]);
    assert_eq!(grey(&fixture, mask, (20, 20)), 128);
    assert_eq!(grey(&fixture, mask, (12, 12)), 0);
}

#[test]
fn the_eraser_and_remove_take_no_opacity() {
    // Draw's opacity leaves the eraser erasing wholly.
    let mut fixture = gpu_free((100, 100), offset_raster());
    let raster = layers(&fixture)[0];
    fixture.canvas.set_booharu(half_opacity());
    fixture.canvas.select(vec![raster]);
    fixture.canvas.set_tool(Tool::Eraser);
    fixture.canvas.set_brush(brush(6.0, [0.0, 0.0, 0.0, 1.0]));
    let effects = gesture(&mut fixture.canvas, (50.0, 50.0), (50.0, 50.0));
    assert_eq!(apply(&mut fixture, effects), ["erase"]);
    assert_eq!(pixel(&fixture, raster, (40, 40))[3], 0);
    assert_eq!(pixel(&fixture, raster, (40, 30))[3], 255);

    // A Remove stroke asks to clean the same pixels at any opacity.
    let mut fixture = gpu_free((100, 100), Vec::new());
    fixture.canvas.set_tool(Tool::Remove);
    fixture.canvas.set_brush(brush(8.0, [0.0, 0.0, 0.0, 1.0]));
    let mut asked = Vec::new();
    for settings in [BooharuSettings::default(), half_opacity()] {
        fixture.canvas.set_booharu(settings);
        let effects = gesture(&mut fixture.canvas, (20.0, 20.0), (40.0, 20.0));
        let [Effect::Inpaint { rect, mask, .. }] = &effects[..] else {
            panic!("{effects:?}");
        };
        asked.push((*rect, mask.clone()));
    }
    assert_eq!(asked[0], asked[1]);
    let (rect, mask) = &asked[0];
    let at = ((20 - rect.y) * rect.width as i32 + (30 - rect.x)) as usize;
    assert_eq!(mask[at], 255);
}
