//! Booharu: a chapter read again into a project whose pages Fix page levels
//! changed since. `place` finds a levelled page by the Original scan it
//! keeps, the image the chapter's page still is, as well as by its scan, so
//! reading the chapter again adds none of its pages a second time.

use std::io::Cursor;

use anyhow::Result;
use booharu::{
    jobs::{self, JobKind},
    levels,
};
use booharu_core::settings::LevelsSettings;
use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
use koharu_scene::{Hash, PageId, Session};
use parking_lot::Mutex;

use super::place;

/// A faded scan, as PNG bytes: ink at `ink`, a mid grey at 120 and paper at
/// 200, which page levels stretch to black and white.
fn faded(ink: u8) -> Vec<u8> {
    let image = RgbImage::from_fn(16, 16, |x, _| {
        Rgb(match x {
            0..4 => [ink; 3],
            4..8 => [120; 3],
            _ => [200; 3],
        })
    });
    let mut bytes = Vec::new();
    DynamicImage::from(image)
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .expect("a PNG");
    bytes
}

/// The file of `page`'s scan, and how many pages the project has.
fn scan(session: &Mutex<Session>, page: PageId) -> (Hash, usize) {
    let project = session.lock().project();
    let scan = project.page(page).expect("the page").background;
    (scan.expect("a scan").file, project.pages().count())
}

#[test]
fn a_page_levelled_since_is_the_page_its_chapter_reads_again() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let session = Mutex::new(Session::create(dir.path().join("Ch.1.khr"))?);
    let original = faded(40);
    let page = place(&session, original.clone(), None)?;
    let job = jobs::start(JobKind::Levels)?;
    let report = levels::fix(
        &session,
        &[page],
        &LevelsSettings::default(),
        || false,
        &job,
        |_| {},
    )?;
    drop(job);
    assert_eq!(report.changed, [page]);
    let (levelled, _) = scan(&session, page);
    assert_ne!(levelled, Hash::of(&original));

    // Read again, the chapter's page is the page levelled, which keeps it
    // as its Original scan, and the project adds none.
    assert_eq!(place(&session, original, None)?, page);
    assert_eq!(scan(&session, page), (levelled, 1));
    // Its levelled scan finds it too, as Koharu's own match does.
    let bytes = session.lock().file(&levelled)?;
    assert_eq!(place(&session, bytes, None)?, page);
    assert_eq!(scan(&session, page), (levelled, 1));

    // Another image is a page of its own, added after the one before it.
    let other = place(&session, faded(50), Some(page))?;
    assert_ne!(other, page);
    let project = session.lock().project();
    let pages: Vec<PageId> = project.pages().map(|page| page.id).collect();
    assert_eq!(pages, [page, other]);
    Ok(())
}
