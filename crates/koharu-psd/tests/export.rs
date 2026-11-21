//! Exporting a page and reading the PSD back with photocraft-psd: the layer
//! tree, each layer's properties and masks, and the blocks that keep shapes
//! and text editable.

use std::io::Cursor;

use koharu_psd::{Labels, export};
use koharu_renderer::{Gpu, Renderer, RendererConfig};
use koharu_scene::{Edit, Hash, Op, Session};
use photocraft_psd::{
    BlendMode, LayerRecord, MaskData, PsdFile, SectionType, Version, VersionedDescriptor,
    descriptor::Value,
};
use serde_json::{Value as Json, json};

fn labels() -> Labels {
    Labels {
        background: "Background".into(),
        cleanup: "Clean-up".into(),
        outline: "Balloon outline".into(),
    }
}

fn block(name: &str, x: f64, source: &str) -> Json {
    json!({"kind": "block", "name": name, "source": {
        "frame": {"box": {"bounds": {"x": x, "y": 20.0, "width": 30.0, "height": 40.0}}},
        "content": source, "direction": "vertical", "language": "ja",
    }})
}

fn translate(block: &str, text: &str, x: f64) -> [Json; 2] {
    [
        json!({"op": "set_text", "layer": block, "text": text}),
        json!({"op": "update_layer", "layer": block, "patch": {"translation": {
            "language": "en", "style": {"chars": {"size": 12.0}},
            "frame": {"box": {"bounds": {"x": x, "y": 20.0, "width": 40.0, "height": 40.0}}},
        }}}),
    ]
}

/// A 200×200 page made from a scan: paint limited by two masks, a group with
/// a mask holding a shape and paragraph text, hidden point text on
/// two lines in two sizes with a stroke, a panel holding a balloon with two
/// blocks, and a free block on the page.
fn poster() -> (tempfile::TempDir, Session, koharu_scene::PageId) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
    let mut scan = Vec::new();
    image::RgbaImage::from_pixel(200, 200, image::Rgba([240, 240, 240, 255]))
        .write_to(&mut Cursor::new(&mut scan), image::ImageFormat::Png)
        .expect("a PNG");
    let file = Hash::of(&scan);
    let white = [1.0, 1.0, 1.0, 1.0];
    let red = [200u8, 30, 30, 255].repeat(64 * 64);
    let band = vec![255u8; 200 * 64];
    let clean = vec![255u8; 30 * 40 * 4];
    let glyphs = vec![255u8; 30 * 40];
    let mut ops: Vec<Json> = vec![
        json!({"op": "add_file", "bytes": scan}),
        json!({"op": "add_page", "id": "$page", "index": 0, "background": file}),
        json!({"op": "add_layer", "id": "$paint", "page": "$page", "index": 0, "layer": {
            "kind": "raster", "name": "Paint", "opacity": 0.5, "blend": "multiply",
        }}),
        json!({"op": "write_pixels", "layer": "$paint", "rect": {"x": 0, "y": 0, "width": 64, "height": 64}, "pixels": red}),
        json!({"op": "add_layer", "id": "$keep", "page": "$page", "parent": "$paint", "index": 0, "layer": {"kind": "mask"}}),
        json!({"op": "write_pixels", "layer": "$keep", "rect": {"x": 0, "y": 0, "width": 48, "height": 64}, "pixels": vec![255u8; 48 * 64]}),
        json!({"op": "add_layer", "id": "$trim", "page": "$page", "parent": "$paint", "index": 1, "layer": {"kind": "mask", "mode": "subtract"}}),
        json!({"op": "write_pixels", "layer": "$trim", "rect": {"x": 32, "y": 0, "width": 32, "height": 64}, "pixels": vec![255u8; 32 * 64]}),
        json!({"op": "add_layer", "id": "$balloon", "page": "$page", "index": 1, "layer": {"kind": "group", "name": "Balloon"}}),
        json!({"op": "add_layer", "page": "$page", "parent": "$balloon", "index": 0, "layer": {
            "kind": "shape", "name": "Outline", "path": "M40 100 A60 40 0 1 0 160 100 A60 40 0 1 0 40 100 Z",
            "fill": white, "stroke": {"color": [0.0, 0.0, 0.0, 1.0], "width": 2.0},
        }}),
        json!({"op": "add_layer", "page": "$page", "parent": "$balloon", "index": 1, "layer": {
            "kind": "text", "name": "Line", "content": "HELLO THERE", "language": "en",
            "style": {"chars": {"size": 20.0}, "align": "center"},
            "frame": {"box": {"bounds": {"x": 60.0, "y": 80.0, "width": 80.0, "height": 40.0}}},
        }}),
        json!({"op": "add_layer", "id": "$cut", "page": "$page", "parent": "$balloon", "index": 0, "layer": {"kind": "mask"}}),
        json!({"op": "write_pixels", "layer": "$cut", "rect": {"x": 0, "y": 64, "width": 200, "height": 64}, "pixels": band}),
        json!({"op": "add_layer", "page": "$page", "index": 2, "layer": {
            "kind": "text", "name": "Sign", "visible": false, "language": "en",
            "transform": [[1.0, 0.0, 20.0], [0.0, 1.0, 160.0]],
            "style": {"chars": {"stroke": white, "stroke_width": 2.0}},
            "content": [{"insert": "BIG\r\n", "attributes": {"size": 24.0}}, {"insert": "small", "attributes": {"size": 12.0}}],
        }}),
        json!({"op": "add_layer", "id": "$panel", "page": "$page", "index": 3, "layer": {"kind": "panel", "name": "Panel", "outline": "M100 0 H200 V100 H100 Z"}}),
        json!({"op": "add_layer", "id": "$speech", "page": "$page", "parent": "$panel", "index": 0, "layer": {"kind": "balloon", "name": "Speech", "outline": "M110 10 H190 V90 H110 Z"}}),
        json!({"op": "add_layer", "id": "$first", "page": "$page", "parent": "$speech", "index": 0, "layer": block("First", 150.0, "最初")}),
        json!({"op": "add_layer", "id": "$second", "page": "$page", "parent": "$speech", "index": 1, "layer": block("Second", 115.0, "二番目")}),
        json!({"op": "add_layer", "id": "$free", "page": "$page", "index": 4, "layer": block("Free", 10.0, "自由")}),
        json!({"op": "write_pixels", "layer": "$first.cleanup", "rect": {"x": 150, "y": 20, "width": 30, "height": 40}, "pixels": clean}),
        json!({"op": "write_pixels", "layer": "$first.mask", "rect": {"x": 150, "y": 20, "width": 30, "height": 40}, "pixels": glyphs}),
    ];
    ops.extend(translate("$first", "First line", 145.0));
    ops.extend(translate("$second", "Second line", 110.0));
    ops.extend(translate("$free", "Free line", 5.0));
    // Locked last: a locked layer takes no more writes.
    ops.push(json!({"op": "update_layer", "layer": "$paint", "patch": {"locked": true}}));
    let change = session
        .apply(Edit {
            label: "test".into(),
            ops: ops
                .into_iter()
                .map(|op| serde_json::from_value::<Op>(op).expect("a valid op"))
                .collect(),
        })
        .expect("an edit");
    (dir, session, change.pages.added[0])
}

