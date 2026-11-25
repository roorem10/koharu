//! Texts, block translations, and attributes.

mod common;
use common::*;
use koharu_scene::{CharStyle, Content, Error, LayerId, Op, RichText, Session, Span, Text};
use serde_json::json;

fn text(session: &Session, layer: LayerId) -> Text {
    match &common::layer(session, layer).content {
        Content::Text(text) => (**text).clone(),
        Content::Block(block) => block.translation.clone().expect("a translation"),
        other => panic!("{other:?} holds no text"),
    }
}

fn set_text(session: &mut Session, layer: LayerId, text: &str) -> koharu_scene::Result<()> {
    session
        .apply(edit(vec![Op::SetText {
            layer: layer.into(),
            text: text.to_owned(),
        }]))
        .map(|_| ())
}

fn marked(text: &RichText) -> Vec<(&str, Option<u16>)> {
    text.0
        .iter()
        .map(|span| (span.insert.as_str(), span.attributes.weight))
        .collect()
}

#[test]
fn set_text_keeps_marks_on_the_characters_that_stayed() {
    let (_dir, mut session, page) = session();
    let layer = add(
        &mut session,
        page,
        None,
        json!({"kind": "text", "content": [
            {"insert": "Hello", "attributes": {"weight": 700}},
            {"insert": " world"},
        ]}),
    );
    set_text(&mut session, layer, "Hello there world").expect("a text");
    assert_eq!(
        marked(&text(&session, layer).content),
        [("Hello", Some(700)), (" there world", None)]
    );
    set_text(&mut session, layer, "Help world").expect("a text");
    assert_eq!(
        marked(&text(&session, layer).content),
        [("Hel", Some(700)), ("p world", None)]
    );

    // The rich editor's spans replace the text whole, joined as stored.
    session
        .apply(edit(vec![Op::SetRichText {
            layer: layer.into(),
            spans: vec![
                Span {
                    insert: "a".into(),
                    attributes: Default::default(),
                },
                Span {
                    insert: "".into(),
                    attributes: Default::default(),
                },
                Span {
                    insert: "b".into(),
                    attributes: Default::default(),
                },
            ],
        }]))
        .expect("spans");
    assert_eq!(marked(&text(&session, layer).content), [("ab", None)]);

    let raster = add(&mut session, page, None, json!({"kind": "raster"}));
    let error = set_text(&mut session, raster, "x").expect_err("a raster has no text");
    assert!(matches!(error, Error::Invalid { .. }), "{error}");
}

#[test]
fn typing_splices_and_marks_rich_text_keeping_the_other_characters_marks() {
    let bold = CharStyle {
        weight: Some(700),
        ..CharStyle::default()
    };
    let text = RichText(vec![
        Span {
            insert: "Hello".to_owned(),
            attributes: bold.clone(),
        },
        Span {
            insert: " world".to_owned(),
            attributes: CharStyle::default(),
        },
    ]);
    // A character typed at the bold word's end takes its marks.
    let typed = text.spliced(5..5, "!", &text.marks_at(5));
    assert_eq!(marked(&typed), [("Hello!", Some(700)), (" world", None)]);
    // Typing over a selection replaces it with the given marks.
    let over = text.spliced(3..8, "p wo", &CharStyle::default());
    assert_eq!(marked(&over), [("Hel", Some(700)), ("p world", None)]);
    // Marking a range splits its spans, and alike neighbours join again.
    let light = CharStyle {
        weight: Some(300),
        ..CharStyle::default()
    };
    let lightened = text.marked(3..8, &light);
    assert_eq!(
        marked(&lightened),
        [("Hel", Some(700)), ("lo wo", Some(300)), ("rld", None)]
    );
    assert_eq!(
        marked(&lightened.marked(3..5, &bold)),
        [("Hello", Some(700)), (" wo", Some(300)), ("rld", None)]
    );
    // At the start a character takes the first one's marks.
    assert_eq!(text.marks_at(0), bold);
    assert_eq!(text.marks_at(6), CharStyle::default());
    assert_eq!(RichText::default().marks_at(0), CharStyle::default());
}

