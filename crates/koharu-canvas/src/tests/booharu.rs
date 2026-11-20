//! The O tool's box, as the canvas answers it, the default look of the text
//! the T tool adds and of a translation typed into a block with none, the
//! clean-up such a translation shows again on a sound effect kept as art,
//! and Escape keeping what's typed, on the canvas's harness.

use std::collections::BTreeMap;

use booharu_core::{
    attrs::{self, Auto},
    lettering::{CategoryFont, LetteringCategory},
    settings::{DefaultTextStyle, LetteringSettings},
    sfx,
};
use koharu_scene::{CharStyle, Fit};

use super::*;
use crate::booharu::{BooharuSettings, BoxModifiers, Effect as Booharu, TextDefaults};

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
    let edits: Vec<&Edit> = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Edit(edit) => Some(edit),
            _ => None,
        })
        .collect();
    match edits[..] {
        [edit] if edit.label == "set_text" => edit.ops.clone(),
        ref other => panic!("{other:?} isn't one set_text edit"),
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
