//! Fixing a page's text blocks by hand, which the desktop's commands and the
//! agent's tools share: a box drawn with the O tool becomes a block, a
//! balloon's text blocks are joined into one, a block is split in two, and a
//! block or a text takes a reviewer's note, each as one undo step.
//!
//! Each builder reads the project as the session holds it and answers the
//! edit, which the caller applies under the same lock. One that would change
//! a locked layer, or a layer inside a locked one, refuses with
//! [`Refusal::Locked`] and no edit, naming the locked layer, where the scene
//! would fail the edit naming only an op.

use std::collections::{BTreeSet, HashSet};

use anyhow::{Context as _, Result, bail, ensure};
use booharu_core::{
    attrs::{self, Attr, Auto, Lettering, QcOk, ReadNote, ReviewNote},
    settings::OcrBoxMode,
};
use image::{Rgb, RgbImage};
use koharu_pipeline::{RunId, Stage};
use koharu_scene::{
    Block, Bounds, Change, Content, Created, Direction, Edit, Frame, Layer, LayerId, Op, Page,
    PageId, PixelFormat, Project, Rect, Ref, Role, Session, Text, Tile, Transform,
};
use kurbo::{Affine, BezPath, PathEl, Point, Shape as _};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// The undo step's label for a box drawn with the O tool.
pub const ADD_BOX: &str = "booharu_add_box";
/// The undo step's label for joining one balloon's text blocks.
pub const JOIN_BALLOON: &str = "booharu_join_balloon";
/// The undo step's label for joining every split balloon of a page.
pub const JOIN_PAGE: &str = "booharu_join_page";
/// The undo step's label for splitting a block in two.
pub const SPLIT_BLOCK: &str = "booharu_split_block";
/// The undo step's label for setting or clearing a reviewer's note.
pub const SET_REVIEW_NOTE: &str = "booharu_set_review_note";

/// The label [`add_box`] gives the block it adds.
const BOX: &str = "box";
/// The label [`split_block`] gives the block's copy, its second half.
const HALF: &str = "half";

/// Why a box action changes nothing, which the interface tells apart from a
/// failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum Refusal {
    /// The box holds no ink to read: the page is nearly one colour there,
    /// or shows only other blocks' lettering, which reading leaves out.
    #[error("there's nothing to read in this box")]
    NothingToRead,
    /// The action would change `layer`, or a layer inside it, and `layer`
    /// is locked.
    #[error("layer {layer} is locked, so this can't change it or what it holds; unlock it first")]
    Locked { layer: LayerId },
}

/// A balloon whose text is split over two or more text blocks, which
/// [`join_balloon`] joins into one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct SplitBalloon {
    pub page: PageId,
    pub balloon: LayerId,
    /// Its text blocks, in reading order; sound effects are left out.
    pub blocks: Vec<LayerId>,
}

/// A block split in two, and the run reading and translating both halves.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct SplitBlock {
    pub change: Change,
    /// The halves in reading order: the block itself, then its copy.
    pub halves: Vec<LayerId>,
    pub run: RunId,
}

/// The stages a box drawn in `mode` goes through, run on its block alone.
pub fn stages(mode: OcrBoxMode) -> BTreeSet<Stage> {
    match mode {
        OcrBoxMode::Read => BTreeSet::from([Stage::Recognize]),
        OcrBoxMode::ReadTranslate => BTreeSet::from([Stage::Recognize, Stage::Translate]),
        OcrBoxMode::ReadTranslateClean => {
            BTreeSet::from([Stage::Recognize, Stage::Inpaint, Stage::Translate])
        }
    }
}

/// The edit adding the block a box drawn over `rect` of `page`, in page
/// pixels, makes, labelled [`ADD_BOX`]; [`added_box`] finds the block in the
/// edit's change. It decodes the page's scan, so the caller holds the
/// session's lock that long.
///
/// The box is cut to the page. It is refused as [`Refusal::NothingToRead`]
/// when the Recognition stage would find no ink there. The block is a text
/// with no source wording yet, in no language, its source's frame the box,
/// vertical when the box is at least 1.15 times as tall as it is wide, with
/// no translation and no provenance, so it counts as made by hand. It goes in
/// the smallest balloon holding at least 90% of the box, else the smallest
/// panel holding its centre, else on the page, where it is read in order
/// among what is there. Only `ReadTranslateClean` gives it a mask, the whole
/// box: the other modes leave it empty, so reading never erases the art.
pub fn add_box(session: &Session, page: PageId, rect: Bounds, mode: OcrBoxMode) -> Result<Edit> {
    let project = session.project();
    let found = project.page(page)?;
    ensure!(
        [rect.x, rect.y, rect.width, rect.height]
            .iter()
            .all(|value| value.is_finite())
            && rect.width > 0.0
            && rect.height > 0.0,
        "a box needs a finite position and a positive width and height"
    );
    let size = kurbo::Rect::new(
        0.0,
        0.0,
        f64::from(found.size.width),
        f64::from(found.size.height),
    );
    let rect = kurbo::Rect::from(rect).intersect(size);
    ensure!(rect.area() > 0.0, "the box lies outside the page");
    let masked = mode == OcrBoxMode::ReadTranslateClean;
    if unreadable(session, &project, found, rect, masked)? {
        return Err(Refusal::NothingToRead.into());
    }
    let parent = holder(&project, page, rect)?;
    if let Some(parent) = parent {
        unlocked(&project, parent.id)?;
    }
    let siblings = parent.map_or(&found.layers, |parent| &parent.children);
    let vertical = rect.height() >= 1.15 * rect.width();
    let direction = if vertical {
        Direction::Vertical
    } else {
        Direction::Horizontal
    };
    let mut ops = vec![serde_json::from_value(json!({
        "op": "add_layer",
        "id": format!("${BOX}"),
        "page": page,
        "parent": parent.map(|parent| parent.id),
        "index": reading_index(siblings, rect, vertical),
        "layer": {"kind": "block", "role": "text", "source": {
            "frame": {"box": {"bounds": bounds(rect)}},
            "direction": direction,
            "language": "und",
        }},
    }))?];
    if masked && let Some(whole) = Rect::covering(rect, found.size.width, found.size.height) {
        ops.push(Op::WritePixels {
            layer: label(&format!("{BOX}.mask"))?,
            rect: whole,
            pixels: vec![u8::MAX; whole.width as usize * whole.height as usize],
        });
    }
    Ok(Edit {
        label: ADD_BOX.to_owned(),
        ops,
    })
}

/// The block an [`add_box`] edit added, once applied as `change`.
pub fn added_box(change: &Change) -> Result<LayerId> {
    created(change, BOX)
}

/// Every balloon on `page`, or on every page in order, whose text is split
/// over two or more text blocks.
pub fn find_split_balloons(project: &Project, page: Option<PageId>) -> Result<Vec<SplitBalloon>> {
    let pages = match page {
        Some(page) => vec![project.page(page)?.id],
        None => project.pages().map(|page| page.id).collect(),
    };
    let mut found = Vec::new();
    for page in pages {
        for group in split(project, page)? {
            found.push(SplitBalloon {
                page,
                balloon: group.balloon.id,
                blocks: group.blocks.iter().map(|(id, _)| *id).collect(),
            });
        }
    }
    Ok(found)
}

/// The edit joining the text blocks of the balloon holding `block` into the
/// first of them, labelled [`JOIN_BALLOON`]. The first keeps its place and
/// its id; its source's frame becomes the box around every block's source,
/// and its source wording and its translation every block's that isn't
/// blank, a line each, the translation placed in its balloon as the
/// Translation stage places one. The others' masks and clean-ups merge into
/// its own, so the lettering they cleaned stays clean, and they go. A
/// balloon holding one text block has nothing to join.
pub fn join_balloon(session: &Session, block: LayerId) -> Result<Edit> {
    let project = session.project();
    let layer = project.layer(block)?;
    let Content::Block(found) = &layer.content else {
        bail!("layer {block} isn't a block");
    };
    ensure!(
        found.role == Role::Text,
        "block {block} is a sound effect, which isn't joined with a balloon's text"
    );
    let (page, holder) = project.parent(block)?;
    let balloon = holder
        .filter(|holder| matches!(holder.content, Content::Balloon(_)))
        .with_context(|| format!("block {block} isn't in a balloon, so there's nothing to join"))?;
    let group = Group {
        balloon,
        blocks: texts(balloon),
    };
    ensure!(
        group.blocks.len() >= 2,
        "balloon {} holds only one text block, so there's nothing to join",
        balloon.id
    );
    Ok(Edit {
        label: JOIN_BALLOON.to_owned(),
        ops: joined(session, &project, page.id, &[group])?,
    })
}