#[test]
fn text_and_duplicates_survive_reopening() {
    let (_dir, path) = path();
    let mut session = Session::create(&path).expect("a project");
    let page = page(&mut session);
    let balloon = add(
        &mut session,
        page,
        None,
        json!({"kind": "balloon", "outline": outline()}),
    );
    let block = add(&mut session, page, Some(balloon), block(0.0));
    set_text(&mut session, block, "Hi").expect("a translation");
    let change = session
        .apply(edit(vec![Op::DuplicateLayer {
            layer: balloon.into(),
            id: None,
            index: None,
        }]))
        .expect("a duplicate");
    let (copy, copied_block) = (created_layer(&change, 0), created_layer(&change, 1));
    assert_eq!(layer(&session, copy).name, layer(&session, balloon).name);
    assert_eq!(text(&session, copied_block).content.text(), "Hi");
    set_text(&mut session, copied_block, "Bye").expect("a translation");
    assert_eq!(
        text(&session, block).content.text(),
        "Hi",
        "the copy is its own"
    );

    let before = session.project();
    drop(session);
    let session = Session::open(&path).expect("it opens");
    assert_eq!(session.project(), before);
    assert_eq!(text(&session, copied_block).content.text(), "Bye");
}

#[test]
fn a_blocks_translation_comes_and_goes_with_its_own_ops() {
    let (_dir, mut session, page) = session();
    let block = add(&mut session, page, None, block(24.0));
    set_text(&mut session, block, "Hello").expect("a translation");
    // It starts in its source's place.
    let Content::Block(added) = &layer(&session, block).content else {
        panic!("not a block")
    };
    assert_eq!(
        added.translation,
        Some(Text {
            content: RichText::from("Hello"),
            ..added.source.clone()
        })
    );
    apply(
        &mut session,
        vec![json!({"op": "update_layer", "layer": block, "patch": {"translation": null}})],
    )
    .expect("no translation");
    let Content::Block(gone) = &layer(&session, block).content else {
        panic!("not a block")
    };
    assert_eq!(gone.translation, None);
    session.undo().expect("undo").expect("a step");
    assert_eq!(text(&session, block).content.text(), "Hello");

    let balloon = add(
        &mut session,
        page,
        None,
        json!({"kind": "balloon", "outline": outline()}),
    );
    let error = set_text(&mut session, balloon, "x").expect_err("a balloon has no text");
    assert!(matches!(error, Error::Invalid { .. }), "{error}");

    // Attributes: the last writer wins, and an empty namespace goes.
    let set = |value: serde_json::Value| json!({"op": "set_attribute", "layer": block, "namespace": "c2pa", "key": "ocr", "value": value});
    apply(&mut session, vec![set(json!({"action": "c2pa.created"}))]).expect("an attribute");
    apply(&mut session, vec![set(json!({"action": "c2pa.edited"}))]).expect("an attribute");
    assert_eq!(
        layer(&session, block).attributes["c2pa"]["ocr"],
        json!({"action": "c2pa.edited"})
    );
    apply(&mut session, vec![set(serde_json::Value::Null)]).expect("no attribute");
    assert!(layer(&session, block).attributes.is_empty());

    // A lock above stops text and attribute writes alike.
    apply(
        &mut session,
        vec![json!({"op": "update_layer", "layer": block, "patch": {"locked": true}})],
    )
    .expect("a lock");
    let error = set_text(&mut session, block, "x").expect_err("locked");
    assert!(matches!(error, Error::Locked { .. }), "{error}");
    let error = apply(&mut session, vec![set(json!(1))]).expect_err("locked");
    assert!(matches!(error, Error::Locked { .. }), "{error}");
}

