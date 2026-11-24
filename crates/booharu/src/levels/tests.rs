//! Page levels on images and on pages a session holds, in a project made in
//! a temporary folder: what [`level`] makes of each kind of image, what a
//! fix and a restore change, as one undo step each, what they skip, and
//! that a fix refused or stopped changes nothing.

use std::{cell::Cell, collections::BTreeMap};

use anyhow::Result;
use booharu_core::{
    attrs::{self, OriginalScan},
    cleanup::is_original_scan,
};
use image::{GenericImageView as _, GrayAlphaImage, LumaA, Rgb, Rgba, RgbaImage};
use koharu_renderer::{Gpu, Renderer, RendererConfig};
use koharu_scene::{Created, Layer, Page, Transform};
use serde_json::Value;

use super::*;
use crate::jobs::{self, JobKind, tests::REGISTRY};

/// A page's side, in pixels.
const SIDE: u32 = 16;

/// A faded scan: ink at 40, a mid grey at 120 and paper at 200, which
/// levels stretch to 0, 128 and 255.
fn faded() -> RgbImage {
    RgbImage::from_fn(SIDE, SIDE, |x, _| {
        Rgb(match x {
            0..4 => [40; 3],
            4..8 => [120; 3],
            _ => [200; 3],
        })
    })
}

/// A scan already black to white, which levels leave as it is.
fn full() -> RgbImage {
    RgbImage::from_fn(SIDE, SIDE, |x, _| Rgb([if x < 8 { 0 } else { 255 }; 3]))
}

/// `image` as PNG bytes.
fn encoded(image: impl Into<DynamicImage>) -> Vec<u8> {
    png(&image.into()).expect("a PNG")
}

/// The image `bytes` hold.
fn read(bytes: &[u8]) -> DynamicImage {
    image::load_from_memory(bytes).expect("an image")
}

/// The darkest and lightest of `image`'s colour channels.
fn range(image: &DynamicImage) -> (u8, u8) {
    let values: Vec<u8> = image.to_rgb8().pixels().flat_map(|pixel| pixel.0).collect();
    (
        values.iter().copied().min().unwrap_or(0),
        values.iter().copied().max().unwrap_or(255),
    )
}

#[test]
fn grey_and_colour_scans_reach_black_and_white_in_their_own_kind_and_size() -> Result<()> {
    let grey = DynamicImage::ImageLuma8(DynamicImage::ImageRgb8(faded()).into_luma8());
    let colour = DynamicImage::ImageRgb8(RgbImage::from_fn(SIDE, SIDE / 2, |x, _| {
        Rgb(if x < 4 { [40, 30, 20] } else { [210, 200, 190] })
    }));
    for source in [grey, colour] {
        let levelled = level(&encoded(source.clone()), &LevelsSettings::default())?
            .expect("a faded scan changes");
        let levelled = read(&levelled);
        assert_eq!(range(&levelled), (0, 255), "{:?}", source.color());
        // Grey is written grey, and colour without alpha as RGB, never RGBA.
        assert_eq!(levelled.color(), source.color());
        assert_eq!(levelled.dimensions(), source.dimensions());
    }
    Ok(())
}

#[test]
fn a_scans_alpha_is_kept() -> Result<()> {
    let colour = RgbaImage::from_fn(4, 2, |x, _| {
        Rgba(if x < 2 {
            [40, 40, 40, 100]
        } else {
            [200, 200, 200, 255]
        })
    });
    let levelled = level(&encoded(colour), &LevelsSettings::default())?.expect("a change");
    let levelled = read(&levelled);
    assert_eq!(levelled.color(), image::ColorType::Rgba8);
    let levelled = levelled.into_rgba8();
    assert_eq!(levelled.get_pixel(0, 0).0, [0, 0, 0, 100]);
    assert_eq!(levelled.get_pixel(3, 0).0, [255; 4]);
    let grey = GrayAlphaImage::from_fn(4, 2, |x, _| {
        LumaA(if x < 2 { [40, 100] } else { [200, 255] })
    });
    let levelled = level(&encoded(grey), &LevelsSettings::default())?.expect("a change");
    let levelled = read(&levelled);
    assert_eq!(levelled.color(), image::ColorType::La8, "grey stays grey");
    let levelled = levelled.into_luma_alpha8();
    assert_eq!(levelled.get_pixel(0, 0).0, [0, 100]);
    assert_eq!(levelled.get_pixel(3, 1).0, [255, 255]);
    Ok(())
}

