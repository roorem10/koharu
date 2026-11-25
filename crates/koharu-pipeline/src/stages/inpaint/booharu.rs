//! Booharu's tests of the Inpainting stage's write for a painted area: where
//! its result goes, under all lettering (D17), the limit a soft Remove
//! stroke's result keeps (T23), and how a soft stroke onto a raster or a
//! clean-up (Koharu 0.95) lays its result in. The stage reads an area by
//! drawing the page on the GPU, so these build its work by hand, as
//! `read::area` reads it, give it the limit the stage's marked lines make,
//! and apply the stage's own write.

use std::{collections::BTreeMap, io::Cursor};

use booharu_core::settings::Snapshot;
use image::{GrayImage, ImageFormat, Rgb, RgbImage};
use koharu_scene::{Content, Created, Edit, Hash, LayerId, Op, PageId, Rect, Ref, Session};
use serde_json::{Value, json};

use super::{AreaWork, Target, Work, write};
use crate::{Area, PipelineConfig, booharu::cleanup, stages::action};

/// A page's side, in pixels.
const SIDE: u32 = 16;
/// The whole page's outline, a panel's or a balloon's.
const OUTLINE: &str = "M0 0 L16 0 L16 16 L0 16 Z";

/// A page of `SIDE` pixels square, `$p`, with a white scan, holding the
/// layers the ops given add on it, by their labels.
struct Fixture {
    _dir: tempfile::TempDir,
    session: Session,
    page: PageId,
    ids: BTreeMap<String, LayerId>,
}

impl Fixture {
    fn new(ops: Vec<Value>) -> Self {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        let mut scan = Vec::new();
        RgbImage::from_pixel(SIDE, SIDE, Rgb([255; 3]))
            .write_to(&mut Cursor::new(&mut scan), ImageFormat::Png)
            .expect("a PNG");
        let page = json!({"op": "add_page", "id": "$p", "index": 0,
            "background": Hash::of(&scan)});
        let mut edit = vec![
            Op::AddFile { bytes: scan },
            serde_json::from_value(page).expect("an op"),
        ];
        edit.extend(
            ops.into_iter()
                .map(|op| serde_json::from_value::<Op>(op).expect("an op")),
        );
        let change = session
            .apply(Edit {
                label: "test".to_owned(),
                ops: edit,
            })
            .expect("the page");
        let mut fixture = Self {
            _dir: dir,
            page: change.pages.added[0],
            session,
            ids: BTreeMap::new(),
        };
        fixture.keep(change.created);
        fixture
    }

    /// Keeps the layers `created` by their labels.
    fn keep(&mut self, created: Vec<Created>) {
        self.ids
            .extend(created.into_iter().filter_map(|created| match created {
                Created::Layer {
                    id,
                    label: Some(label),
                } => Some((label, id)),
                _ => None,
            }));
    }

    /// Applies `ops`, keeping the layers they create by their labels.
    fn apply(&mut self, ops: Vec<Op>) {
        let change = self
            .session
            .apply(Edit {
                label: "test".to_owned(),
                ops,
            })
            .expect("applied");
        self.keep(change.created);
    }

    fn id(&self, label: &str) -> LayerId {
        self.ids[label]
    }

    /// The labels of the page's top level layers, from the bottom up, each
    /// unlabelled one as `?`.
    fn order(&self) -> Vec<String> {
        let project = self.session.project();
        project
            .page(self.page)
            .expect("the page")
            .layers
            .iter()
            .map(|layer| {
                self.ids
                    .iter()
                    .find_map(|(label, id)| (*id == layer.id).then(|| label.clone()))
                    .unwrap_or_else(|| "?".to_owned())
            })
            .collect()
    }

    /// Writes the result of a run on `area` the way the Inpainting stage
    /// does, with `config` as the run's: the stage's work as it reads the
    /// area, the limit its marked lines make, and its own write.
    fn inpaint(&mut self, config: &PipelineConfig, area: &Area) {
        self.redrawn(config, area, None, Rgb([255; 3]));
    }

    /// Writes the result of a run on `area`, the model having redrawn it
    /// all `color`, onto `layer`, a raster or a block's clean-up, when there
    /// is one, as a Remove stroke onto one has it (Koharu 0.95), the way the
    /// Inpainting stage does, with `config` as the run's.
    fn redrawn(
        &mut self,
        config: &PipelineConfig,
        area: &Area,
        layer: Option<LayerId>,
        color: Rgb<u8>,
    ) {
        let painted = AreaWork {
            area: area.clone(),
            prompt: None,
            layer,
        };
        let soft = cleanup::soft(config, Some(&painted));
        let (rect, mask) = cleanup::read(&painted.area, (SIDE, SIDE)).expect("on the page");
        let mut work = Work {
            image: RgbImage::from_pixel(SIDE, SIDE, color),
            mask: GrayImage::new(SIDE, SIDE),
            regions: Vec::new(),
            target: Target::Area { rect, mask },
            windows: Vec::new(),
        };
        if let Target::Area { rect, mask } = &mut work.target {
            *mask = cleanup::limit(soft.as_ref(), *rect, std::mem::take(mask));
        }
        let made = action("c2pa.created", "lama", None);
        let ops = write::ops(&self.session, self.page, work, layer, &made, "").expect("the ops");
        self.apply(ops);
    }

