//! How new layers parse: what fails, and what each kind starts with. How a
//! patch merges is in `edits.rs`.

use koharu_scene::{Blend, Block, Content, Frame, NewLayer, Role, Text};
use serde_json::{Value, json};

fn source() -> Value {
    json!({"frame": {"box": {"bounds": {"x": 0.0, "y": 0.0, "width": 10.0, "height": 20.0}}}})
}

fn new_layer(value: Value) -> Result<NewLayer, serde_json::Error> {
    serde_json::from_value(value)
}

#[test]
fn a_misspelt_field_fails_to_parse() {
    // Serde doesn't promise that a flattened enum whose variants deny unknown
    // fields refuses a typo beside it, so this pins it.
    assert!(new_layer(json!({"kind": "raster", "opacty": 0.5})).is_err());
    assert!(
        new_layer(json!({"kind": "block", "source": {"frame": source()["frame"], "contnt": ""}}))
            .is_err()
    );
    assert!(new_layer(json!({"kind": "panel", "outline": "M0 0 L1 1 Z", "outlin": ""})).is_err());
    assert!(new_layer(json!({"kind": "baloon", "outline": "M0 0 L1 1 Z"})).is_err());
    assert!(
        new_layer(json!({"kind": "text", "frame": {"box": {"bounds": source()["frame"]["box"]["bounds"], "fitt": "none"}}}))
            .is_err()
    );
}

#[test]
fn opacity_runs_from_zero_to_one() {
    assert!(new_layer(json!({"kind": "group", "opacity": 1.5})).is_err());
    assert!(new_layer(json!({"kind": "group", "opacity": -0.1})).is_err());
    assert!(new_layer(json!({"kind": "group", "opacity": 0.0})).is_ok());
}

#[test]
fn a_block_starts_as_text_with_no_translation() {
    let layer = new_layer(json!({"kind": "block", "source": source()})).expect("a block");
    assert_eq!(
        layer.blend, None,
        "the kind picks pass-through when it's added"
    );
    let Content::Block(block) = layer.content else {
        panic!("{:?}", layer.content)
    };
    let Block {
        role,
        source: Text { frame, content, .. },
        translation,
        mask,
        cleanup,
    } = *block;
    assert_eq!(role, Role::Text);
    assert!(matches!(frame, Frame::Box { .. }));
    assert_eq!(content.text(), "");
    assert_eq!(translation, None);
    assert!(mask.visible && cleanup.visible, "both parts start shown");
    // A misspelt field inside a part is refused, as beside a layer's kind.
    assert!(
        new_layer(json!({"kind": "block", "source": source(), "cleanup": {
            "id": mask.id, "name": "", "visible": true, "locked": false, "opacity": 1.0,
            "blend": "normal", "attributes": {}, "offst": {"x": 0, "y": 0},
        }}))
        .is_err()
    );
    assert!(
        new_layer(json!({"kind": "block"})).is_err(),
        "a block needs a source"
    );
    assert_ne!(Blend::Normal, Blend::PassThrough);
}

/// Booharu: a text layer whose characters hold `chars`.
fn booharu_text(chars: Value) -> Result<NewLayer, serde_json::Error> {
    new_layer(json!({"kind": "text", "style": {"chars": chars}}))
}

/// Booharu: a gradient with a stop at each of `offsets`.
fn booharu_gradient(offsets: &[f64]) -> Value {
    let stops: Vec<Value> = offsets
        .iter()
        .map(|offset| json!({"offset": offset, "color": [0.0, 0.0, 0.0, 1.0]}))
        .collect();
    json!({"kind": "linear", "angle": 45.0, "stops": stops})
}

