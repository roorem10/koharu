//! PSD export's report: beside each page's PSD, `{number}.psd.report.json`
//! lists the texts the export drew into pixels, as their words carry effects
//! of their own, each over a hidden type layer to edit (`koharu_psd::Notice`),
//! so they can be found in Photoshop. A page with none has no report, and
//! one an earlier export left there goes.

use std::{io::ErrorKind, path::Path};

use koharu_psd::{Labels, Notice};
use koharu_renderer::Renderer;
use koharu_scene::{PageId, Session};
use parking_lot::Mutex;

/// Writes `page` in `destination` as `{number}.psd`, as Koharu's export
/// does, holding the project only while the PSD is made, then its report
/// beside it ([`report`]).
pub(crate) fn psd(
    session: &Mutex<Session>,
    renderer: &Renderer,
    page: PageId,
    labels: &Labels,
    destination: &Path,
    number: &str,
) -> anyhow::Result<()> {
    let mut notices = Vec::new();
    let psd = koharu_psd::export_noting(&session.lock(), renderer, page, labels, &mut notices)?;
    fs_err::write(destination.join(format!("{number}.psd")), psd)?;
    report(destination, number, &notices)
}

/// Writes `notices` in `destination` as `{number}.psd.report.json`, a
/// pretty JSON array of 0.0.4's rows with the name each text's layers carry,
/// or with none removes the report an earlier export left there, if any.
fn report(destination: &Path, number: &str, notices: &[Notice]) -> anyhow::Result<()> {
    let path = destination.join(format!("{number}.psd.report.json"));
    if notices.is_empty() {
        match fs_err::remove_file(&path) {
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            removed => removed?,
        }
    } else {
        fs_err::write(&path, serde_json::to_vec_pretty(notices)?)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use koharu_scene::{LayerId, PageId, Uuid};

    use super::*;

    /// What an export of page 1 tells about the text `name` of layer
    /// `layer`, which it drew into pixels.
    fn rasterized(layer: u128, name: &str) -> Notice {
        Notice {
            editable_copy: "hidden_plain_styles",
            layer: LayerId(Uuid::from_u128(layer)),
            name: name.to_owned(),
            page: PageId(Uuid::from_u128(1)),
            reason: "word_effects_rasterized",
        }
    }

    #[test]
    fn texts_drawn_into_pixels_are_reported_beside_the_psd() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let notices = [rasterized(2, "Hello"), rasterized(3, "Wait…\nWhat?")];
        report(dir.path(), "007", &notices)?;
        let written = fs_err::read_to_string(dir.path().join("007.psd.report.json"))?;
        // 0.0.4's row, its keys in order, with the name the layers carry.
        let expected = indoc! {r#"
            [
              {
                "editable_copy": "hidden_plain_styles",
                "layer": "00000000-0000-0000-0000-000000000002",
                "name": "Hello",
                "page": "00000000-0000-0000-0000-000000000001",
                "reason": "word_effects_rasterized"
              },
              {
                "editable_copy": "hidden_plain_styles",
                "layer": "00000000-0000-0000-0000-000000000003",
                "name": "Wait…\nWhat?",
                "page": "00000000-0000-0000-0000-000000000001",
                "reason": "word_effects_rasterized"
              }
            ]"#};
        assert_eq!(written, expected);
        Ok(())
    }

    #[test]
    fn a_page_with_none_takes_away_the_report_an_earlier_export_left() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let stale = dir.path().join("001.psd.report.json");
        report(dir.path(), "001", &[rasterized(2, "Hello")])?;
        report(dir.path(), "002", &[rasterized(3, "Bye")])?;
        assert!(stale.exists());
        report(dir.path(), "001", &[])?;
        assert!(!stale.exists());
        // Another page's report stays.
        assert!(dir.path().join("002.psd.report.json").exists());
        Ok(())
    }

    #[test]
    fn a_page_with_none_and_no_report_writes_nothing() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        report(dir.path(), "001", &[])?;
        assert_eq!(fs_err::read_dir(dir.path())?.count(), 0);
        Ok(())
    }
}
