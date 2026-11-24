//! The shapes `booharu_clean_areas` cleans, on pages a project made in a
//! temporary folder holds: which pixels a rectangle and a polygon take, by
//! their centres and by nonzero winding, joined, clipped to the page, and
//! the run cleaning them, and what's refused.

use std::io::Cursor;

use image::{GrayImage, ImageFormat};
use koharu_scene::{Edit, Hash, Op, Session};
use kurbo::{BezPath, Shape as _};
use serde_json::json;

use super::*;

/// A project in a temporary folder holding a page of `WIDTH` by `HEIGHT`
/// pixels with a scan, and one of that size without.
struct Fixture {
    _dir: tempfile::TempDir,
    session: Session,
    scanned: PageId,
    blank: PageId,
}

const WIDTH: u32 = 20;
const HEIGHT: u32 = 16;

impl Fixture {
    fn new() -> Result<Self> {
        let dir = tempfile::tempdir()?;
        let mut session = Session::create(dir.path().join("test.khr"))?;
        let mut scan = Vec::new();
        GrayImage::from_pixel(WIDTH, HEIGHT, image::Luma([200]))
            .write_to(&mut Cursor::new(&mut scan), ImageFormat::Png)?;
        let file = Hash::of(&scan);
        let pages = [
            json!({"op": "add_page", "index": 0, "background": file}),
            json!({"op": "add_page", "index": 1,
                "size": {"width": WIDTH, "height": HEIGHT}}),
        ];
        let mut ops = vec![Op::AddFile { bytes: scan }];
        for page in pages {
            ops.push(serde_json::from_value(page)?);
        }
        let change = session.apply(Edit {
            label: "test".to_owned(),
            ops,
        })?;
        let [scanned, blank] = change.pages.added[..] else {
            bail!("two pages: {:?}", change.pages.added);
        };
        Ok(Self {
            _dir: dir,
            session,
            scanned,
            blank,
        })
    }

    /// The area `shapes` cover on the page with a scan.
    fn area(&self, shapes: &Shapes) -> Result<Area> {
        area(&self.session.project(), self.scanned, shapes)
    }

    /// Why `shapes` are refused on the page with a scan.
    fn refused(&self, shapes: &Shapes) -> String {
        format!("{:#}", self.area(shapes).expect_err("refused"))
    }
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rectangle {
    Rectangle {
        x,
        y,
        width,
        height,
    }
}

fn polygon(corners: &[(f64, f64)]) -> Vec<Point> {
    corners.iter().map(|&(x, y)| Point { x, y }).collect()
}

fn rects(rects: Vec<Rectangle>) -> Shapes {
    Shapes {
        rects,
        polygons: Vec::new(),
    }
}

fn polygons(polygons: Vec<Vec<Point>>) -> Shapes {
    Shapes {
        rects: Vec::new(),
        polygons,
    }
}

/// The page pixels `area` cleans, as (x, y).
fn cleaned(area: &Area) -> Vec<(i32, i32)> {
    let width = area.rect.width as usize;
    let mut pixels = Vec::new();
    for (at, &value) in area.pixels.iter().enumerate() {
        assert!(value == 0 || value == 255, "{value} is neither 0 nor 255");
        if value != 0 {
            pixels.push((
                area.rect.x + (at % width) as i32,
                area.rect.y + (at / width) as i32,
            ));
        }
    }
    pixels
}

/// The page pixels whose centres lie inside `path` by nonzero winding, as
/// kurbo counts it, which none of the polygons tested puts on an edge.
fn contained(path: &BezPath) -> Vec<(i32, i32)> {
    let mut pixels = Vec::new();
    for y in 0..HEIGHT as i32 {
        for x in 0..WIDTH as i32 {
            if path.contains(kurbo::Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5)) {
                pixels.push((x, y));
            }
        }
    }
    pixels
}

fn path(corners: &[(f64, f64)]) -> BezPath {
    let mut path = BezPath::new();
    path.move_to(corners[0]);
    for &corner in &corners[1..] {
        path.line_to(corner);
    }
    path.close_path();
    path
}

#[test]
fn a_rect_cleans_exactly_the_pixels_whose_centres_it_holds() -> Result<()> {
    let fixture = Fixture::new()?;
    let area = fixture.area(&rects(vec![rect(3.0, 2.0, 4.0, 3.0)]))?;
    assert_eq!(
        area.rect,
        Rect {
            x: 3,
            y: 2,
            width: 4,
            height: 3,
        }
    );
    assert_eq!(area.pixels, vec![255; 12]);
    assert_eq!(covered(&area), 12);
    // Edges between pixels' centres take the pixels whose centres they hold:
    // 3.5 and 4.5 across, 1.5 and 2.5 down.
    let area = fixture.area(&rects(vec![rect(2.6, 1.4, 2.0, 1.2)]))?;
    assert_eq!(cleaned(&area), [(3, 1), (4, 1), (3, 2), (4, 2)]);
    // A centre on a left or top edge is in, on a right or bottom one out, so
    // rects sharing an edge share no pixel.
    let area = fixture.area(&rects(vec![rect(0.5, 0.5, 2.0, 1.0)]))?;
    assert_eq!(cleaned(&area), [(0, 0), (1, 0)]);
    let beside = fixture.area(&rects(vec![rect(2.5, 0.5, 1.0, 1.0)]))?;
    assert_eq!(cleaned(&beside), [(2, 0)]);
    Ok(())
}

