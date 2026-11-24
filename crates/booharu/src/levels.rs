//! Page levels (D37): Fix page levels stretches a page's scan from its
//! darkest ink to its lightest paper, black to white, and can whiten its
//! paper, with booharu-rawprep's curve, as Booharu 0.0.4's raw clean-up did;
//! Restore original pages puts the scan back. A page's first fix keeps the
//! scan as it came in a hidden, locked Original scan layer under everything
//! (`booharu_core::cleanup::original_scan` finds it), which later fixes
//! keep, so Restore gives back the scan as it was imported. Levels come from
//! the page's scan as it is, so a second fix as the first is the identity,
//! and the page is reported unchanged.
//!
//! - [`level`] levels an image's bytes; [`preview`] shows what a fix would
//!   do to a page, and [`status`] which pages have an original and which a
//!   fix or a restore would skip now, by [`blocked`]'s rule.
//! - [`fix`] goes in two phases: each page's new scan is made and stored
//!   first, page by page, without the session's lock, then one edit,
//!   `booharu_levels`, one undo step, sets the new scans and adds the
//!   originals, under one lock, after checking each page again.
//! - [`restore`] puts each page's original back in one edit,
//!   `booharu_restore_original`.
//!
//! Both are refused while a run is going ([`RUN_GOING`]), and a fix while
//! another goes (`JobKind::Levels`). The desktop's command and the agent's
//! tool refuse a fix while Koharu's Gallery still loads pages into the
//! chapter, with [`PAGES_LOADING`], before they lock the session
//! ([`startable`]); Restore isn't refused then.

use std::{collections::BTreeSet, io::Cursor};

use anyhow::{Context as _, Result, ensure};
use base64::Engine as _;
use booharu_core::{
    cleanup::original_scan,
    settings::{LevelsSettings, MAX_CLIP_PERCENT},
    styles,
};
use booharu_rawprep::{auto_levels, levels_stats, whiten_paper};
use image::{DynamicImage, GrayImage, ImageFormat, ImageReader, Luma, RgbImage};
use koharu_pipeline::Event;
use koharu_scene::{Background, Content, Edit, Hash, LayerId, Op, PageId, Project, Session};
use parking_lot::Mutex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{jobs::Job, runs};

/// Why a fix is refused while pages are still loading into the chapter, as
/// Koharu's Gallery loads a chapter's once it opened the chapter as the
/// project: what the desktop's command and the agent's tool both answer,
/// each checking before it locks the session.
pub const PAGES_LOADING: &str = "Fix page levels can't start while pages are still loading into this chapter; wait until they're in.";

/// Why a fix or a restore is refused while a run is going, or a Run press.
pub const RUN_GOING: &str =
    "Pages can't change while a run is going; wait for it to end, or stop it.";

/// What a fix that was stopped answers: it changed no page.
pub const STOPPED: &str = "Fix page levels was stopped; no page changed.";

/// The name of the layer a page's first fix keeps its scan in, which the
/// interface shows in the user's language instead.
pub const ORIGINAL_SCAN: &str = "Original scan";

/// The longest side, in pixels, of a preview's pictures.
pub const PREVIEW_SIDE: u32 = 800;

/// The label of a fix's edit, as the history shows it.
const FIXED: &str = "booharu_levels";
/// The label of a restore's edit.
const RESTORED: &str = "booharu_restore_original";

/// Why a fix or a restore left a page as it was, which the interface words in
/// the user's language.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, specta::Type, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LevelsSkip {
    /// The page has no scan.
    NoScan,
    /// A block's clean-up has pixels, made from the scan as it is.
    Cleanup,
    /// A raster the Inpainting stage made is on the page, a Remove stroke's
    /// or a Generative Fill's, made from the scan as it is.
    Inpainted,
    /// Levelling changes no pixel, or the scan already is the original.
    Unchanged,
    /// The scan changed while the fix levelled it.
    ScanChanged,
    /// The page has no Original scan to put back.
    NoOriginal,
    /// The Original scan is inside a locked layer, or holds layers of its
    /// own.
    Locked,
}

/// A page a fix or a restore left as it was, and why.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, specta::Type, JsonSchema)]
pub struct LevelsSkipped {
    pub page: PageId,
    pub reason: LevelsSkip,
}