/// The edit joining every balloon of `page` whose text is split over two or
/// more text blocks, as [`join_balloon`] joins one, labelled [`JOIN_PAGE`].
/// A locked block in any of them refuses them all.
pub fn join_page(session: &Session, page: PageId) -> Result<Edit> {
    let project = session.project();
    let groups = split(&project, page)?;
    ensure!(
        !groups.is_empty(),
        "no balloon on page {page} holds more than one text block, so there's nothing to join"
    );
    Ok(Edit {
        label: JOIN_PAGE.to_owned(),
        ops: joined(session, &project, page, &groups)?,
    })
}

/// The smallest a half of a split block may be, across its lines, in pixels.
const HALF_SIZE: f64 = 4.0;

/// The edit splitting `block` in two across its lines, labelled
/// [`SPLIT_BLOCK`]: the block keeps the half read first, the right one of
/// vertical text and the top one of horizontal text, and a copy just after
/// it, read next, takes the other. Each source's frame is its half, cut in
/// the source's own coordinates, so a tilted source stays tilted; each mask
/// and clean-up keeps only its pixels on its side of the cut. Both lose their
/// source wording and their translation, for the Recognition and
/// Translation stages to write again; [`split_halves`] finds both in the
/// edit's change. A block whose halves would be under 4 pixels is refused.
pub fn split_block(session: &Session, block: LayerId) -> Result<Edit> {
    let project = session.project();
    let layer = project.layer(block)?;
    let Content::Block(found) = &layer.content else {
        bail!("layer {block} isn't a block");
    };
    unlocked(&project, block)?;
    unlocked_parts(found)?;
    let source = &found.source;
    let frame = match &source.frame {
        Frame::Box { bounds, .. } => kurbo::Rect::from(*bounds),
        Frame::Path { path, .. } => BezPath::from(path).bounding_box(),
        Frame::Point => bail!("block {block}'s source has no frame to split"),
    };
    let turn = Affine::from(source.transform);
    ensure!(
        turn.determinant().abs() > f64::EPSILON,
        "block {block}'s source is flat on the page"
    );
    let vertical = source.direction == Direction::Vertical;
    let cut = if vertical {
        frame.center().x
    } else {
        frame.center().y
    };
    let (first, second) = match vertical {
        true => (
            kurbo::Rect { x0: cut, ..frame },
            kurbo::Rect { x1: cut, ..frame },
        ),
        false => (
            kurbo::Rect { y1: cut, ..frame },
            kurbo::Rect { y0: cut, ..frame },
        ),
    };
    let across = if vertical {
        first.width().min(second.width())
    } else {
        first.height().min(second.height())
    };
    ensure!(
        across >= HALF_SIZE,
        "block {block} is too small to split: its halves would be under {HALF_SIZE} pixels"
    );
    let copy = label(HALF)?;
    let halved = |rect: kurbo::Rect| {
        json!({
            "source": {"frame": {"box": {"bounds": bounds(rect)}, "path": null}, "content": ""},
            "translation": null,
        })
    };
    let mut ops = vec![
        Op::DuplicateLayer {
            layer: block.into(),
            id: Some(copy.clone()),
            index: None,
        },
        Op::UpdateLayer {
            layer: block.into(),
            patch: halved(first),
        },
        Op::UpdateLayer {
            layer: copy.clone(),
            patch: halved(second),
        },
    ];
    // Whether a point of the page is on the first half's side of the cut,
    // in the source's own coordinates.
    let own = turn.inverse();
    let first_side = |point: Point| {
        let point = own * point;
        if vertical {
            point.x >= cut
        } else {
            point.y < cut
        }
    };
    for (part, copied) in [
        (found.mask.id, label(&format!("{HALF}.mask"))?),
        (found.cleanup.id, label(&format!("{HALF}.cleanup"))?),
    ] {
        ops.extend(kept_side(session, part, part.into(), &|point| {
            first_side(point)
        })?);
        ops.extend(kept_side(session, part, copied, &|point| {
            !first_side(point)
        })?);
    }
    ops.extend(dropped(layer, &block.into()));
    ops.extend(dropped(layer, &copy));
    Ok(Edit {
        label: SPLIT_BLOCK.to_owned(),
        ops,
    })
}

/// The halves a [`split_block`] edit made of `block`, once applied as
/// `change`, in reading order: the block itself, then its copy.
pub fn split_halves(block: LayerId, change: &Change) -> Result<Vec<LayerId>> {
    Ok(vec![block, created(change, HALF)?])
}

/// The edit setting the reviewer's note on `layer`, a block or a text, to
/// `note`, or removing it when `note` is missing or blank, labelled
/// [`SET_REVIEW_NOTE`].
pub fn review_note(project: &Project, layer: LayerId, note: Option<String>) -> Result<Edit> {
    let found = project.layer(layer)?;
    ensure!(
        matches!(found.content, Content::Block(_) | Content::Text(_)),
        "layer {layer} is neither a block nor a text, so it takes no note"
    );
    unlocked(project, layer)?;
    let op = match note.filter(|note| !note.trim().is_empty()) {
        Some(note) => attrs::set(layer, &ReviewNote(note))?,
        None => attrs::clear::<ReviewNote>(layer),
    };
    Ok(Edit {
        label: SET_REVIEW_NOTE.to_owned(),
        ops: vec![op],
    })
}

/// A balloon's text blocks, in reading order, which a join makes one.
struct Group<'p> {
    balloon: &'p Layer,
    blocks: Vec<(LayerId, &'p Block)>,
}

/// The text blocks `balloon` holds, in tree order, which is reading order.
fn texts(balloon: &Layer) -> Vec<(LayerId, &Block)> {
    balloon
        .children
        .iter()
        .filter_map(|layer| match &layer.content {
            Content::Block(block) if block.role == Role::Text => Some((layer.id, &**block)),
            _ => None,
        })
        .collect()
}

/// `page`'s balloons whose text is split over two or more text blocks.
fn split(project: &Project, page: PageId) -> Result<Vec<Group<'_>>> {
    Ok(project
        .layers_of(page)?
        .into_iter()
        .filter(|layer| matches!(layer.content, Content::Balloon(_)))
        .map(|balloon| Group {
            balloon,
            blocks: texts(balloon),
        })
        .filter(|group| group.blocks.len() >= 2)
        .collect())
}

/// The ops joining each of `groups`, balloons of `page`, into its first
/// block.
fn joined(session: &Session, project: &Project, page: PageId, groups: &[Group]) -> Result<Vec<Op>> {
    for group in groups {
        for (id, _) in &group.blocks {
            unlocked(project, *id)?;
        }
        // The kept block's mask and clean-up take the others' pixels.
        unlocked_parts(group.blocks[0].1)?;
    }
    // Where each balloon's text stands once joined, and the page's other
    // texts, which share a balloon with a text reaching into it.
    let unions: Vec<kurbo::Rect> = groups
        .iter()
        .map(|group| {
            group
                .blocks
                .iter()
                .filter_map(|(_, block)| block.source.bounds())
                .reduce(|union, rect| union.union(rect))
                .with_context(|| {
                    format!(
                        "balloon {}'s texts have no frames to join",
                        group.balloon.id
                    )
                })
        })
        .collect::<Result<_>>()?;
    let joining: HashSet<LayerId> = groups
        .iter()
        .flat_map(|group| group.blocks.iter().map(|(id, _)| *id))
        .collect();
    let standing: Vec<kurbo::Rect> = project
        .blocks(page)?
        .into_iter()
        .filter(|(id, block)| block.role == Role::Text && !joining.contains(id))
        .filter_map(|(_, block)| block.source.bounds())
        .collect();
    let mut ops = Vec::new();
    for (at, group) in groups.iter().enumerate() {
        let Content::Balloon(balloon) = &group.balloon.content else {
            bail!("layer {} isn't a balloon", group.balloon.id);
        };
        let reach = BezPath::from(&balloon.outline).bounding_box();
        let shared = standing
            .iter()
            .chain(
                unions
                    .iter()
                    .enumerate()
                    .filter(|(other, _)| *other != at)
                    .map(|(_, union)| union),
            )
            .any(|rect| rect.intersect(reach).area() > 0.0);
        ops.extend(join(session, project, group, unions[at], shared)?);
    }
    Ok(ops)
}