#[test]
fn a_triangle_takes_the_pixels_whose_centres_it_holds() -> Result<()> {
    let fixture = Fixture::new()?;
    // Its long side, x + y = 8, runs through centres, which it leaves out as
    // a right side: each row r holds the 7 - r pixels left of it.
    let area = fixture.area(&polygons(vec![polygon(&[
        (0.0, 0.0),
        (8.0, 0.0),
        (0.0, 8.0),
    ])]))?;
    let expected: Vec<(i32, i32)> = (0..7)
        .flat_map(|y| (0..7 - y).map(move |x| (x, y)))
        .collect();
    assert_eq!(cleaned(&area), expected);
    assert_eq!(covered(&area), 28);
    assert_eq!(
        area.rect,
        Rect {
            x: 0,
            y: 0,
            width: 7,
            height: 7,
        }
    );
    // Any triangle, by the centres kurbo finds inside it.
    let corners = [(1.3, 0.7), (17.1, 4.2), (5.4, 14.9)];
    let area = fixture.area(&polygons(vec![polygon(&corners)]))?;
    let inside = contained(&path(&corners));
    assert!(inside.len() > 50, "{}", inside.len());
    assert_eq!(cleaned(&area), inside);
    Ok(())
}

#[test]
fn a_polygon_crossing_itself_fills_by_nonzero_winding() -> Result<()> {
    let fixture = Fixture::new()?;
    // A five-pointed star, drawn by every second corner of a pentagon: its
    // middle, wound around twice, is filled too, which even-odd would leave
    // empty.
    let star: Vec<(f64, f64)> = (0..5)
        .map(|k| {
            let angle = (-90.0 + 144.0 * f64::from(k)).to_radians();
            (10.0 + 7.3 * angle.cos(), 8.0 + 7.3 * angle.sin())
        })
        .collect();
    let area = fixture.area(&polygons(vec![polygon(&star)]))?;
    let outline = path(&star);
    assert_eq!(cleaned(&area), contained(&outline));
    let middle = kurbo::Point::new(10.5, 8.5);
    assert_eq!(outline.winding(middle).abs(), 2);
    assert!(cleaned(&area).contains(&(10, 8)));
    // A square gone round twice is filled whole.
    let twice = [
        (2.0, 2.0),
        (6.0, 2.0),
        (6.0, 6.0),
        (2.0, 6.0),
        (2.0, 2.0),
        (6.0, 2.0),
        (6.0, 6.0),
        (2.0, 6.0),
    ];
    let area = fixture.area(&polygons(vec![polygon(&twice)]))?;
    assert_eq!(covered(&area), 16);
    // A bow tie, its two loops wound opposite ways, fills both.
    let bow = [(0.0, 0.0), (8.0, 8.0), (8.0, 0.0), (0.0, 8.0)];
    let area = fixture.area(&polygons(vec![polygon(&bow)]))?;
    assert_eq!(cleaned(&area), contained(&path(&bow)));
    assert!(cleaned(&area).contains(&(1, 4)) && cleaned(&area).contains(&(6, 4)));
    Ok(())
}

#[test]
fn shapes_are_joined_and_never_cut_each_other() -> Result<()> {
    let fixture = Fixture::new()?;
    // A rect, clockwise, and a square drawn the other way round, overlapping
    // by 2 by 2, with a polygon away from both: one area holding all three.
    let shapes = Shapes {
        rects: vec![rect(0.0, 0.0, 4.0, 4.0)],
        polygons: vec![
            polygon(&[(2.0, 2.0), (2.0, 6.0), (6.0, 6.0), (6.0, 2.0)]),
            polygon(&[(10.0, 10.0), (12.0, 10.0), (12.0, 12.0), (10.0, 12.0)]),
        ],
    };
    let area = fixture.area(&shapes)?;
    assert_eq!(covered(&area), 16 + 16 - 4 + 4);
    assert_eq!(
        area.rect,
        Rect {
            x: 0,
            y: 0,
            width: 12,
            height: 12,
        }
    );
    let pixels = cleaned(&area);
    assert!(pixels.contains(&(3, 3)), "the overlap stays");
    assert!(!pixels.contains(&(8, 8)), "nothing between them");
    Ok(())
}