/// What a fix or a restore did: the pages it changed and those it left as
/// they were, each in the order they were asked for.
#[derive(
    Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, specta::Type, JsonSchema,
)]
pub struct LevelsReport {
    pub changed: Vec<PageId>,
    pub skipped: Vec<LevelsSkipped>,
}

/// What a fix would do to a page: the black and white points its levels
/// take, measured on the whole scan, and the scan before and after, each a
/// base64 PNG at most [`PREVIEW_SIDE`] pixels on its longer side.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct LevelsPreview {
    pub black: u8,
    pub white: u8,
    pub before: String,
    pub after: String,
}

/// A page as the page menu reads it, to show Restore original scan: whether
/// it has an Original scan to restore, and why a fix or a restore would
/// skip it now.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct LevelsStatus {
    pub page: PageId,
    pub original: bool,
    pub blocked: Option<LevelsSkip>,
}

/// How far a fix's first phase is: `done` of its `total` pages levelled,
/// told after each page.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct LevelsProgress {
    pub done: u32,
    pub total: u32,
}

/// `bytes`, an image, levelled as `settings` say: rawprep's auto levels,
/// when on, then its paper whitening, when on, on its colour, its alpha
/// kept, as a PNG. A grey image is written grey, as the curve is the same in
/// every channel, and one without alpha without it, so an RGB scan stays
/// RGB. None when no pixel changes, and, reading nothing, when both are off.
/// Fails on bytes that aren't an image, and on a clip outside 0 to 49.9.
pub fn level(bytes: &[u8], settings: &LevelsSettings) -> Result<Option<Vec<u8>>> {
    checked(settings)?;
    if !settings.auto_levels && !settings.whiten {
        return Ok(None);
    }
    let source = decoded(bytes)?;
    let colour = source.to_rgb8();
    let levelled = adjusted(&colour, settings);
    if levelled == colour {
        return Ok(None);
    }
    Ok(Some(png(&replaced(&source, levelled))?))
}

/// What a fix as `settings` say would do to `page`'s scan in the project
/// `session` holds. Fails on a page with no scan.
pub fn preview(
    session: &Mutex<Session>,
    page: PageId,
    settings: &LevelsSettings,
) -> Result<LevelsPreview> {
    checked(settings)?;
    let bytes = {
        let session = session.lock();
        let project = session.project();
        let background = project
            .page(page)?
            .background
            .context("this page has no scan to level")?;
        session.file(&background.file)?
    };
    previewed(&bytes, settings)
}

/// What a fix as `settings` say would do to `bytes`, an image: the points
/// from the whole image, as its levels take them, and both pictures made
/// smaller.
fn previewed(bytes: &[u8], settings: &LevelsSettings) -> Result<LevelsPreview> {
    let source = decoded(bytes)?;
    let colour = source.to_rgb8();
    let stats = levels_stats(&colour, settings.clip_percent);
    let after = replaced(&source, adjusted(&colour, settings));
    let picture = |image: DynamicImage| -> Result<String> {
        let smaller = match image.width().max(image.height()) > PREVIEW_SIDE {
            true => image.thumbnail(PREVIEW_SIDE, PREVIEW_SIDE),
            false => image,
        };
        Ok(base64::engine::general_purpose::STANDARD.encode(png(&smaller)?))
    };
    Ok(LevelsPreview {
        black: stats.black,
        white: stats.white,
        before: picture(source)?,
        after: picture(after)?,
    })
}

/// Each of `pages` the project `session` holds, as the page menu reads it,
/// decoding nothing; a page the project doesn't hold is left out, as it was
/// removed.
pub fn status(session: &Session, pages: &[PageId]) -> Result<Vec<LevelsStatus>> {
    let project = session.project();
    let mut statuses = Vec::new();
    for &page in &distinct(pages) {
        let Ok(at) = project.page(page) else {
            continue;
        };
        statuses.push(LevelsStatus {
            page,
            original: original_scan(at).is_some(),
            blocked: blocked(session, page)?,
        });
    }
    Ok(statuses)
}