#[test]
fn a_scan_levels_leave_as_it_is_gives_nothing() -> Result<()> {
    let settings = LevelsSettings::default();
    assert_eq!(level(&encoded(full()), &settings)?, None);
    // A second fix as the first is the identity.
    let once = level(&encoded(faded()), &settings)?.expect("a change");
    assert_eq!(level(&once, &settings)?, None);
    // With both off nothing changes, whatever the scan.
    let off = LevelsSettings {
        auto_levels: false,
        whiten: false,
        ..settings.clone()
    };
    assert_eq!(level(&encoded(faded()), &off)?, None);
    // Whitening alone turns paper above its threshold white, after levels
    // when both are on.
    let whiten = LevelsSettings {
        auto_levels: false,
        whiten: true,
        threshold: 150,
        ..settings.clone()
    };
    let whitened = read(&level(&encoded(faded()), &whiten)?.expect("whitened"));
    let whitened = whitened.to_rgb8();
    assert_eq!(whitened.get_pixel(0, 0).0, [40; 3]);
    assert_eq!(whitened.get_pixel(5, 0).0, [120; 3]);
    assert_eq!(whitened.get_pixel(12, 0).0, [255; 3]);
    let both = LevelsSettings {
        whiten: true,
        threshold: 100,
        ..settings.clone()
    };
    let both = read(&level(&encoded(faded()), &both)?.expect("levelled and whitened"));
    assert_eq!(
        both.to_rgb8().get_pixel(5, 0).0,
        [255; 3],
        "128 is above 100"
    );
    // Bytes that aren't an image fail, as does a clip a section wouldn't read.
    assert!(level(b"not an image", &settings).is_err());
    let clip = LevelsSettings {
        clip_percent: 50.0,
        ..settings
    };
    assert!(level(&encoded(faded()), &clip).is_err());
    Ok(())
}

#[test]
fn a_preview_measures_the_whole_scan_and_is_at_most_800_pixels() -> Result<()> {
    // Dots of ink too small to survive making the picture smaller still set
    // the black point.
    let dotted = RgbImage::from_fn(1600, 1200, |x, y| {
        Rgb(if x % 4 == 0 && y % 4 == 0 {
            [40; 3]
        } else {
            [200; 3]
        })
    });
    let dots = previewed(&encoded(dotted), &LevelsSettings::default())?;
    assert_eq!((dots.black, dots.white), (40, 200));
    let engine = base64::engine::general_purpose::STANDARD;
    for picture in [&dots.before, &dots.after] {
        let image = read(&engine.decode(picture)?);
        assert_eq!(image.dimensions(), (800, 600));
    }
    // A small scan isn't made larger.
    let small = previewed(&encoded(faded()), &LevelsSettings::default())?;
    let after = read(&engine.decode(&small.after)?);
    assert_eq!(after.dimensions(), (SIDE, SIDE));
    assert_eq!(range(&after), (0, 255));
    assert_eq!(range(&read(&engine.decode(&small.before)?)), (40, 200));
    // A page's preview reads its scan; a page with none has nothing to show.
    let fixture = Fixture::new();
    let page = fixture.page(Some(faded()), Vec::new())?.0;
    let shown = preview(&fixture.session, page, &LevelsSettings::default())?;
    assert_eq!(shown, small);
    let blank = fixture.page(None, Vec::new())?.0;
    assert!(preview(&fixture.session, blank, &LevelsSettings::default()).is_err());
    Ok(())
}