// Booharu: a gradient takes 2 to 8 stops from 0 to 1, never falling, and a
// letter spacing runs from -500 to 2000; anything else is refused, never
// read as something else.
#[test]
fn booharu_gradients_and_letter_spacing_out_of_bounds_are_refused() {
    for offsets in [
        &[0.0, 1.0][..],
        &[0.5, 0.5],
        &[0.0; 8],
        &[0.0, 0.3, 0.3, 1.0],
    ] {
        let chars = json!({"fill_gradient": booharu_gradient(offsets)});
        assert!(booharu_text(chars).is_ok(), "{offsets:?}");
    }
    for offsets in [
        &[0.5][..],
        &[0.0; 9],
        &[-0.1, 1.0],
        &[0.0, 1.1],
        &[0.7, 0.2],
        &[0.0, 0.6, 0.4, 1.0],
    ] {
        let chars = json!({"fill_gradient": booharu_gradient(offsets)});
        assert!(booharu_text(chars).is_err(), "{offsets:?}");
    }
    let mut unknown = booharu_gradient(&[0.0, 1.0]);
    unknown["spread"] = json!("repeat");
    let mut unknown_in_stop = booharu_gradient(&[0.0, 1.0]);
    unknown_in_stop["stops"][0]["midpoint"] = json!(0.5);
    let mut conic = booharu_gradient(&[0.0, 1.0]);
    conic["kind"] = json!("conic");
    let mut kindless = booharu_gradient(&[0.0, 1.0]);
    kindless.as_object_mut().expect("a gradient").remove("kind");
    for gradient in [unknown, unknown_in_stop, conic, kindless] {
        assert!(
            booharu_text(json!({"fill_gradient": gradient})).is_err(),
            "{gradient}"
        );
    }

    for tracking in [-500.0, 0.0, 2000.0] {
        assert!(
            booharu_text(json!({"tracking": tracking})).is_ok(),
            "{tracking}"
        );
    }
    assert!(booharu_text(json!({"tracking": null})).is_ok());
    for tracking in [-501.0, 2001.0] {
        assert!(
            booharu_text(json!({"tracking": tracking})).is_err(),
            "{tracking}"
        );
    }
    // JSON holds no NaN or infinity, so such a value never reaches the scene.
    let stop = r#"{"offset": 0, "color": [0, 0, 0, 1]}"#;
    for chars in [
        format!(
            r#"{{"fill_gradient": {{"kind": "linear", "stops": [{{"offset": NaN, "color": [0, 0, 0, 1]}}, {stop}]}}}}"#
        ),
        format!(
            r#"{{"fill_gradient": {{"kind": "linear", "angle": 1e999, "stops": [{stop}, {stop}]}}}}"#
        ),
        r#"{"tracking": NaN}"#.to_owned(),
        r#"{"tracking": -Infinity}"#.to_owned(),
        r#"{"tracking": -1e999}"#.to_owned(),
    ] {
        let layer = format!(r#"{{"kind": "text", "style": {{"chars": {chars}}}}}"#);
        assert!(serde_json::from_str::<NewLayer>(&layer).is_err(), "{chars}");
    }
}

/// Booharu: the schema of each property named `key` in `schema`, and of
/// each one the patch schema allows `null` for.
fn booharu_properties<'v>(schema: &'v Value, key: &str, found: &mut Vec<&'v Value>) {
    match schema {
        Value::Object(map) => {
            if let Some(property) = map
                .get("properties")
                .and_then(|properties| properties.get(key))
            {
                found.push(match property.get("anyOf") {
                    Some(Value::Array(options)) if options.len() == 2 => &options[0],
                    _ => property,
                });
            }
            map.values()
                .for_each(|value| booharu_properties(value, key, found));
        }
        Value::Array(values) => values
            .iter()
            .for_each(|value| booharu_properties(value, key, found)),
        _ => {}
    }
}

// Booharu: the edit tool's schema states the same limits, wherever it names
// a letter spacing or a gradient's stops: as a new layer's values and in
// `update_layer`'s patch.
#[test]
fn booharu_the_schema_states_the_gradient_and_letter_spacing_limits() {
    use koharu_scene::Op;
    use schemars::generate::SchemaSettings;

    let schema = SchemaSettings::draft2020_12()
        .with(|settings| settings.inline_subschemas = true)
        .into_generator()
        .into_root_schema_for::<Op>();
    let schema = serde_json::to_value(schema).expect("a schema");

    let mut tracking = Vec::new();
    booharu_properties(&schema, "tracking", &mut tracking);
    let mut stops = Vec::new();
    booharu_properties(&schema, "stops", &mut stops);
    // A text's style and its marks, and a block's source and translation,
    // as a new layer and in a patch.
    assert!(tracking.len() >= 4, "{tracking:?}");
    assert!(stops.len() >= 4, "{stops:?}");
    for tracking in tracking {
        let limits = (tracking["minimum"].as_f64(), tracking["maximum"].as_f64());
        assert_eq!(limits, (Some(-500.0), Some(2000.0)), "{tracking}");
    }
    for stops in stops {
        let count = (stops["minItems"].as_u64(), stops["maxItems"].as_u64());
        assert_eq!(count, (Some(2), Some(8)), "{stops}");
        let mut offsets = Vec::new();
        booharu_properties(&stops["items"], "offset", &mut offsets);
        assert_eq!(offsets.len(), 1, "{stops}");
        let limits = (
            offsets[0]["minimum"].as_f64(),
            offsets[0]["maximum"].as_f64(),
        );
        assert_eq!(limits, (Some(0.0), Some(1.0)), "{stops}");
    }
}
