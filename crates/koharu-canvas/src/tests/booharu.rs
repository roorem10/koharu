//! The K tool's box, which the canvas adds as Koharu's block and asks the
//! host to read, the default look of the text the T tool adds and of a
//! translation typed into a block with none, the marker of a translation
//! typed by hand such a translation writes in its step, the clean-up it
//! shows again on a sound effect kept as art, and, while typing,
//! Escape keeping what's typed, marks taken off, size steps, the selected
//! text the type bar quotes and the empty line a line break typed at the end
//! starts, a right-click's box menu, a box styled before it's translated,
//! which the canvas types into, outlines, resizes, moves and turns as a
//! block with none, the brush's feel, Draw's opacity and the soft strokes
//! below a hardness of 1, and Show original on a levelled page, on the
//! canvas's harness. The K tool's and the brush's tests ask for no GPU
//! (`gpu_free`), so they run without one, but for a soft stroke's preview,
//! which is drawn, as are Show original's pixels.

use std::collections::BTreeMap;

use booharu_core::{
    attrs::{self, Auto},
    lettering::{CategoryFont, LetteringCategory},
    settings::{DefaultTextStyle, LetteringSettings},
    sfx, status, styles,
};
use koharu_scene::{CharStyle, Fit, RichText};
use kurbo::{Circle, Size};

use super::*;
use crate::booharu::{
    BooharuSettings, BoxModifiers, BrushFeel, Effect as Booharu, TextDefaults, Unmark,
};

const NONE: Modifiers = Modifiers::empty();

