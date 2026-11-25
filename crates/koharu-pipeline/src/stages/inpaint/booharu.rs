//! Booharu's tests of the Inpainting stage's write for a painted area: where
//! its result goes, under all lettering (D17), and the limit a soft Remove
//! stroke's result keeps (T23). The stage reads an area by drawing the page
//! on the GPU, so these build its work by hand, as `read::area` reads it,
//! give it the limit the stage's marked lines make, and apply the stage's own
//! write.

use std::{collections::BTreeMap, io::Cursor};

use booharu_core::settings::Snapshot;
use image::{GrayImage, ImageFormat, Rgb, RgbImage};
use koharu_scene::{Created, Edit, Hash, LayerId, Op, PageId, Rect, Session};
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
        let painted = AreaWork {
            area: area.clone(),
            prompt: None,
            layer: None,
        };
        let soft = cleanup::soft(config, Some(&painted));
        let (rect, mask) = cleanup::read(&painted.area, (SIDE, SIDE)).expect("on the page");
        let mut work = Work {
            image: RgbImage::from_pixel(SIDE, SIDE, Rgb([255; 3])),
            mask: GrayImage::new(SIDE, SIDE),
            regions: Vec::new(),
            target: Target::Area { rect, mask },
            windows: Vec::new(),
        };
        if let Target::Area { rect, mask } = &mut work.target {
            *mask = cleanup::limit(soft.as_ref(), *rect, std::mem::take(mask));
        }
        let made = action("c2pa.created", "lama", None);
        let ops = write::ops(&self.session, self.page, work, None, &made, "").expect("the ops");
        self.apply(ops);
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
