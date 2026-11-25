//! Soft Remove edges (T23): a run whose snapshot asks for a soft limit
//! (`booharu.soft_limit`, which only the desktop sets, for a Remove stroke
//! painted with a soft brush) keeps its area's own coverage as its result's
//! limit, so the result fades in as the stroke's edge does, as the renderer
//! applies the limit's grey through its mask. The model still redraws every
//! pixel the area touches, as Koharu's does, so only what shows of its work
//! is soft. Any other run, Generative Fill's, the agent's, and a hard
//! stroke's among them, keeps Koharu's limit, every pixel the area touches
//! whole.
//!
//! Two marked lines in the Inpainting stage's `inpaint()` reach it: [`soft`]
//! keeps the area before the stage takes it apart, and [`limit`] makes the
//! limit the stage's write gives the result's mask. A stroke onto a raster
//! or a clean-up (Koharu 0.95) has no mask of its own for that limit, so a
//! marked line in the write's `onto()` lays the result in by it instead
//! ([`soften`]): blended into the layer's pixels by the limit's grey, and
//! opening a clean-up's mask by it, never closing what's open.

use image::{GrayImage, Luma};
use koharu_scene::{LayerId, Op, Rect, Ref, Session};

use crate::{Area, PipelineConfig, stages::inpaint::AreaWork};

/// The area whose coverage is the run's soft limit: a copy of `area`'s own,
/// when `config`'s snapshot asks for a soft limit, else none, so Koharu's
/// limit stands.
pub(crate) fn soft(config: &PipelineConfig, area: Option<&AreaWork>) -> Option<Area> {
    if !config.booharu.soft_limit {
        return None;
    }
    area.map(|work| work.area.clone())
}

/// The limit over `rect`, in page pixels, that a run's result keeps: `mask`
/// itself, as the stage read it from the area, every pixel it touches 255,
/// when there's no `soft` area; else, at each pixel `mask` sets, the `soft`
/// area's own coverage there, and 0 wherever `mask` is clear. The area's
/// rectangle may reach past the page or past `rect`, and a pixel it doesn't
/// hold is 0.
pub(crate) fn limit(soft: Option<&Area>, rect: Rect, mask: GrayImage) -> GrayImage {
    let Some(area) = soft else {
        return mask;
    };
    let (width, height) = mask.dimensions();
    GrayImage::from_fn(width, height, |x, y| {
        if mask.get_pixel(x, y).0[0] == 0 {
            return Luma([0]);
        }
        let page = (
            i64::from(rect.x) + i64::from(x),
            i64::from(rect.y) + i64::from(y),
        );
        Luma([coverage(area, page)])
    })
}

/// `area`'s own coverage at the page pixel `x`, `y`, or 0 outside its
/// rectangle.
fn coverage(area: &Area, (x, y): (i64, i64)) -> u8 {
    let (column, row) = (x - i64::from(area.rect.x), y - i64::from(area.rect.y));
    let (width, height) = (i64::from(area.rect.width), i64::from(area.rect.height));
    if !(0..width).contains(&column) || !(0..height).contains(&row) {
        return 0;
    }
    usize::try_from(row * width + column)
        .ok()
        .and_then(|at| area.pixels.get(at))
        .copied()
        .unwrap_or(0)
}