/// The ops joining `group` into its first block, its texts' frames making
/// `union`. Its translation fills its balloon, as the Translation stage
/// places a balloon's only text, or `union` when the balloon is `shared`
/// with a text reaching into it, which the stage would leave its own room.
fn join(
    session: &Session,
    project: &Project,
    group: &Group,
    union: kurbo::Rect,
    shared: bool,
) -> Result<Vec<Op>> {
    let [(kept, first), rest @ ..] = group.blocks.as_slice() else {
        bail!("balloon {} holds no text block", group.balloon.id);
    };
    let lines = |texts: Vec<String>| {
        texts
            .into_iter()
            .filter(|text| !text.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let source = lines(
        group
            .blocks
            .iter()
            .map(|(_, block)| block.source.content.text())
            .collect(),
    );
    // The language of the first text read, which a blank one has none of.
    let language = group
        .blocks
        .iter()
        .map(|(_, block)| &block.source)
        .find(|source| !source.content.text().trim().is_empty())
        .map_or(&first.source.language, |source| &source.language);
    let joined = bounds(union);
    let mut ops = vec![Op::UpdateLayer {
        layer: (*kept).into(),
        patch: json!({"source": {
            "frame": {"box": {"bounds": joined}, "path": null},
            "transform": Transform::default(),
            "content": source,
            "language": language,
        }}),
    }];
    let translated: Vec<&Text> = group
        .blocks
        .iter()
        .filter_map(|(_, block)| block.translation.as_ref())
        .collect();
    if let Some(translation) = translated.first() {
        let target = translation.language.clone();
        ops.push(Op::SetText {
            layer: (*kept).into(),
            text: lines(
                translated
                    .iter()
                    .map(|translation| translation.content.text())
                    .collect(),
            ),
        });
        // The stage's lettering for the kept block, in the frame its joined
        // source has.
        let mut patch = match koharu_pipeline::placement(project, *kept, &target)? {
            Op::UpdateLayer { patch, .. } => patch,
            other => bail!("a placement answered {other:?}"),
        };
        let Content::Balloon(balloon) = &group.balloon.content else {
            bail!("layer {} isn't a balloon", group.balloon.id);
        };
        patch["translation"]["frame"] = match shared {
            true => {
                json!({"box": {"bounds": joined, "fit": "best", "anchor": "center"}, "path": null})
            }
            false => json!({
                "path": {"path": balloon.outline, "fit": "best", "anchor": "center"},
                "box": null,
            }),
        };
        patch["translation"]["transform"] = json!(Transform::default());
        ops.push(Op::UpdateLayer {
            layer: (*kept).into(),
            patch,
        });
        if first.translation.is_none() {
            ops.push(Op::UpdateLayer {
                layer: (*kept).into(),
                patch: json!({"translation": {"language": target}, "cleanup": {"visible": true}}),
            });
        }
    }
    let others: Vec<&Block> = rest.iter().map(|(_, block)| *block).collect();
    ops.extend(merged(session, first, &others)?);
    ops.extend(dropped(project.layer(*kept)?, &(*kept).into()));
    ops.extend(rest.iter().map(|(id, _)| Op::RemoveLayer {
        layer: (*id).into(),
    }));
    Ok(ops)
}

/// The ops writing `others`' mask and clean-up pixels into `kept`'s, over
/// where the others have any: the mask the most covering of theirs, and each
/// clean-up pixel the one under the most covering mask, `kept`'s on a tie,
/// or whichever clean-up has one there. A removed block takes its clean-up
/// with it, so the lettering it cleaned would show again.
fn merged(session: &Session, kept: &Block, others: &[&Block]) -> Result<Vec<Op>> {
    // Where the others' tiles lie on the page.
    let mut area: Option<Rect> = None;
    for other in others {
        for part in [other.mask.id, other.cleanup.id] {
            let tiles = session.tiles(part, None)?;
            for tile in &tiles.tiles {
                let at = Rect {
                    x: tile.x * Tile::SIZE as i32 + tiles.offset.x,
                    y: tile.y * Tile::SIZE as i32 + tiles.offset.y,
                    width: Tile::SIZE,
                    height: Tile::SIZE,
                };
                area = Some(area.map_or(at, |area| union(area, at)));
            }
        }
    }
    let Some(area) = area else {
        return Ok(Vec::new());
    };
    let read =
        |part: LayerId, offset: koharu_scene::Offset| session.pixels(part, shifted(area, offset));
    let (mask_offset, cleanup_offset) = (kept.mask.content.offset, kept.cleanup.content.offset);
    let mut mask = read(kept.mask.id, mask_offset)?;
    let mut cleanup = read(kept.cleanup.id, cleanup_offset)?;
    let (was_mask, was_cleanup) = (mask.clone(), cleanup.clone());
    for other in others {
        let their_mask = read(other.mask.id, other.mask.content.offset)?;
        let their_cleanup = read(other.cleanup.id, other.cleanup.content.offset)?;
        for (at, value) in mask.iter_mut().enumerate() {
            let pixel = at * 4..at * 4 + 4;
            let theirs = their_cleanup[at * 4 + 3] != 0;
            if theirs && (their_mask[at] > *value || cleanup[at * 4 + 3] == 0) {
                cleanup[pixel.clone()].copy_from_slice(&their_cleanup[pixel]);
            }
            *value = (*value).max(their_mask[at]);
        }
    }
    let mut ops = Vec::new();
    if mask != was_mask {
        ops.push(Op::WritePixels {
            layer: kept.mask.id.into(),
            rect: shifted(area, mask_offset),
            pixels: mask,
        });
    }
    if cleanup != was_cleanup {
        ops.push(Op::WritePixels {
            layer: kept.cleanup.id.into(),
            rect: shifted(area, cleanup_offset),
            pixels: cleanup,
        });
    }
    Ok(ops)
}

/// The ops writing `part`'s tiles to `target`, a half's copy of it, with
/// every pixel not on its `side` of the cut cleared; none for a tile with
/// nothing to clear.
fn kept_side(
    session: &Session,
    part: LayerId,
    target: Ref,
    side: &dyn Fn(Point) -> bool,
) -> Result<Vec<Op>> {
    let tiles = session.tiles(part, None)?;
    let bytes = match tiles.format {
        PixelFormat::Rgba8 => 4,
        PixelFormat::Gray8 => 1,
    };
    let size = Tile::SIZE as usize;
    let mut ops = Vec::new();
    for tile in &tiles.tiles {
        let mut pixels = tile.pixels.clone();
        let mut cleared = false;
        for (at, pixel) in pixels.chunks_exact_mut(bytes).enumerate() {
            if pixel.iter().all(|&value| value == 0) {
                continue;
            }
            let x =
                i64::from(tile.x) * size as i64 + (at % size) as i64 + i64::from(tiles.offset.x);
            let y =
                i64::from(tile.y) * size as i64 + (at / size) as i64 + i64::from(tiles.offset.y);
            if !side(Point::new(x as f64 + 0.5, y as f64 + 0.5)) {
                pixel.fill(0);
                cleared = true;
            }
        }
        if cleared {
            ops.push(Op::WritePixels {
                layer: target.clone(),
                rect: Rect {
                    x: tile.x * Tile::SIZE as i32,
                    y: tile.y * Tile::SIZE as i32,
                    width: Tile::SIZE,
                    height: Tile::SIZE,
                },
                pixels,
            });
        }
    }
    Ok(ops)
}

/// The ops dropping from `layer`, as `target`, the Booharu keys that
/// describe a block's text, which a join or split rewrites.
fn dropped(layer: &Layer, target: &Ref) -> Vec<Op> {
    fn drop<A: Attr>(layer: &Layer, target: &Ref, ops: &mut Vec<Op>) {
        if layer
            .attributes
            .get(attrs::NAMESPACE)
            .is_some_and(|keys| keys.contains_key(A::KEY))
        {
            ops.push(attrs::clear::<A>(target.clone()));
        }
    }
    let mut ops = Vec::new();
    drop::<Lettering>(layer, target, &mut ops);
    drop::<Auto>(layer, target, &mut ops);
    drop::<QcOk>(layer, target, &mut ops);
    drop::<ReadNote>(layer, target, &mut ops);
    ops
}

/// Refuses when `layer`, or a layer holding it, is locked.
fn unlocked(project: &Project, layer: LayerId) -> Result<()> {
    let mut at = Some(project.layer(layer)?);
    while let Some(layer) = at {
        if layer.locked {
            return Err(Refusal::Locked { layer: layer.id }.into());
        }
        at = project.parent(layer.id)?.1;
    }
    Ok(())
}

/// Refuses when `block`'s mask or clean-up is locked itself, which a write
/// to its pixels fails on.
fn unlocked_parts(block: &Block) -> Result<()> {
    for (id, locked) in [
        (block.mask.id, block.mask.locked),
        (block.cleanup.id, block.cleanup.locked),
    ] {
        if locked {
            return Err(Refusal::Locked { layer: id }.into());
        }
    }
    Ok(())
}

/// What a box over `rect` of `page` goes in: the smallest balloon whose
/// outline covers at least 90% of it, as Booharu 0.0.4's O tool chose a
/// box's balloon, else the smallest panel holding its centre; `None` for the
/// page itself.
fn holder(project: &Project, page: PageId, rect: kurbo::Rect) -> Result<Option<&Layer>> {
    /// The smallest of `layers` whose outline `holds` the box.
    fn smallest<'p>(
        layers: &[&'p Layer],
        balloons: bool,
        holds: impl Fn(&BezPath) -> bool,
    ) -> Option<&'p Layer> {
        layers
            .iter()
            .filter_map(|layer| match &layer.content {
                Content::Balloon(balloon) if balloons => Some((*layer, &balloon.outline)),
                Content::Panel(panel) if !balloons => Some((*layer, &panel.outline)),
                _ => None,
            })
            .map(|(layer, outline)| (layer, BezPath::from(outline)))
            .filter(|(_, outline)| holds(outline))
            .map(|(layer, outline)| (layer, outline.area().abs()))
            .min_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(layer, _)| layer)
    }
    let layers = project.layers_of(page)?;
    let centre = rect.center();
    Ok(smallest(&layers, true, |outline| {
        covered(outline, rect) >= HELD * rect.area()
    })
    .or_else(|| smallest(&layers, false, |outline| outline.contains(centre))))
}