/// Why a fix or a restore would skip `page` of the project `session` holds,
/// whatever else it finds there: it has no scan, or work was made from its
/// scan as it is, a block's clean-up with pixels or a raster the Inpainting
/// stage made, which a new scan would no longer match. A page with rasters
/// painted by hand isn't blocked.
pub fn blocked(session: &Session, page: PageId) -> Result<Option<LevelsSkip>> {
    let project = session.project();
    if project.page(page)?.background.is_none() {
        return Ok(Some(LevelsSkip::NoScan));
    }
    let empty = Hash::of(&[]);
    for layer in project.layers_of(page)? {
        match &layer.content {
            Content::Block(block) if session.tiles_hash(block.cleanup.id)? != empty => {
                return Ok(Some(LevelsSkip::Cleanup));
            }
            Content::Raster(_) if styles::recorded(layer, "inpainting") => {
                return Ok(Some(LevelsSkip::Inpainted));
            }
            _ => {}
        }
    }
    Ok(None)
}

/// Levels `pages` of the project `session` holds as `settings` say, as one
/// undo step, `booharu_levels`, answering what it changed and what it
/// skipped. `running` tells whether a run is going on the pipeline, as
/// `runs::run_going` of its events does; a Run press going refuses the fix
/// too (`runs::group_going`), asked only while the session's lock isn't
/// held. `job` is the fix's `JobKind::Levels` job, whose cancel stops it;
/// `progress` is told after each page of its first phase.
///
/// The first phase levels each page in turn, holding the session's lock
/// only to read its scan and to store the new one, as an edit holding only
/// the file, which adds no undo step; the file is swept at the next open if
/// no edit names it. The second, refused while a Run press goes, then
/// under one lock, refused while a run goes, checks each levelled page
/// again, its scan unchanged and nothing made from it since, and applies one
/// edit: each page's new scan, visible as the old one was, and, on a page
/// with no Original scan yet, a hidden, locked Original scan layer under
/// everything holding the old one. Refused, as stopped, or with a page the
/// project doesn't hold, it changes nothing.
pub fn fix(
    session: &Mutex<Session>,
    pages: &[PageId],
    settings: &LevelsSettings,
    running: impl Fn() -> bool,
    job: &Job,
    mut progress: impl FnMut(LevelsProgress),
) -> Result<LevelsReport> {
    checked(settings)?;
    ensure!(!(running() || pressed()), RUN_GOING);
    let pages = distinct(pages);
    held(&session.lock().project(), &pages)?;
    let total = u32::try_from(pages.len()).unwrap_or(u32::MAX);
    let mut skipped = Vec::new();
    // Each page levelled: the page, its scan as read, and its new one.
    let mut levelled = Vec::new();
    for (done, &page) in (1..).zip(&pages) {
        ensure!(!job.cancelled(), STOPPED);
        match made(session, page, settings)? {
            Ok((old, new)) => levelled.push((page, old, new)),
            Err(reason) => skipped.push(LevelsSkipped { page, reason }),
        }
        progress(LevelsProgress { done, total });
    }
    ensure!(!pressed(), RUN_GOING);
    let mut session = session.lock();
    ensure!(!job.cancelled(), STOPPED);
    ensure!(!running(), RUN_GOING);
    let project = session.project();
    let (mut changed, mut ops) = (Vec::new(), Vec::new());
    for (page, old, new) in levelled {
        let background = project.page(page).ok().and_then(|at| at.background);
        let Some(background) = background.filter(|background| background.file == old) else {
            skipped.push(LevelsSkipped {
                page,
                reason: LevelsSkip::ScanChanged,
            });
            continue;
        };
        if let Some(reason) = blocked(&session, page)? {
            skipped.push(LevelsSkipped { page, reason });
            continue;
        }
        ops.push(Op::SetBackground {
            page: page.into(),
            background: Some(Background {
                file: new,
                visible: background.visible,
            }),
        });
        if original_scan(project.page(page)?).is_none() {
            ops.push(original(page, old)?);
        }
        changed.push(page);
    }
    if !ops.is_empty() {
        session.apply(Edit {
            label: FIXED.to_owned(),
            ops,
        })?;
    }
    Ok(report(&pages, changed, skipped))
}