/// A project in a temporary folder, with its pages added by
/// [`page`](Self::page).
struct Fixture {
    dir: tempfile::TempDir,
    session: Mutex<Session>,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("a folder");
        let session = Session::create(dir.path().join("test.khr")).expect("a project");
        Self {
            dir,
            session: Mutex::new(session),
        }
    }

    /// A page `SIDE` pixels square showing `scan`, or with no scan, with the
    /// layers the ops given add on `$p`, by their labels, in one edit.
    fn page(
        &self,
        scan: Option<RgbImage>,
        layers: Vec<Value>,
    ) -> Result<(PageId, BTreeMap<String, LayerId>)> {
        let mut ops = Vec::new();
        let mut page = json!({"op": "add_page", "id": "$p", "index": 0,
            "size": {"width": SIDE, "height": SIDE}});
        if let Some(scan) = scan {
            let bytes = encoded(scan);
            page["size"] = Value::Null;
            page["background"] = json!(Hash::of(&bytes));
            ops.push(Op::AddFile { bytes });
        }
        ops.push(serde_json::from_value(page)?);
        for layer in layers {
            ops.push(serde_json::from_value(layer)?);
        }
        let change = self.session.lock().apply(Edit {
            label: "test".to_owned(),
            ops,
        })?;
        let labels = change
            .created
            .into_iter()
            .filter_map(|created| match created {
                Created::Layer {
                    id,
                    label: Some(label),
                } => Some((label, id)),
                _ => None,
            })
            .collect();
        Ok((change.pages.added[0], labels))
    }

    /// Applies `ops` as one edit, as the user would.
    fn edit(&self, ops: Vec<Value>) -> Result<()> {
        let ops = ops
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<Vec<Op>, _>>()?;
        self.session.lock().apply(Edit {
            label: "test".to_owned(),
            ops,
        })?;
        Ok(())
    }

    /// `page` as the project holds it now.
    fn now(&self, page: PageId) -> Page {
        self.session
            .lock()
            .project()
            .page(page)
            .expect("the page")
            .clone()
    }

    /// The file of `page`'s scan.
    fn scan(&self, page: PageId) -> Hash {
        self.now(page).background.expect("a scan").file
    }

    /// Every Original scan on `page`, at any depth.
    fn originals(&self, page: PageId) -> Vec<Layer> {
        let project = self.session.lock().project();
        project
            .layers_of(page)
            .expect("the page")
            .into_iter()
            .filter(|layer| is_original_scan(layer))
            .cloned()
            .collect()
    }

    /// The history's step labels.
    fn steps(&self) -> Vec<String> {
        self.session.lock().history().steps
    }

    /// Fixes `pages` as `settings` say, with no run going, as a Levels job.
    fn fix(&self, pages: &[PageId], settings: &LevelsSettings) -> Result<LevelsReport> {
        let job = jobs::start(JobKind::Levels)?;
        fix(&self.session, pages, settings, || false, &job, |_| {})
    }

    fn restore(&self, pages: &[PageId]) -> Result<LevelsReport> {
        restore(&self.session, pages, || false)
    }
}

/// The op adding `layer` as `$label` at `index` among the page's top level
/// layers, or among `parent`'s.
fn add(label: &str, parent: Option<&str>, index: usize, layer: Value) -> Value {
    json!({"op": "add_layer", "id": format!("${label}"), "page": "$p",
        "parent": parent.map(|parent| format!("${parent}")), "index": index, "layer": layer})
}

/// A block, with no clean-up yet, as Detect adds one.
fn block() -> Value {
    json!({"kind": "block", "source": {"content": "やあ"}})
}

/// The report of a fix or a restore that changed `changed` and skipped
/// `skipped`.
fn reported(changed: &[PageId], skipped: &[(PageId, LevelsSkip)]) -> LevelsReport {
    LevelsReport {
        changed: changed.to_vec(),
        skipped: skipped
            .iter()
            .map(|&(page, reason)| LevelsSkipped { page, reason })
            .collect(),
    }
}

#[test]
fn a_fix_is_one_undo_step_of_a_new_scan_and_one_original_under_everything() -> Result<()> {
    let _held = REGISTRY.blocking_lock();
    let fixture = Fixture::new();
    // A Draw stroke, which levels don't refuse.
    let (page, _) = fixture.page(
        Some(faded()),
        vec![add("drawn", None, 0, json!({"kind": "raster"}))],
    )?;
    let old = fixture.now(page).background.expect("a scan");
    let steps = fixture.steps();
    let job = jobs::start(JobKind::Levels)?;
    let mut told = Vec::new();
    let report = fix(
        &fixture.session,
        &[page],
        &LevelsSettings::default(),
        || false,
        &job,
        |progress| told.push(progress),
    )?;
    drop(job);
    assert_eq!(report, reported(&[page], &[]));
    assert_eq!(told, [LevelsProgress { done: 1, total: 1 }]);
    let mut after = steps.clone();
    after.push("booharu_levels".to_owned());
    assert_eq!(fixture.steps(), after, "one undo step");

    let levelled = fixture.now(page);
    let scan = levelled.background.expect("a scan");
    assert_ne!(scan.file, old.file);
    assert_eq!(scan.visible, old.visible);
    let bytes = fixture.session.lock().file(&scan.file)?;
    assert_eq!(range(&read(&bytes)), (0, 255));
    // The scan as imported, in one hidden, locked, marked image under
    // everything, the Draw stroke above it.
    assert_eq!(fixture.originals(page).len(), 1);
    let first = &levelled.layers[0];
    assert_eq!(levelled.layers.len(), 2);
    assert!(matches!(&first.content, Content::Image(image)
        if image.file == old.file && image.transform == Transform::default()));
    assert_eq!(first.name, ORIGINAL_SCAN);
    assert!(!first.visible && first.locked && first.children.is_empty());
    assert_eq!(attrs::get::<OriginalScan>(first), Some(OriginalScan(true)));
    assert_eq!(original_scan(&levelled), Some((first.id, old.file)));

    // Undo brings back the scan and takes the original away.
    fixture.session.lock().undo()?;
    let back = fixture.now(page);
    assert_eq!(back.background, Some(old));
    assert!(fixture.originals(page).is_empty());
    assert_eq!(back.layers.len(), 1);
    Ok(())
}