/// The share of a box a balloon's outline must cover to hold the box.
const HELD: f64 = 0.9;

/// The area of `outline` inside `rect`: each of its subpaths, flattened to a
/// polygon, clipped to the rectangle, as Booharu 0.0.4 clipped a balloon's
/// polygon, so a round balloon's corners and the gap between two balloons
/// don't count.
fn covered(outline: &BezPath, rect: kurbo::Rect) -> f64 {
    let mut polygons: Vec<Vec<Point>> = Vec::new();
    // A line after a close starts a new subpath at the closed one's start.
    let mut closed = false;
    kurbo::flatten(outline, 0.25, |element| match element {
        PathEl::MoveTo(point) => {
            polygons.push(vec![point]);
            closed = false;
        }
        PathEl::LineTo(point) => match polygons.last_mut() {
            Some(polygon) if !closed => polygon.push(point),
            Some(polygon) => {
                let start = polygon[0];
                polygons.push(vec![start, point]);
                closed = false;
            }
            None => polygons.push(vec![point]),
        },
        PathEl::ClosePath => closed = true,
        // Flattening leaves only moves, lines, and closes.
        PathEl::QuadTo(..) | PathEl::CurveTo(..) => {}
    });
    // Each subpath's area as the shoelace formula gives it, signed by which
    // way round it runs, so a hole running the other way counts against the
    // shape around it.
    polygons
        .iter()
        .map(|polygon| {
            let clipped = clipped(polygon, rect);
            clipped
                .iter()
                .zip(clipped.iter().cycle().skip(1))
                .map(|(a, b)| a.x * b.y - a.y * b.x)
                .sum::<f64>()
                / 2.0
        })
        .sum::<f64>()
        .abs()
}

/// `polygon` clipped to `rect`, one side at a time (Sutherland and Hodgman).
fn clipped(polygon: &[Point], rect: kurbo::Rect) -> Vec<Point> {
    // Each side as whether it bounds y rather than x, where it lies, and
    // whether the inside is after it.
    let sides = [
        (false, rect.x0, true),
        (false, rect.x1, false),
        (true, rect.y0, true),
        (true, rect.y1, false),
    ];
    let mut polygon = polygon.to_vec();
    for (down, edge, after) in sides {
        let coordinate = |point: Point| if down { point.y } else { point.x };
        let inside = |point: Point| match after {
            true => coordinate(point) >= edge,
            false => coordinate(point) <= edge,
        };
        let input = std::mem::take(&mut polygon);
        let Some(&last) = input.last() else {
            break;
        };
        let mut previous = last;
        for current in input {
            if inside(current) != inside(previous) {
                let t =
                    (edge - coordinate(previous)) / (coordinate(current) - coordinate(previous));
                polygon.push(previous.lerp(current, t));
            }
            if inside(current) {
                polygon.push(current);
            }
            previous = current;
        }
    }
    polygon
}

/// Where among `siblings` a block at `rect` goes to be read in order: just
/// before the first sibling whose first text it is read before, else last.
/// Vertical text reads right to left then top to bottom, horizontal text top
/// to bottom then left to right, as Booharu 0.0.4 ordered a balloon's boxes.
/// A sibling holding no text is no part of the reading.
fn reading_index(siblings: &[Layer], rect: kurbo::Rect, vertical: bool) -> usize {
    siblings
        .iter()
        .position(|sibling| {
            first_text(sibling).is_some_and(|text| {
                let order = match vertical {
                    true => text
                        .x0
                        .total_cmp(&rect.x0)
                        .then(rect.y0.total_cmp(&text.y0)),
                    false => rect
                        .y0
                        .total_cmp(&text.y0)
                        .then(rect.x0.total_cmp(&text.x0)),
                };
                order.is_lt()
            })
        })
        .unwrap_or(siblings.len())
}

/// Where the first text in `layer`, itself or one inside it, lies on the
/// page.
fn first_text(layer: &Layer) -> Option<kurbo::Rect> {
    let own = match &layer.content {
        Content::Block(block) => block.source.bounds(),
        Content::Text(text) => text.bounds(),
        _ => None,
    };
    own.or_else(|| layer.children.iter().find_map(first_text))
}

/// Whether the Recognition stage would find nothing to read in `rect` of
/// `page`: the scan there, cropped as the stage crops a block's source frame,
/// with the glyphs of the page's blocks blanked as it blanks another block's
/// unless the new block's own mask, the whole box when `masked`, holds them,
/// is [`blank`]. A page without a scan has nothing to read.
fn unreadable(
    session: &Session,
    project: &Project,
    page: &Page,
    rect: kurbo::Rect,
    masked: bool,
) -> Result<bool> {
    let Some(background) = page.background else {
        return Ok(true);
    };
    let scan = image::load_from_memory(&session.file(&background.file)?)?;
    let Some(cut) = Rect::covering(rect, scan.width(), scan.height()) else {
        return Ok(true);
    };
    let mut crop = scan
        .crop_imm(cut.x as u32, cut.y as u32, cut.width, cut.height)
        .to_rgb8();
    if !masked {
        blank_glyphs(session, project, page.id, cut, &mut crop)?;
    }
    Ok(blank(&crop))
}

/// Blanks in `crop`, the scan over `cut`, the pixels the masks of `page`'s
/// blocks hold, in the crop's median colour, as the Recognition stage leaves
/// other blocks' glyphs out of a block's crop.
fn blank_glyphs(
    session: &Session,
    project: &Project,
    page: PageId,
    cut: Rect,
    crop: &mut RgbImage,
) -> Result<()> {
    let median: [u8; 3] = std::array::from_fn(|channel| {
        let mut values: Vec<u8> = crop.pixels().map(|pixel| pixel.0[channel]).collect();
        values.sort_unstable();
        values.get(values.len() / 2).copied().unwrap_or(u8::MAX)
    });
    let size = i64::from(Tile::SIZE);
    for (_, block) in project.blocks(page)? {
        let tiles = session.tiles(block.mask.id, Some(shifted(cut, block.mask.content.offset)))?;
        for tile in &tiles.tiles {
            for (at, &value) in tile.pixels.iter().enumerate() {
                let x = i64::from(tile.x) * size + at as i64 % size + i64::from(tiles.offset.x)
                    - i64::from(cut.x);
                let y = i64::from(tile.y) * size + at as i64 / size + i64::from(tiles.offset.y)
                    - i64::from(cut.y);
                if value != 0
                    && (0..i64::from(cut.width)).contains(&x)
                    && (0..i64::from(cut.height)).contains(&y)
                {
                    crop.put_pixel(x as u32, y as u32, Rgb(median));
                }
            }
        }
    }
    Ok(())
}

/// How far apart two pixels compared for an edge lie: a scan softens a
/// letter's edge over a pixel or two.
const REACH: u32 = 2;
/// The step in a colour channel, out of 255, that makes an edge: above a
/// scan's grain and a soft shading's change over `REACH` pixels, below even a
/// grey letter's stroke against its paper.
const EDGE: u8 = 48;
/// How many edge pixels make something to read; fewer are a speck of dust.
const INK: usize = 16;

/// Whether `crop` is nearly one colour, holding nothing to read: fewer than
/// `INK` of its pixels differ by `EDGE` from the one `REACH` across or down.
/// A soft shading changes too little from one pixel to the next to count,
/// unlike a letter's stroke.
fn blank(crop: &RgbImage) -> bool {
    let (width, height) = crop.dimensions();
    let differs =
        |a: &Rgb<u8>, b: &Rgb<u8>| a.0.iter().zip(b.0).any(|(&a, b)| a.abs_diff(b) >= EDGE);
    let mut edges = 0;
    for (x, y, pixel) in crop.enumerate_pixels() {
        let across = x + REACH < width && differs(pixel, crop.get_pixel(x + REACH, y));
        let down = y + REACH < height && differs(pixel, crop.get_pixel(x, y + REACH));
        if across || down {
            edges += 1;
            if edges == INK {
                return false;
            }
        }
    }
    true
}

