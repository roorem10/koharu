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

use std::{
    collections::{BTreeSet, HashSet},
    ops::Deref,
};

use anyhow::{Context as _, Result, bail, ensure};
use booharu_core::{
    attrs::{self, Attr, Auto, Flags, Lettering, QcOk, ReadNote, ReviewNote},
    settings::OcrBoxMode,
    sfx, status,
    styles::text_hash,
};
use image::{DynamicImage, Rgb, RgbImage};
use koharu_pipeline::{RunId, Stage};
use koharu_scene::{
    Block, Bounds, Change, Content, Created, Direction, Edit, Frame, Hash, Layer, LayerId, Op,
    Page, PageId, PixelFormat, Project, Rect, Ref, Role, Session, Tile, Transform,
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
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum Refusal {
    /// The box holds no ink to read: the page is nearly one colour there,
    /// or shows only other blocks' lettering, which reading leaves out.
    #[error("there's nothing to read in this box")]
    NothingToRead,
    /// The action would change `layer`, or a layer inside it, and `layer`
    /// is locked. `name` names it as the Layers panel does, for whoever
    /// reads the message; its id is for the agent's tools.
    #[error(
        "{name} is locked, so this can't change it or what it holds; unlock it in Layers first \
         (layer {layer})"
    )]
    Locked { layer: LayerId, name: String },
}

/// A balloon whose text is split over two or more text blocks, which a
/// join makes one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct SplitBalloon {
    pub page: PageId,
    pub balloon: LayerId,
    /// Its text blocks, in reading order; sound effects and hidden blocks
    /// are left out.
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

/// The stages a split block's halves go through, run on them alone: both
/// are read and translated again, each keeping its part of the clean-up.
pub fn split_stages() -> BTreeSet<Stage> {
    BTreeSet::from([Stage::Recognize, Stage::Translate])
}

/// A page's scan, decoded, which [`add_box`] reads a box against, and
/// Read chapter and Explain this line show a model.
pub struct Scan {
    /// The file it was decoded from, which the page may have changed since.
    file: Hash,
    pub(crate) image: DynamicImage,
}

/// `page`'s scan, decoded, which [`add_box`] reads a box against; `None`
/// when the page has none. `lock` locks the session only while the scan's
/// bytes are read, as the pipeline's stages read a scan: decoding a big page
/// takes a while, which the session's other users shouldn't wait on.
pub fn read_scan<S: Deref<Target = Session>>(
    lock: impl FnOnce() -> S,
    page: PageId,
) -> Result<Option<Scan>> {
    let read = {
        let session = lock();
        match session.project().page(page)?.background {
            Some(background) => Some((background.file, session.file(&background.file)?)),
            None => None,
        }
    };
    read.map(|(file, bytes)| {
        Ok(Scan {
            file,
            image: image::load_from_memory(&bytes)?,
        })
    })
    .transpose()
}