    /// The pixel at `x`, `y` on the page of `layer`, a raster or a block's
    /// clean-up or mask, whose pixels sit at the page's own coordinates.
    fn pixel(&self, layer: LayerId, (x, y): (i32, i32)) -> Vec<u8> {
        let at = Rect {
            x,
            y,
            width: 1,
            height: 1,
        };
        self.session.pixels(layer, at).expect("the layer's pixels")
    }

    /// Fills `layer` with `pixel`, a byte a channel, over `rect`.
    fn fill(&mut self, layer: LayerId, rect: Rect, pixel: &[u8]) {
        let pixels = pixel.repeat((rect.width * rect.height) as usize);
        self.apply(vec![Op::WritePixels {
            layer: Ref::from(layer),
            rect,
            pixels,
        }]);
    }

    /// The clean-up and the mask of the block `label`.
    fn cleanup(&self, label: &str) -> (LayerId, LayerId) {
        let project = self.session.project();
        match &project.layer(self.id(label)).expect("the block").content {
            Content::Block(block) => (block.cleanup.id, block.mask.id),
            _ => panic!("{label} is no block"),
        }
    }

    /// The limit the result written last holds over the whole page, a byte
    /// per pixel.
    fn limit(&self) -> GrayImage {
        let page = Rect {
            x: 0,
            y: 0,
            width: SIDE,
            height: SIDE,
        };
        let pixels = self
            .session
            .pixels(self.id("limit"), page)
            .expect("the limit's pixels");
        GrayImage::from_raw(SIDE, SIDE, pixels).expect("a byte per pixel")
    }
}

/// The op adding `layer` as `$label` at `index` among the page's top level
/// layers, or among `parent`'s.
fn add(label: &str, parent: Option<&str>, index: usize, layer: Value) -> Value {
    json!({"op": "add_layer", "id": format!("${label}"), "page": "$p",
        "parent": parent.map(|parent| format!("${parent}")), "index": index, "layer": layer})
}

/// A raster: a Draw stroke's, or, `inpainted`, an earlier result.
fn raster(inpainted: bool) -> Value {
    match inpainted {
        true => json!({"kind": "raster", "attributes": {"c2pa": {
            "inpainting": {"action": "c2pa.created"}}}}),
        false => json!({"kind": "raster"}),
    }
}

/// An area of `width` by `height` at `x`, `y`, each pixel's coverage
/// `value` gives from its column and row in the area.
fn area((x, y): (i32, i32), (width, height): (u32, u32), value: impl Fn(u32, u32) -> u8) -> Area {
    Area {
        rect: Rect {
            x,
            y,
            width,
            height,
        },
        pixels: (0..height)
            .flat_map(|row| (0..width).map(move |column| (column, row)))
            .map(|(column, row)| value(column, row))
            .collect(),
    }
}

/// A hard stroke over the 4 by 4 square at 2, 2.
fn square() -> Area {
    area((2, 2), (4, 4), |_, _| 255)
}

/// Koharu's config, as Generative Fill's, the agent's, and a hard stroke's
/// runs hold it.
fn hard() -> PipelineConfig {
    PipelineConfig::default()
}

/// A soft Remove stroke's config, as the desktop queues it.
fn soft() -> PipelineConfig {
    PipelineConfig {
        booharu: Snapshot {
            soft_limit: true,
            ..Snapshot::default()
        },
        ..PipelineConfig::default()
    }
}