/// `page`'s scan levelled as `settings` say and stored, with the hash of the
/// scan it read and of the new one; or why the fix skips it.
fn made(
    session: &Mutex<Session>,
    page: PageId,
    settings: &LevelsSettings,
) -> Result<std::result::Result<(Hash, Hash), LevelsSkip>> {
    let (old, bytes) = {
        let session = session.lock();
        if let Some(reason) = blocked(&session, page)? {
            return Ok(Err(reason));
        }
        let project = session.project();
        let old = project
            .page(page)?
            .background
            .context("a page with no scan is blocked")?
            .file;
        (old, session.file(&old)?)
    };
    let Some(png) = level(&bytes, settings)? else {
        return Ok(Err(LevelsSkip::Unchanged));
    };
    drop(bytes);
    let new = Hash::of(&png);
    // A file alone changes nothing, so it adds no undo step.
    session.lock().apply(Edit {
        label: FIXED.to_owned(),
        ops: vec![Op::AddFile { bytes: png }],
    })?;
    Ok(Ok((old, new)))
}

/// The op adding `page`'s Original scan, holding `file`: hidden, locked and
/// marked, at the top level's first place, drawn first, under everything.
fn original(page: PageId, file: Hash) -> Result<Op> {
    Ok(serde_json::from_value(json!({
        "op": "add_layer",
        "page": page,
        "index": 0,
        "layer": {
            "kind": "image",
            "file": file,
            "name": ORIGINAL_SCAN,
            "visible": false,
            "locked": true,
            "attributes": {"booharu": {"original_scan": true}},
        },
    }))?)
}

/// Puts back the Original scan of each of `pages` the project `session`
/// holds, as one undo step, `booharu_restore_original`, answering what it
/// changed and what it skipped. `running` tells whether a run is going, as
/// [`fix`] takes it, and a Run press going refuses it too, asked before the
/// session's lock is taken, as a fix asks it. A page is skipped with no
/// Original scan, when it's blocked ([`blocked`]), when its original is
/// inside a locked layer or holds layers of its own, which removing it
/// would take, or when its scan already is the original. On the others, the
/// original's layer goes, and its file is the page's scan again, visible as
/// the scan was. Refused, or with a page the project doesn't hold, it
/// changes nothing.
pub fn restore(
    session: &Mutex<Session>,
    pages: &[PageId],
    running: impl Fn() -> bool,
) -> Result<LevelsReport> {
    ensure!(!pressed(), RUN_GOING);
    let mut session = session.lock();
    ensure!(!running(), RUN_GOING);
    let pages = distinct(pages);
    let project = session.project();
    held(&project, &pages)?;
    let (mut changed, mut skipped, mut ops) = (Vec::new(), Vec::new(), Vec::new());
    for &page in &pages {
        match restored(&session, &project, page)? {
            Ok(restoring) => {
                ops.extend(restoring);
                changed.push(page);
            }
            Err(reason) => skipped.push(LevelsSkipped { page, reason }),
        }
    }
    if !ops.is_empty() {
        session.apply(Edit {
            label: RESTORED.to_owned(),
            ops,
        })?;
    }
    Ok(report(&pages, changed, skipped))
}

/// The ops putting `page`'s Original scan back, or why a restore skips it.
fn restored(
    session: &Session,
    project: &Project,
    page: PageId,
) -> Result<std::result::Result<Vec<Op>, LevelsSkip>> {
    let at = project.page(page)?;
    let Some((layer, file)) = original_scan(at) else {
        return Ok(Err(LevelsSkip::NoOriginal));
    };
    if let Some(reason) = blocked(session, page)? {
        return Ok(Err(reason));
    }
    if !project.layer(layer)?.children.is_empty() || locked_above(project, layer)? {
        return Ok(Err(LevelsSkip::Locked));
    }
    let Some(background) = at.background.filter(|background| background.file != file) else {
        return Ok(Err(LevelsSkip::Unchanged));
    };
    Ok(Ok(vec![
        Op::UpdateLayer {
            layer: layer.into(),
            patch: json!({"locked": false}),
        },
        Op::RemoveLayer {
            layer: layer.into(),
        },
        Op::SetBackground {
            page: page.into(),
            background: Some(Background {
                file,
                visible: background.visible,
            }),
        },
    ]))
}

/// Whether a layer holding `layer`, at any depth, is locked.
fn locked_above(project: &Project, layer: LayerId) -> Result<bool> {
    let mut at = layer;
    loop {
        match project.parent(at)? {
            (_, Some(parent)) if parent.locked => return Ok(true),
            (_, Some(parent)) => at = parent.id,
            (_, None) => return Ok(false),
        }
    }
}