/// The edit adding the block a box drawn over `rect` of `page`, in page
/// pixels, makes, labelled [`ADD_BOX`]; [`added_box`] finds the block in the
/// edit's change. `scan` is the page's, as [`read_scan`] decodes it; one the
/// page has changed since fails the box.
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
pub fn add_box(
    session: &Session,
    page: PageId,
    rect: Bounds,
    mode: OcrBoxMode,
    scan: Option<&Scan>,
) -> Result<Edit> {
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
    if unreadable(session, &project, found, rect, masked, scan)? {
        return Err(Refusal::NothingToRead.into());
    }
    let parent = holder(&project, page, rect)?;
    if let Some(parent) = parent {
        unlocked(&project, parent.id)?;
    }
    let siblings = parent.map_or(&found.layers, |parent| &parent.children);
    let in_balloon = parent.is_some_and(|parent| matches!(parent.content, Content::Balloon(_)));
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
        "index": reading_index(siblings, rect, vertical, in_balloon),
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
/// its own, so the lettering they cleaned stays clean, and they go, their
/// reviewer's notes and flags joining its own. The joined clean-up shows
/// when any of theirs showed over lettering, and a text left as scanned, as
/// the Translation stage leaves one the model gave back unchanged, then
/// gives the translation its source wording in its place. A balloon holding
/// one text block has nothing to join, and a hidden block, which isn't
/// drawn, is no part of its balloon's text. The first keeps its hand-edit
/// record, so a style set on it by hand stays a hand edit, and its joined
/// wording, which no automatic step wrote, counts as changed by hand; the
/// record loses the kept mark of a sound effect kept as art when the joined
/// clean-up shows or the join writes a translation.
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
    ensure!(
        layer.visible,
        "block {block} is hidden, so it isn't joined with its balloon's text"
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
/// Translation stages to write again, with Koharu's records of reading and
/// translating the whole block ([`unread`]), and keep the clean-up shown or
/// hidden as it was, a sound effect kept as art with its kept mark and a
/// line given back with its record. Both are marked as split from then on,
/// so Detect again keeps them; [`split_halves`] finds both in the edit's
/// change. A block whose halves would be under 4 pixels is refused.
pub fn split_block(session: &Session, block: LayerId) -> Result<Edit> {
    let project = session.project();
    let layer = project.layer(block)?;
    let Content::Block(found) = &layer.content else {
        bail!("layer {block} isn't a block");
    };
    unlocked(&project, block)?;
    unlocked_parts(&project, block)?;
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
    for half in [block.into(), copy] {
        ops.extend(dropped(layer, &half, true)?);
        ops.extend(unread(layer, &half));
    }
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

/// The text blocks `balloon` holds, in tree order, which is reading order,
/// a hidden one left out: it isn't drawn, so it's no part of the balloon's
/// text, and a join leaves it as it is.
fn texts(balloon: &Layer) -> Vec<(LayerId, &Block)> {
    balloon
        .children
        .iter()
        .filter(|layer| layer.visible)
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
        unlocked_parts(project, group.blocks[0].0)?;
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
///
/// The joined clean-up shows when any block's clean-up showed over its
/// lettering, so that lettering stays clean, and it then covers every
/// block's. A block whose clean-up was hidden, as the Translation stage hides
/// one whose text the model left unchanged, so its lettering showed, then
/// gives the joined translation its source wording in its place; with no
/// translation to hold that wording, the join is refused. When no clean-up
/// showed over lettering, the joined one is hidden, so every block's
/// lettering shows as it did.
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
    // Which blocks have a clean-up to cover their lettering with, and
    // whether any showed it.
    let cleaned: Vec<bool> = group
        .blocks
        .iter()
        .map(|(_, block)| cleans(session, block))
        .collect::<Result<_>>()?;
    let covered = group
        .blocks
        .iter()
        .zip(&cleaned)
        .any(|((_, block), &cleaned)| cleaned && block.cleanup.visible);
    // Each block's line of the joined translation: its translation, or the
    // source wording of lettering the joined clean-up covers anew.
    let wording: Vec<String> = group
        .blocks
        .iter()
        .zip(&cleaned)
        .filter_map(|((_, block), &cleaned)| match &block.translation {
            Some(translation) => Some(translation.content.text()),
            None => {
                (covered && cleaned && !block.cleanup.visible).then(|| block.source.content.text())
            }
        })
        .collect();
    let target = group
        .blocks
        .iter()
        .find_map(|(_, block)| block.translation.as_ref())
        .map(|translation| translation.language.clone());
    let translated = target.is_some();
    if let Some(target) = target {
        ops.push(Op::SetText {
            layer: (*kept).into(),
            text: lines(wording),
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
                patch: json!({"translation": {"language": target}}),
            });
        }
    } else {
        ensure!(
            lines(wording).is_empty(),
            "balloon {}'s texts are partly cleaned and none is translated, so joining them would \
             erase lettering; translate them first",
            group.balloon.id
        );
    }
    // With no clean-up to show, a translated block's shows, as the
    // Translation stage leaves one.
    let shown = match cleaned.contains(&true) {
        true => covered,
        false => translated || first.cleanup.visible,
    };
    if shown != first.cleanup.visible {
        ops.push(Op::UpdateLayer {
            layer: (*kept).into(),
            patch: json!({"cleanup": {"visible": shown}}),
        });
    }
    // A block kept as art no longer keeps it once its clean-up shows or it
    // has a translation, so it loses the kept mark, which would read a
    // clean-up the join showed as one shown by hand (`sfx::hand_shown`).
    if shown || translated {
        ops.extend(sfx::unmarked(project.layer(*kept)?)?);
    }
    let others: Vec<&Block> = rest.iter().map(|(_, block)| *block).collect();
    ops.extend(merged(session, first, &others)?);
    ops.extend(dropped(project.layer(*kept)?, &(*kept).into(), false)?);
    ops.extend(carried(project, group)?);
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

/// The ops giving `group`'s first block, which a join keeps, the reviewer's
/// notes and the flags of every block, as their texts are joined: the notes
/// a line each in reading order, and the flags one after another, each once;
/// none while only the first has any.
fn carried(project: &Project, group: &Group) -> Result<Vec<Op>> {
    let layers = group
        .blocks
        .iter()
        .map(|(id, _)| project.layer(*id))
        .collect::<koharu_scene::Result<Vec<_>>>()?;
    let [kept, rest @ ..] = layers.as_slice() else {
        return Ok(Vec::new());
    };
    let note = |layer: &Layer| {
        attrs::get::<ReviewNote>(layer)
            .map(|ReviewNote(note)| note)
            .filter(|note| !note.trim().is_empty())
    };
    let flags = |layer: &Layer| {
        attrs::get::<Flags>(layer)
            .map(|Flags(flags)| flags)
            .unwrap_or_default()
    };
    let mut ops = Vec::new();
    if rest.iter().any(|layer| note(layer).is_some()) {
        let mut notes: Vec<String> = Vec::new();
        for found in layers.iter().filter_map(|layer| note(layer)) {
            if !notes.contains(&found) {
                notes.push(found);
            }
        }
        ops.push(attrs::set(kept.id, &ReviewNote(notes.join("\n")))?);
    }
    if rest.iter().any(|layer| !flags(layer).is_empty()) {
        let mut all = Vec::new();
        for flag in layers.iter().flat_map(|layer| flags(layer)) {
            if !all.contains(&flag) {
                all.push(flag);
            }
        }
        ops.push(attrs::set(kept.id, &Flags(all))?);
    }
    Ok(ops)
}

/// Whether `block` has a clean-up to cover its lettering with: pixels in its
/// mask and in its clean-up, which shows only inside the mask.
fn cleans(session: &Session, block: &Block) -> Result<bool> {
    for part in [block.mask.id, block.cleanup.id] {
        if session.tiles(part, None)?.tiles.is_empty() {
            return Ok(false);
        }
    }
    Ok(true)
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
/// describe a block's text, which a join or split rewrites, and with
/// `record` its hand-edit record. A split puts a half's own record in its
/// place ([`half_record`]), as its halves are read and translated again,
/// which records them anew. A join keeps it, so the joined block's style
/// set by hand stays a hand edit, and its joined wording, which differs from
/// what the record holds, counts as one too.
fn dropped(layer: &Layer, target: &Ref, record: bool) -> Result<Vec<Op>> {
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
    if record {
        ops.push(attrs::set(target.clone(), &half_record(layer))?);
    }
    drop::<QcOk>(layer, target, &mut ops);
    drop::<ReadNote>(layer, target, &mut ops);
    Ok(ops)
}

/// The hand-edit record a split gives each half of the block `layer` holds,
/// read before the edit: split, as the split cut the half's box by hand
/// though it keeps the detection record it was copied with, so Detect again
/// keeps it (D1), as it keeps a box drawn with the O tool, while Apply
/// restyles it, as a split writes no wording or look of its own; the
/// kept mark of an effect Booharu kept as art; and, for a line the
/// Translation stage gave back unchanged, the translation of no text that
/// records it (`styles::given_back`), as Koharu's stage leaves no record of
/// its own there. The half keeps the clean-up as it shows, so neither reads
/// as hidden by hand (`sfx::hand_kept`), which the split's run would leave
/// untranslated. Nothing else: that run reads and translates the half
/// again, which records it anew.
fn half_record(layer: &Layer) -> Auto {
    let given_back = match &layer.content {
        Content::Block(block) => status::given_back(layer, block),
        _ => false,
    };
    Auto {
        split: true,
        kept: Auto::of(layer).kept,
        text: given_back.then(|| text_hash("")),
        ..Auto::default()
    }
}

/// The ops dropping from `layer`, as `target`, a half a split makes of it,
/// Koharu's records of reading and translating it, `c2pa.ocr` and
/// `c2pa.translation`: they describe the whole block's source and
/// translation, which neither half holds any more, and the split's run
/// writes fresh ones. Until it does, the planner plans reading and
/// translating the half, and wording typed into it counts as typed by hand
/// (`edited`), which the run then leaves alone. A half of a translated block
/// whose clean-up was hidden by hand has no translation and its clean-up
/// hidden, as one hidden by hand on a block with none (`sfx::hand_kept`),
/// which the run leaves untranslated, for Redo boxes. Detection's record
/// stays, as the half is still part of the box detection found, and so does
/// inpainting's, as the half keeps its part of the clean-up. Undo puts them
/// back with the rest of the block.
fn unread(layer: &Layer, target: &Ref) -> Vec<Op> {
    [status::OCR, status::TRANSLATION]
        .into_iter()
        .filter(|step| booharu_core::styles::recorded(layer, step))
        .map(|step| Op::SetAttribute {
            layer: target.clone(),
            namespace: "c2pa".to_owned(),
            key: step.to_owned(),
            value: None,
        })
        .collect()
}

/// Refuses when `layer`, or a layer holding it, is locked.
pub(crate) fn unlocked(project: &Project, layer: LayerId) -> Result<()> {
    let mut at = Some(project.layer(layer)?);
    while let Some(layer) = at {
        if layer.locked {
            let name = named(layer);
            return Err(Refusal::Locked {
                layer: layer.id,
                name,
            }
            .into());
        }
        at = project.parent(layer.id)?.1;
    }
    Ok(())
}

/// Refuses when the mask or the clean-up of `block` is locked itself, which
/// a write to its pixels fails on.
fn unlocked_parts(project: &Project, block: LayerId) -> Result<()> {
    let layer = project.layer(block)?;
    let Content::Block(found) = &layer.content else {
        bail!("layer {block} isn't a block");
    };
    for (id, locked, part) in [
        (found.mask.id, found.mask.locked, "mask"),
        (found.cleanup.id, found.cleanup.locked, "clean-up"),
    ] {
        if locked {
            let name = format!("the {part} of {}", named(layer));
            return Err(Refusal::Locked { layer: id, name }.into());
        }
    }
    Ok(())
}

/// The most characters of a text that [`named`] quotes.
const NAMED_TEXT: usize = 32;

/// `layer` as the Layers panel names it: by its own name, else by its text,
/// a block's translation before its source, else by its kind.
fn named(layer: &Layer) -> String {
    let name = layer.name.trim();
    if !name.is_empty() {
        return format!("\"{name}\"");
    }
    let text = match &layer.content {
        Content::Block(block) => block
            .translation
            .as_ref()
            .map(|translation| translation.content.text())
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| block.source.content.text()),
        Content::Text(text) => text.content.text(),
        _ => String::new(),
    };
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return format!("the {}", layer.kind().as_ref().replace('_', " "));
    }
    let quoted: String = text.chars().take(NAMED_TEXT).collect();
    let more = if quoted.len() < text.len() { "…" } else { "" };
    format!("the text \"{quoted}{more}\"")
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
/// Among a balloon's texts, `in_balloon`, vertical text reads right to left
/// then top to bottom, and horizontal text top to bottom then left to right,
/// as Booharu 0.0.4 ordered a balloon's boxes. Among a panel's or the page's
/// balloons and texts, as a manga page reads, those sharing a row read right
/// to left and the rows top to bottom, so a vertical line low on the page
/// isn't read before a balloon above it. A sibling holding no text is no
/// part of the reading.
fn reading_index(siblings: &[Layer], rect: kurbo::Rect, vertical: bool, in_balloon: bool) -> usize {
    siblings
        .iter()
        .position(|sibling| {
            first_text(sibling).is_some_and(|text| {
                let order = match (in_balloon, vertical) {
                    (true, true) => text
                        .x0
                        .total_cmp(&rect.x0)
                        .then(rect.y0.total_cmp(&text.y0)),
                    (true, false) => rect
                        .y0
                        .total_cmp(&text.y0)
                        .then(rect.x0.total_cmp(&text.x0)),
                    // One row: the one further right first.
                    (false, _) if rect.y0 < text.y1 && text.y0 < rect.y1 => text
                        .center()
                        .x
                        .total_cmp(&rect.center().x)
                        .then(rect.y0.total_cmp(&text.y0)),
                    (false, _) => rect.y0.total_cmp(&text.y0),
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
/// is [`blank`]. A page without a scan has nothing to read, and `scan`, the
/// page's as [`read_scan`] decoded it, fails the box once the page's scan
/// has changed.
fn unreadable(
    session: &Session,
    project: &Project,
    page: &Page,
    rect: kurbo::Rect,
    masked: bool,
    scan: Option<&Scan>,
) -> Result<bool> {
    let Some(background) = page.background else {
        return Ok(true);
    };
    let scan = scan
        .filter(|scan| scan.file == background.file)
        .context("the page's scan changed while the box was being read; draw it again")?;
    let Some(cut) = Rect::covering(rect, scan.image.width(), scan.image.height()) else {
        return Ok(true);
    };
    let mut crop = scan
        .image
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
/// The fewest edge pixels that make something to read: a speck of dust
/// three pixels across makes fewer.
const INK: usize = 24;
/// The share of a box's pixels that must be edges for it to hold something
/// to read, one in this many: in a wide box, a speck of dust a few pixels
/// across makes fewer, and even a short stroke of lettering more.
const SHARE: usize = 1000;

/// Whether `crop` is nearly one colour, holding nothing to read: fewer of
/// its pixels than `INK`, or than one in `SHARE`, differ by `EDGE` from the
/// one `REACH` across or down. A soft shading changes too little from one
/// pixel to the next to count, unlike a letter's stroke.
fn blank(crop: &RgbImage) -> bool {
    let (width, height) = crop.dimensions();
    let ink = INK.max(width as usize * height as usize / SHARE);
    let differs =
        |a: &Rgb<u8>, b: &Rgb<u8>| a.0.iter().zip(b.0).any(|(&a, b)| a.abs_diff(b) >= EDGE);
    let mut edges = 0;
    for (x, y, pixel) in crop.enumerate_pixels() {
        let across = x + REACH < width && differs(pixel, crop.get_pixel(x + REACH, y));
        let down = y + REACH < height && differs(pixel, crop.get_pixel(x, y + REACH));
        if across || down {
            edges += 1;
            if edges == ink {
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
    use booharu_core::attrs::Flag;
    use image::ImageFormat;
    use koharu_scene::Text;
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

        fn scan(&self) -> Result<Option<Scan>> {
            read_scan(|| &self.session, self.page)
        }

        /// The [`add_box`] edit of a box, read against the page's scan.
        fn box_edit(&self, rect: Bounds, mode: OcrBoxMode) -> Result<Edit> {
            add_box(&self.session, self.page, rect, mode, self.scan()?.as_ref())
        }

        /// Adds the box an [`add_box`] edit adds, answering its block.
        fn add_box(&mut self, rect: Bounds, mode: OcrBoxMode) -> Result<LayerId> {
            let change = self.session.apply(self.box_edit(rect, mode)?)?;
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
        result.err()?.downcast_ref::<Refusal>().cloned()
    }

    /// The layer a [`Refusal::Locked`] names, when `result` is one.
    fn locked(result: Result<Edit>) -> Option<LayerId> {
        match refusal(result)? {
            Refusal::Locked { layer, .. } => Some(layer),
            Refusal::NothingToRead => None,
        }
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
    fn a_speck_of_dust_is_nothing_to_read() {
        /// A `size` square of paper with a dark `dot` square in its middle.
        fn dotted(size: u32, dot: u32) -> RgbImage {
            let middle = (size - dot) / 2..(size + dot) / 2;
            RgbImage::from_fn(size, size, |x, y| {
                Rgb(match middle.contains(&x) && middle.contains(&y) {
                    true => [20; 3],
                    false => [250; 3],
                })
            })
        }
        // Dust in a wide box, up to a few pixels across.
        for dot in [3, 4, 5] {
            assert!(blank(&dotted(200, dot)), "a dot {dot} pixels across");
        }
        // A short stroke in the same box is lettering.
        let mut stroke = dotted(200, 0);
        for x in 100..102 {
            for y in 90..105 {
                stroke.put_pixel(x, y, Rgb([20; 3]));
            }
        }
        assert!(!blank(&stroke));
        // In a box drawn close around it, a dot four pixels across, such as
        // a middle dot, is something to read, and one three across isn't.
        assert!(!blank(&dotted(30, 4)));
        assert!(blank(&dotted(30, 3)));
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

        let outside = page.box_edit(page_bounds(300.0, 0.0, 10.0, 10.0), OcrBoxMode::Read);
        assert!(outside.is_err_and(|error| error.to_string().contains("outside the page")));
        let flat = page.box_edit(page_bounds(10.0, 10.0, 0.0, 10.0), OcrBoxMode::Read);
        assert!(flat.is_err());
        Ok(())
    }

    #[test]
    fn a_box_with_nothing_to_read_adds_nothing() -> Result<()> {
        let mut page = Fixture::new(200, 200, &[rect(10.0, 10.0, 50.0, 50.0)])?;
        let blank = page.box_edit(
            page_bounds(100.0, 100.0, 50.0, 50.0),
            OcrBoxMode::ReadTranslate,
        );
        assert_eq!(refusal(blank), Some(Refusal::NothingToRead));
        let inked = page_bounds(10.0, 10.0, 50.0, 50.0);
        assert!(page.box_edit(inked, OcrBoxMode::Read).is_ok());

        // Lettering another block's mask holds is left out of the reading,
        // as the Recognition stage leaves it out, unless the box's own mask,
        // the whole box when it's cleaned, holds it too.
        let other = page.add(None, text_block(inked, Direction::Horizontal, "あ"))?;
        let mask = page.block(other)?.mask.id;
        page.write(mask, pixel_rect(10, 10, 50, 50), &[255])?;
        let covered = page.box_edit(inked, OcrBoxMode::ReadTranslate);
        assert_eq!(refusal(covered), Some(Refusal::NothingToRead));
        let cleaned = page.box_edit(inked, OcrBoxMode::ReadTranslateClean);
        assert!(cleaned.is_ok());
        Ok(())
    }

    #[test]
    fn a_box_read_against_a_scan_the_page_changed_since_fails() -> Result<()> {
        let page = Fixture::new(100, 100, &[rect(10.0, 10.0, 50.0, 50.0)])?;
        let inked = page_bounds(10.0, 10.0, 50.0, 50.0);
        let scan = page.scan()?.context("the page's scan")?;
        let read = |scan| add_box(&page.session, page.page, inked, OcrBoxMode::Read, scan);
        assert!(read(Some(&scan)).is_ok());
        let other = Scan {
            file: Hash::of(b"another scan"),
            image: scan.image.clone(),
        };
        assert!(read(Some(&other)).is_err_and(|error| error.to_string().contains("scan changed")));
        assert!(read(None).is_err_and(|error| error.to_string().contains("scan changed")));
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

    #[test]
    fn a_box_beside_balloons_is_read_by_rows_right_to_left() -> Result<()> {
        let mut page = Fixture::new(800, 1000, &[rect(0.0, 0.0, 800.0, 1000.0)])?;
        let panel = page.add(
            None,
            json!({"kind": "panel", "outline": outline(0.0, 0.0, 800.0, 1000.0)}),
        )?;
        let balloon = |page: &mut Fixture, x: f64, y: f64| -> Result<LayerId> {
            let balloon = page.add(
                Some(panel),
                json!({"kind": "balloon", "outline": outline(x - 20.0, y - 20.0, 80.0, 240.0)}),
            )?;
            let text = page_bounds(x, y, 40.0, 200.0);
            page.add(Some(balloon), text_block(text, Direction::Vertical, "あ"))?;
            Ok(balloon)
        };
        // One balloon at the top right, one lower at the left.
        let right = balloon(&mut page, 600.0, 50.0)?;
        let left = balloon(&mut page, 100.0, 300.0)?;

        // A vertical line low on the right, in no balloon, is read after
        // both, though it lies right of them.
        let low = page.add_box(page_bounds(650.0, 700.0, 40.0, 200.0), OcrBoxMode::Read)?;
        // One in the left balloon's row, right of it, is read before it.
        let beside = page.add_box(page_bounds(400.0, 320.0, 40.0, 150.0), OcrBoxMode::Read)?;
        assert_eq!(page.holder(low)?, Some(panel));
        let children: Vec<LayerId> = page
            .session
            .project()
            .layer(panel)?
            .children
            .iter()
            .map(|layer| layer.id)
            .collect();
        assert_eq!(children, [right, beside, left, low]);
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
                &booharu_core::settings::Snapshot::default(),
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

    /// A balloon holding two vertical text blocks, right to left, reading
    /// `first` and `second`, each with its lettering cleaned: its mask and
    /// clean-up full over where it lies.
    fn two_cleaned(first: &str, second: &str) -> Result<(Fixture, [LayerId; 2])> {
        let mut page = Fixture::new(400, 400, &[rect(0.0, 0.0, 400.0, 400.0)])?;
        let balloon = page.add(
            None,
            json!({"kind": "balloon", "outline": outline(0.0, 0.0, 200.0, 200.0)}),
        )?;
        let mut blocks = Vec::new();
        for (x, text) in [(120, first), (10, second)] {
            let bounds = page_bounds(f64::from(x), 20.0, 40.0, 100.0);
            let block = page.add(Some(balloon), text_block(bounds, Direction::Vertical, text))?;
            let found = page.block(block)?;
            let lies = pixel_rect(x, 20, 40, 100);
            page.write(found.mask.id, lies, &[255])?;
            page.write(found.cleanup.id, lies, &[1, 2, 3, 255])?;
            blocks.push(block);
        }
        Ok((page, [blocks[0], blocks[1]]))
    }

    /// Translates `block` as `text`, its clean-up shown under it.
    fn translate(page: &mut Fixture, block: LayerId, text: &str) -> Result<()> {
        let english: koharu_scene::LanguageIdentifier = "en".parse()?;
        let edit = crate::translation::edit(
            &page.session.project(),
            block,
            text.to_owned(),
            &english,
            &booharu_core::settings::Snapshot::default(),
        )?;
        page.session.apply(edit)?;
        Ok(())
    }

    /// Leaves `block` as the Translation stage leaves a text the model gave
    /// back unchanged: untranslated, its clean-up hidden, so its lettering
    /// shows.
    fn unchanged(page: &mut Fixture, block: LayerId) -> Result<()> {
        page.update(
            block,
            json!({"translation": null, "cleanup": {"visible": false}}),
        )
    }

    fn joined_translation(page: &Fixture, block: LayerId) -> Result<Option<String>> {
        Ok(page
            .block(block)?
            .translation
            .map(|translation| translation.content.text()))
    }

    #[test]
    fn a_join_carries_the_wording_of_lettering_its_clean_up_covers() -> Result<()> {
        // A heart the model left as it was, read after a translated text:
        // the joined clean-up covers it, so the translation carries it.
        let (mut page, [first, second]) = two_cleaned("どうして", "♡")?;
        translate(&mut page, first, "Why?")?;
        unchanged(&mut page, second)?;
        page.session.apply(join_balloon(&page.session, first)?)?;
        assert_eq!(
            joined_translation(&page, first)?.as_deref(),
            Some("Why?\n♡")
        );
        let joined = page.block(first)?;
        assert!(joined.cleanup.visible);
        let cleanup = page
            .session
            .pixels(joined.cleanup.id, pixel_rect(10, 20, 40, 100))?;
        assert!(
            cleanup
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| *pixel == [1, 2, 3, 255])
        );

        // The same read first, in the block the join keeps.
        let (mut page, [first, second]) = two_cleaned("!!", "どうして")?;
        unchanged(&mut page, first)?;
        translate(&mut page, second, "Why?")?;
        page.session.apply(join_balloon(&page.session, second)?)?;
        assert_eq!(
            joined_translation(&page, first)?.as_deref(),
            Some("!!\nWhy?")
        );
        assert!(page.block(first)?.cleanup.visible);

        // A text whose clean-up holds nothing, such as a box drawn to read
        // it, still shows its lettering: the translation doesn't repeat it.
        let (mut page, [first, second]) = two_cleaned("どうして", "♡")?;
        translate(&mut page, first, "Why?")?;
        unchanged(&mut page, second)?;
        page.write(
            page.block(second)?.mask.id,
            pixel_rect(10, 20, 40, 100),
            &[0],
        )?;
        page.session.apply(join_balloon(&page.session, first)?)?;
        assert_eq!(joined_translation(&page, first)?.as_deref(), Some("Why?"));
        Ok(())
    }

    #[test]
    fn a_joined_clean_up_shows_when_any_showed() -> Result<()> {
        // The kept text's clean-up hidden by hand: the other's lettering
        // stays clean, so the joined clean-up shows over both.
        let (mut page, [first, second]) = two_cleaned("あ", "う")?;
        translate(&mut page, first, "Ah")?;
        translate(&mut page, second, "Oh")?;
        page.update(first, json!({"cleanup": {"visible": false}}))?;
        page.session.apply(join_balloon(&page.session, first)?)?;
        assert_eq!(joined_translation(&page, first)?.as_deref(), Some("Ah\nOh"));
        assert!(page.block(first)?.cleanup.visible);

        // None showed: every text's lettering shows as it did.
        let (mut page, [first, second]) = two_cleaned("!!", "♡")?;
        unchanged(&mut page, first)?;
        unchanged(&mut page, second)?;
        page.session.apply(join_balloon(&page.session, second)?)?;
        assert_eq!(joined_translation(&page, first)?, None);
        assert!(!page.block(first)?.cleanup.visible);

        // One cleaned and one left as scanned, with no translation to carry
        // the scanned one's wording: refused.
        let (mut page, [first, second]) = two_cleaned("あ", "♡")?;
        unchanged(&mut page, second)?;
        let steps = page.steps().len();
        assert!(
            join_balloon(&page.session, first)
                .is_err_and(|error| error.to_string().contains("translate them first"))
        );
        assert_eq!(page.steps().len(), steps);
        Ok(())
    }

    #[test]
    fn a_join_showing_or_translating_a_line_kept_as_art_drops_its_mark() -> Result<()> {
        use booharu_core::{lettering::LetteringCategory, settings::LetteringSettings};
        // A line kept as art, with no clean-up of its own, read first beside
        // a cleaned line that shows its clean-up, untranslated or translated.
        for translation in [None, Some("Why?")] {
            let (mut page, [first, second]) = two_cleaned("ドン", "どうして")?;
            let mask = page.block(first)?.mask.id;
            page.write(mask, pixel_rect(120, 20, 40, 100), &[0])?;
            let kept = sfx::kept_original(page.session.project().layer(first)?);
            assert!(!kept.is_empty());
            page.session.apply(edit(kept))?;
            if let Some(text) = translation {
                translate(&mut page, second, text)?;
            }

            page.session.apply(join_balloon(&page.session, first)?)?;

            // The join shows the clean-up over the other line's lettering,
            // which no hand showed, and the line keeps the art no more.
            let joined = page.block(first)?;
            let layer = page.session.project().layer(first)?.clone();
            assert!(joined.cleanup.visible);
            assert_eq!(joined_translation(&page, first)?.as_deref(), translation);
            assert!(!Auto::of(&layer).kept);
            assert!(!sfx::hand_shown(&layer, &joined));
            if translation.is_some() {
                continue;
            }
            // So picking Sound Effect keeps it as art again, as for any
            // line.
            let (picked, _) = crate::lettering::set_category(
                &page.session.project(),
                &[first],
                Some(LetteringCategory::Sfx),
                crate::lettering::Chooser::User,
                &LetteringSettings::default(),
                None,
            )?;
            page.session.apply(picked.context("the pick's edit")?)?;
            let layer = page.session.project().layer(first)?.clone();
            assert!(!page.block(first)?.cleanup.visible);
            assert!(Auto::of(&layer).kept);
        }
        Ok(())
    }

    #[test]
    fn a_join_keeps_every_texts_notes_and_flags() -> Result<()> {
        let Split {
            mut page,
            blocks: [first, blank, last],
            ..
        } = split_balloon()?;
        let flag = |message: &str| Flag {
            kind: "review".to_owned(),
            message: message.to_owned(),
        };
        page.session.apply(edit(vec![
            attrs::set(first, &ReviewNote("Check the tone".to_owned()))?,
            attrs::set(last, &ReviewNote("A pun on kami".to_owned()))?,
            attrs::set(blank, &Flags(vec![flag("Missing text?")]))?,
            attrs::set(last, &Flags(vec![flag("Glossary")]))?,
        ]))?;
        let before = page.session.project();

        page.session.apply(join_balloon(&page.session, first)?)?;

        let joined = page.session.project().layer(first)?.clone();
        assert_eq!(
            attrs::get::<ReviewNote>(&joined),
            Some(ReviewNote("Check the tone\nA pun on kami".to_owned()))
        );
        assert_eq!(
            attrs::get::<Flags>(&joined),
            Some(Flags(vec![flag("Missing text?"), flag("Glossary")]))
        );
        // One undo gives each text its own again.
        page.session.undo()?;
        assert!(page.session.project() == before);
        Ok(())
    }

    /// The op recording on `block` that a Koharu stage made its part `step`
    /// by `action`, as the stages record it in `c2pa`.
    fn made(block: LayerId, step: &str, action: &str) -> Op {
        Op::SetAttribute {
            layer: block.into(),
            namespace: "c2pa".to_owned(),
            key: step.to_owned(),
            value: Some(json!({"action": action})),
        }
    }

    /// Leaves `block` as a run of Koharu's stages leaves one, with Booharu's
    /// hooks: found and read, its wording recorded, then translated as
    /// `text`, with the default look and the record of what it wrote.
    fn run_on(page: &mut Fixture, block: LayerId, text: &str) -> Result<()> {
        let project = page.session.project();
        let wording = page.block(block)?.source.content.text();
        page.session.apply(edit(vec![
            made(block, "detection", "c2pa.created"),
            made(block, "ocr", "c2pa.created"),
            booharu_core::styles::recognized(project.layer(block)?, &wording)?,
        ]))?;
        stage_translates(page, block, text)
    }

    /// Translates `block` as `text` as Koharu's Translation stage does, with
    /// Booharu's hook: the default look and the record of what it wrote.
    fn stage_translates(page: &mut Fixture, block: LayerId, text: &str) -> Result<()> {
        let english: koharu_scene::LanguageIdentifier = "en".parse()?;
        let project = page.session.project();
        let placed = koharu_pipeline::placement(&project, block, &english)?;
        let mut ops = vec![
            Op::SetText {
                layer: block.into(),
                text: text.to_owned(),
            },
            placed.clone(),
        ];
        ops.extend(booharu_core::styles::translated(
            &project,
            block,
            text,
            &placed,
            &booharu_core::settings::LetteringSettings::default(),
            None,
        )?);
        ops.push(Op::UpdateLayer {
            layer: block.into(),
            patch: json!({"translation": {"language": english}, "cleanup": {"visible": true}}),
        });
        ops.push(made(block, "translation", "c2pa.translated"));
        page.session.apply(edit(ops))?;
        Ok(())
    }

    #[test]
    fn a_join_keeps_a_style_set_by_hand_a_hand_edit() -> Result<()> {
        let (mut page, [first, second]) = two_cleaned("あ", "う")?;
        run_on(&mut page, first, "Ah")?;
        run_on(&mut page, second, "Oh")?;
        let edited = |page: &Fixture| -> Result<_> {
            Ok(booharu_core::edited(page.session.project().layer(first)?))
        };
        assert!(!edited(&page)?.hand_edited());
        // The kept block's font set by hand, as the TypeBar sets one.
        page.update(
            first,
            json!({"translation": {"style": {"chars": {"family": "Comic Neue"}}}}),
        )?;

        page.session.apply(join_balloon(&page.session, first)?)?;

        let joined = edited(&page)?;
        assert!(joined.style, "{joined:?}");
        // Its joined wording is no automatic step's either.
        assert!(joined.source && joined.text, "{joined:?}");
        // So Apply keeps it, unless told to include hand edits.
        let project = page.session.project();
        let scope = crate::styles::StyleScope::Blocks {
            blocks: vec![first],
        };
        let (restyled, counts) = crate::styles::apply(
            &project,
            &scope,
            false,
            None,
            &booharu_core::settings::LetteringSettings::default(),
            Some("CCWildWords"),
            booharu_core::attrs::Origin::Automatic,
        )?;
        assert!(restyled.is_none());
        assert_eq!(counts.kept_hand_edited, 1);
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
        assert_eq!(locked(join_balloon(&page.session, first)), Some(blank));
        assert_eq!(locked(join_page(&page.session, page.page)), Some(blank));
        page.update(blank, json!({"locked": false}))?;
        page.update(balloon, json!({"locked": true}))?;
        assert_eq!(locked(join_balloon(&page.session, first)), Some(balloon));
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
    fn a_hidden_block_is_no_part_of_its_balloons_text() -> Result<()> {
        let Split {
            mut page,
            balloon,
            blocks: [first, blank, last],
            sfx,
        } = split_balloon()?;
        page.update(first, json!({"visible": false}))?;
        assert_eq!(
            find_split_balloons(&page.session.project(), Some(page.page))?,
            [SplitBalloon {
                page: page.page,
                balloon,
                blocks: vec![blank, last],
            }]
        );
        assert!(
            join_balloon(&page.session, first)
                .is_err_and(|error| error.to_string().contains("hidden"))
        );

        // The first shown is kept, and the hidden one stays as it was.
        page.session.apply(join_balloon(&page.session, last)?)?;
        let project = page.session.project();
        let children: Vec<LayerId> = project
            .layer(balloon)?
            .children
            .iter()
            .map(|layer| layer.id)
            .collect();
        assert_eq!(children, [first, blank, sfx]);
        assert_eq!(page.block(first)?.source.content.text(), "あ");
        assert_eq!(page.block(blank)?.source.content.text(), "う");
        // One text shows, with nothing to join it with.
        assert!(find_split_balloons(&project, None)?.is_empty());
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
            &booharu_core::settings::Snapshot::default(),
        )?)?;
        let found = page.block(block)?;
        let lies = found.source.bounds().context("a frame")?;
        let whole = Rect::covering(lies, 400, 400).context("pixels")?;
        page.write(found.mask.id, whole, &[255])?;
        page.write(found.cleanup.id, whole, &[1, 2, 3, 255])?;
        let lettering = Lettering {
            category: booharu_core::lettering::LetteringCategory::Shout,
            source: attrs::LetteringSource::User,
            confidence: None,
            role: None,
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
        assert_eq!(locked(split_block(&page.session, block)), Some(block));
        Ok(())
    }

    #[test]
    fn a_split_effect_kept_as_art_keeps_its_mark() -> Result<()> {
        use booharu_core::{
            protect::{self, Reason},
            runs::RunStage,
            settings::{LetteringSettings, SfxMode},
            sfx, styles,
        };
        let mut page = Fixture::new(400, 400, &[rect(0.0, 0.0, 400.0, 400.0)])?;
        let block = page.add(
            None,
            json!({"kind": "block", "role": "sfx", "source": {
                "frame": {"box": {"bounds": page_bounds(100.0, 20.0, 40.0, 100.0)}},
                "direction": "vertical",
                "content": "ドドド",
            }, "attributes": {"c2pa": {
                "detection": {"action": "c2pa.created"},
                "ocr": {"action": "c2pa.created"},
                "inpainting": {"action": "c2pa.created"},
            }}}),
        )?;
        // Kept as art: no translation, its clean-up hidden with the mark.
        let kept = sfx::kept_original(page.session.project().layer(block)?);
        assert!(!kept.is_empty());
        page.session.apply(edit(kept))?;
        let change = page.session.apply(split_block(&page.session, block)?)?;
        let halves = split_halves(block, &change)?;
        // The split's own run reads each half, which keeps the mark.
        for (&id, words) in halves.iter().zip(["ドド", "ド"]) {
            page.update(id, json!({"source": {"content": words}}))?;
            let read = styles::recognized(page.session.project().layer(id)?, words)?;
            page.session.apply(edit(vec![read]))?;
        }

        let skip = LetteringSettings::default();
        let all = LetteringSettings {
            sfx: SfxMode::TranslateAll,
            ..LetteringSettings::default()
        };
        let project = page.session.project();
        let on = project.page(page.page)?;
        for &id in &halves {
            let layer = project.layer(id)?;
            assert!(Auto::of(layer).kept);
            assert!(!page.block(id)?.cleanup.visible);
            // Left as art while the mode keeps it, and translated once it
            // doesn't, never taken for a clean-up hidden by hand.
            let translating = |settings| {
                protect::left_out(on, layer, RunStage::Translate, settings, false, false)
            };
            assert_eq!(translating(&skip), Some(Reason::SoundEffect));
            assert_eq!(translating(&all), None);
        }
        // The planner reads it the same way.
        assert!(crate::runs::to_translate(&page.session, &[page.page], &skip)?.is_empty());
        assert_eq!(
            crate::runs::to_translate(&page.session, &[page.page], &all)?,
            [page.page]
        );
        Ok(())
    }

    #[test]
    fn a_split_line_given_back_stays_given_back_so_its_run_translates_it() -> Result<()> {
        use booharu_core::{protect, runs::RunStage, settings::LetteringSettings, sfx};
        // Given back on its first translation, and on a later one, which
        // leaves the earlier one's record, which the split drops.
        for earlier in [false, true] {
            let mut page = Fixture::new(400, 400, &[rect(0.0, 0.0, 400.0, 400.0)])?;
            let block = page.add(
                None,
                json!({"kind": "block", "source": {
                    "frame": {"box": {"bounds": page_bounds(100.0, 20.0, 40.0, 100.0)}},
                    "direction": "vertical",
                    "content": "!?",
                }, "attributes": {"c2pa": {
                    "detection": {"action": "c2pa.created"},
                    "ocr": {"action": "c2pa.created"},
                }}}),
            )?;
            if earlier {
                page.session
                    .apply(edit(vec![made(block, "translation", "c2pa.translated")]))?;
            }
            // Given back unchanged, as the Translation stage leaves such a
            // line: no translation, its clean-up hidden, and only Booharu's
            // record of a translation of no text, as Koharu's stage records
            // none there.
            let given = booharu_core::styles::given_back(page.session.project().layer(block)?)?;
            page.session.apply(edit(vec![
                Op::UpdateLayer {
                    layer: block.into(),
                    patch: json!({"cleanup": {"visible": false}}),
                },
                given,
            ]))?;
            let change = page.session.apply(split_block(&page.session, block)?)?;
            let halves = split_halves(block, &change)?;
            let settings = LetteringSettings::default();
            for (&id, words) in halves.iter().zip(["!", "?"]) {
                // The split's own run reads each half.
                page.update(id, json!({"source": {"content": words}}))?;
                let read =
                    booharu_core::styles::recognized(page.session.project().layer(id)?, words)?;
                page.session.apply(edit(vec![read]))?;
                let project = page.session.project();
                let (on, layer, half) = (
                    project.page(page.page)?,
                    project.layer(id)?,
                    page.block(id)?,
                );
                assert!(!half.cleanup.visible && half.translation.is_none());
                assert!(!booharu_core::styles::recorded(layer, status::TRANSLATION));
                // Still a line given back, never a clean-up hidden by hand,
                // so the run translates it.
                assert!(status::given_back(layer, &half));
                assert!(!sfx::hand_kept(layer, &half));
                assert_eq!(
                    protect::left_out(on, layer, RunStage::Translate, &settings, false, false),
                    None
                );
            }
        }
        Ok(())
    }

    #[test]
    fn a_split_drops_the_blocks_reading_and_translation_for_its_run_to_write() -> Result<()> {
        use booharu_core::{
            protect::{self, Reason},
            runs::RunStage,
            settings::{LetteringSettings, QcSettings},
            styles::recorded,
        };
        let mut page = Fixture::new(400, 400, &[rect(0.0, 0.0, 400.0, 400.0)])?;
        let bounds = page_bounds(100.0, 20.0, 40.0, 100.0);
        let block = page.add(None, text_block(bounds, Direction::Vertical, "あいう"))?;
        // Detected, read, cleaned and translated by the stages.
        run_on(&mut page, block, "Who?")?;
        page.session
            .apply(edit(vec![made(block, "inpainting", "c2pa.created")]))?;
        let settings = LetteringSettings::default();
        assert!(crate::runs::plan(&page.session, &[page.page], &settings)?.is_empty());
        let before = page.session.history().current;

        let change = page.session.apply(split_block(&page.session, block)?)?;
        let halves = split_halves(block, &change)?;

        // Until the split's run reads and translates the halves, the planner
        // plans both, and the quality check takes neither for a block read
        // as empty.
        let plan = crate::runs::plan(&page.session, &[page.page], &settings)?;
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].stages, [Stage::Recognize, Stage::Translate]);
        let project = page.session.project();
        let renderer = koharu_renderer::Renderer::new(koharu_renderer::RendererConfig {
            font_families: Vec::new(),
        });
        let qc = QcSettings { min_size: 0.01 };
        let inputs = crate::qc::Inputs::new(&project, &qc, "en".parse()?, settings.clone(), None);
        let found = crate::qc::check_page(&renderer, &project, page.page, &inputs)?;
        assert!(found.is_empty(), "{found:?}");

        // Wording typed into a half before its run is typed by hand, which
        // the run leaves alone, as Redo boxes counts it.
        translate(&mut page, halves[0], "Wh")?;
        page.update(halves[1], json!({"source": {"content": "う"}}))?;
        let project = page.session.project();
        let on = project.page(page.page)?;
        for (id, stage) in [
            (halves[0], RunStage::Translate),
            (halves[1], RunStage::Recognize),
        ] {
            let layer = project.layer(id)?;
            assert_eq!(
                protect::left_out(on, layer, stage, &settings, false, false),
                Some(Reason::HandFixed)
            );
            let counts = crate::redo::count_redo(&project, &[id], &[stage], &settings)?;
            assert_eq!((counts.ready, counts.hand_fixed), (0, 1));
        }

        // The halves keep detection's and inpainting's records, and undo
        // gives the block back Koharu's records of reading and translating
        // it.
        for &id in &halves {
            let layer = project.layer(id)?;
            assert!(recorded(layer, status::DETECTION) && recorded(layer, status::INPAINTING));
            assert!(!recorded(layer, status::OCR) && !recorded(layer, status::TRANSLATION));
        }
        page.session.go_to(before)?;
        let layer = page.session.project().layer(block)?.clone();
        assert!(recorded(&layer, status::OCR) && recorded(&layer, status::TRANSLATION));
        Ok(())
    }

    #[test]
    fn detect_again_keeps_a_splits_halves_and_apply_restyles_them() -> Result<()> {
        use booharu_core::{protect, runs::RunStage, settings::LetteringSettings};
        let mut page = Fixture::new(400, 400, &[rect(0.0, 0.0, 400.0, 400.0)])?;
        let bounds = page_bounds(100.0, 20.0, 40.0, 100.0);
        let block = page.add(None, text_block(bounds, Direction::Vertical, "あいう"))?;
        // Detected, read, and translated by the stages.
        run_on(&mut page, block, "Who?")?;
        let keeps = |page: &Fixture, id: LayerId| -> Result<bool> {
            let project = page.session.project();
            Ok(protect::keeps(project.page(page.page)?, project.layer(id)?))
        };
        assert!(!keeps(&page, block)?);

        let change = page.session.apply(split_block(&page.session, block)?)?;
        let halves = split_halves(block, &change)?;
        let settings = LetteringSettings::default();
        for (&id, (words, text)) in halves.iter().zip([("あい", "Wh"), ("う", "o?")]) {
            // The split's own run reads and translates each half, which no
            // hand edit keeps it from.
            let project = page.session.project();
            let (on, layer) = (project.page(page.page)?, project.layer(id)?);
            for stage in [RunStage::Recognize, RunStage::Translate] {
                assert_eq!(
                    protect::left_out(on, layer, stage, &settings, false, false),
                    None
                );
            }
            page.update(id, json!({"source": {"content": words}}))?;
            run_on(&mut page, id, text)?;
            // Each half keeps the detection record it was copied with and is
            // marked as split, which Detect again keeps, and nothing about
            // it counts as changed by hand.
            let layer = page.session.project().layer(id)?.clone();
            assert!(booharu_core::styles::recorded(&layer, "detection"));
            assert!(Auto::of(&layer).split);
            assert_eq!(
                booharu_core::edited(&layer),
                booharu_core::Edited::default()
            );
            assert!(keeps(&page, id)?);
        }

        // A box drawn with the O tool, translated by the stage, is drawn by
        // hand, which Apply keeps; the halves, whose run wrote their look,
        // take the new font, as a split writes no wording or look of its own.
        let drawn = page.add_box(page_bounds(250.0, 20.0, 40.0, 100.0), OcrBoxMode::Read)?;
        stage_translates(&mut page, drawn, "Hey")?;
        assert!(booharu_core::edited(page.session.project().layer(drawn)?).drawn);
        let project = page.session.project();
        let mut named = halves.clone();
        named.push(drawn);
        let scope = crate::styles::StyleScope::Blocks { blocks: named };
        let font = Some("CCWildWords");
        let counted = crate::styles::count(&project, &scope, false, None, &settings, font)?;
        let (restyled, applied) = crate::styles::apply(
            &project,
            &scope,
            false,
            None,
            &settings,
            font,
            booharu_core::attrs::Origin::Automatic,
        )?;
        assert_eq!(counted, applied);
        assert_eq!(
            (
                applied.changed,
                applied.kept_hand_edited,
                applied.untranslated
            ),
            (2, 1, 0)
        );
        page.session.apply(restyled.context("Apply's edit")?)?;
        let family = |id: LayerId| -> Result<Option<String>> {
            Ok(page
                .block(id)?
                .translation
                .and_then(|translation| translation.style.chars.family))
        };
        for &half in &halves {
            assert_eq!(family(half)?.as_deref(), Some("CCWildWords"));
        }
        assert_ne!(family(drawn)?.as_deref(), Some("CCWildWords"));
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
            locked(review_note(
                &page.session.project(),
                block,
                Some("No".to_owned())
            )),
            Some(group)
        );
        Ok(())
    }

    #[test]
    fn a_locked_layer_is_named_as_the_layers_panel_names_it() -> Result<()> {
        let mut page = Fixture::new(100, 100, &[])?;
        let panel = page.add(
            None,
            json!({"kind": "panel", "outline": outline(0.0, 0.0, 100.0, 100.0)}),
        )?;
        let block = page.add(
            Some(panel),
            text_block(
                page_bounds(10.0, 10.0, 20.0, 20.0),
                Direction::Horizontal,
                "Hello\n there",
            ),
        )?;
        let refused = |page: &Fixture| -> String {
            let note = review_note(&page.session.project(), block, Some("No".to_owned()));
            note.err()
                .map(|error| error.to_string())
                .unwrap_or_default()
        };

        page.update(panel, json!({"locked": true}))?;
        assert_eq!(
            refused(&page),
            format!(
                "the panel is locked, so this can't change it or what it holds; unlock it in \
                 Layers first (layer {panel})"
            )
        );
        // A layer's own name, which a lock keeps from changing.
        page.update(panel, json!({"locked": false}))?;
        page.update(panel, json!({"name": "Page top"}))?;
        page.update(panel, json!({"locked": true}))?;
        assert!(refused(&page).starts_with("\"Page top\" is locked"));
        page.update(panel, json!({"locked": false}))?;
        page.update(block, json!({"locked": true}))?;
        assert!(refused(&page).starts_with("the text \"Hello there\" is locked"));

        // A part of a block, which a split writes to, by its block.
        page.update(block, json!({"locked": false}))?;
        let mask = page.block(block)?.mask.id;
        page.update(block, json!({"mask": {"locked": true}}))?;
        let split = split_block(&page.session, block);
        assert_eq!(
            split.err().map(|error| error.to_string()),
            Some(format!(
                "the mask of the text \"Hello there\" is locked, so this can't change it or what \
                 it holds; unlock it in Layers first (layer {mask})"
            ))
        );
        Ok(())
    }
}