#[test]
fn an_areas_result_goes_under_all_lettering_on_the_page() {
    // An earlier result, then the page's lettering: a block, and a hidden,
    // locked text layer above it.
    let mut lettered = Fixture::new(vec![
        add("earlier", None, 0, raster(true)),
        add(
            "block",
            None,
            1,
            json!({"kind": "block", "source": {"content": "やあ"}}),
        ),
        add(
            "text",
            None,
            2,
            json!({"kind": "text", "content": "Hi",
            "visible": false, "locked": true}),
        ),
    ]);
    lettered.inpaint(&hard(), &square());
    assert_eq!(
        lettered.order(),
        ["earlier", "inpainted", "block", "text"],
        "over the earlier result and under the block"
    );
    // A panel holding a balloon letters the page through it.
    let mut panel = Fixture::new(vec![
        add("draw", None, 0, raster(false)),
        add(
            "panel",
            None,
            1,
            json!({"kind": "panel", "outline": OUTLINE}),
        ),
        add(
            "balloon",
            Some("panel"),
            0,
            json!({"kind": "balloon", "outline": OUTLINE}),
        ),
    ]);
    panel.inpaint(&soft(), &square());
    assert_eq!(panel.order(), ["draw", "inpainted", "panel"]);
}

#[test]
fn on_a_page_with_no_lettering_an_areas_result_goes_on_top_as_koharus_does() {
    let mut drawn = Fixture::new(vec![add("draw", None, 0, raster(false))]);
    drawn.inpaint(&hard(), &square());
    assert_eq!(drawn.order(), ["draw", "inpainted"]);
    let mut empty = Fixture::new(Vec::new());
    empty.inpaint(&hard(), &square());
    assert_eq!(empty.order(), ["inpainted"]);
    // A second result goes over the first, which keeps no label now.
    empty.inpaint(&hard(), &square());
    assert_eq!(empty.order(), ["?", "inpainted"]);
}

#[test]
fn a_soft_strokes_result_keeps_the_strokes_own_coverage_as_its_limit() {
    // A soft stroke reaching past the page's top left corner, its edge
    // fading, with a hole of zero bytes inside it.
    let value = |column: u32, row: u32| match (column, row) {
        (3, 2) => 0,
        _ => (40 * (column.min(row) + 1)).min(255) as u8,
    };
    let stroke = area((-2, -1), (7, 6), value);
    let mut soft_page = Fixture::new(Vec::new());
    soft_page.inpaint(&soft(), &stroke);
    let mut hard_page = Fixture::new(Vec::new());
    hard_page.inpaint(&hard(), &stroke);
    let (soft_limit, hard_limit) = (soft_page.limit(), hard_page.limit());
    for (x, y, pixel) in soft_limit.enumerate_pixels() {
        let own = match (x, y) {
            // The page pixel x, y is the stroke's column x + 2, row y + 1.
            (0..5, 0..5) => value(x + 2, y + 1),
            _ => 0,
        };
        assert_eq!(pixel.0[0], own, "the soft limit at {x}, {y}");
        let whole = if own == 0 { 0 } else { 255 };
        assert_eq!(
            hard_limit.get_pixel(x, y).0[0],
            whole,
            "the hard limit at {x}, {y}"
        );
    }
    assert!(
        soft_limit
            .pixels()
            .any(|pixel| !matches!(pixel.0[0], 0 | 255)),
        "a soft edge shows"
    );
    // The model's own result is whole over the area either way: only its
    // limit is soft.
    let rect = Rect {
        x: 0,
        y: 0,
        width: 5,
        height: 5,
    };
    for fixture in [&soft_page, &hard_page] {
        let drawn = fixture
            .session
            .pixels(fixture.id("inpainted"), rect)
            .expect("the result's pixels");
        assert!(drawn.chunks(4).all(|pixel| pixel == [255; 4]));
    }
}

/// A soft stroke's one row at 4, 4, eight pixels long, its coverage falling
/// from 255 to 64 over its first four pixels and again over its last four,
/// and the row under it, which it doesn't touch.
fn fading() -> Area {
    const ROW: [u8; 8] = [255, 192, 128, 64, 255, 192, 128, 64];
    area((4, 4), (8, 2), |column, row| match row {
        0 => ROW[column as usize],
        _ => 0,
    })
}

/// The left half of the page, 8 pixels wide.
fn left_half() -> Rect {
    Rect {
        x: 0,
        y: 0,
        width: SIDE / 2,
        height: SIDE,
    }
}

/// What a straight RGBA `pixel`, gated by a mask's byte `open`, shows over
/// the page's white scan, a channel each, as the renderer composites it.
fn shown(pixel: &[u8], open: u8) -> [f64; 3] {
    let alpha = f64::from(pixel[3]) / 255.0 * f64::from(open) / 255.0;
    [0, 1, 2].map(|channel| f64::from(pixel[channel]) * alpha + 255.0 * (1.0 - alpha))
}