// Booharu: the border keys stay out of the file until set.
#[test]
fn booharu_border_keys_are_left_out_until_set() {
    use koharu_scene::{Rgba, StrokeMethod, TextStyle};

    // A style without them writes what Koharu writes, so its project stays
    // readable there.
    let plain = serde_json::to_value(TextStyle::default()).expect("a style");
    assert!(plain.get("stroke_method").is_none());
    assert!(plain["chars"].get("outer_stroke").is_none());
    assert!(plain["chars"].get("outer_stroke_width").is_none());

    let style = TextStyle {
        chars: CharStyle {
            outer_stroke: Some(Rgba([0.0, 0.0, 0.0, 1.0])),
            outer_stroke_width: Some(3.0),
            ..CharStyle::default()
        },
        stroke_method: Some(StrokeMethod::Contour),
        ..TextStyle::default()
    };
    let written = serde_json::to_value(&style).expect("a style");
    assert_eq!(written["stroke_method"], json!("contour"));
    assert_eq!(
        serde_json::from_value::<TextStyle>(written).expect("it reads back"),
        style
    );
    // A mark's second border wins for its span, as every character key does.
    let marked = CharStyle {
        outer_stroke_width: Some(5.0),
        ..CharStyle::default()
    };
    assert_eq!(marked.or(&style.chars).outer_stroke_width, Some(5.0));
    assert_eq!(
        marked.or(&style.chars).outer_stroke,
        style.chars.outer_stroke
    );
}

// Booharu: a text with no border method, Koharu's or Booharu's, takes
// Koharu's border, and each stored method reads back as itself.
#[test]
fn booharu_no_border_method_is_koharus_border() {
    use koharu_scene::{StrokeMethod, TextStyle};

    let koharus: TextStyle = serde_json::from_value(json!({"line_height": 1.1})).expect("a style");
    assert_eq!(koharus.stroke_method, None);
    assert_eq!(
        koharus.stroke_method.unwrap_or_default(),
        StrokeMethod::Expansion
    );
    for (stored, method) in [
        ("expansion", StrokeMethod::Expansion),
        ("contour", StrokeMethod::Contour),
    ] {
        let style: TextStyle =
            serde_json::from_value(json!({"stroke_method": stored})).expect("a style");
        assert_eq!(style.stroke_method, Some(method), "{stored}");
    }
}

// Booharu: a gradient and letter spacing stay out of the file until set, and
// a text with both reopens as it was written.
#[test]
fn booharu_gradient_and_letter_spacing_are_left_out_until_set() {
    use koharu_scene::{Gradient, GradientKind, GradientStop, Rgba};

    let plain = serde_json::to_value(CharStyle::default()).expect("a style");
    assert!(plain.get("fill_gradient").is_none());
    assert!(plain.get("tracking").is_none());

    let gradient = json!({"kind": "radial", "angle": 30.0, "stops": [
        {"offset": 0.0, "color": [1.0, 0.0, 0.0, 1.0]},
        {"offset": 0.5, "color": [0.0, 0.0, 1.0, 0.5]},
        {"offset": 0.5, "color": [0.0, 1.0, 0.0, 1.0]},
    ]});
    let (_dir, path) = path();
    let mut session = Session::create(&path).expect("a project");
    let page = page(&mut session);
    let layer = add(
        &mut session,
        page,
        None,
        json!({"kind": "text",
            "content": [
                {"insert": "Hi ", "attributes": {"tracking": 50.0}},
                {"insert": "there", "attributes": {"fill_gradient": gradient}},
            ],
            "style": {"chars": {"fill_gradient": gradient, "tracking": -120.0}},
        }),
    );
    let written = text(&session, layer);
    let expected = Gradient {
        kind: GradientKind::Radial,
        angle: 30.0,
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: Rgba([1.0, 0.0, 0.0, 1.0]),
            },
            GradientStop {
                offset: 0.5,
                color: Rgba([0.0, 0.0, 1.0, 0.5]),
            },
            GradientStop {
                offset: 0.5,
                color: Rgba([0.0, 1.0, 0.0, 1.0]),
            },
        ],
    };
    assert_eq!(written.style.chars.fill_gradient, Some(expected.clone()));
    assert_eq!(written.style.chars.tracking, Some(-120.0));
    assert_eq!(written.content.0[0].attributes.tracking, Some(50.0));
    assert_eq!(
        written.content.0[1].attributes.fill_gradient,
        Some(expected)
    );
    let stored = serde_json::to_value(&written.style.chars).expect("a style");
    assert_eq!(stored["fill_gradient"]["kind"], json!("radial"));
    assert_eq!(stored["tracking"], json!(-120.0));

    let before = session.project();
    drop(session);
    let session = Session::open(&path).expect("it opens");
    assert_eq!(session.project(), before);

    // An angle left out reads as 0, as Photoshop's left to right.
    let gradient: Gradient = serde_json::from_value(json!({"kind": "linear", "stops": [
        {"offset": 0, "color": [0, 0, 0, 1]},
        {"offset": 1, "color": [1, 1, 1, 1]},
    ]}))
    .expect("a gradient");
    assert_eq!(gradient.angle, 0.0);
}