#[test]
fn a_second_fix_after_detect_keeps_the_scan_as_imported_as_the_one_original() -> Result<()> {
    let _held = REGISTRY.blocking_lock();
    let fixture = Fixture::new();
    let (page, _) = fixture.page(Some(faded()), Vec::new())?;
    let imported = fixture.scan(page);
    fixture.fix(&[page], &LevelsSettings::default())?;
    // Detect adds its blocks from the first place up, under what it keeps.
    fixture.edit(vec![
        json!({"op": "add_layer", "page": page, "index": 0, "layer": block()}),
    ])?;
    assert!(is_original_scan(&fixture.now(page).layers[1]));
    // Whitening the levelled mid grey changes the page again.
    let whiten = LevelsSettings {
        whiten: true,
        threshold: 100,
        ..LevelsSettings::default()
    };
    let levelled = fixture.scan(page);
    assert_eq!(fixture.fix(&[page], &whiten)?, reported(&[page], &[]));
    assert_ne!(fixture.scan(page), levelled);
    let originals = fixture.originals(page);
    assert_eq!(originals.len(), 1, "one original");
    assert!(matches!(&originals[0].content, Content::Image(image) if image.file == imported));
    // A third fix as the second changes nothing, and says so.
    let steps = fixture.steps();
    assert_eq!(
        fixture.fix(&[page], &whiten)?,
        reported(&[], &[(page, LevelsSkip::Unchanged)])
    );
    assert_eq!(fixture.steps(), steps);
    Ok(())
}

#[test]
fn a_restore_is_one_undo_step_back_to_the_scan_as_imported() -> Result<()> {
    let _held = REGISTRY.blocking_lock();
    let fixture = Fixture::new();
    let (page, _) = fixture.page(Some(faded()), Vec::new())?;
    let imported = fixture.now(page).background.expect("a scan");
    // Hidden by hand, the scan stays hidden through both.
    fixture.edit(vec![json!({"op": "set_background", "page": page,
        "background": {"file": imported.file, "visible": false}})])?;
    fixture.fix(&[page], &LevelsSettings::default())?;
    let levelled = fixture.scan(page);
    let steps = fixture.steps();
    assert_eq!(fixture.restore(&[page])?, reported(&[page], &[]));
    let mut after = steps.clone();
    after.push("booharu_restore_original".to_owned());
    assert_eq!(fixture.steps(), after, "one undo step");
    let restored = fixture.now(page);
    assert_eq!(
        restored.background,
        Some(Background {
            file: imported.file,
            visible: false,
        })
    );
    assert!(restored.layers.is_empty(), "the original's layer is gone");
    // Nothing left to put back.
    assert_eq!(
        fixture.restore(&[page])?,
        reported(&[], &[(page, LevelsSkip::NoOriginal)])
    );
    assert_eq!(fixture.steps(), after);
    // Undo gives the levelled page back, with its original.
    fixture.session.lock().undo()?;
    assert_eq!(fixture.scan(page), levelled);
    assert_eq!(fixture.originals(page).len(), 1);
    Ok(())
}