/// Whether a fix may start now, with the events the pipeline `told`, as
/// `Pipeline::told` gives them, and whether pages are still `loading` into
/// the open project, as Koharu's Gallery loads a chapter's once it opened
/// the chapter as the project: not while a run goes or a Run press does
/// ([`RUN_GOING`]), nor while pages are still loading ([`PAGES_LOADING`]),
/// as a fix levels only the pages the project has as it starts. Asked
/// before the session's lock is taken, as the app tells whether pages load
/// under the open project's.
pub fn startable(told: &[Event], loading: bool) -> Result<()> {
    ensure!(!(runs::run_going(told) || pressed()), RUN_GOING);
    ensure!(!loading, PAGES_LOADING);
    Ok(())
}

/// Whether a Run press, or a group of runs carrying on, is going: it holds
/// the group's place (`runs::group_going`). Asked only while the session's
/// lock isn't held, never under it: a press takes the place, then the
/// session's lock as it plans and queues, as a group's follower does as it
/// goes on, so asking for the place while holding the lock could wait on
/// one waiting on it, for good. A press that takes the place once this was
/// asked plans once the edit is in, as if the fix or the restore had ended
/// first, unless its run was queued meanwhile, which `running` then tells.
fn pressed() -> bool {
    runs::group_going().is_some()
}

/// Fails on a clip a `[booharu-levels]` section wouldn't read.
fn checked(settings: &LevelsSettings) -> Result<()> {
    let clip = settings.clip_percent;
    ensure!(
        clip.is_finite() && (0.0..=MAX_CLIP_PERCENT).contains(&clip),
        "clipping {clip}% of a page's pixels at each end of its levels isn't from 0 to \
         {MAX_CLIP_PERCENT}%"
    );
    Ok(())
}

/// Fails on a page of `pages` that `project` doesn't hold.
fn held(project: &Project, pages: &[PageId]) -> Result<()> {
    for &page in pages {
        project.page(page)?;
    }
    Ok(())
}

/// `pages` with each page once, in their order.
fn distinct(pages: &[PageId]) -> Vec<PageId> {
    let mut seen = BTreeSet::new();
    pages
        .iter()
        .copied()
        .filter(|page| seen.insert(*page))
        .collect()
}

/// The report of `changed` and `skipped`, each in the order of `pages`.
fn report(pages: &[PageId], changed: Vec<PageId>, mut skipped: Vec<LevelsSkipped>) -> LevelsReport {
    let place = |page: &PageId| pages.iter().position(|asked| asked == page);
    skipped.sort_by_key(|left| place(&left.page));
    LevelsReport { changed, skipped }
}

/// The image `bytes` hold, in whatever format they are.
fn decoded(bytes: &[u8]) -> Result<DynamicImage> {
    ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()?
        .decode()
        .context("the page's scan isn't an image that reads")
}

/// `colour` levelled as `settings` say, then whitened, when they say so.
fn adjusted(colour: &RgbImage, settings: &LevelsSettings) -> RgbImage {
    let levelled = match settings.auto_levels {
        true => auto_levels(colour, settings.clip_percent),
        false => colour.clone(),
    };
    match settings.whiten {
        true => whiten_paper(&levelled, settings.threshold),
        false => levelled,
    }
}

/// `source` with `colour` as its colour, in its own kind: grey stays grey,
/// written from one channel, as every channel of a grey levelled is the
/// same, and its alpha, if it has one, stays as it was.
fn replaced(source: &DynamicImage, colour: RgbImage) -> DynamicImage {
    let kind = source.color();
    match (kind.has_color(), kind.has_alpha()) {
        (true, false) => DynamicImage::ImageRgb8(colour),
        (true, true) => {
            let mut image = source.to_rgba8();
            for (pixel, colour) in image.pixels_mut().zip(colour.pixels()) {
                pixel.0[..3].copy_from_slice(&colour.0);
            }
            DynamicImage::ImageRgba8(image)
        }
        (false, false) => DynamicImage::ImageLuma8(GrayImage::from_fn(
            colour.width(),
            colour.height(),
            |x, y| Luma([colour.get_pixel(x, y).0[0]]),
        )),
        (false, true) => {
            let mut image = source.to_luma_alpha8();
            for (pixel, colour) in image.pixels_mut().zip(colour.pixels()) {
                pixel.0[0] = colour.0[0];
            }
            DynamicImage::ImageLumaA8(image)
        }
    }
}

/// `image` as PNG bytes.
fn png(image: &DynamicImage) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    image.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests;