/// Lays a soft limit's result onto `layer` as `ops` hold it, Koharu's write
/// of `drawn`, the result's RGBA pixels over `rect` of the page, onto the
/// layer, and of `area`, the limit over `rect`, onto `mask`, the block's
/// mask when `layer` is a block's clean-up, which opens it there; Koharu's
/// write takes the result whole wherever the limit is above 0. A limit of
/// 0s and 255s, a hard stroke's, any but a soft Remove stroke's (T23),
/// stays so, as blending gives the same bytes there. A soft one's edge is
/// laid in by its grey instead ([`blended`]): what the layer and its mask
/// show there afterwards is the result over what they showed, in that
/// proportion, so a soft edge stays soft on a raster, where it has no mask
/// of its own, and a clean-up's mask opens by it but never closes.
pub(crate) fn soften(
    session: &Session,
    ops: &mut [Op],
    (layer, mask): (LayerId, Option<LayerId>),
    rect: Rect,
    area: &GrayImage,
    drawn: &[u8],
) -> anyhow::Result<()> {
    if area.pixels().all(|pixel| matches!(pixel.0[0], 0 | 255)) {
        return Ok(());
    }
    let size = area.as_raw().len();
    // Each layer's pixels over `rect`, in its own coordinates, as Koharu's
    // write reads them.
    let held = |id: LayerId| -> anyhow::Result<Vec<u8>> {
        let offset = session.tiles(id, Some(rect))?.offset;
        let own = Rect {
            x: rect.x - offset.x,
            y: rect.y - offset.y,
            ..rect
        };
        Ok(session.pixels(id, own)?)
    };
    let (old, shown) = (held(layer)?, mask.map(&held).transpose()?);
    // A layer that isn't RGBA, or a mask that isn't a byte a pixel, Koharu's
    // write lays as it does.
    if old.len() != size * 4
        || drawn.len() != size * 4
        || shown.as_ref().is_some_and(|shown| shown.len() != size)
    {
        return Ok(());
    }
    let (pixels, opened) = blended(&old, shown.as_deref(), area.as_raw(), drawn);
    for op in ops {
        let Op::WritePixels {
            layer: written,
            pixels: bytes,
            ..
        } = op
        else {
            continue;
        };
        if *written == Ref::from(layer) {
            bytes.clone_from(&pixels);
        } else if let (Some(mask), Some(opened)) = (mask, &opened)
            && *written == Ref::from(mask)
        {
            bytes.clone_from(opened);
        }
    }
    Ok(())
}

/// `old`, a layer's straight RGBA pixels, with `drawn`, a result's, laid
/// over them by `coverage`, a byte a pixel, and `mask`, the layer's mask's
/// bytes, which gate what it shows, opened by it: what the two show
/// afterwards is `drawn` over what they showed, at each pixel's coverage, as
/// the renderer multiplies a clean-up's alpha by its mask. With no mask, it
/// is `drawn` composited over `old`, as Photoshop's Normal blend lays a
/// layer at that opacity. A mask opens as much again of what it held closed
/// as the coverage takes, so it never closes; a layer's alpha is what shows
/// over the mask's opening. Where the coverage is 0 both stay as they are,
/// and where it's 255 they hold `drawn` and open wholly, as Koharu's write
/// leaves them.
fn blended(
    old: &[u8],
    mask: Option<&[u8]>,
    coverage: &[u8],
    drawn: &[u8],
) -> (Vec<u8>, Option<Vec<u8>>) {
    let mut pixels = old.to_vec();
    let mut opened = mask.map(<[u8]>::to_vec);
    let unit = |byte: u8| f64::from(byte) / 255.0;
    let byte = |value: f64| (value * 255.0).round().clamp(0.0, 255.0) as u8;
    for (at, &covered) in coverage.iter().enumerate() {
        let new = &drawn[at * 4..at * 4 + 4];
        let c = unit(covered) * unit(new[3]);
        if c == 0.0 {
            continue;
        }
        let pixel = &mut pixels[at * 4..at * 4 + 4];
        let closed = mask.map_or(1.0, |mask| unit(mask[at]));
        // What the layer showed through its mask, and what it shows now.
        let was = unit(pixel[3]) * closed;
        let shows = c + was * (1.0 - c);
        let open = c + closed * (1.0 - c);
        for channel in 0..3 {
            let mixed = c * unit(new[channel]) + was * (1.0 - c) * unit(pixel[channel]);
            pixel[channel] = byte(mixed / shows);
        }
        pixel[3] = byte(shows / open);
        if let Some(opened) = &mut opened {
            opened[at] = byte(open);
        }
    }
    (pixels, opened)
}

/// The rectangle and the mask over it that the Inpainting stage reads from
/// `area` on a `width` by `height` page, as its `read::area` does, which
/// draws the page on the GPU first: every pixel the area touches on the
/// page 255, over the rectangle covering them; none when it touches none.
#[cfg(test)]
pub(crate) fn read(area: &Area, (width, height): (u32, u32)) -> Option<(Rect, GrayImage)> {
    let columns = i64::from(area.rect.width.max(1));
    let touched: Vec<(i64, i64)> = area
        .pixels
        .iter()
        .enumerate()
        .filter(|(_, value)| **value != 0)
        .map(|(at, _)| {
            let at = at as i64;
            (
                i64::from(area.rect.x) + at % columns,
                i64::from(area.rect.y) + at / columns,
            )
        })
        .filter(|(x, y)| (0..i64::from(width)).contains(x) && (0..i64::from(height)).contains(y))
        .collect();
    let x0 = touched.iter().map(|(x, _)| *x).min()?;
    let y0 = touched.iter().map(|(_, y)| *y).min()?;
    let x1 = touched.iter().map(|(x, _)| *x).max()?;
    let y1 = touched.iter().map(|(_, y)| *y).max()?;
    let rect = Rect {
        x: x0 as i32,
        y: y0 as i32,
        width: (x1 - x0 + 1) as u32,
        height: (y1 - y0 + 1) as u32,
    };
    let mask = GrayImage::from_fn(rect.width, rect.height, |x, y| {
        let page = (x0 + i64::from(x), y0 + i64::from(y));
        Luma([if touched.contains(&page) { 255 } else { 0 }])
    });
    Some((rect, mask))
}