fn record<'a>(file: &'a PsdFile, name: &str) -> &'a LayerRecord {
    file.layers()
        .iter()
        .find(|record| record.name() == name)
        .expect("a record of that name")
}

/// A type layer's text descriptor: after the version, the transform, and the
/// text version.
fn type_text(record: &LayerRecord) -> VersionedDescriptor {
    let data = &record.block(b"TySh").expect("a type layer").data;
    VersionedDescriptor::parse_prefix(&data[2 + 48 + 2..])
        .expect("a descriptor")
        .0
}

fn words(record: &LayerRecord) -> String {
    let text = type_text(record);
    let Some(Value::Text(content)) = text.descriptor.get("Txt ") else {
        panic!("no text");
    };
    content.to_string_lossy()
}

fn engine_data(record: &LayerRecord) -> String {
    let text = type_text(record);
    let Some(Value::RawData(bytes)) = text.descriptor.get("EngineData") else {
        panic!("no EngineData");
    };
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn a_page_exports_as_layers_photoshop_can_edit() {
    let (_dir, session, page) = poster();
    // Installed fonts only, so the test downloads none.
    let renderer = Renderer::new(RendererConfig {
        font_families: Vec::new(),
    });
    if let Err(error) = Gpu::get() {
        eprintln!("skipped without a GPU: {error}");
        return;
    }
    let bytes = export(&session, &renderer, page, &labels()).expect("a PSD");
    let file = PsdFile::from_bytes(&bytes).expect("it reads back");
    assert_eq!((file.header.width, file.header.height), (200, 200));
    assert_eq!(file.has_real_merged_data(), Some(true));

    // Bottom to top, each folder over its divider: the scan, the paint, the
    // group, the sign, then the panel with its balloon of two blocks, and a
    // free block. A block holds its clean-up, its source, and its translation.
    let names: Vec<String> = file.layers().iter().map(LayerRecord::name).collect();
    assert_eq!(
        names,
        [
            "Background",
            "Paint",
            "</Layer group>",
            "Outline",
            "Line",
            "Balloon",
            "Sign",
            "</Layer group>",
            "</Layer group>",
            "Balloon outline",
            "</Layer group>",
            "Clean-up",
            "最初",
            "First line",
            "First",
            "</Layer group>",
            "Clean-up",
            "二番目",
            "Second line",
            "Second",
            "Speech",
            "Panel",
            "</Layer group>",
            "Clean-up",
            "自由",
            "Free line",
            "Free",
        ]
    );
    assert!(file.layers()[0].is_visible());

    let paint = record(&file, "Paint");
    assert_eq!(
        (paint.blend_mode, paint.opacity),
        (BlendMode::Multiply, 128)
    );
    assert_eq!(
        paint.block(b"lspf").expect("a lock").data,
        0x8000_0000u32.to_be_bytes()
    );
    // Its masks, one adding and one subtracting, merge into its one user
    // mask, cropped to what shows.
    assert!(paint.block(b"vmsk").is_none());
    let MaskData::Mask(mask) = &paint.mask else {
        panic!("the paint's masks merge into its user mask");
    };
    assert_eq!(
        (
            mask.rect.left,
            mask.rect.top,
            mask.rect.right,
            mask.rect.bottom
        ),
        (0, 0, 32, 64)
    );
    // Its own pixels are what it painted, at its place.
    assert_eq!(
        (
            paint.rect.left,
            paint.rect.top,
            paint.rect.right,
            paint.rect.bottom
        ),
        (0, 0, 64, 64)
    );

    let balloon = record(&file, "Balloon");
    assert_eq!(balloon.section_type(), SectionType::OpenFolder);
    assert_eq!(balloon.blend_mode, BlendMode::PassThrough);
    let MaskData::Mask(mask) = &balloon.mask else {
        panic!("the group's mask is its user mask");
    };
    assert_eq!(
        (
            mask.rect.top,
            mask.rect.bottom,
            mask.rect.left,
            mask.rect.right
        ),
        (64, 128, 0, 200)
    );

    let outline = record(&file, "Outline");
    for key in [b"vmsk", b"vscg", b"vstk"] {
        assert!(
            outline.block(key).is_some(),
            "{}",
            String::from_utf8_lossy(key)
        );
    }

    let line = record(&file, "Line");
    assert_eq!(words(line), "HELLO THERE");
    let data = engine_data(line);
    assert!(
        data.contains("/ShapeType 1") && data.contains("/Justification 2"),
        "{data}"
    );

    // Two sizes make two style runs, which reach the closing paragraph mark;
    // the \r\n is one paragraph mark.
    let sign = record(&file, "Sign");
    assert!(!sign.is_visible());
    assert!(sign.block(b"lfx2").is_some());
    // Its pixels are the black glyphs alone, which the effect strokes white.
    let red = sign.decode_channel(0, 8, Version::Psd).expect("pixels");
    assert!(
        red.iter().all(|&value| value < 128),
        "a stroke is in the pixels"
    );
    let data = engine_data(sign);
    let runs = data.split("/RunLengthArray").nth(2).expect("runs");
    assert!(runs.trim_start().starts_with("[ 4 6 ]"), "{runs}");
    assert!(data.contains("/ShapeType 0"));

    // A panel is a folder that doesn't clip; a balloon's outline is a hidden
    // shape inside its folder.
    let panel = record(&file, "Panel");
    assert_eq!(panel.section_type(), SectionType::OpenFolder);
    assert!(
        panel.block(b"vmsk").is_none(),
        "the panel draws its children past its outline"
    );
    assert!(!record(&file, "Balloon outline").is_visible());
    assert_eq!(
        record(&file, "Speech").section_type(),
        SectionType::OpenFolder
    );

    // A block's clean-up shows through its mask, its source is a hidden type
    // layer, and its translation a type layer.
    let first_cleanup = file
        .layers()
        .iter()
        .position(|record| record.name() == "最初")
        .map(|at| &file.layers()[at - 1])
        .expect("the first block's clean-up");
    let MaskData::Mask(mask) = &first_cleanup.mask else {
        panic!("a clean-up's user mask is its block's mask");
    };
    assert_eq!(
        (
            mask.rect.left,
            mask.rect.top,
            mask.rect.right,
            mask.rect.bottom
        ),
        (150, 20, 180, 60)
    );
    for (source, translation) in [
        ("最初", "First line"),
        ("二番目", "Second line"),
        ("自由", "Free line"),
    ] {
        let source_record = record(&file, source);
        assert!(!source_record.is_visible(), "{source}");
        assert_eq!(words(source_record), source);
        let translation_record = record(&file, translation);
        assert!(translation_record.is_visible());
        assert_eq!(words(translation_record), translation);
    }
}

// Booharu: a text's effects as Photoshop holds them. Effects its words all
// share stay on its type layer, a second border and a gradient in `lmfx`, and
// a word's letter spacing is its style run's tracking; a text whose words
// carry their own is drawn into a pixel layer over a hidden type layer of its
// words in plain styles to edit, numbered among the type layers, and the
// export notes it (S13).
mod booharu {
    use koharu_psd::{Notice, export_noting};
    use koharu_scene::{LayerId, PageId};

    use super::*;

    const RED: [f64; 4] = [1.0, 0.0, 0.0, 1.0];
    const BLUE: [f64; 4] = [0.0, 0.0, 1.0, 1.0];
    const GREEN: [f64; 4] = [0.0, 1.0, 0.0, 1.0];
    const DARK_GREEN: [f64; 4] = [0.0, 0.5, 0.0, 1.0];

    /// The words every text here holds.
    const WORDS: &str = "Hello brave world";

    /// The red of a gradient's first stop as EngineData writes a fill: alpha,
    /// red, green, blue.
    const FIRST_STOP: &str = "[ 1.0 1.0 0.0 0.0 ]";

    /// A gradient from red to blue, which paints nothing green.
    fn gradient() -> Json {
        json!({"kind": "linear", "angle": 0.0, "stops": [
            {"offset": 0.0, "color": RED},
            {"offset": 1.0, "color": BLUE},
        ]})
    }

    /// `WORDS`, the middle word with a green border of its own.
    fn bordered() -> Json {
        json!([
            {"insert": "Hello "},
            {"insert": "brave", "attributes": {"stroke": GREEN, "stroke_width": 3.0}},
            {"insert": " world"},
        ])
    }

    /// A 200×200 page with no scan, holding what `ops` add to `$page`.
    fn page(ops: Vec<Json>) -> (tempfile::TempDir, Session, PageId) {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        let page = json!({"op": "add_page", "id": "$page", "index": 0,
            "size": {"width": 200, "height": 200}});
        let change = session
            .apply(Edit {
                label: "test".into(),
                ops: std::iter::once(page)
                    .chain(ops)
                    .map(|op| serde_json::from_value::<Op>(op).expect("a valid op"))
                    .collect(),
            })
            .expect("an edit");
        (dir, session, change.pages.added[0])
    }

    /// `page` exported and read back, with the notices of its export; `None`
    /// without a GPU.
    fn exported(session: &Session, page: PageId) -> Option<(PsdFile, Vec<Notice>)> {
        // Installed fonts only, so the test downloads none.
        let renderer = Renderer::new(RendererConfig {
            font_families: Vec::new(),
        });
        if let Err(error) = Gpu::get() {
            eprintln!("skipped without a GPU: {error}");
            return None;
        }
        let mut notices = Vec::new();
        let bytes =
            export_noting(session, &renderer, page, &labels(), &mut notices).expect("a PSD");
        let file = PsdFile::from_bytes(&bytes).expect("it reads back");
        Some((file, notices))
    }

    /// The names of `file`'s layers, bottom to top.
    fn names(file: &PsdFile) -> Vec<String> {
        file.layers().iter().map(LayerRecord::name).collect()
    }

    /// The id of the layer of `page` named `name`.
    fn id(session: &Session, page: PageId, name: &str) -> LayerId {
        session
            .project()
            .layers_of(page)
            .expect("its layers")
            .into_iter()
            .find(|layer| layer.name == name)
            .expect("a layer of that name")
            .id
    }

    /// What the export tells of a text it drew into pixels over a copy.
    fn notice(page: PageId, layer: LayerId, name: &str) -> Notice {
        Notice {
            editable_copy: "hidden_plain_styles",
            layer,
            name: name.to_owned(),
            page,
            reason: "word_effects_rasterized",
        }
    }

    /// The style runs of a type layer, as its EngineData writes them.
    fn style_runs(record: &LayerRecord) -> String {
        let data = engine_data(record);
        let runs = data.split("/StyleRun").nth(1).expect("style runs");
        runs.split("/GridInfo").next().unwrap_or(runs).to_owned()
    }

    /// The fill of each style run of a type layer, as EngineData writes it.
    fn fills(record: &LayerRecord) -> Vec<String> {
        style_runs(record)
            .split("/FillColor")
            .skip(1)
            .map(|fill| {
                let values = fill.split("/Values").nth(1).expect("a colour");
                values[..=values.find(']').expect("its end")]
                    .trim()
                    .to_owned()
            })
            .collect()
    }

    /// Whether a layer is a type layer of `WORDS` in plain styles: each run
    /// filled with the gradient's first stop, and no effects.
    fn plain(record: &LayerRecord) -> bool {
        let fills = fills(record);
        words(record) == WORDS
            && !fills.is_empty()
            && fills.iter().all(|fill| fill == FIRST_STOP)
            && !effects(record)
    }

    /// A type layer's number among the file's type layers (`TextIndex`).
    fn index(record: &LayerRecord) -> i32 {
        match type_text(record).descriptor.get("TextIndex") {
            Some(Value::Integer(index)) => *index,
            other => panic!("TextIndex is {other:?}"),
        }
    }

    /// Whether `record` carries layer effects.
    fn effects(record: &LayerRecord) -> bool {
        record.block(b"lfx2").is_some() || record.block(b"lmfx").is_some()
    }

    /// Whether some pixel of `record` is green, as a border is here, which
    /// no fill from red to blue is.
    fn green(record: &LayerRecord) -> bool {
        let channel = |id| record.decode_channel(id, 8, Version::Psd).expect("pixels");
        let (red, green, blue) = (channel(0), channel(1), channel(2));
        red.iter()
            .zip(&green)
            .zip(&blue)
            .any(|((&red, &green), &blue)| green > red.max(blue).saturating_add(64))
    }

    /// A drawing of `WORDS`, borders and all, as a shown pixel layer with no
    /// type data and no effects, over its hidden copy to edit, whose pixels
    /// are its letters alone.
    fn drawn_over_a_copy(copy: &LayerRecord, drawing: &LayerRecord) {
        assert!(!copy.is_visible());
        assert!(plain(copy), "{:?}", fills(copy));
        assert!(!green(copy), "a border is in the copy's pixels");
        assert!(drawing.is_visible());
        assert!(drawing.block(b"TySh").is_none());
        assert!(!effects(drawing));
        assert!(
            green(drawing),
            "the word's border is missing from its pixels"
        );
    }

    #[test]
    fn a_text_s_effects_stay_effects_or_go_into_pixels_over_a_copy() {
        let (_dir, session, page) = page(vec![
            json!({"op": "add_layer", "id": "$words", "page": "$page", "index": 0, "layer": {
                "kind": "text", "name": "Words", "opacity": 0.5, "blend": "multiply",
                "language": "en", "content": bordered(),
                "style": {"chars": {"size": 16.0, "fill_gradient": gradient()}},
                "frame": {"box": {"bounds": {"x": 10.0, "y": 10.0, "width": 180.0, "height": 40.0}}},
            }}),
            json!({"op": "add_layer", "id": "$keep", "page": "$page", "parent": "$words", "index": 0, "layer": {"kind": "mask"}}),
            json!({"op": "write_pixels", "layer": "$keep", "rect": {"x": 0, "y": 0, "width": 100, "height": 200}, "pixels": vec![255u8; 100 * 200]}),
            // Its middle word spaced out, which its style run holds.
            json!({"op": "add_layer", "page": "$page", "index": 1, "layer": {
                "kind": "text", "name": "Shared", "language": "en", "content": [
                    {"insert": "Hello "},
                    {"insert": "brave", "attributes": {"tracking": 12.6}},
                    {"insert": " world"},
                ],
                "style": {"chars": {
                    "size": 16.0, "fill_gradient": gradient(),
                    "stroke": GREEN, "stroke_width": 3.0,
                    "outer_stroke": DARK_GREEN, "outer_stroke_width": 2.0,
                }},
                "frame": {"box": {"bounds": {"x": 10.0, "y": 110.0, "width": 180.0, "height": 40.0}}},
            }}),
            // Locked last: a locked layer takes no more writes.
            json!({"op": "update_layer", "layer": "$words", "patch": {"locked": true}}),
        ]);
        let Some((file, notices)) = exported(&session, page) else {
            return;
        };

        // Bottom to top: the text whose one word has a border of its own,
        // drawn over its copy, then the text whose words share their effects.
        assert_eq!(names(&file), ["Words", "Words", "Shared"]);
        let (copy, drawing) = (&file.layers()[0], &file.layers()[1]);
        drawn_over_a_copy(copy, drawing);
        // Both keep the layer's opacity, blend mode, lock, and mask.
        for record in [copy, drawing] {
            assert_eq!(
                (record.blend_mode, record.opacity),
                (BlendMode::Multiply, 128)
            );
            assert_eq!(
                record.block(b"lspf").expect("a lock").data,
                0x8000_0000u32.to_be_bytes()
            );
            let MaskData::Mask(mask) = &record.mask else {
                panic!("the text's mask is its user mask");
            };
            assert_eq!(
                (
                    mask.rect.left,
                    mask.rect.top,
                    mask.rect.right,
                    mask.rect.bottom
                ),
                (0, 0, 100, 200)
            );
        }
        // One notice, of the text by its own id and name.
        assert_eq!(
            notices,
            [notice(page, id(&session, page, "Words"), "Words")]
        );

        // Effects the words share stay one type layer's: both borders and
        // the gradient in `lmfx`, while its pixels are its letters alone.
        let shared = record(&file, "Shared");
        assert!(shared.is_visible());
        assert_eq!(words(shared), WORDS);
        // Type layers are numbered as they're written, the copy taking the
        // number after those before it.
        assert_eq!((index(copy), index(shared)), (0, 1));
        // A word's letter spacing is its style run's tracking, rounded to a
        // whole thousandth of an em, after AutoLeading and before
        // AutoKerning as Photoshop writes it; the other words have none.
        assert_eq!(engine_data(shared).matches("/Tracking").count(), 1);
        let styles = style_runs(shared);
        let style = styles
            .split("/StyleSheetData")
            .find(|style| style.contains("/Tracking"))
            .expect("a style run with tracking");
        assert!(style.contains("/Tracking 13\n"), "{style}");
        let at = |key| {
            style
                .find(key)
                .unwrap_or_else(|| panic!("no {key} in {style}"))
        };
        assert!(at("/AutoLeading") < at("/Tracking"), "{style}");
        assert!(at("/Tracking") < at("/AutoKerning"), "{style}");
        assert!(shared.block(b"lfx2").is_none());
        let data = &shared
            .block(b"lmfx")
            .expect("effects of more than one kind")
            .data;
        let (lmfx, _) = VersionedDescriptor::parse_prefix(&data[4..]).expect("a descriptor");
        let count = |key| match lmfx.descriptor.get(key) {
            Some(Value::List(items)) => items.len(),
            _ => 0,
        };
        assert_eq!((count("frameFXMulti"), count("gradientFillMulti")), (2, 1));
        assert!(!green(shared), "a border is in the pixels");
    }

    #[test]
    fn a_translation_whose_words_carry_their_own_effects_stays_in_its_block() {
        let (_dir, session, page) = page(vec![
            json!({"op": "add_layer", "id": "$block", "page": "$page", "index": 0, "layer": block("Boxed", 10.0, "箱")}),
            json!({"op": "set_rich_text", "layer": "$block", "spans": bordered()}),
            json!({"op": "update_layer", "layer": "$block", "patch": {"translation": {
                "language": "en",
                "style": {"chars": {"size": 16.0, "fill_gradient": gradient()}},
                "frame": {"box": {"bounds": {"x": 10.0, "y": 100.0, "width": 180.0, "height": 40.0}}},
            }}}),
        ]);
        let Some((file, notices)) = exported(&session, page) else {
            return;
        };

        // Inside the block's folder, bottom to top: its clean-up, its source
        // hidden, then its translation drawn over its copy.
        assert_eq!(
            names(&file),
            ["</Layer group>", "Clean-up", "箱", WORDS, WORDS, "Boxed"]
        );
        assert_eq!(
            record(&file, "Boxed").section_type(),
            SectionType::OpenFolder
        );
        assert!(!record(&file, "箱").is_visible());
        drawn_over_a_copy(&file.layers()[3], &file.layers()[4]);
        // The hidden source is the first type layer, and the copy the next.
        assert_eq!((index(&file.layers()[2]), index(&file.layers()[3])), (0, 1));
        // One notice, of the translation by its block's id and its words.
        assert_eq!(notices, [notice(page, id(&session, page, "Boxed"), WORDS)]);
    }
}