/// The rectangle around both `a` and `b`.
fn union(a: Rect, b: Rect) -> Rect {
    let end = |at: i32, length: u32| i64::from(at) + i64::from(length);
    let (x, y) = (a.x.min(b.x), a.y.min(b.y));
    Rect {
        x,
        y,
        width: (end(a.x, a.width).max(end(b.x, b.width)) - i64::from(x)) as u32,
        height: (end(a.y, a.height).max(end(b.y, b.height)) - i64::from(y)) as u32,
    }
}

/// `rect` on the page in the own coordinates of a picture whose grid sits at
/// `offset`.
fn shifted(rect: Rect, offset: koharu_scene::Offset) -> Rect {
    Rect {
        x: rect.x - offset.x,
        y: rect.y - offset.y,
        ..rect
    }
}

/// `rect` as a text frame's bounds.
fn bounds(rect: kurbo::Rect) -> Bounds {
    Bounds {
        x: rect.x0,
        y: rect.y0,
        width: rect.width(),
        height: rect.height(),
    }
}

/// `$label`, naming a layer an edit makes.
fn label(label: &str) -> Result<Ref> {
    Ref::try_from(format!("${label}")).map_err(anyhow::Error::msg)
}

/// The layer the edit applied as `change` made under `label`.
fn created(change: &Change, label: &str) -> Result<LayerId> {
    change
        .created
        .iter()
        .find_map(|created| match created {
            Created::Layer {
                id,
                label: Some(found),
            } if found == label => Some(*id),
            _ => None,
        })
        .with_context(|| format!("the edit made no layer ${label}"))
}

#[cfg(test)]
mod tests {
    use image::ImageFormat;
    use koharu_scene::Hash;
    use serde_json::Value;

    use super::*;

    /// A project of one page whose scan is white, with lines of black
    /// lettering over each of `ink`.
    struct Fixture {
        _dir: tempfile::TempDir,
        session: Session,
        page: PageId,
    }

    /// A white `width` by `height` scan, with a black line two pixels thick
    /// every six down each of `ink`.
    fn scan(width: u32, height: u32, ink: &[kurbo::Rect]) -> RgbImage {
        RgbImage::from_fn(width, height, |x, y| {
            let (x, y) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
            let inked = ink
                .iter()
                .any(|rect| rect.contains(Point::new(x, y)) && (y - rect.y0).rem_euclid(6.0) < 2.0);
            Rgb(if inked { [0; 3] } else { [255; 3] })
        })
    }

    fn rect(x: f64, y: f64, width: f64, height: f64) -> kurbo::Rect {
        kurbo::Rect::new(x, y, x + width, y + height)
    }

    fn page_bounds(x: f64, y: f64, width: f64, height: f64) -> Bounds {
        Bounds {
            x,
            y,
            width,
            height,
        }
    }

    /// A rectangle's outline as SVG path data.
    fn outline(x: f64, y: f64, width: f64, height: f64) -> String {
        format!(
            "M{x} {y} L{} {y} L{} {} L{x} {} Z",
            x + width,
            x + width,
            y + height,
            y + height
        )
    }

    /// A text block whose source fills a box, reading `content`.
    fn text_block(bounds: Bounds, direction: Direction, content: &str) -> Value {
        json!({"kind": "block", "source": {
            "frame": {"box": {"bounds": bounds}},
            "direction": direction,
            "content": content,
            "language": "ja",
        }})
    }

    fn op(value: Value) -> Result<Op> {
        Ok(serde_json::from_value(value)?)
    }

    fn edit(ops: Vec<Op>) -> Edit {
        Edit {
            label: "test".to_owned(),
            ops,
        }
    }

    impl Fixture {
        fn new(width: u32, height: u32, ink: &[kurbo::Rect]) -> Result<Self> {
            let dir = tempfile::tempdir()?;
            let mut session = Session::create(dir.path().join("test.khr"))?;
            let mut png = Vec::new();
            scan(width, height, ink)
                .write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png)?;
            let hash = Hash::of(&png);
            let change = session.apply(edit(vec![
                Op::AddFile { bytes: png },
                op(json!({"op": "add_page", "index": 0, "background": hash}))?,
            ]))?;
            let page = *change.pages.added.first().context("the page")?;
            Ok(Self {
                _dir: dir,
                session,
                page,
            })
        }

        /// Adds `layer` under `parent`, or on the page, last.
        fn add(&mut self, parent: Option<LayerId>, layer: Value) -> Result<LayerId> {
            let project = self.session.project();
            let index = match parent {
                Some(parent) => project.layer(parent)?.children.len(),
                None => project.page(self.page)?.layers.len(),
            };
            let change = self.session.apply(edit(vec![op(json!({
                "op": "add_layer", "id": "$new", "page": self.page,
                "parent": parent, "index": index, "layer": layer,
            }))?]))?;
            created(&change, "new")
        }

        fn update(&mut self, layer: LayerId, patch: Value) -> Result<()> {
            self.session.apply(edit(vec![Op::UpdateLayer {
                layer: layer.into(),
                patch,
            }]))?;
            Ok(())
        }

        fn write(&mut self, part: LayerId, rect: Rect, value: &[u8]) -> Result<()> {
            let pixels = value.repeat(rect.width as usize * rect.height as usize);
            self.session.apply(edit(vec![Op::WritePixels {
                layer: part.into(),
                rect,
                pixels,
            }]))?;
            Ok(())
        }

        fn block(&self, id: LayerId) -> Result<Block> {
            match &self.session.project().layer(id)?.content {
                Content::Block(block) => Ok((**block).clone()),
                other => bail!("{other:?} isn't a block"),
            }
        }

        fn holder(&self, id: LayerId) -> Result<Option<LayerId>> {
            Ok(self.session.project().parent(id)?.1.map(|layer| layer.id))
        }

        fn steps(&self) -> Vec<String> {
            self.session.history().steps
        }