#[cfg(test)]
mod tests {
    use booharu_core::settings::Snapshot;

    use super::*;

    /// An area of `width` by `height` at `x`, `y`, each pixel's coverage
    /// `value` gives from its column and row in the area.
    fn area(
        (x, y): (i32, i32),
        (width, height): (u32, u32),
        value: impl Fn(u32, u32) -> u8,
    ) -> Area {
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

    #[test]
    fn with_no_soft_area_the_limit_is_the_mask_as_read() {
        // A stroke's antialiased edge, read as every pixel it touches.
        let stroke = area((1, 1), (4, 3), |column, row| match (column, row) {
            (0, _) | (3, _) => 64,
            (_, 1) => 255,
            _ => 0,
        });
        let (rect, mask) = read(&stroke, (16, 16)).expect("on the page");
        assert_eq!(limit(None, rect, mask.clone()), mask);
        assert!(mask.pixels().all(|pixel| matches!(pixel.0[0], 0 | 255)));
    }

    #[test]
    fn a_soft_limit_is_the_areas_own_coverage_where_the_mask_is_set() {
        // A soft stroke reaching past the page's top left corner, with a
        // hole of zero bytes inside it.
        let value = |column: u32, row: u32| match (column, row) {
            (2, 2) => 0,
            _ => (10 * row + column + 1) as u8,
        };
        let stroke = area((-2, -1), (5, 4), value);
        let (rect, mask) = read(&stroke, (8, 8)).expect("on the page");
        assert_eq!(
            rect,
            Rect {
                x: 0,
                y: 0,
                width: 3,
                height: 3
            },
            "the pixels it touches on the page"
        );
        let soft = limit(Some(&stroke), rect, mask.clone());
        assert_eq!(soft.dimensions(), mask.dimensions());
        for (x, y, pixel) in soft.enumerate_pixels() {
            // The page pixel x, y is the area's column x + 2, row y + 1.
            let own = value(x + 2, y + 1);
            assert_eq!(pixel.0[0], own, "at {x}, {y}");
            assert_eq!(mask.get_pixel(x, y).0[0] != 0, own != 0, "at {x}, {y}");
        }
        assert_eq!(soft.get_pixel(0, 1).0[0], 0, "the hole stays clear");
        // A rectangle reaching past the area's holds 0 there, as the area
        // doesn't hold it, even where a mask is set.
        let wider = Rect {
            x: 0,
            y: 0,
            width: 4,
            height: 4,
        };
        let all = GrayImage::from_pixel(4, 4, Luma([255]));
        let limited = limit(Some(&stroke), wider, all);
        assert_eq!(
            limited.get_pixel(3, 0).0[0],
            0,
            "past the area's right side"
        );
        assert_eq!(limited.get_pixel(0, 3).0[0], 0, "past the area's bottom");
        assert_eq!(limited.get_pixel(2, 2).0[0], value(4, 3));
    }

    #[test]
    fn only_a_snapshot_asking_for_one_keeps_the_area() {
        let work = AreaWork {
            area: area((0, 0), (2, 2), |_, _| 128),
            prompt: None,
            layer: None,
        };
        // Koharu's config, as Generative Fill's and the agent's runs hold it.
        let hard = PipelineConfig::default();
        assert_eq!(soft(&hard, Some(&work)), None);
        let asked = PipelineConfig {
            booharu: Snapshot {
                soft_limit: true,
                ..Snapshot::default()
            },
            ..PipelineConfig::default()
        };
        assert_eq!(soft(&asked, Some(&work)), Some(work.area.clone()));
        assert_eq!(soft(&asked, None), None, "a page run has no area");
    }
}