#[test]
fn neither_changes_a_page_with_work_made_from_its_scan_but_both_change_one_painted_by_hand()
-> Result<()> {
    let _held = REGISTRY.blocking_lock();
    let fixture = Fixture::new();
    let cleaned = vec![
        add("b", None, 0, block()),
        json!({"op": "write_pixels", "layer": "$b.cleanup",
            "rect": {"x": 0, "y": 0, "width": 2, "height": 2}, "pixels": vec![255u8; 2 * 2 * 4]}),
    ];
    let inpainted = json!({"kind": "raster", "attributes": {"c2pa": {
        "inpainting": {"action": "c2pa.created"}}}});
    let (clean_up, _) = fixture.page(Some(faded()), cleaned.clone())?;
    let (result, _) = fixture.page(Some(faded()), vec![add("r", None, 0, inpainted.clone())])?;
    let (blank, _) = fixture.page(None, Vec::new())?;
    let (painted, _) = fixture.page(
        Some(faded()),
        vec![
            add("paint", None, 0, json!({"kind": "raster"})),
            json!({"op": "write_pixels", "layer": "$paint",
                "rect": {"x": 0, "y": 0, "width": 2, "height": 2},
                "pixels": vec![255u8; 2 * 2 * 4]}),
        ],
    )?;
    // A block found but not cleaned yet holds no clean-up pixels.
    let (found, _) = fixture.page(Some(faded()), vec![add("b", None, 0, block())])?;
    let pages = [clean_up, result, blank, painted, found];
    let statuses = status(&fixture.session.lock(), &pages)?;
    let blocked: Vec<_> = statuses.iter().map(|status| status.blocked).collect();
    assert_eq!(
        blocked,
        [
            Some(LevelsSkip::Cleanup),
            Some(LevelsSkip::Inpainted),
            Some(LevelsSkip::NoScan),
            None,
            None
        ]
    );
    assert!(statuses.iter().all(|status| !status.original));
    let report = fixture.fix(&pages, &LevelsSettings::default())?;
    assert_eq!(
        report,
        reported(
            &[painted, found],
            &[
                (clean_up, LevelsSkip::Cleanup),
                (result, LevelsSkip::Inpainted),
                (blank, LevelsSkip::NoScan),
            ]
        )
    );
    let statuses = status(&fixture.session.lock(), &pages)?;
    let originals: Vec<_> = statuses.iter().map(|status| status.original).collect();
    assert_eq!(originals, [false, false, false, true, true]);

    // Work made from a levelled scan keeps it: a restore skips the page.
    fixture.edit(vec![
        json!({"op": "add_layer", "page": painted, "index": 2, "layer": inpainted}),
    ])?;
    let block = fixture.now(found).layers[1].id;
    let project = fixture.session.lock().project();
    let Content::Block(held) = &project.layer(block)?.content else {
        anyhow::bail!("not a block");
    };
    fixture.edit(vec![json!({"op": "write_pixels", "layer": held.cleanup.id,
        "rect": {"x": 0, "y": 0, "width": 2, "height": 2}, "pixels": vec![255u8; 2 * 2 * 4]})])?;
    let steps = fixture.steps();
    assert_eq!(
        fixture.restore(&[painted, found, blank])?,
        reported(
            &[],
            &[
                (painted, LevelsSkip::Inpainted),
                (found, LevelsSkip::Cleanup),
                (blank, LevelsSkip::NoOriginal),
            ]
        )
    );
    assert_eq!(fixture.steps(), steps, "nothing changed");
    Ok(())
}