#[test]
fn shapes_are_clipped_at_the_pages_edges() -> Result<()> {
    let fixture = Fixture::new()?;
    let area = fixture.area(&rects(vec![rect(-5.0, -5.0, 8.0, 7.0)]))?;
    assert_eq!(
        area.rect,
        Rect {
            x: 0,
            y: 0,
            width: 3,
            height: 2,
        }
    );
    assert_eq!(covered(&area), 6);
    // A triangle reaching past the right and bottom edges, and far past.
    let corners = [(15.0, 10.0), (1e9, 10.0), (15.0, 1e9)];
    let area = fixture.area(&polygons(vec![polygon(&corners)]))?;
    assert_eq!(
        area.rect,
        Rect {
            x: 15,
            y: 10,
            width: 5,
            height: 6,
        }
    );
    assert_eq!(covered(&area), 30);
    // The whole page, from a rect larger than it.
    let area = fixture.area(&rects(vec![rect(-1e300, -1e300, 2e300, 2e300)]))?;
    assert_eq!(
        (area.rect, covered(&area)),
        (
            Rect {
                x: 0,
                y: 0,
                width: WIDTH,
                height: HEIGHT,
            },
            u64::from(WIDTH * HEIGHT)
        )
    );
    Ok(())
}

#[test]
fn what_cleans_nothing_or_isnt_a_shape_is_refused() -> Result<()> {
    let fixture = Fixture::new()?;
    assert!(
        fixture
            .refused(&Shapes::default())
            .contains("there are no shapes to clean"),
    );
    let two = polygons(vec![polygon(&[(0.0, 0.0), (5.0, 5.0)])]);
    assert!(
        fixture
            .refused(&two)
            .contains("polygon 1 has 2 points; a polygon needs at least 3")
    );
    for shapes in [
        rects(vec![rect(f64::NAN, 0.0, 4.0, 4.0)]),
        rects(vec![rect(0.0, 0.0, f64::INFINITY, 4.0)]),
        // Finite, but its right edge isn't.
        rects(vec![rect(f64::MAX, 0.0, f64::MAX, 4.0)]),
    ] {
        assert!(fixture.refused(&shapes).contains("rect 1's"), "{shapes:?}");
    }
    let infinite = Shapes {
        rects: vec![rect(0.0, 0.0, 4.0, 4.0)],
        polygons: vec![
            polygon(&[(0.0, 0.0), (5.0, 0.0), (5.0, 5.0)]),
            polygon(&[(0.0, 0.0), (5.0, f64::NEG_INFINITY), (5.0, 5.0)]),
        ],
    };
    assert!(
        fixture
            .refused(&infinite)
            .contains("point 2 of polygon 2 isn't at finite numbers")
    );
    // Shapes holding no pixel's centre on the page: off it, with no width,
    // flat, or too thin to reach a centre.
    for nothing in [
        rects(vec![rect(30.0, 0.0, 5.0, 5.0)]),
        rects(vec![rect(-6.0, -6.0, 5.0, 5.0)]),
        rects(vec![rect(2.0, 2.0, 0.0, 5.0), rect(2.0, 2.0, -3.0, 5.0)]),
        rects(vec![rect(2.6, 2.0, 0.8, 5.0)]),
        polygons(vec![polygon(&[(1.0, 1.0), (5.0, 5.0), (9.0, 9.0)])]),
    ] {
        assert!(
            fixture
                .refused(&nothing)
                .contains("the shapes cover no pixel of the page, which is 20 by 16 pixels"),
            "{nothing:?}"
        );
    }
    // A page with no scan has nothing to clean, and one gone isn't there.
    let shapes = rects(vec![rect(0.0, 0.0, 4.0, 4.0)]);
    let project = fixture.session.project();
    let error = area(&project, fixture.blank, &shapes).expect_err("no scan");
    assert!(
        error.to_string().contains("this page has no scan"),
        "{error}"
    );
    let gone: PageId = "00000000-0000-0000-0000-000000000009".parse()?;
    assert!(area(&project, gone, &shapes).is_err());
    Ok(())
}

#[test]
fn the_run_cleans_the_area_alone_at_once_with_the_saved_settings() -> Result<()> {
    let fixture = Fixture::new()?;
    let area = fixture.area(&rects(vec![rect(1.0, 1.0, 2.0, 2.0)]))?;
    let mut config = PipelineConfig::default();
    config.translator.target = "fr".parse()?;
    let request = request(fixture.scanned, area.clone(), config);
    assert_eq!(
        request.target,
        Target::Area {
            page: fixture.scanned,
            area,
            prompt: None,
        }
    );
    assert_eq!(request.stages, BTreeSet::from([Stage::Inpaint]));
    let config = request.config.expect("the saved settings");
    assert_eq!(config.translator.target.to_string(), "fr");
    // Its limit stays the area's own, as the Remove brush's hard strokes do.
    assert!(!config.booharu.soft_limit && !config.booharu.redo);
    Ok(())
}