/// A 400×400 page at its own size, with the K tool, the text block tool,
/// picked; it asks for no GPU.
fn k_tool() -> Fixture {
    let mut fixture = gpu_free((400, 400), Vec::new());
    fixture.canvas.set_tool(Tool::Block);
    fixture
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

/// What a K tool box's `effects` hold, which hold nothing else, in their
/// order: Koharu's edit adding the block, labelled `add_text`, its
/// selection, and Booharu's ask to read it, which names the block selected.
fn drawn(effects: &[Effect]) -> (Edit, Booharu) {
    match effects {
        [
            Effect::Edit(edit),
            Effect::Selection(selected),
            Effect::Booharu(read @ Booharu::Block { layer, .. }),
        ] if edit.label == "add_text" && selected[..] == [*layer] => (edit.clone(), *read),
        other => panic!("{other:?} isn't a K tool box"),
    }
}

/// The block Booharu's ask to read names, with the modifiers it holds.
fn read_block(read: Booharu) -> (LayerId, BoxModifiers) {
    match read {
        Booharu::Block {
            layer, modifiers, ..
        } => (layer, modifiers),
        other => panic!("{other:?} reads no block"),
    }
}

#[test]
fn a_drag_adds_koharus_block_and_asks_the_host_to_read_it() {
    let mut fixture = k_tool();
    // Dragged up and to the left, as any way round.
    let effects = draw(
        &mut fixture.canvas,
        (150.0, 100.0),
        (50.0, 60.0),
        NONE,
        NONE,
    );
    let (edit, read) = drawn(&effects);
    assert_eq!(
        read,
        Booharu::Block {
            page: fixture.page,
            layer: read_block(read).0,
            modifiers: BoxModifiers::default(),
        }
    );
    // The block the edit adds, over the box, as Koharu's tool adds it.
    fixture
        .session
        .lock()
        .apply(edit)
        .expect("the edit applies");
    let (block, _) = read_block(read);
    let project = fixture.session.lock().project();
    let koharu_scene::Content::Block(added) = &project.layer(block).expect("the block").content
    else {
        panic!("{block} isn't a block");
    };
    assert_eq!(
        added.source.bounds(),
        Some(Rect::new(50.0, 60.0, 150.0, 100.0))
    );
}

#[test]
fn a_click_adds_and_asks_nothing() {
    let mut fixture = k_tool();
    let canvas = &mut fixture.canvas;
    assert!(draw(canvas, (50.0, 50.0), (53.0, 53.0), NONE, NONE).is_empty());
    // Zoomed out to half, three screen pixels are six of the page's, and
    // still a click.
    canvas.set_camera(TranslateScale::scale(0.5));
    assert!(draw(canvas, (50.0, 50.0), (53.0, 53.0), NONE, NONE).is_empty());
    // Short only one way, it's a box, as Koharu's is, for the host to find
    // nothing in, the O tool's rule of four pixels either way gone with it.
    canvas.set_camera(TranslateScale::default());
    let (_, read) = drawn(&draw(canvas, (50.0, 50.0), (250.0, 53.0), NONE, NONE));
    assert!(matches!(read, Booharu::Block { .. }));
}

#[test]
fn the_modifiers_held_as_the_pointer_lifts_pass_through() {
    let mut fixture = k_tool();
    let canvas = &mut fixture.canvas;
    let mut held = |pressed, lifted| {
        let effects = draw(canvas, (50.0, 50.0), (150.0, 120.0), pressed, lifted);
        read_block(drawn(&effects).1).1
    };
    let modifiers = |alt, shift| BoxModifiers { alt, shift };
    assert_eq!(held(NONE, NONE), modifiers(false, false));
    assert_eq!(held(NONE, Modifiers::ALT), modifiers(true, false));
    assert_eq!(held(NONE, SHIFT), modifiers(false, true));
    assert_eq!(held(NONE, Modifiers::ALT | SHIFT), modifiers(true, true));
    // What was let go of before the pointer lifted counts for nothing.
    assert_eq!(held(Modifiers::ALT | SHIFT, NONE), modifiers(false, false));
}

#[test]
fn a_cancelled_drag_draws_no_box() {
    let mut fixture = k_tool();
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
    let mut fixture = k_tool();
    fixture
        .canvas
        .input(down((50.0, 50.0), NONE))
        .expect("input");
    fixture.canvas.input(drag(1, (100.0, 90.0))).expect("input");
    // The last box's block is read while this one is drawn.
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
    // Its block goes on the page as the change left it.
    let (edit, read) = drawn(&effects);
    assert_eq!(apply(&mut fixture, vec![Effect::Edit(edit)]), ["add_text"]);
    let (block, _) = read_block(read);
    assert_eq!(layers(&fixture).len(), 2);
    assert_eq!(layers(&fixture)[1], block);
}

#[test]
fn a_block_the_k_tool_draws_is_made_by_hand_and_types_with_the_default_look() {
    let mut fixture = k_tool();
    let effects = draw(
        &mut fixture.canvas,
        (40.0, 40.0),
        (240.0, 120.0),
        NONE,
        NONE,
    );
    let (edit, read) = drawn(&effects);
    assert_eq!(apply(&mut fixture, vec![Effect::Edit(edit)]), ["add_text"]);
    let (block, _) = read_block(read);
    // No detection made it, so runs and Detect again keep it as hand-made.
    let project = fixture.session.lock().project();
    let layer = project.layer(block).expect("the block");
    assert!(layer.attributes.is_empty(), "{:?}", layer.attributes);
    assert!(styles::edited(layer).drawn);
    // Its first translation, typed into it, gets a new translation's look.
    fixture.canvas.set_booharu(pushed(false, Some(WHITE)));
    fixture.canvas.select(Vec::new());
    let typed = type_translation(&mut fixture, block, (140.0, 80.0));
    assert_eq!(typed.family.as_deref(), Some(FONT));
    assert_eq!(typed.fill, Some(RED));
    assert_eq!(typed.stroke, Some(WHITE));
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
    // Typing needs no GPU, only a draw does (`gpu_free`).
    let fixture = gpu_free(
        (400, 400),
        vec![
            json!({"op": "add_layer", "page": "$page", "index": 0, "layer": block("text", 50.0)}),
            json!({"op": "add_layer", "page": "$page", "index": 1, "layer": block("sfx", 250.0)}),
        ],
    );
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

/// The ops Koharu's typing gives `block` for "Hi", as `ops`, the commit's,
/// hold them first: the words alone, as the scene adds a block's missing
/// translation as its source's blank (Koharu's #1494).
fn koharus(block: LayerId, ops: &[Op]) -> Value {
    let Some(Op::SetRichText { spans, .. }) = ops.first() else {
        panic!("{ops:?} hold no words first");
    };
    let words: String = spans.iter().map(|span| span.insert.as_str()).collect();
    assert_eq!(words, "Hi");
    json_of(&[Op::SetRichText {
        layer: block.into(),
        spans: spans.clone(),
    }])
}

/// The record of a translation typed by hand that a first translation's
/// step writes on `block`, whose record held nothing else: the marker alone.
fn marker(block: LayerId) -> Op {
    let typed = Auto {
        typed: true,
        ..Auto::default()
    };
    attrs::set(block, &typed).expect("the record")
}

/// Whether `block`'s translation lies where its source does, as the scene
/// starts a first translation (Koharu's #1494): its frame, transform and
/// direction.
fn in_sources_place(fixture: &Fixture, block: LayerId) -> bool {
    let found = layer(fixture, block);
    ["frame", "transform", "direction"]
        .iter()
        .all(|key| found["translation"][key] == found["source"][key])
}

#[test]
fn a_translation_typed_into_an_effect_kept_as_art_shows_its_clean_up_in_the_same_step() {
    let Some((mut fixture, _, boom)) = untranslated() else {
        return;
    };
    keep_as_art(&mut fixture, boom);
    let effects = commit_translation(&mut fixture, boom, (300.0, 100.0));
    let ops = set_text(&effects);
    // Koharu's op, then the clean-up shown and the record holding the
    // marker of a translation typed by hand in place of the mark, as it
    // held only the mark.
    assert_eq!(ops.len(), 3, "{ops:?}");
    assert_eq!(json_of(&ops[..1]), koharus(boom, &ops));
    let shown = [
        Op::UpdateLayer {
            layer: boom.into(),
            patch: json!({"cleanup": {"visible": true}}),
        },
        marker(boom),
    ];
    assert_eq!(json_of(&ops[1..]), json_of(&shown));

    assert_eq!(apply(&mut fixture, effects), ["set_text"]);
    let typed = layer(&fixture, boom);
    assert!(typed["translation"].is_object(), "{typed}");
    assert!(in_sources_place(&fixture, boom), "{typed}");
    assert_eq!(typed["cleanup"]["visible"], json!(true));
    assert_eq!(
        typed["attributes"]["booharu"]["auto"],
        json!({"typed": true})
    );
    // One step: undone, the effect is kept as art again, with no marker.
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
    assert_eq!(
        undone["attributes"]["booharu"]["auto"],
        json!({"kept": true})
    );
}

#[test]
fn a_translation_typed_into_a_block_without_the_mark_is_koharus_with_the_marker() {
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
    let mut expected = koharus(boom, &ops);
    expected
        .as_array_mut()
        .expect("a list")
        .push(json_of(&[marker(boom)])[0].clone());
    assert_eq!(json_of(&ops), expected);
    assert_eq!(apply(&mut fixture, effects), ["set_text"]);
    let typed = layer(&fixture, boom);
    assert!(typed["translation"].is_object(), "{typed}");
    assert!(in_sources_place(&fixture, boom), "{typed}");
    assert_eq!(typed["cleanup"]["visible"], json!(false));
    assert_eq!(
        typed["attributes"]["booharu"]["auto"],
        json!({"typed": true})
    );

    // Dialogue, its clean-up showing, alike.
    let effects = commit_translation(&mut fixture, talk, (100.0, 100.0));
    let ops = set_text(&effects);
    assert_eq!(json_of(&ops[1..]), json_of(&[marker(talk)]));
    assert_eq!(json_of(&ops[..1]), koharus(talk, &ops));
}

#[test]
fn a_first_translation_typed_on_the_canvas_writes_the_marker_in_its_step_and_undo_takes_both() {
    let Some((mut fixture, talk, _)) = untranslated() else {
        return;
    };
    fixture.canvas.set_booharu(pushed(false, None));
    let before = steps(&fixture);
    let effects = commit_translation(&mut fixture, talk, (100.0, 100.0));
    // The look, Koharu's words and the marker, in the commit's one edit.
    let ops = set_text(&effects);
    assert!(
        matches!(
            ops[..],
            [
                Op::UpdateLayer { .. },
                Op::SetRichText { .. },
                Op::SetAttribute { .. }
            ]
        ),
        "{ops:?}"
    );
    assert_eq!(json_of(&ops[2..]), json_of(&[marker(talk)]));
    assert_eq!(apply(&mut fixture, effects), ["set_text"]);
    assert_eq!(steps(&fixture), before + 1);
    let typed = layer(&fixture, talk);
    assert_eq!(typed["translation"]["content"], json!([{"insert": "Hi"}]));
    assert_eq!(
        typed["attributes"]["booharu"]["auto"],
        json!({"typed": true})
    );
    // Undone, the text and the marker go together.
    fixture
        .session
        .lock()
        .undo()
        .expect("the undo")
        .expect("a step");
    fixture.render();
    let undone = layer(&fixture, talk);
    assert!(undone["translation"].is_null(), "{undone}");
    assert!(undone["attributes"]["booharu"].is_null(), "{undone}");
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
        Tool::Block,
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
/// starts as its source's blank (Koharu's #1494): a placeholder.
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
    // in the block's box, and the type bar showing the style set by hand.
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
    // Koharu's first translation: its place and style, then its words, and
    // the marker of a translation typed by hand.
    let ops = set_text(&effects);
    assert!(
        matches!(
            ops[..],
            [
                Op::UpdateLayer { .. },
                Op::SetRichText { .. },
                Op::SetAttribute { .. }
            ]
        ),
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
fn a_translation_typed_by_hand_and_cleared_is_retyped_in_its_own_frame() {
    let Some((mut fixture, talk, _)) = untranslated() else {
        return;
    };
    // A first translation typed in, with the marker of one typed by hand.
    let effects = commit_translation(&mut fixture, talk, (100.0, 100.0));
    assert_eq!(apply(&mut fixture, effects), ["set_text"]);
    // Resized and moved by hand, then cleared: blank on purpose, no
    // placeholder.
    other_writer(
        &mut fixture,
        vec![
            json!({"op": "update_layer", "layer": talk, "patch": {"translation": {
                "frame": {"box": {"bounds": {"x": 40.0, "y": 40.0, "width": 150.0, "height": 120.0}}},
                "transform": [[1.0, 0.0, 4.0], [0.0, 1.0, 4.0]],
            }}}),
            json!({"op": "set_text", "layer": talk, "text": ""}),
        ],
    );
    assert!(!placeholder(&fixture, talk));
    let cleared = layer(&fixture, talk)["translation"].clone();
    assert_ne!(cleared["frame"], layer(&fixture, talk)["source"]["frame"]);

    // Typed into again, it's Koharu's retyping alone, in its own frame and
    // transform, never its source's place.
    fixture.canvas.select(Vec::new());
    let effects = commit_translation(&mut fixture, talk, (100.0, 100.0));
    let ops = set_text(&effects);
    assert!(matches!(ops[..], [Op::SetRichText { .. }]), "{ops:?}");
    assert_eq!(apply(&mut fixture, effects), ["set_text"]);
    let typed = layer(&fixture, talk)["translation"].clone();
    assert_eq!(typed["content"], json!([{"insert": "Hi"}]));
    assert_eq!(typed["frame"], cleared["frame"]);
    assert_eq!(typed["transform"], cleared["transform"]);
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
        asked.push(inpaint(&effects));
    }
    assert_eq!(asked[0], asked[1]);
    let (rect, mask) = &asked[0];
    let at = ((20 - rect.y) * rect.width as i32 + (30 - rect.x)) as usize;
    assert_eq!(mask[at], 255);
}

/// Booharu's settings with every brush tool at `hardness`.
fn hardness(hardness: f64) -> BooharuSettings {
    BooharuSettings {
        brush: BrushFeel {
            draw_hardness: hardness,
            eraser_hardness: hardness,
            remove_hardness: hardness,
            ..BrushFeel::default()
        },
        ..BooharuSettings::default()
    }
}

/// A press at the first of `points`, a drag to each of the rest in turn,
/// and the release at the last.
fn stroke(canvas: &mut Canvas, points: &[(f64, f64)]) -> Vec<Effect> {
    let mut effects = canvas.input(down(points[0], NONE)).expect("input");
    for &point in &points[1..] {
        effects.extend(canvas.input(drag(1, point)).expect("input"));
    }
    let last = points[points.len() - 1];
    effects.extend(canvas.input(up(1, last)).expect("input"));
    effects
}

/// Booharu 0.0.4's coverage of a soft stroke through `points` on a page of
/// `size`, a byte a page pixel row by row.
fn soft_coverage(size: (u32, u32), points: &[(f64, f64)], diameter: f64, hard: f64) -> Vec<u8> {
    let points: Vec<Point> = points.iter().map(|&point| Point::from(point)).collect();
    crate::booharu::brush::tests::reference(size, &points, diameter, hard)
}

/// The one inpaint request `effects` hold: its rect and its mask. With no
/// raster or clean-up selected, Koharu's Remove adds a raster for it and
/// selects it first (Koharu 0.95), which the effects before it hold.
fn inpaint(effects: &[Effect]) -> (koharu_scene::Rect, Vec<u8>) {
    let asked: Vec<_> = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Inpaint { rect, mask, .. } => Some((*rect, mask.clone())),
            _ => None,
        })
        .collect();
    let [asked] = &asked[..] else {
        panic!("{effects:?}");
    };
    asked.clone()
}

/// A layer's pixels as the session holds them, read once, each by its own
/// coordinates; transparent where it has none.
fn pixels(fixture: &Fixture, layer: LayerId) -> impl Fn(i32, i32) -> [u8; 4] {
    let tiles = fixture.session.lock().tiles(layer, None).expect("tiles");
    let side = koharu_scene::Tile::SIZE as i32;
    move |x, y| {
        let tile = tiles
            .tiles
            .iter()
            .find(|tile| x.div_euclid(side) == tile.x && y.div_euclid(side) == tile.y);
        tile.map_or([0; 4], |tile| {
            let at = ((y.rem_euclid(side) * side + x.rem_euclid(side)) * 4) as usize;
            [0, 1, 2, 3].map(|channel| tile.pixels[at + channel])
        })
    }
}

#[test]
fn a_soft_remove_stroke_asks_to_clean_its_soft_coverage_over_koharus_pixels() {
    let size = (300, 200);
    let mut fixture = gpu_free(size, Vec::new());
    fixture.canvas.set_tool(Tool::Remove);
    fixture.canvas.set_brush(brush(24.0, [0.0, 0.0, 0.0, 1.0]));
    // A click, a two-point stroke, and a stroke running off the page's
    // right and bottom edges.
    for points in [
        vec![(60.3, 80.6)],
        vec![(20.0, 50.0), (120.5, 70.25)],
        vec![(40.0, 150.0), (90.0, 120.0), (150.0, 170.0), (295.0, 196.0)],
    ] {
        fixture.canvas.set_booharu(BooharuSettings::default());
        let (koharus, _) = inpaint(&stroke(&mut fixture.canvas, &points));
        for hard in [0.0, 0.5, 0.9] {
            fixture.canvas.set_booharu(hardness(hard));
            let (rect, mask) = inpaint(&stroke(&mut fixture.canvas, &points));
            // The pixels Koharu's stroke would clean, each at 0.0.4's
            // coverage, and none it covers outside them.
            assert_eq!(rect, koharus, "{points:?} at {hard}");
            let coverage = soft_coverage(size, &points, 24.0, hard);
            for y in 0..size.1 as i32 {
                for x in 0..size.0 as i32 {
                    let value = coverage[(y * size.0 as i32 + x) as usize];
                    let (column, row) = (x - rect.x, y - rect.y);
                    if (0..rect.width as i32).contains(&column)
                        && (0..rect.height as i32).contains(&row)
                    {
                        let asked = mask[(row * rect.width as i32 + column) as usize];
                        assert_eq!(asked, value, "({x}, {y}) of {points:?} at {hard}");
                    } else {
                        assert_eq!(value, 0, "({x}, {y}) lies outside {rect:?}");
                    }
                }
            }
            assert!(mask.iter().any(|&value| value > 0 && value < 255));
        }
    }
}

#[test]
fn at_a_hardness_of_1_a_stroke_is_koharus_byte_for_byte() {
    let mut fixture = gpu_free((100, 100), Vec::new());
    fixture.canvas.set_tool(Tool::Remove);
    fixture.canvas.set_brush(brush(9.0, [0.0, 0.0, 0.0, 1.0]));
    // A click asks to clean its dot's outline, antialiased, as Koharu's
    // brush outlines one, whatever else the feel sets.
    let click = [(40.3, 50.6)];
    let dot = Circle::new(click[0], 4.5).to_path(0.1);
    let koharus = crate::tools::brush::coverage(&dot, Size::new(100.0, 100.0)).expect("a mask");
    for settings in [BooharuSettings::default(), hardness(1.0), half_opacity()] {
        fixture.canvas.set_booharu(settings);
        assert_eq!(inpaint(&stroke(&mut fixture.canvas, &click)), koharus);
    }
    // Which no soft stroke's is, even the hardest.
    fixture.canvas.set_booharu(hardness(0.99));
    let (rect, mask) = inpaint(&stroke(&mut fixture.canvas, &click));
    assert_eq!(rect, koharus.0);
    assert_ne!(mask, koharus.1);
}

#[test]
fn a_soft_draw_or_eraser_stroke_writes_its_soft_coverage() {
    // Draw paints a new raster in the brush's colour at Draw's opacity
    // times the coverage, as Koharu paints its own.
    let mut fixture = gpu_free((100, 100), Vec::new());
    fixture.canvas.set_booharu(BooharuSettings {
        brush: BrushFeel {
            draw_hardness: 0.3,
            draw_opacity: 0.5,
            ..BrushFeel::default()
        },
        ..BooharuSettings::default()
    });
    fixture.canvas.set_tool(Tool::Draw);
    fixture.canvas.set_brush(brush(20.0, [1.0, 0.0, 0.0, 1.0]));
    let points = [(20.0, 50.0), (80.0, 50.0)];
    let effects = stroke(&mut fixture.canvas, &points);
    assert_eq!(apply(&mut fixture, effects), ["paint"]);
    let [raster] = layers(&fixture)[..] else {
        panic!("one layer");
    };
    let coverage = soft_coverage((100, 100), &points, 20.0, 0.3);
    let painted = pixels(&fixture, raster);
    for y in 0..100 {
        for x in 0..100 {
            let alpha = (f32::from(coverage[(y * 100 + x) as usize]) * 0.5).round() as u8;
            let [r, g, b, a] = painted(x, y);
            assert_eq!(a, alpha, "({x}, {y})");
            assert!(a == 0 || [r, g, b] == [255, 0, 0], "({x}, {y})");
        }
    }
    assert_eq!(painted(50, 50), [255, 0, 0, 128]);
    assert!((1..128).contains(&painted(50, 58)[3]));

    // The eraser takes the coverage off a raster's alpha.
    let mut fixture = gpu_free((100, 100), offset_raster());
    let raster = layers(&fixture)[0];
    fixture.canvas.set_booharu(hardness(0.5));
    fixture.canvas.select(vec![raster]);
    fixture.canvas.set_tool(Tool::Eraser);
    fixture.canvas.set_brush(brush(16.0, [0.0, 0.0, 0.0, 1.0]));
    let effects = stroke(&mut fixture.canvas, &[(50.0, 50.0)]);
    assert_eq!(apply(&mut fixture, effects), ["erase"]);
    let coverage = soft_coverage((100, 100), &[(50.0, 50.0)], 16.0, 0.5);
    let erased = pixels(&fixture, raster);
    // The raster's white square, from its own (30, 30), at page (40, 40).
    for y in 30..50 {
        for x in 30..50 {
            let covered = coverage[((y + 10) * 100 + x + 10) as usize];
            assert_eq!(erased(x, y), [255, 255, 255, 255 - covered], "({x}, {y})");
        }
    }
    assert_eq!(erased(40, 40)[3], 0);
    assert!((1..255).contains(&erased(33, 40)[3]));
}

/// Booharu's settings with Draw's, the Eraser's and Remove's hardness each
/// its own.
fn hardnesses(draw: f64, eraser: f64, remove: f64) -> BooharuSettings {
    BooharuSettings {
        brush: BrushFeel {
            draw_hardness: draw,
            eraser_hardness: eraser,
            remove_hardness: remove,
            ..BrushFeel::default()
        },
        ..BooharuSettings::default()
    }
}

/// An inpaint request's mask laid on a page of `size`, a byte a page pixel
/// row by row, none outside its rect.
fn on_page(size: (u32, u32), (rect, mask): (koharu_scene::Rect, Vec<u8>)) -> Vec<u8> {
    let mut page = vec![0; (size.0 * size.1) as usize];
    for row in 0..rect.height as i32 {
        for column in 0..rect.width as i32 {
            let (x, y) = (rect.x + column, rect.y + row);
            if (0..size.0 as i32).contains(&x) && (0..size.1 as i32).contains(&y) {
                page[(y * size.0 as i32 + x) as usize] =
                    mask[(row * rect.width as i32 + column) as usize];
            }
        }
    }
    page
}

#[test]
fn a_remove_strokes_new_raster_goes_under_all_lettering_and_stays_selected() {
    // A raster drawn by hand, under a block that letters the page and a
    // text over it.
    let ops = vec![
        json!({"op": "add_layer", "page": "$page", "index": 0,
            "layer": {"kind": "raster"}}),
        json!({"op": "add_layer", "page": "$page", "index": 1,
            "layer": {"kind": "block", "source": {"content": "やあ"}}}),
        json!({"op": "add_layer", "page": "$page", "index": 2,
            "layer": {"kind": "text", "content": "Hi"}}),
    ];
    for settings in [BooharuSettings::default(), hardness(0.5)] {
        let mut fixture = gpu_free((100, 100), ops.clone());
        let before = layers(&fixture);
        fixture.canvas.set_booharu(settings);
        fixture.canvas.set_tool(Tool::Remove);
        fixture.canvas.set_brush(brush(8.0, [0.0, 0.0, 0.0, 1.0]));
        let effects = gesture(&mut fixture.canvas, (20.0, 20.0), (40.0, 20.0));
        let Some(Effect::Inpaint { layer, .. }) = effects.last() else {
            panic!("{effects:?}");
        };
        let layer = *layer;
        assert_eq!(apply(&mut fixture, effects), ["add_layer"]);
        // Over the raster drawn by hand, under the block and the text.
        assert_eq!(layers(&fixture), [before[0], layer, before[1], before[2]]);
        assert_eq!(fixture.canvas.selection(), [layer]);
        // The next stroke goes onto it, and adds none.
        let effects = gesture(&mut fixture.canvas, (60.0, 60.0), (70.0, 60.0));
        assert!(matches!(effects[..], [Effect::Inpaint { layer: next, .. }] if next == layer));
        // With the raster drawn by hand selected, a stroke goes onto it.
        fixture.canvas.select(vec![before[0]]);
        let effects = gesture(&mut fixture.canvas, (60.0, 60.0), (70.0, 60.0));
        assert!(matches!(effects[..], [Effect::Inpaint { layer: onto, .. }] if onto == before[0]));
    }
    // A page with no lettering takes it at its top, as Koharu's does.
    let mut fixture = gpu_free((100, 100), ops[..1].to_vec());
    let before = layers(&fixture);
    fixture.canvas.set_tool(Tool::Remove);
    fixture.canvas.set_brush(brush(8.0, [0.0, 0.0, 0.0, 1.0]));
    let effects = gesture(&mut fixture.canvas, (20.0, 20.0), (40.0, 20.0));
    let Some(Effect::Inpaint { layer, .. }) = effects.last() else {
        panic!("{effects:?}");
    };
    let layer = *layer;
    apply(&mut fixture, effects);
    assert_eq!(layers(&fixture), [before[0], layer]);
}

#[test]
fn a_stroke_takes_its_own_tools_hardness_whatever_the_others_are() {
    let size = (100, 100);
    // Remove soft at its own hardness, with Draw's softer and the
    // Eraser's hard.
    let mut fixture = gpu_free(size, Vec::new());
    fixture.canvas.set_tool(Tool::Remove);
    fixture.canvas.set_brush(brush(16.0, [0.0, 0.0, 0.0, 1.0]));
    let points = [(30.0, 40.0), (70.0, 55.5)];
    fixture.canvas.set_booharu(hardnesses(0.3, 1.0, 0.6));
    let asked = inpaint(&stroke(&mut fixture.canvas, &points));
    assert_eq!(
        on_page(size, asked),
        soft_coverage(size, &points, 16.0, 0.6)
    );
    // And Koharu's own at its own 1, with Draw's and the Eraser's soft: a
    // click asks to clean its dot's outline, antialiased.
    fixture.canvas.set_booharu(hardnesses(0.3, 0.6, 1.0));
    let click = [(40.3, 50.6)];
    let dot = Circle::new(click[0], 8.0).to_path(0.1);
    let koharu_dot = crate::tools::brush::coverage(&dot, Size::new(100.0, 100.0)).expect("a mask");
    assert_eq!(inpaint(&stroke(&mut fixture.canvas, &click)), koharu_dot);

    // The Eraser's click on a raster's white square, from the raster's own
    // (30, 30), at page (40, 40): the square's pixels as it leaves them.
    let erased = |settings: BooharuSettings| {
        let mut fixture = gpu_free(size, offset_raster());
        let raster = layers(&fixture)[0];
        fixture.canvas.set_booharu(settings);
        fixture.canvas.select(vec![raster]);
        fixture.canvas.set_tool(Tool::Eraser);
        fixture.canvas.set_brush(brush(16.0, [0.0, 0.0, 0.0, 1.0]));
        let effects = stroke(&mut fixture.canvas, &[(50.0, 50.0)]);
        assert_eq!(apply(&mut fixture, effects), ["erase"]);
        let left = pixels(&fixture, raster);
        let left = &left;
        (30..50)
            .flat_map(|y| (30..50).map(move |x| left(x, y)))
            .collect::<Vec<_>>()
    };
    // Koharu's at its own 1, with Draw's and Remove's soft.
    let koharu_erased = erased(BooharuSettings::default());
    assert_eq!(erased(hardnesses(0.3, 1.0, 0.6)), koharu_erased);
    // Soft at its own hardness, with Draw's and Remove's hard.
    let coverage = soft_coverage(size, &[(50.0, 50.0)], 16.0, 0.6);
    let left = |x: i32, y: i32| 255 - coverage[((y + 10) * 100 + x + 10) as usize];
    let soft: Vec<_> = (30..50)
        .flat_map(|y| (30..50).map(move |x| [255, 255, 255, left(x, y)]))
        .collect();
    assert_eq!(erased(hardnesses(1.0, 0.6, 1.0)), soft);
    assert_ne!(soft, koharu_erased);
}

#[test]
fn a_soft_strokes_preview_shows_its_tinted_coverage_across_its_tiles() {
    let Some(mut fixture) = fixture((600, 300), Vec::new()) else {
        return;
    };
    let canvas = &mut fixture.canvas;
    canvas.set_viewport(Rect::new(0.0, 0.0, 400.0, 200.0), 1.0);
    let camera = TranslateScale::new(Vec2::new(10.3, 5.7), 0.61);
    canvas.set_camera(camera);
    let kept = canvas.camera();
    assert_eq!(
        (kept.translation, kept.scale),
        (camera.translation, camera.scale)
    );
    canvas.set_booharu(hardness(0.5));
    canvas.set_tool(Tool::Draw);
    canvas.set_brush(brush(60.0, [0.0, 0.0, 0.0, 1.0]));
    // Along the edge between the first and second rows of tiles, across
    // the first column's edge, the stroke so far, still under way.
    let points = [(150.0, 256.0), (400.0, 256.0)];
    let window = |page: (f64, f64)| (camera * Point::from(page)).into();
    canvas.input(down(window(points[0]), NONE)).expect("input");
    canvas.input(drag(1, window(points[1]))).expect("input");
    assert!(canvas.draw().expect("a draw").is_some());
    let snapshot = canvas.snapshot().expect("pixels");
    let coverage = soft_coverage((600, 300), &points, 60.0, 0.5);
    // Where four tiles meet, at page pixel (256, 256), clear of the brush's
    // ring at the pointer, each pixel shows the page pixel its centre
    // lies over in the stroke's black at its coverage, over the white
    // page: no tile's edge shows.
    let mut compared = 0;
    for y in 140..186 {
        for x in 140..196 {
            let page = camera.inverse() * Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
            // A centre on a page pixel's edge could show either side's.
            let edge = |value: f64| (value - value.round()).abs() < 1e-3;
            if edge(page.x) || edge(page.y) {
                continue;
            }
            let at = page.y.floor() as usize * 600 + page.x.floor() as usize;
            let want = 255 - coverage[at];
            let [r, g, b, a] = snapshot.get_pixel(x, y).0;
            assert!(
                r.abs_diff(want) <= 2 && [g, b, a] == [r, r, 255],
                "({x}, {y}) shows {:?}, not {want}",
                [r, g, b, a]
            );
            compared += 1;
        }
    }
    assert!(compared > 2000, "{compared}");
}

/// A 200×200 levelled page: its scan levelled to blue, with the scan as it
/// came in, green, kept in a hidden, locked Original scan as page levels
/// adds it, and a red square over both. It asks for no GPU (`gpu_free`).
fn levelled() -> Fixture {
    let png = |color: [u8; 3]| {
        let mut bytes = Vec::new();
        RgbImage::from_pixel(200, 200, Rgb(color))
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .expect("a PNG");
        bytes
    };
    let (scan, original) = (png([0, 0, 255]), png([0, 255, 0]));
    let (scan_file, original_file) = (Hash::of(&scan), Hash::of(&original));
    gpu_free(
        (200, 200),
        vec![
            json!({"op": "add_file", "bytes": scan}),
            json!({"op": "add_file", "bytes": original}),
            json!({"op": "set_background", "page": "$page",
                "background": {"file": scan_file, "visible": true}}),
            json!({"op": "add_layer", "page": "$page", "index": 0, "layer": {
                "kind": "image", "file": original_file, "name": "Original scan",
                "visible": false, "locked": true,
                "attributes": {"booharu": {"original_scan": true}},
            }}),
            json!({"op": "add_layer", "page": "$page", "index": 1, "layer": {
                "kind": "shape", "path": "M50 50 H100 V100 H50 Z", "fill": [1.0, 0.0, 0.0, 1.0],
            }}),
        ],
    )
}

/// The layer Show original draws in place of `fixture`'s background, as its
/// page stands now; none for Koharu's background.
fn shown_original(fixture: &Fixture) -> Option<LayerId> {
    let session = fixture.session.lock();
    let frame = fixture
        .renderer
        .render(&session, fixture.page)
        .expect("a frame");
    let snapshot = crate::Snapshot::new(frame, &session).expect("the page");
    crate::booharu::original_scan(&snapshot).and_then(|drawn| drawn.layer())
}

/// Show original draws a levelled page's scan as it came in, its hidden,
/// locked Original scan, in place of the levelled background, and leaves
/// the page as it was; a page with none shows its background, as Koharu's
/// does.
#[test]
fn show_original_draws_a_levelled_pages_original_scan() {
    let mut fixture = levelled();
    let original = {
        let session = fixture.session.lock();
        let project = session.project();
        let page = project.page(fixture.page).expect("the page");
        booharu_core::cleanup::original_scan(page).map(|(layer, _)| layer)
    };
    assert!(original.is_some());
    assert_eq!(shown_original(&fixture), original);
    assert_eq!(shown_original(&gpu_free((200, 200), red_square())), None);

    if Gpu::get().is_err() {
        eprintln!("the drawing skipped without a GPU");
        return;
    }
    let (red, green, blue) = ([255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]);
    for (shown, square, corner) in [(false, red, blue), (true, green, green), (false, red, blue)] {
        fixture.canvas.set_original(shown);
        assert!(fixture.canvas.draw().expect("a draw").is_some());
        let snapshot = fixture.canvas.snapshot().expect("pixels");
        assert_eq!(snapshot.get_pixel(75, 75).0, square, "shown: {shown}");
        assert_eq!(snapshot.get_pixel(25, 25).0, corner, "shown: {shown}");
    }
}