#[test]
fn a_restore_leaves_an_original_held_by_a_locked_layer_or_holding_one() -> Result<()> {
    let fixture = Fixture::new();
    let levelled = encoded(full());
    let file = Hash::of(&encoded(faded()));
    let original = |locked: bool| {
        json!({"kind": "image", "file": file, "name": ORIGINAL_SCAN, "visible": false,
            "locked": locked, "attributes": {"booharu": {"original_scan": true}}})
    };
    // Pages showing a levelled scan, each with the original as imported:
    // in a locked group, holding a mask, and as it should be.
    let page = |layers: Vec<Value>| -> Result<PageId> {
        let (page, _) = fixture.page(Some(faded()), layers)?;
        let ops = vec![
            Op::AddFile {
                bytes: levelled.clone(),
            },
            Op::SetBackground {
                page: page.into(),
                background: Some(Background {
                    file: Hash::of(&levelled),
                    visible: true,
                }),
            },
        ];
        fixture.session.lock().apply(Edit {
            label: "test".to_owned(),
            ops,
        })?;
        Ok(page)
    };
    // The group is locked once it holds the original, as nothing goes into
    // a locked layer.
    let grouped = page(vec![
        add("group", None, 0, json!({"kind": "group"})),
        add("original", Some("group"), 0, original(true)),
        json!({"op": "update_layer", "layer": "$group", "patch": {"locked": true}}),
    ])?;
    let masked = page(vec![
        add("original", None, 0, original(false)),
        add("mask", Some("original"), 0, json!({"kind": "mask"})),
    ])?;
    let plain = page(vec![add("original", None, 0, original(true))])?;
    // A page whose scan is its original already.
    let (same, _) = fixture.page(
        Some(faded()),
        vec![add("original", None, 0, original(true))],
    )?;
    let report = fixture.restore(&[grouped, masked, plain, same])?;
    assert_eq!(
        report,
        reported(
            &[plain],
            &[
                (grouped, LevelsSkip::Locked),
                (masked, LevelsSkip::Locked),
                (same, LevelsSkip::Unchanged),
            ]
        )
    );
    assert_eq!(fixture.scan(plain), file);
    assert_eq!(fixture.originals(grouped).len(), 1);
    assert_eq!(fixture.originals(masked).len(), 1);
    Ok(())
}

#[test]
fn a_fix_refused_or_stopped_changes_nothing_and_its_files_are_swept() -> Result<()> {
    let _held = REGISTRY.blocking_lock();
    let fixture = Fixture::new();
    let (page, _) = fixture.page(Some(faded()), Vec::new())?;
    let old = fixture.scan(page);
    let steps = fixture.steps();
    let settings = LevelsSettings::default();
    let new = Hash::of(&level(&encoded(faded()), &settings)?.expect("a change"));
    let refused = |error: anyhow::Error| error.to_string().contains("while a run is going");

    // Refused as it starts: nothing is even levelled.
    let job = jobs::start(JobKind::Levels)?;
    let error = fix(&fixture.session, &[page], &settings, || true, &job, |_| {});
    assert!(error.is_err_and(refused));
    assert!(fixture.session.lock().file(&new).is_err());
    // A run queued while it levelled refuses its edit.
    let asked = Cell::new(0);
    let running = || {
        asked.set(asked.get() + 1);
        asked.get() > 1
    };
    let error = fix(&fixture.session, &[page], &settings, running, &job, |_| {});
    assert!(error.is_err_and(refused));
    assert_eq!(asked.get(), 2);
    // Stopped as it levelled.
    let stopped = |error: anyhow::Error| error.to_string() == STOPPED;
    let error = fix(
        &fixture.session,
        &[page],
        &settings,
        || false,
        &job,
        |_| {
            jobs::cancel(JobKind::Levels);
        },
    );
    assert!(error.is_err_and(stopped));
    drop(job);
    // Stopped before it started.
    let job = jobs::start(JobKind::Levels)?;
    job.cancel().cancel();
    let error = fix(&fixture.session, &[page], &settings, || false, &job, |_| {});
    assert!(error.is_err_and(stopped));
    drop(job);
    // Restore is refused while a run goes too.
    let error = restore(&fixture.session, &[page], || true);
    assert!(error.is_err_and(refused));
    // A page the project doesn't hold fails the whole fix.
    let gone = Fixture::new().page(None, Vec::new())?.0;
    assert!(fixture.fix(&[page, gone], &settings).is_err());
    assert!(fixture.restore(&[page, gone]).is_err());

    assert_eq!(fixture.steps(), steps);
    assert_eq!(fixture.scan(page), old);
    assert!(fixture.originals(page).is_empty());
    // What the first phase stored, no edit names, so it's swept at the next
    // open.
    assert!(fixture.session.lock().file(&new).is_ok(), "stored");
    let path = fixture.session.lock().path().to_owned();
    let Fixture { dir, session } = fixture;
    drop(session);
    let reopened = Session::open(&path)?;
    assert!(reopened.file(&new).is_err(), "swept");
    assert!(reopened.file(&old).is_ok());
    drop((reopened, dir));
    Ok(())
}