// Booharu: a fill and a gradient are one paint, so a word given a colour
// shows it in a gradient box, and a word with neither takes the box's paint.
#[test]
fn booharu_a_fill_and_a_gradient_are_one_paint() {
    use koharu_scene::{Gradient, GradientKind, GradientStop, Rgba};

    let (red, blue) = (Rgba([1.0, 0.0, 0.0, 1.0]), Rgba([0.0, 0.0, 1.0, 1.0]));
    let gradient = |angle: f64| Gradient {
        kind: GradientKind::Linear,
        angle,
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: red,
            },
            GradientStop {
                offset: 1.0,
                color: blue,
            },
        ],
    };
    let boxed = CharStyle {
        fill: Some(blue),
        fill_gradient: Some(gradient(90.0)),
        tracking: Some(100.0),
        ..CharStyle::default()
    };
    let coloured = CharStyle {
        fill: Some(red),
        ..CharStyle::default()
    };
    let own = CharStyle {
        fill_gradient: Some(gradient(0.0)),
        ..CharStyle::default()
    };

    // A colour on a word beats the gradient the box gives it.
    let word = coloured.or(&boxed);
    assert_eq!((word.fill, word.fill_gradient), (Some(red), None));
    // A word's own gradient beats the box's colour and gradient alike.
    let word = own.or(&CharStyle {
        fill: Some(blue),
        ..CharStyle::default()
    });
    assert_eq!(
        (word.fill, word.fill_gradient),
        (Some(blue), Some(gradient(0.0)))
    );
    assert_eq!(own.or(&boxed).fill_gradient, Some(gradient(0.0)));
    // A word with neither takes the box's paint, and its letter spacing.
    let word = CharStyle::default().or(&boxed);
    assert_eq!(
        (word.fill, word.fill_gradient, word.tracking),
        (Some(blue), Some(gradient(90.0)), Some(100.0))
    );
    // A gradient beside a colour keeps it, and so wins.
    let both = CharStyle {
        fill: Some(red),
        fill_gradient: Some(gradient(0.0)),
        ..CharStyle::default()
    };
    assert_eq!(both.or(&boxed).fill_gradient, Some(gradient(0.0)));
    // A word's letter spacing beats the box's.
    let spaced = CharStyle {
        tracking: Some(-50.0),
        ..CharStyle::default()
    };
    assert_eq!(spaced.or(&boxed).tracking, Some(-50.0));

    // So colouring words takes their gradient mark off, as a selection
    // styled while typing and the words edit both mark through `or`.
    let text = RichText(vec![Span {
        insert: "Hello".to_owned(),
        attributes: own.clone(),
    }]);
    assert_eq!(text.marked(0..5, &coloured).0[0].attributes, coloured);
    assert_eq!(
        text.marked(0..5, &spaced).0[0].attributes,
        CharStyle {
            tracking: Some(-50.0),
            ..own
        }
    );
}