        /// Adds the box an [`add_box`] edit adds, answering its block.
        fn add_box(&mut self, rect: Bounds, mode: OcrBoxMode) -> Result<LayerId> {
            let change = self
                .session
                .apply(add_box(&self.session, self.page, rect, mode)?)?;
            added_box(&change)
        }
    }

    /// A text's frame as `(x, y, width, height)`, when it's a box.
    fn frame(text: &Text) -> Option<(f64, f64, f64, f64)> {
        match &text.frame {
            Frame::Box { bounds, .. } => Some((bounds.x, bounds.y, bounds.width, bounds.height)),
            _ => None,
        }
    }

    fn refusal(result: Result<Edit>) -> Option<Refusal> {
        result.err()?.downcast_ref::<Refusal>().copied()
    }

    fn pixel_rect(x: i32, y: i32, width: u32, height: u32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn blankness_is_told_by_edges_not_by_colour() {
        let white = RgbImage::from_pixel(60, 30, Rgb([255; 3]));
        assert!(blank(&white));
        let grey = RgbImage::from_pixel(60, 30, Rgb([128; 3]));
        assert!(blank(&grey));
        // A soft shading, across and down, changes too little between
        // neighbours to count.
        let gradient = RgbImage::from_fn(60, 30, |x, y| {
            Rgb([(40 + x * 3) as u8, (60 + y * 4) as u8, (90 + x + y) as u8])
        });
        assert!(blank(&gradient));
        // A scan's grain is no lettering either.
        let grain = RgbImage::from_fn(60, 30, |x, y| Rgb([240 + ((x * 7 + y * 13) % 11) as u8; 3]));
        assert!(blank(&grain));
        // Nor is a speck of dust.
        let mut speck = white.clone();
        speck.put_pixel(30, 15, Rgb([0; 3]));
        assert!(blank(&speck));
        // A few strokes are.
        let mut strokes = white.clone();
        for x in [12, 13, 30, 31, 44, 45] {
            for y in 6..24 {
                strokes.put_pixel(x, y, Rgb([0; 3]));
            }
        }
        assert!(!blank(&strokes));
        // So is grey lettering on grey paper.
        let mut grey_strokes = grey.clone();
        for y in [10, 11] {
            for x in 5..55 {
                grey_strokes.put_pixel(x, y, Rgb([60; 3]));
            }
        }
        assert!(!blank(&grey_strokes));
    }

    #[test]
    fn a_box_goes_in_the_smallest_balloon_covering_it_else_its_panel() -> Result<()> {
        let mut page = Fixture::new(400, 400, &[rect(0.0, 0.0, 400.0, 400.0)])?;
        let panel = page.add(
            None,
            json!({"kind": "panel", "outline": outline(0.0, 0.0, 400.0, 200.0)}),
        )?;
        let big = page.add(
            Some(panel),
            json!({"kind": "balloon", "outline": outline(10.0, 10.0, 180.0, 180.0)}),
        )?;
        let small = page.add(
            Some(panel),
            json!({"kind": "balloon", "outline": outline(20.0, 20.0, 100.0, 100.0)}),
        )?;
        // A round balloon, whose corners hold nothing.
        let round = page.add(
            None,
            json!({"kind": "balloon", "outline": "M380 300 C380 344.18 344.18 380 300 380 \
                C255.82 380 220 344.18 220 300 C220 255.82 255.82 220 300 220 \
                C344.18 220 380 255.82 380 300 Z"}),
        )?;
        let mode = OcrBoxMode::Read;

        // Inside both balloons: the smaller.
        let inside = page.add_box(page_bounds(30.0, 30.0, 60.0, 60.0), mode)?;
        assert_eq!(page.holder(inside)?, Some(small));
        // 92% inside the small one is enough, 85% isn't.
        let mostly = page.add_box(page_bounds(28.0, 30.0, 100.0, 50.0), mode)?;
        assert_eq!(page.holder(mostly)?, Some(small));
        let partly = page.add_box(page_bounds(35.0, 30.0, 100.0, 50.0), mode)?;
        assert_eq!(page.holder(partly)?, Some(big));
        // In the panel, in no balloon.
        let panelled = page.add_box(page_bounds(250.0, 50.0, 50.0, 50.0), mode)?;
        assert_eq!(page.holder(panelled)?, Some(panel));
        // In the round balloon's middle, and in its corner, which only its
        // bounding box covers: on the page.
        let middle = page.add_box(page_bounds(280.0, 280.0, 40.0, 40.0), mode)?;
        assert_eq!(page.holder(middle)?, Some(round));
        let corner = page.add_box(page_bounds(222.0, 222.0, 28.0, 28.0), mode)?;
        assert_eq!(page.holder(corner)?, None);
        Ok(())
    }

    #[test]
    fn a_box_is_cut_to_the_page_and_made_by_hand() -> Result<()> {
        let mut page = Fixture::new(200, 100, &[rect(0.0, 0.0, 200.0, 100.0)])?;
        let steps = page.steps().len();

        let cut = page.add_box(
            page_bounds(150.0, -20.0, 100.0, 60.0),
            OcrBoxMode::ReadTranslate,
        )?;

        assert_eq!(page.steps().len(), steps + 1);
        assert_eq!(page.steps().last().map(String::as_str), Some(ADD_BOX));
        let block = page.block(cut)?;
        assert_eq!(frame(&block.source), Some((150.0, 0.0, 50.0, 40.0)));
        assert_eq!(block.role, Role::Text);
        assert_eq!(block.source.direction, Direction::Horizontal);
        assert_eq!(block.source.content.text(), "");
        assert_eq!(block.source.language.to_string(), "und");
        assert_eq!(block.translation, None);
        // No provenance, so it counts as made by hand.
        assert!(page.session.project().layer(cut)?.attributes.is_empty());

        // At least 1.15 times as tall as it is wide reads vertically.
        let tall = page.add_box(page_bounds(10.0, 10.0, 40.0, 47.0), OcrBoxMode::Read)?;
        assert_eq!(page.block(tall)?.source.direction, Direction::Vertical);
        let squat = page.add_box(page_bounds(60.0, 10.0, 40.0, 45.0), OcrBoxMode::Read)?;
        assert_eq!(page.block(squat)?.source.direction, Direction::Horizontal);

        let outside = add_box(
            &page.session,
            page.page,
            page_bounds(300.0, 0.0, 10.0, 10.0),
            OcrBoxMode::Read,
        );
        assert!(outside.is_err_and(|error| error.to_string().contains("outside the page")));
        let flat = add_box(
            &page.session,
            page.page,
            page_bounds(10.0, 10.0, 0.0, 10.0),
            OcrBoxMode::Read,
        );
        assert!(flat.is_err());
        Ok(())
    }

    #[test]
    fn a_box_with_nothing_to_read_adds_nothing() -> Result<()> {
        let mut page = Fixture::new(200, 200, &[rect(10.0, 10.0, 50.0, 50.0)])?;
        let blank = add_box(
            &page.session,
            page.page,
            page_bounds(100.0, 100.0, 50.0, 50.0),
            OcrBoxMode::ReadTranslate,
        );
        assert_eq!(refusal(blank), Some(Refusal::NothingToRead));
        let inked = page_bounds(10.0, 10.0, 50.0, 50.0);
        assert!(add_box(&page.session, page.page, inked, OcrBoxMode::Read).is_ok());

        // Lettering another block's mask holds is left out of the reading,
        // as the Recognition stage leaves it out, unless the box's own mask,
        // the whole box when it's cleaned, holds it too.
        let other = page.add(None, text_block(inked, Direction::Horizontal, "あ"))?;
        let mask = page.block(other)?.mask.id;
        page.write(mask, pixel_rect(10, 10, 50, 50), &[255])?;
        let covered = add_box(&page.session, page.page, inked, OcrBoxMode::ReadTranslate);
        assert_eq!(refusal(covered), Some(Refusal::NothingToRead));
        let cleaned = add_box(
            &page.session,
            page.page,
            inked,
            OcrBoxMode::ReadTranslateClean,
        );
        assert!(cleaned.is_ok());
        Ok(())
    }

    #[test]
    fn only_a_cleaned_box_gets_a_mask_the_whole_box() -> Result<()> {
        let mut page = Fixture::new(200, 200, &[rect(0.0, 0.0, 200.0, 200.0)])?;
        let read = page.add_box(
            page_bounds(10.0, 10.0, 50.0, 30.0),
            OcrBoxMode::ReadTranslate,
        )?;
        let mask = page.block(read)?.mask.id;
        assert!(page.session.tiles(mask, None)?.tiles.is_empty());

        let steps = page.steps().len();
        let cleaned = page.add_box(
            page_bounds(100.5, 100.5, 50.0, 30.0),
            OcrBoxMode::ReadTranslateClean,
        )?;
        assert_eq!(page.steps().len(), steps + 1);
        let mask = page.block(cleaned)?.mask.id;
        // The pixels the box touches, and none around them.
        let around = page.session.pixels(mask, pixel_rect(99, 99, 54, 34))?;
        for (at, &value) in around.iter().enumerate() {
            let (x, y) = (99 + at % 54, 99 + at / 54);
            let inside = (100..151).contains(&x) && (100..131).contains(&y);
            assert_eq!(value, if inside { 255 } else { 0 }, "at {x}, {y}");
        }

        assert_eq!(stages(OcrBoxMode::Read), BTreeSet::from([Stage::Recognize]));
        assert_eq!(
            stages(OcrBoxMode::ReadTranslate),
            BTreeSet::from([Stage::Recognize, Stage::Translate])
        );
        assert_eq!(
            stages(OcrBoxMode::ReadTranslateClean),
            BTreeSet::from([Stage::Recognize, Stage::Inpaint, Stage::Translate])
        );
        Ok(())
    }

    #[test]
    fn a_box_is_read_in_order_among_its_neighbours() -> Result<()> {
        let mut page = Fixture::new(400, 400, &[rect(0.0, 0.0, 400.0, 400.0)])?;
        let balloon = page.add(
            None,
            json!({"kind": "balloon", "outline": outline(0.0, 0.0, 300.0, 200.0)}),
        )?;
        let vertical = |x| page_bounds(x, 20.0, 30.0, 120.0);
        let right = page.add(
            Some(balloon),
            text_block(vertical(200.0), Direction::Vertical, "一"),
        )?;
        let left = page.add(
            Some(balloon),
            text_block(vertical(40.0), Direction::Vertical, "三"),
        )?;

        // Vertical text reads right to left.
        let middle = page.add_box(vertical(120.0), OcrBoxMode::Read)?;
        let first = page.add_box(vertical(250.0), OcrBoxMode::Read)?;
        let children: Vec<LayerId> = page
            .session
            .project()
            .layer(balloon)?
            .children
            .iter()
            .map(|layer| layer.id)
            .collect();
        assert_eq!(children, [first, right, middle, left]);

        // Horizontal text reads top to bottom, and the page's layers in the
        // order of their first text.
        let top = page.add(
            None,
            text_block(
                page_bounds(20.0, 220.0, 100.0, 30.0),
                Direction::Horizontal,
                "a",
            ),
        )?;
        let bottom = page.add(
            None,
            text_block(
                page_bounds(20.0, 340.0, 100.0, 30.0),
                Direction::Horizontal,
                "c",
            ),
        )?;
        let between = page.add_box(page_bounds(20.0, 280.0, 100.0, 30.0), OcrBoxMode::Read)?;
        let layers: Vec<LayerId> = page
            .session
            .project()
            .page(page.page)?
            .layers
            .iter()
            .map(|layer| layer.id)
            .collect();
        assert_eq!(layers, [balloon, top, between, bottom]);
        Ok(())
    }

    /// A balloon holding three vertical text blocks, right to left, the
    /// middle one's wording blank, and a sound effect.
    struct Split {
        page: Fixture,
        balloon: LayerId,
        blocks: [LayerId; 3],
        sfx: LayerId,
    }

    fn split_balloon() -> Result<Split> {
        let mut page = Fixture::new(400, 400, &[rect(0.0, 0.0, 400.0, 400.0)])?;
        let balloon = page.add(
            None,
            json!({"kind": "balloon", "outline": outline(0.0, 0.0, 200.0, 200.0)}),
        )?;
        let vertical = |x| page_bounds(x, 20.0, 40.0, 100.0);
        let first = page.add(
            Some(balloon),
            text_block(vertical(120.0), Direction::Vertical, "あ"),
        )?;
        let blank = page.add(
            Some(balloon),
            text_block(vertical(60.0), Direction::Vertical, " "),
        )?;
        let last = page.add(
            Some(balloon),
            text_block(vertical(10.0), Direction::Vertical, "う"),
        )?;
        let sfx = page.add(
            Some(balloon),
            json!({"kind": "block", "role": "sfx", "source": {
                "frame": {"box": {"bounds": page_bounds(150.0, 150.0, 30.0, 30.0)}},
            }}),
        )?;
        Ok(Split {
            page,
            balloon,
            blocks: [first, blank, last],
            sfx,
        })
    }

    #[test]
    fn joining_a_balloon_keeps_its_first_block() -> Result<()> {
        let Split {
            mut page,
            balloon,
            blocks: [first, blank, last],
            sfx,
        } = split_balloon()?;
        let english: koharu_scene::LanguageIdentifier = "en".parse()?;
        for (block, text) in [(first, "Ah"), (last, "Oh")] {
            page.session.apply(crate::translation::edit(
                &page.session.project(),
                block,
                text.to_owned(),
                &english,
            )?)?;
        }
        // The lettering each block cleaned: the last's mask and clean-up,
        // and the blank one's mask.
        let (last_mask, last_cleanup) = {
            let block = page.block(last)?;
            (block.mask.id, block.cleanup.id)
        };
        page.write(last_mask, pixel_rect(10, 20, 40, 100), &[255])?;
        page.write(
            last_cleanup,
            pixel_rect(10, 20, 40, 100),
            &[200, 100, 50, 255],
        )?;
        page.write(
            page.block(blank)?.mask.id,
            pixel_rect(60, 20, 40, 100),
            &[128],
        )?;
        assert_eq!(
            find_split_balloons(&page.session.project(), Some(page.page))?,
            [SplitBalloon {
                page: page.page,
                balloon,
                blocks: vec![first, blank, last],
            }]
        );
        let before = page.session.project();
        let steps = page.steps().len();

        // Any of its blocks names the balloon.
        page.session.apply(join_balloon(&page.session, last)?)?;

        assert_eq!(page.steps().len(), steps + 1);
        assert_eq!(page.steps().last().map(String::as_str), Some(JOIN_BALLOON));
        let project = page.session.project();
        let children: Vec<LayerId> = project
            .layer(balloon)?
            .children
            .iter()
            .map(|layer| layer.id)
            .collect();
        assert_eq!(children, [first, sfx]);
        assert!(project.layer(blank).is_err() && project.layer(last).is_err());
        let joined = page.block(first)?;
        assert_eq!(frame(&joined.source), Some((10.0, 20.0, 150.0, 100.0)));
        assert_eq!(joined.source.content.text(), "あ\nう");
        let translation = joined.translation.context("a translation")?;
        assert_eq!(translation.content.text(), "Ah\nOh");
        assert_eq!(translation.language, english);
        // Placed as the Translation stage places a balloon's only text: in
        // the whole balloon.
        let Frame::Path { path, .. } = &translation.frame else {
            bail!("{:?} isn't the balloon's outline", translation.frame);
        };
        assert_eq!(path.as_str(), outline(0.0, 0.0, 200.0, 200.0));
        // The others' lettering stays cleaned.
        let mask = page
            .session
            .pixels(joined.mask.id, pixel_rect(0, 20, 120, 1))?;
        assert!(mask[10..50].iter().all(|&value| value == 255));
        assert!(mask[60..100].iter().all(|&value| value == 128));
        let cleanup = page
            .session
            .pixels(joined.cleanup.id, pixel_rect(10, 20, 40, 100))?;
        assert!(
            cleanup
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| *pixel == [200, 100, 50, 255])
        );
        assert!(find_split_balloons(&project, None)?.is_empty());

        // One undo puts everything back.
        page.session.undo()?;
        assert!(page.session.project() == before);
        Ok(())
    }

    #[test]
    fn a_locked_block_or_balloon_stops_a_join() -> Result<()> {
        let Split {
            mut page,
            balloon,
            blocks: [first, blank, _],
            ..
        } = split_balloon()?;
        page.update(blank, json!({"locked": true}))?;
        assert_eq!(
            refusal(join_balloon(&page.session, first)),
            Some(Refusal::Locked { layer: blank })
        );
        assert_eq!(
            refusal(join_page(&page.session, page.page)),
            Some(Refusal::Locked { layer: blank })
        );
        page.update(blank, json!({"locked": false}))?;
        page.update(balloon, json!({"locked": true}))?;
        assert_eq!(
            refusal(join_balloon(&page.session, first)),
            Some(Refusal::Locked { layer: balloon })
        );
        Ok(())
    }

    #[test]
    fn a_balloon_of_one_text_has_nothing_to_join() -> Result<()> {
        let mut page = Fixture::new(200, 200, &[rect(0.0, 0.0, 200.0, 200.0)])?;
        let balloon = page.add(
            None,
            json!({"kind": "balloon", "outline": outline(0.0, 0.0, 200.0, 200.0)}),
        )?;
        let only = page.add(
            Some(balloon),
            text_block(
                page_bounds(20.0, 20.0, 40.0, 100.0),
                Direction::Vertical,
                "あ",
            ),
        )?;
        let alone = page.add(
            None,
            text_block(
                page_bounds(100.0, 20.0, 40.0, 100.0),
                Direction::Vertical,
                "い",
            ),
        )?;
        for result in [
            join_balloon(&page.session, only),
            join_balloon(&page.session, alone),
            join_page(&page.session, page.page),
        ] {
            assert!(result.is_err_and(|error| error.to_string().contains("nothing to join")));
        }
        Ok(())
    }

    #[test]
    fn joining_a_page_joins_every_split_balloon_in_one_step() -> Result<()> {
        let mut page = Fixture::new(400, 400, &[rect(0.0, 0.0, 400.0, 400.0)])?;
        let mut balloons = Vec::new();
        for y in [0.0, 200.0] {
            let balloon = page.add(
                None,
                json!({"kind": "balloon", "outline": outline(0.0, y, 300.0, 180.0)}),
            )?;
            for (x, text) in [(150.0, "一"), (50.0, "二")] {
                page.add(
                    Some(balloon),
                    text_block(
                        page_bounds(x, y + 20.0, 40.0, 100.0),
                        Direction::Vertical,
                        text,
                    ),
                )?;
            }
            balloons.push(balloon);
        }
        let steps = page.steps().len();

        page.session.apply(join_page(&page.session, page.page)?)?;

        assert_eq!(page.steps().len(), steps + 1);
        assert_eq!(page.steps().last().map(String::as_str), Some(JOIN_PAGE));
        let project = page.session.project();
        for balloon in balloons {
            let children = &project.layer(balloon)?.children;
            assert_eq!(children.len(), 1);
            let Content::Block(joined) = &children[0].content else {
                bail!("the joined block");
            };
            assert_eq!(joined.source.content.text(), "一\n二");
            // No block had a translation, so the joined one has none.
            assert_eq!(joined.translation, None);
        }
        Ok(())
    }

    /// A page holding one block whose source fills `bounds`, reading
    /// `direction`, under `transform`, translated, its mask and clean-up
    /// full over where it lies, with a lettering category and a note.
    fn one_block(
        bounds: Bounds,
        direction: Direction,
        transform: Transform,
    ) -> Result<(Fixture, LayerId)> {
        let mut page = Fixture::new(400, 400, &[rect(0.0, 0.0, 400.0, 400.0)])?;
        let block = page.add(
            None,
            json!({"kind": "block", "source": {
                "frame": {"box": {"bounds": bounds}},
                "direction": direction,
                "transform": transform,
                "content": "あいう",
            }}),
        )?;
        let english: koharu_scene::LanguageIdentifier = "en".parse()?;
        page.session.apply(crate::translation::edit(
            &page.session.project(),
            block,
            "Hello".to_owned(),
            &english,
        )?)?;
        let found = page.block(block)?;
        let lies = found.source.bounds().context("a frame")?;
        let whole = Rect::covering(lies, 400, 400).context("pixels")?;
        page.write(found.mask.id, whole, &[255])?;
        page.write(found.cleanup.id, whole, &[1, 2, 3, 255])?;
        let lettering = Lettering {
            category: "shout".to_owned(),
            source: attrs::LetteringSource::User,
            confidence: None,
        };
        page.session.apply(edit(vec![
            attrs::set(block, &lettering)?,
            attrs::set(block, &ReviewNote("Check it".to_owned()))?,
        ]))?;
        Ok((page, block))
    }

    /// Where on the page, in `rect`, `part`'s pixels are set.
    fn set_columns(page: &Fixture, part: LayerId, rect: Rect) -> Result<Vec<(i32, i32)>> {
        let tiles = page.session.tiles(part, None)?;
        let bytes = match tiles.format {
            PixelFormat::Rgba8 => 4,
            PixelFormat::Gray8 => 1,
        };
        let pixels = page.session.pixels(part, shifted(rect, tiles.offset))?;
        Ok(pixels
            .chunks_exact(bytes)
            .enumerate()
            .filter(|(_, pixel)| pixel.iter().any(|&value| value != 0))
            .map(|(at, _)| {
                (
                    rect.x + (at % rect.width as usize) as i32,
                    rect.y + (at / rect.width as usize) as i32,
                )
            })
            .collect())
    }

    #[test]
    fn splitting_vertical_text_keeps_its_right_half_first() -> Result<()> {
        let (mut page, block) = one_block(
            page_bounds(100.0, 20.0, 40.0, 100.0),
            Direction::Vertical,
            Transform::default(),
        )?;
        let steps = page.steps().len();

        let change = page.session.apply(split_block(&page.session, block)?)?;
        let halves = split_halves(block, &change)?;

        assert_eq!(page.steps().len(), steps + 1);
        assert_eq!(page.steps().last().map(String::as_str), Some(SPLIT_BLOCK));
        let order: Vec<LayerId> = page
            .session
            .project()
            .page(page.page)?
            .layers
            .iter()
            .map(|layer| layer.id)
            .collect();
        assert_eq!(order, halves);
        let [right, left] = [page.block(halves[0])?, page.block(halves[1])?];
        assert_eq!(frame(&right.source), Some((120.0, 20.0, 20.0, 100.0)));
        assert_eq!(frame(&left.source), Some((100.0, 20.0, 20.0, 100.0)));
        for (id, half) in [(halves[0], &right), (halves[1], &left)] {
            assert_eq!(half.source.content.text(), "");
            assert_eq!(half.translation, None);
            let layer = page.session.project().layer(id)?.clone();
            assert_eq!(attrs::get::<Lettering>(&layer), None);
            assert_eq!(
                attrs::get::<ReviewNote>(&layer),
                Some(ReviewNote("Check it".to_owned()))
            );
        }
        // Each half keeps the mask and clean-up on its side of the cut.
        let row = pixel_rect(90, 50, 60, 1);
        let columns =
            |pixels: Vec<(i32, i32)>| pixels.into_iter().map(|(x, _)| x).collect::<Vec<_>>();
        for part in [right.mask.id, right.cleanup.id] {
            assert_eq!(
                columns(set_columns(&page, part, row)?),
                (120..140).collect::<Vec<_>>()
            );
        }
        for part in [left.mask.id, left.cleanup.id] {
            assert_eq!(
                columns(set_columns(&page, part, row)?),
                (100..120).collect::<Vec<_>>()
            );
        }
        Ok(())
    }

    #[test]
    fn splitting_horizontal_text_keeps_its_top_half_first() -> Result<()> {
        let (mut page, block) = one_block(
            page_bounds(20.0, 100.0, 100.0, 40.0),
            Direction::Horizontal,
            Transform::default(),
        )?;
        let change = page.session.apply(split_block(&page.session, block)?)?;
        let halves = split_halves(block, &change)?;
        let [top, bottom] = [page.block(halves[0])?, page.block(halves[1])?];
        assert_eq!(frame(&top.source), Some((20.0, 100.0, 100.0, 20.0)));
        assert_eq!(frame(&bottom.source), Some((20.0, 120.0, 100.0, 20.0)));
        let column = pixel_rect(50, 90, 1, 60);
        let rows = |pixels: Vec<(i32, i32)>| pixels.into_iter().map(|(_, y)| y).collect::<Vec<_>>();
        assert_eq!(
            rows(set_columns(&page, top.mask.id, column)?),
            (100..120).collect::<Vec<_>>()
        );
        assert_eq!(
            rows(set_columns(&page, bottom.mask.id, column)?),
            (120..140).collect::<Vec<_>>()
        );
        Ok(())
    }

    #[test]
    fn a_turned_source_is_cut_in_its_own_coordinates() -> Result<()> {
        // Horizontal text turned a quarter clockwise: its own top is the
        // page's right.
        let turned = Transform([[0.0, -1.0, 200.0], [1.0, 0.0, 0.0]]);
        let (mut page, block) = one_block(
            page_bounds(0.0, 0.0, 100.0, 40.0),
            Direction::Horizontal,
            turned,
        )?;
        let change = page.session.apply(split_block(&page.session, block)?)?;
        let halves = split_halves(block, &change)?;
        let [top, bottom] = [page.block(halves[0])?, page.block(halves[1])?];
        assert_eq!(frame(&top.source), Some((0.0, 0.0, 100.0, 20.0)));
        assert_eq!(frame(&bottom.source), Some((0.0, 20.0, 100.0, 20.0)));
        assert_eq!(top.source.transform, turned);
        let row = pixel_rect(150, 50, 60, 1);
        let columns =
            |pixels: Vec<(i32, i32)>| pixels.into_iter().map(|(x, _)| x).collect::<Vec<_>>();
        assert_eq!(
            columns(set_columns(&page, top.mask.id, row)?),
            (180..200).collect::<Vec<_>>()
        );
        assert_eq!(
            columns(set_columns(&page, bottom.mask.id, row)?),
            (160..180).collect::<Vec<_>>()
        );
        Ok(())
    }

    #[test]
    fn a_small_or_locked_block_isnt_split() -> Result<()> {
        let (mut page, block) = one_block(
            page_bounds(100.0, 20.0, 6.0, 100.0),
            Direction::Vertical,
            Transform::default(),
        )?;
        assert!(
            split_block(&page.session, block)
                .is_err_and(|error| error.to_string().contains("too small"))
        );
        page.update(block, json!({"locked": true}))?;
        assert_eq!(
            refusal(split_block(&page.session, block)),
            Some(Refusal::Locked { layer: block })
        );
        Ok(())
    }

    #[test]
    fn a_review_note_is_set_cleared_and_refused_when_locked() -> Result<()> {
        let mut page = Fixture::new(100, 100, &[])?;
        let group = page.add(None, json!({"kind": "group"}))?;
        let block = page.add(
            Some(group),
            text_block(
                page_bounds(10.0, 10.0, 20.0, 20.0),
                Direction::Horizontal,
                "あ",
            ),
        )?;
        let text = page.add(None, json!({"kind": "text", "content": "Hi"}))?;
        let raster = page.add(None, json!({"kind": "raster"}))?;
        let note = |page: &Fixture, id| -> Result<Option<ReviewNote>> {
            Ok(attrs::get(page.session.project().layer(id)?))
        };
        let steps = page.steps().len();

        for id in [block, text] {
            page.session.apply(review_note(
                &page.session.project(),
                id,
                Some("Reads well".to_owned()),
            )?)?;
            assert_eq!(note(&page, id)?, Some(ReviewNote("Reads well".to_owned())));
        }
        assert_eq!(page.steps().len(), steps + 2);
        assert_eq!(
            page.steps().last().map(String::as_str),
            Some(SET_REVIEW_NOTE)
        );
        page.session.apply(review_note(
            &page.session.project(),
            block,
            Some("  ".to_owned()),
        )?)?;
        assert_eq!(note(&page, block)?, None);
        page.session
            .apply(review_note(&page.session.project(), text, None)?)?;
        assert_eq!(note(&page, text)?, None);

        assert!(review_note(&page.session.project(), raster, Some("No".to_owned())).is_err());
        page.update(group, json!({"locked": true}))?;
        assert_eq!(
            refusal(review_note(
                &page.session.project(),
                block,
                Some("No".to_owned())
            )),
            Some(Refusal::Locked { layer: group })
        );
        Ok(())
    }
}