#[test]
fn a_scan_changed_while_it_was_levelled_is_left_as_it_is() -> Result<()> {
    let _held = REGISTRY.blocking_lock();
    let fixture = Fixture::new();
    let (first, _) = fixture.page(Some(faded()), Vec::new())?;
    let (second, _) = fixture.page(Some(faded()), Vec::new())?;
    let other = encoded(RgbImage::from_fn(SIDE, SIDE, |x, _| {
        Rgb([if x < 2 { 60 } else { 180 }; 3])
    }));
    let job = jobs::start(JobKind::Levels)?;
    // The first page's scan replaced as the fix levels the pages.
    let report = fix(
        &fixture.session,
        &[first, second, first],
        &LevelsSettings::default(),
        || false,
        &job,
        |progress| {
            if progress.done == 1 {
                let file = Hash::of(&other);
                let ops = vec![
                    Op::AddFile {
                        bytes: other.clone(),
                    },
                    Op::SetBackground {
                        page: first.into(),
                        background: Some(Background {
                            file,
                            visible: true,
                        }),
                    },
                ];
                fixture
                    .session
                    .lock()
                    .apply(Edit {
                        label: "test".to_owned(),
                        ops,
                    })
                    .expect("a new scan");
            }
        },
    )?;
    // Each page once, in the order asked.
    assert_eq!(
        report,
        reported(&[second], &[(first, LevelsSkip::ScanChanged)])
    );
    assert_eq!(fixture.scan(first), Hash::of(&other));
    assert!(fixture.originals(first).is_empty());
    assert_eq!(fixture.originals(second).len(), 1);
    Ok(())
}

#[test]
fn the_original_survives_closing_and_opening_the_project() -> Result<()> {
    let _held = REGISTRY.blocking_lock();
    let fixture = Fixture::new();
    let (page, _) = fixture.page(Some(faded()), Vec::new())?;
    let imported = fixture.scan(page);
    let bytes = fixture.session.lock().file(&imported)?;
    fixture.fix(&[page], &LevelsSettings::default())?;
    let levelled = fixture.scan(page);
    let path = fixture.session.lock().path().to_owned();
    let Fixture { dir, session } = fixture;
    drop(session);
    let reopened = Session::open(&path)?;
    assert_eq!(reopened.file(&imported)?, bytes, "kept byte for byte");
    assert!(reopened.file(&levelled).is_ok());
    let project = reopened.project();
    let page = project.page(page)?;
    assert_eq!(original_scan(page).map(|(_, file)| file), Some(imported));
    drop((reopened, dir));
    Ok(())
}

#[test]
fn the_page_draws_the_same_with_its_original_scan() -> Result<()> {
    let _held = REGISTRY.blocking_lock();
    let fixture = Fixture::new();
    let (page, _) = fixture.page(Some(faded()), Vec::new())?;
    fixture.fix(&[page], &LevelsSettings::default())?;
    let (original, _) = original_scan(&fixture.now(page)).expect("an original");
    let renderer = Renderer::new(RendererConfig {
        font_families: Vec::new(),
    });
    let frame = renderer.render(&fixture.session.lock(), page)?;
    // The original is in the frame, hidden, and the levelled scan shows.
    let drawn = frame.layer(original).expect("the original's node");
    assert!(!drawn.visible());
    assert!(frame.background().is_some_and(|scan| scan.visible()));
    if Gpu::get().is_err() {
        eprintln!("skipped drawing without a GPU");
        return Ok(());
    }
    let with = frame.rasterize()?;
    let without = frame.without(&[original]).rasterize()?;
    assert_eq!(with.image, without.image);
    Ok(())
}

#[test]
fn a_fix_cant_start_while_a_run_goes_or_pages_still_load() {
    use koharu_pipeline::{Outcome, RunId, Stage};

    // No Run press goes while the registry is held.
    let _held = REGISTRY.blocking_lock();
    let queued = Event::Queued {
        run: RunId(1),
        steps: 1,
        stages: vec![Stage::Inpaint],
        pages: false,
    };
    let refused =
        |told: &[Event], loading| startable(told, loading).expect_err("refused").to_string();
    assert_eq!(refused(std::slice::from_ref(&queued), false), RUN_GOING);
    assert_eq!(refused(&[], true), PAGES_LOADING);
    assert_eq!(refused(std::slice::from_ref(&queued), true), RUN_GOING);
    startable(&[], false).expect("with no run and every page in");
    let ended = Event::Ended {
        run: RunId(1),
        outcome: Outcome::Done,
    };
    startable(&[queued, ended], false).expect("once the run ended");
}