#[test]
fn a_soft_stroke_onto_a_raster_blends_its_result_in_by_its_coverage() {
    // A raster painted red over the left half of the page, clear on the
    // right, which the stroke crosses from one to the other.
    let new = || {
        let mut fixture = Fixture::new(vec![add("paint", None, 0, raster(false))]);
        fixture.fill(fixture.id("paint"), left_half(), &[255, 0, 0, 255]);
        fixture
    };
    let blue = Rgb([0, 0, 255]);
    let mut soft_page = new();
    let paint = soft_page.id("paint");
    soft_page.redrawn(&soft(), &fading(), Some(paint), blue);
    let at = |x| soft_page.pixel(paint, (x, 4));
    // Over red, the result laid in by its coverage, still opaque.
    assert_eq!(at(4), [0, 0, 255, 255]);
    assert_eq!(at(5), [63, 0, 192, 255]);
    assert_eq!(at(6), [127, 0, 128, 255]);
    assert_eq!(at(7), [191, 0, 64, 255]);
    // Over nothing, the result at its coverage, a soft edge in the raster.
    assert_eq!(at(8), [0, 0, 255, 255]);
    assert_eq!(at(9), [0, 0, 255, 192]);
    assert_eq!(at(10), [0, 0, 255, 128]);
    assert_eq!(at(11), [0, 0, 255, 64]);
    // What the stroke doesn't touch stays as it was.
    assert_eq!(at(3), [255, 0, 0, 255]);
    assert_eq!(soft_page.pixel(paint, (5, 5)), [255, 0, 0, 255]);
    assert_eq!(soft_page.pixel(paint, (9, 5)), [0; 4]);
    assert_eq!(soft_page.order(), ["paint"], "no new raster");
    // A hard stroke's result replaces every pixel it touches whole, as
    // Koharu's write lays it.
    let mut hard_page = new();
    let paint = hard_page.id("paint");
    hard_page.redrawn(&hard(), &fading(), Some(paint), blue);
    for x in 4..12 {
        assert_eq!(hard_page.pixel(paint, (x, 4)), [0, 0, 255, 255], "at {x}");
    }
    assert_eq!(hard_page.pixel(paint, (3, 4)), [255, 0, 0, 255]);
}

#[test]
fn a_soft_stroke_onto_a_clean_up_opens_its_mask_by_its_coverage_and_never_closes_it() {
    // A block whose clean-up is red all over, its mask open on the left half
    // of the page, so the red shows there, and closed on the right.
    let new = || {
        let mut fixture = Fixture::new(vec![add(
            "block",
            None,
            0,
            json!({"kind": "block", "source": {"content": "やあ"}}),
        )]);
        let (cleanup, mask) = fixture.cleanup("block");
        let page = Rect {
            width: SIDE,
            ..left_half()
        };
        fixture.fill(cleanup, page, &[255, 0, 0, 255]);
        fixture.fill(mask, left_half(), &[255]);
        fixture
    };
    let blue = Rgb([0, 0, 255]);
    let mut soft_page = new();
    let (cleanup, mask) = soft_page.cleanup("block");
    let before: Vec<[f64; 3]> = (3..12)
        .map(|x| {
            shown(
                &soft_page.pixel(cleanup, (x, 4)),
                soft_page.pixel(mask, (x, 4))[0],
            )
        })
        .collect();
    soft_page.redrawn(&soft(), &fading(), Some(cleanup), blue);
    let coverage = [0, 255, 192, 128, 64, 255, 192, 128, 64];
    for (at, x) in (3..12).enumerate() {
        let (pixel, open) = (
            soft_page.pixel(cleanup, (x, 4)),
            soft_page.pixel(mask, (x, 4))[0],
        );
        // What shows is the result over what showed, by its coverage.
        let c = f64::from(coverage[at]) / 255.0;
        let wanted = [0, 1, 2].map(|channel| {
            let result = [0.0, 0.0, 255.0][channel];
            result * c + before[at][channel] * (1.0 - c)
        });
        let now = shown(&pixel, open);
        for channel in 0..3 {
            assert!(
                (now[channel] - wanted[channel]).abs() <= 2.0,
                "at {x}: {now:?}, not {wanted:?}"
            );
        }
        // The mask opens by the coverage where it was closed, and stays
        // open where it was.
        let opened = if x < 8 { 255 } else { coverage[at] };
        assert_eq!(open, opened, "the mask at {x}");
    }
    // A hard stroke's result opens the mask wholly wherever it touches, as
    // Koharu's write lays it.
    let mut hard_page = new();
    let (cleanup, mask) = hard_page.cleanup("block");
    hard_page.redrawn(&hard(), &fading(), Some(cleanup), blue);
    for x in 4..12 {
        assert_eq!(hard_page.pixel(cleanup, (x, 4)), [0, 0, 255, 255], "at {x}");
        assert_eq!(hard_page.pixel(mask, (x, 4)), [255], "at {x}");
    }
    assert_eq!(hard_page.pixel(mask, (12, 4)), [0]);
}
