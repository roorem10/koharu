//! The quality check on pages a session holds: what [`check_page`] finds,
//! measured with a renderer that sets Latin text in installed fonts, so no
//! test needs a CJK or named font or downloads one, the glossary slips
//! [`check_glossary`] finds without one, and the edits that mark findings
//! OK and flag lines.

use std::collections::BTreeMap;

use anyhow::Result;
use booharu_core::{
    attrs::{self, Flag, Flags, QcOk},
    context::GlossaryEntry,
    qc::fingerprint,
    settings::SfxMode,
    styles,
};
use koharu_pipeline::Stage;
use koharu_renderer::{Renderer, RendererConfig};
use koharu_scene::{Change, Created, Edit, Op, Session, Transform};
use serde_json::{Value, json};

use super::*;
use crate::boxes::Refusal;

/// A renderer that sets text in installed fonts, so no test downloads one.
fn renderer() -> Renderer {
    Renderer::new(RendererConfig {
        font_families: Vec::new(),
    })
}

/// A page a session holds, with its layers by the names their ops gave.
struct Fixture {
    _dir: tempfile::TempDir,
    session: Session,
    page: PageId,
    layers: BTreeMap<String, LayerId>,
}

impl Fixture {
    /// A page `width` by `height` holding the layers `layers` add.
    fn new(width: u32, height: u32, layers: Vec<Value>) -> Self {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        let mut ops = vec![json!({"op": "add_page", "id": "$page", "index": 0,
            "size": {"width": width, "height": height}})];
        ops.extend(layers);
        let change = session.apply(edit(ops)).expect("a page");
        let page = change.pages.added[0];
        let layers = change
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
        Self {
            _dir: dir,
            session,
            page,
            layers,
        }
    }

    fn id(&self, name: &str) -> LayerId {
        self.layers[name]
    }

    fn project(&self) -> Project {
        self.session.project()
    }

    fn update(&mut self, name: &str, patch: Value) {
        let op = json!({"op": "update_layer", "layer": self.id(name), "patch": patch});
        self.session.apply(edit(vec![op])).expect("an update");
    }

    /// Applies `built`, an edit a builder made, when there is one.
    fn apply(&mut self, built: Option<Edit>) -> Option<Change> {
        built.map(|edit| self.session.apply(edit).expect("the edit"))
    }

    fn steps(&self) -> Vec<String> {
        self.session.history().steps
    }

    fn layer(&self, name: &str) -> Layer {
        self.project()
            .layer(self.id(name))
            .expect("the layer")
            .clone()
    }

    /// The findings on the page, with `inputs` of [`inputs`].
    fn check(&self) -> Vec<Finding> {
        let project = self.project();
        check_page(&renderer(), &project, self.page, &inputs(&project)).expect("a check")
    }

    /// The checks found on the layer `name`, each with whether it's hidden.
    fn found(&self, name: &str) -> Vec<(String, bool)> {
        let id = self.id(name);
        self.check()
            .into_iter()
            .filter(|finding| finding.layer == id)
            .map(|finding| (finding.check, finding.hidden))
            .collect()
    }
}

fn edit(ops: Vec<Value>) -> Edit {
    Edit {
        label: "test".to_owned(),
        ops: ops
            .into_iter()
            .map(|op| serde_json::from_value::<Op>(op).expect("an op"))
            .collect(),
    }
}

/// What the checks read beside a page in these tests: the smallest
/// readable text 1% of the page's shorter side, English the target, and no
/// series.
fn inputs(project: &Project) -> Inputs {
    Inputs::new(
        project,
        &QcSettings { min_size: 0.01 },
        "en".parse().expect("a language"),
        LetteringSettings::default(),
        None,
    )
}

/// A box frame at `x`, `y`, `width` by `height`, fitting its text by `fit`.
fn boxed(x: f64, y: f64, width: f64, height: f64, fit: &str) -> Value {
    json!({"box": {"bounds": {"x": x, "y": y, "width": width, "height": height}, "fit": fit}})
}

/// The `c2pa` records of Koharu's stages `steps`.
fn c2pa(steps: &[&str]) -> Value {
    let steps: serde_json::Map<String, Value> = steps
        .iter()
        .map(|step| ((*step).to_owned(), json!({"action": "c2pa.created"})))
        .collect();
    json!({"c2pa": steps})
}

/// The op adding a block named `name` at `index` of the page, its source
/// `source`, its translation `translation`, which may be null, and the
/// attributes `attributes`.
fn block(name: &str, index: usize, source: &str, translation: Value, attributes: Value) -> Value {
    let y = 10.0 + 60.0 * index as f64;
    json!({"op": "add_layer", "id": format!("${name}"), "page": "$page", "index": index,
        "layer": {"kind": "block", "attributes": attributes, "translation": translation,
            "source": {"content": source, "frame": boxed(10.0, y, 200.0, 50.0, "none")}}})
}

/// An English translation reading `text` in the frame `frame`, set in
/// `family`, or the default font for none.
fn translation(text: &str, frame: Value, family: Option<&str>) -> Value {
    json!({"content": text, "frame": frame, "language": "en",
        "style": {"chars": {"family": family, "size": 24}}})
}

/// A block read and translated by Koharu's stages, its translation large
/// enough to read.
fn translated(name: &str, index: usize, text: &str) -> Value {
    block(
        name,
        index,
        "原文",
        translation(text, boxed(10.0, 10.0, 300.0, 150.0, "best"), None),
        c2pa(&["detection", "ocr", "translation"]),
    )
}

/// A block read by Koharu's stages and left untranslated.
fn untranslated(name: &str, index: usize) -> Value {
    block(
        name,
        index,
        "原文",
        Value::Null,
        c2pa(&["detection", "ocr"]),
    )
}

fn checks(findings: &[(String, bool)]) -> Vec<&str> {
    findings.iter().map(|(check, _)| check.as_str()).collect()
}

#[test]
fn sizes_and_overflow_are_measured_as_the_renderer_draws_the_text() -> Result<()> {
    let latin = "Hello there";
    let page = Fixture::new(
        400,
        400,
        vec![
            block(
                "large",
                0,
                "",
                translation(latin, boxed(10.0, 10.0, 300.0, 150.0, "best"), None),
                json!({}),
            ),
            block(
                "tiny",
                1,
                "",
                translation(latin, boxed(10.0, 10.0, 6.0, 3.0, "best"), None),
                json!({}),
            ),
            block(
                "fixed",
                2,
                "",
                translation(latin, boxed(10.0, 10.0, 6.0, 3.0, "none"), None),
                json!({}),
            ),
            block(
                "blank",
                3,
                "",
                translation("  ", boxed(10.0, 10.0, 6.0, 3.0, "none"), None),
                json!({}),
            ),
        ],
    );
    // The limit is 1% of 400 pixels.
    assert_eq!(page.found("large"), []);
    let tiny: Vec<Finding> = page
        .check()
        .into_iter()
        .filter(|finding| finding.layer == page.id("tiny"))
        .collect();
    assert!(
        tiny.iter().any(|finding| matches!(
            finding.problem,
            Problem::TooSmall { size, limit } if size < limit && limit == 4.0
        )),
        "{tiny:?}"
    );
    assert!(
        tiny.iter()
            .all(|finding| finding.text == latin && !finding.hidden)
    );
    assert_eq!(checks(&page.found("fixed")), ["overflow"]);
    assert_eq!(page.found("blank"), []);

    // A transform twice the size draws it twice the size on the page.
    let renderer = renderer();
    let Content::Block(large) = page.layer("large").content else {
        panic!("a block");
    };
    let text = large.translation.clone().expect("a translation");
    let doubled = Text {
        transform: Transform([[2.0, 0.0, 0.0], [0.0, 2.0, 0.0]]),
        ..text.clone()
    };
    let once = measure(&renderer, page.id("large"), &text)?.expect("measured");
    let twice = measure(&renderer, page.id("large"), &doubled)?.expect("measured");
    assert!(
        (twice.base - 2.0 * once.base).abs() < 0.01,
        "{once:?} {twice:?}"
    );
    assert_eq!(once.runs.len(), twice.runs.len());
    for (once, twice) in once.runs.iter().zip(&twice.runs) {
        assert!((twice.size - 2.0 * once.size).abs() < 0.01);
    }
    assert!(once.base >= 24.0 && once.runs.iter().all(|run| run.text == latin));
    // Blank text has nothing to measure.
    let blank = Text {
        content: koharu_scene::RichText(Vec::new()),
        ..text
    };
    assert_eq!(measure(&renderer, page.id("large"), &blank)?, None);
    Ok(())
}

#[test]
fn a_mark_hides_its_finding_only_while_the_line_is_unchanged() -> Result<()> {
    let mut page = Fixture::new(
        400,
        400,
        vec![translated("done", 0, "Done."), untranslated("line", 1)],
    );
    let line = page.id("line");
    assert_eq!(page.found("done"), []);
    assert_eq!(
        page.found("line"),
        [(MISSING_TRANSLATION.to_owned(), false)]
    );

    let steps = page.steps().len();
    let mark = mark_qc_ok(&page.project(), line, MISSING_TRANSLATION, true)?;
    page.apply(mark);
    assert_eq!(page.steps().len(), steps + 1);
    assert_eq!(page.steps().last().map(String::as_str), Some(MARK_QC_OK));
    assert_eq!(page.found("line"), [(MISSING_TRANSLATION.to_owned(), true)]);
    let printed = fingerprint(&page.layer("line"))?;
    assert_eq!(
        attrs::get::<QcOk>(&page.layer("line")),
        Some(QcOk(vec![format!("missing_translation@{printed}")]))
    );

    // A flag is a finding of its own, never hidden, and leaves the mark.
    let (flag, _) = flag_line(&page.project(), &[(line, "Untranslated pun".to_owned())])?;
    page.apply(flag);
    assert_eq!(
        page.found("line"),
        [
            (MISSING_TRANSLATION.to_owned(), true),
            (FLAG.to_owned(), false)
        ]
    );

    // Changing the line shows the finding again.
    page.update("line", json!({"source": {"content": "原文だ"}}));
    assert_eq!(
        page.found("line"),
        [
            (MISSING_TRANSLATION.to_owned(), false),
            (FLAG.to_owned(), false)
        ]
    );
    // Undo brings the line, and so the mark, back.
    page.session.undo()?;
    assert_eq!(
        page.found("line")[0],
        (MISSING_TRANSLATION.to_owned(), true)
    );

    // Taking the mark back shows it.
    let clear = mark_qc_ok(&page.project(), line, MISSING_TRANSLATION, false)?;
    page.apply(clear);
    assert_eq!(page.steps().last().map(String::as_str), Some(CLEAR_QC_OK));
    assert_eq!(
        page.found("line")[0],
        (MISSING_TRANSLATION.to_owned(), false)
    );
    assert_eq!(attrs::get::<QcOk>(&page.layer("line")), None);

    // A mark of the agent's flag, which Mark OK refuses but Koharu's own
    // edits could write, hides nothing: a flag goes only when dismissed.
    let printed = fingerprint(&page.layer("line"))?;
    page.session.apply(Edit {
        label: "test".to_owned(),
        ops: vec![attrs::set(line, &QcOk(vec![format!("{FLAG}@{printed}")]))?],
    })?;
    assert_eq!(
        page.found("line"),
        [
            (MISSING_TRANSLATION.to_owned(), false),
            (FLAG.to_owned(), false)
        ]
    );
    Ok(())
}

#[test]
fn locked_and_hidden_lines_give_nothing() {
    let mut page = Fixture::new(
        400,
        400,
        vec![
            translated("done", 0, "Done."),
            untranslated("open", 1),
            untranslated("locked", 2),
            untranslated("hidden", 3),
            json!({"op": "add_layer", "id": "$group", "page": "$page", "index": 4,
                "layer": {"kind": "group"}}),
            json!({"op": "add_layer", "id": "$held", "page": "$page", "parent": "$group",
                "index": 0, "layer": {"kind": "block", "attributes": c2pa(&["detection", "ocr"]),
                    "source": {"content": "原文", "frame": boxed(10.0, 300.0, 200.0, 50.0, "none")}}}),
        ],
    );
    page.update("locked", json!({"locked": true}));
    page.update("hidden", json!({"visible": false}));
    page.update("group", json!({"locked": true}));
    let found: Vec<LayerId> = page.check().iter().map(|finding| finding.layer).collect();
    assert_eq!(found, [page.id("open")]);
}

#[test]
fn a_text_layer_gets_only_the_size_checks_and_flags() -> Result<()> {
    let mut page = Fixture::new(
        400,
        400,
        vec![
            translated("done", 0, "Done."),
            json!({"op": "add_layer", "id": "$caption", "page": "$page", "index": 1,
                "layer": {"kind": "text", "content": "  Tiny words here ",
                    "frame": boxed(10.0, 300.0, 6.0, 3.0, "best")}}),
        ],
    );
    let caption = page.id("caption");
    let (flag, flagged) = flag_line(&page.project(), &[(caption, "Translate this".to_owned())])?;
    assert_eq!(flagged.flagged, [caption]);
    page.apply(flag);
    let found: Vec<Finding> = page
        .check()
        .into_iter()
        .filter(|finding| finding.layer == caption)
        .collect();
    let checks: Vec<&str> = found.iter().map(|finding| finding.check.as_str()).collect();
    assert_eq!(checks, [TOO_SMALL, FLAG], "{found:?}");
    assert_eq!(found[1].source, Source::Agent);
    assert_eq!(
        found[1].problem,
        Problem::Flag {
            message: "Translate this".to_owned()
        }
    );
    assert!(
        found
            .iter()
            .all(|finding| finding.text == "Tiny words here")
    );
    Ok(())
}

#[test]
fn a_line_given_back_unchanged_is_leftover_source_not_missing() -> Result<()> {
    let mut page = Fixture::new(
        400,
        400,
        vec![translated("done", 0, "Done."), untranslated("back", 1)],
    );
    // As Koharu's Translation stage writes a line it was given back: no
    // translation, its clean-up hidden, and the record of no text.
    let back = page.id("back");
    let ops = vec![
        serde_json::from_value::<Op>(json!({"op": "update_layer", "layer": back,
            "patch": {"cleanup": {"visible": false}}}))?,
        styles::given_back(&page.layer("back"))?,
    ];
    page.session.apply(Edit {
        label: "test".to_owned(),
        ops,
    })?;
    let found: Vec<Finding> = page
        .check()
        .into_iter()
        .filter(|finding| finding.layer == back)
        .collect();
    assert_eq!(
        found
            .iter()
            .map(|finding| &finding.problem)
            .collect::<Vec<_>>(),
        [&Problem::LeftoverSource { unchanged: true }]
    );
    assert_eq!(found[0].text, "原文");
    Ok(())
}

#[test]
fn a_line_given_back_and_styled_since_is_still_leftover_source() -> Result<()> {
    let mut page = Fixture::new(
        400,
        400,
        vec![translated("done", 0, "Done."), untranslated("back", 1)],
    );
    let back = page.id("back");
    let ops = vec![
        serde_json::from_value::<Op>(json!({"op": "update_layer", "layer": back,
            "patch": {"cleanup": {"visible": false}}}))?,
        styles::given_back(&page.layer("back"))?,
    ];
    page.session.apply(Edit {
        label: "test".to_owned(),
        ops,
    })?;
    // A style picked on it since leaves a placeholder, no translation: the
    // page still shows its source.
    page.session.apply(Edit {
        label: "test".to_owned(),
        ops: vec![crate::testing::type_bar_style(back)],
    })?;
    let found: Vec<Problem> = page
        .check()
        .into_iter()
        .filter(|finding| finding.layer == back)
        .map(|finding| finding.problem)
        .collect();
    assert_eq!(found, [Problem::LeftoverSource { unchanged: true }]);
    Ok(())
}

#[test]
fn locked_and_hand_set_fonts_count_toward_the_majority() -> Result<()> {
    let frame = || boxed(10.0, 10.0, 300.0, 150.0, "best");
    let automatic = || c2pa(&["detection", "ocr", "translation"]);
    let font = |name: &str, index, family, attributes| {
        block(
            name,
            index,
            "原文",
            translation("Hello", frame(), Some(family)),
            attributes,
        )
    };
    // A font typed by hand, with no stage's record, counts.
    let page = Fixture::new(
        400,
        400,
        vec![
            font("auto", 0, "Alpha", automatic()),
            font("typed", 1, "Alpha", c2pa(&["detection", "ocr"])),
            font("odd", 2, "Beta", automatic()),
        ],
    );
    let project = page.project();
    let lettering = LetteringSettings::default();
    let expected = Majorities::from([(LetteringCategory::Floating, Some("Alpha".to_owned()))]);
    assert_eq!(majorities(&project, &lettering), expected);
    let view = project.page(page.page)?;
    let inputs = inputs(&project);
    let facts_of = |name: &str| facts(view, &page.layer(name), true, &inputs).expect("facts");
    assert!(facts_of("typed").hand_set);
    assert!(!facts_of("auto").hand_set && !facts_of("odd").hand_set);
    let judge = Judge {
        limit: 4.0,
        target: &inputs.target,
        majorities: &inputs.majorities,
    };
    assert_eq!(
        rules::problems(&facts_of("odd"), &judge),
        [Problem::OddFont {
            expected: Some("Alpha".to_owned()),
            font: Some("Beta".to_owned())
        }]
    );

    // So does a locked one.
    let mut page = Fixture::new(
        400,
        400,
        vec![
            font("auto", 0, "Alpha", automatic()),
            font("locked", 1, "Alpha", automatic()),
            font("odd", 2, "Beta", automatic()),
        ],
    );
    page.update("locked", json!({"locked": true}));
    assert_eq!(majorities(&page.project(), &lettering), expected);
    Ok(())
}

#[test]
fn a_font_the_agent_chose_or_changed_by_hand_is_hand_set() -> Result<()> {
    let mut page = Fixture::new(
        400,
        400,
        vec![block(
            "line",
            0,
            "原文",
            translation(
                "Hello",
                boxed(10.0, 10.0, 300.0, 150.0, "best"),
                Some("Alpha"),
            ),
            c2pa(&["detection", "ocr", "translation"]),
        )],
    );
    let hand_set = |page: &Fixture| -> Result<bool> {
        let project = page.project();
        let inputs = inputs(&project);
        Ok(
            facts(project.page(page.page)?, &page.layer("line"), true, &inputs)
                .expect("facts")
                .hand_set,
        )
    };
    let id = page.id("line");
    let record = |family: &str, origin: Option<&str>| {
        let mut record = json!({"style": {"family": family}});
        if let Some(origin) = origin {
            record["origin"] = json!(origin);
        }
        json!({"op": "set_attribute", "layer": id, "namespace": "booharu", "key": "auto",
            "value": record})
    };
    assert!(!hand_set(&page)?, "a stage wrote it");
    page.session.apply(edit(vec![record("Alpha", None)]))?;
    assert!(!hand_set(&page)?, "as Booharu's step lettered it");
    page.session.apply(edit(vec![record("Beta", None)]))?;
    assert!(hand_set(&page)?, "changed since Booharu's step");
    page.session
        .apply(edit(vec![record("Alpha", Some("agent"))]))?;
    assert!(hand_set(&page)?, "the agent's choice");
    Ok(())
}

#[test]
fn marking_ok_keeps_one_current_entry_a_check_and_drops_the_rest() -> Result<()> {
    let mut page = Fixture::new(400, 400, vec![translated("line", 0, "The text")]);
    let line = page.id("line");
    let printed = fingerprint(&page.layer("line"))?;
    let held = QcOk(vec![
        "too_small@0000000000000000".to_owned(),
        "overflow".to_owned(),
        format!("odd_font@{printed}"),
    ]);
    page.session.apply(Edit {
        label: "test".to_owned(),
        ops: vec![attrs::set(line, &held)?],
    })?;
    let marks = |page: &Fixture| attrs::get::<QcOk>(&page.layer("line")).map(|QcOk(marks)| marks);

    page.apply(mark_qc_ok(&page.project(), line, OVERFLOW, true)?);
    assert_eq!(
        marks(&page),
        Some(vec![
            format!("odd_font@{printed}"),
            format!("overflow@{printed}")
        ])
    );
    let steps = page.steps().len();
    assert!(mark_qc_ok(&page.project(), line, OVERFLOW, true)?.is_none());

    // A glossary slip's term is matched as terms are.
    page.apply(mark_qc_ok(&page.project(), line, "glossary:魔王", true)?);
    assert!(mark_qc_ok(&page.project(), line, "glossary:魔 王", true)?.is_none());
    page.apply(mark_qc_ok(&page.project(), line, "glossary:魔 王", false)?);
    assert_eq!(page.steps().len(), steps + 2);
    assert_eq!(
        marks(&page),
        Some(vec![
            format!("odd_font@{printed}"),
            format!("overflow@{printed}")
        ])
    );

    // Taking back a mark it doesn't have changes nothing; the last one's
    // going removes the key.
    assert!(mark_qc_ok(&page.project(), line, EMPTY_SOURCE, false)?.is_none());
    page.apply(mark_qc_ok(&page.project(), line, OVERFLOW, false)?);
    page.apply(mark_qc_ok(&page.project(), line, ODD_FONT, false)?);
    assert_eq!(marks(&page), None);
    assert_eq!(page.steps().last().map(String::as_str), Some(CLEAR_QC_OK));
    Ok(())
}

#[test]
fn marking_ok_refuses_flags_unknown_checks_and_locked_lines() -> Result<()> {
    let mut page = Fixture::new(
        400,
        400,
        vec![
            json!({"op": "add_layer", "id": "$group", "page": "$page", "index": 0,
                "layer": {"kind": "group"}}),
            json!({"op": "add_layer", "id": "$line", "page": "$page", "parent": "$group",
                "index": 0, "layer": {"kind": "block",
                    "source": {"content": "原文", "frame": boxed(10.0, 10.0, 200.0, 50.0, "none")}}}),
            json!({"op": "add_layer", "id": "$raster", "page": "$page", "index": 1,
                "layer": {"kind": "raster"}}),
        ],
    );
    let (project, line) = (page.project(), page.id("line"));
    let refused = |check: &str| {
        mark_qc_ok(&project, line, check, true)
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default()
    };
    assert!(
        refused(FLAG).contains("booharu_dismiss_flag"),
        "{}",
        refused(FLAG)
    );
    assert!(refused("bogus").contains("no check \"bogus\""));
    assert!(refused("overflow@0123456789abcdef").contains("no check"));
    assert!(refused("glossary: ").contains("term's source"));
    assert!(mark_qc_ok(&project, line, "glossary:Shale", true)?.is_some());
    assert!(mark_qc_ok(&project, page.id("raster"), OVERFLOW, true).is_err());

    page.update("group", json!({"locked": true}));
    let locked = mark_qc_ok(&page.project(), line, OVERFLOW, true)
        .err()
        .and_then(|error| error.downcast_ref::<Refusal>().cloned());
    assert!(
        matches!(locked, Some(Refusal::Locked { layer, .. }) if layer == page.id("group")),
        "{locked:?}"
    );
    Ok(())
}

#[test]
fn flagging_lines_is_one_step_leaving_out_locked_lines_and_duplicates() -> Result<()> {
    let mut page = Fixture::new(
        400,
        400,
        vec![
            translated("a", 0, "First"),
            translated("b", 1, "Second"),
            json!({"op": "add_layer", "id": "$t", "page": "$page", "index": 2,
                "layer": {"kind": "text", "content": "Caption"}}),
            json!({"op": "add_layer", "id": "$raster", "page": "$page", "index": 3,
                "layer": {"kind": "raster"}}),
        ],
    );
    page.update("b", json!({"locked": true}));
    let (a, b, t) = (page.id("a"), page.id("b"), page.id("t"));
    let flags = |page: &Fixture, name: &str| attrs::get::<Flags>(&page.layer(name));

    // A bad line refuses the whole call.
    let long = "x".repeat(MAX_FLAG_MESSAGE + 1);
    for bad in [
        vec![(a, "Fine".to_owned()), (t, "  ".to_owned())],
        vec![(a, "Fine".to_owned()), (t, long)],
        vec![
            (a, "Fine".to_owned()),
            (page.id("raster"), "Fine".to_owned()),
        ],
        vec![],
    ] {
        assert!(flag_line(&page.project(), &bad).is_err(), "{bad:?}");
    }
    assert!(
        flag_line(&page.project(), &[(a, "x".repeat(MAX_FLAG_MESSAGE))])?
            .0
            .is_some()
    );

    let steps = page.steps().len();
    let (built, flagged) = flag_line(
        &page.project(),
        &[
            (a, " Check the name ".to_owned()),
            (t, "Joke?".to_owned()),
            (a, "Check the name".to_owned()),
            (b, "Locked".to_owned()),
            (a, "Too long".to_owned()),
        ],
    )?;
    assert_eq!(
        flagged,
        Flagged {
            flagged: vec![a, t, a],
            duplicates: vec![a],
            locked: vec![b],
        }
    );
    page.apply(built);
    assert_eq!(page.steps().len(), steps + 1);
    assert_eq!(page.steps().last().map(String::as_str), Some(FLAG_LINE));
    assert_eq!(
        flags(&page, "a"),
        Some(Flags(vec![
            Flag::review("Check the name"),
            Flag::review("Too long")
        ]))
    );
    assert_eq!(flags(&page, "t"), Some(Flags(vec![Flag::review("Joke?")])));
    assert_eq!(flags(&page, "b"), None);

    // A flag the line has adds nothing.
    let (built, flagged) = flag_line(&page.project(), &[(a, "Too long".to_owned())])?;
    assert!(built.is_none());
    assert_eq!(flagged.duplicates, [a]);
    Ok(())
}

#[test]
fn dismissing_takes_one_flag_or_all_and_refuses_locked_lines() -> Result<()> {
    let mut page = Fixture::new(400, 400, vec![translated("a", 0, "First")]);
    let a = page.id("a");
    let lines: Vec<(LayerId, String)> = ["One", "Two", "Three"]
        .into_iter()
        .map(|message| (a, message.to_owned()))
        .collect();
    page.apply(flag_line(&page.project(), &lines)?.0);
    let flags = |page: &Fixture| attrs::get::<Flags>(&page.layer("a"));

    let (built, dismissed) = dismiss_flag(&page.project(), a, Some(" Two "))?;
    assert_eq!(dismissed, 1);
    page.apply(built);
    assert_eq!(page.steps().last().map(String::as_str), Some(DISMISS_FLAG));
    assert_eq!(
        flags(&page),
        Some(Flags(vec![Flag::review("One"), Flag::review("Three")]))
    );
    assert!(matches!(
        dismiss_flag(&page.project(), a, Some("Nope"))?,
        (None, 0)
    ));

    let (built, dismissed) = dismiss_flag(&page.project(), a, None)?;
    assert_eq!(dismissed, 2);
    page.apply(built);
    assert_eq!(flags(&page), None);
    assert!(matches!(dismiss_flag(&page.project(), a, None)?, (None, 0)));
    // Undo brings them back.
    page.session.undo()?;
    assert_eq!(flags(&page).map(|Flags(flags)| flags.len()), Some(2));

    page.update("a", json!({"locked": true}));
    let locked = dismiss_flag(&page.project(), a, None)
        .err()
        .and_then(|error| error.downcast_ref::<Refusal>().cloned());
    assert!(matches!(locked, Some(Refusal::Locked { layer, .. }) if layer == a));
    Ok(())
}

#[test]
fn glossary_slips_are_found_as_the_page_check_finds_them_laying_no_text_out() -> Result<()> {
    let frame = || boxed(10.0, 10.0, 300.0, 150.0, "best");
    let automatic = || c2pa(&["detection", "ocr", "translation"]);
    let mut page = Fixture::new(
        400,
        400,
        vec![
            block(
                "slips",
                0,
                "魔王だ",
                translation(" The king! ", frame(), None),
                automatic(),
            ),
            block(
                "follows",
                1,
                "魔王だ",
                translation("The Demon King!", frame(), None),
                automatic(),
            ),
            block(
                "locked",
                2,
                "魔王だ",
                translation("The king!", frame(), None),
                automatic(),
            ),
        ],
    );
    page.update("locked", json!({"locked": true}));
    let series = Series {
        glossary: vec![GlossaryEntry::new("魔王", "Demon King")],
        ..Series::default()
    };
    let project = page.project();
    let inputs = Inputs::new(
        &project,
        &QcSettings { min_size: 0.01 },
        "en".parse().expect("a language"),
        LetteringSettings::default(),
        Some(series),
    );
    let slips = check_glossary(&project, page.page, &inputs)?;
    assert_eq!(
        slips,
        [Finding {
            page: page.page,
            layer: page.id("slips"),
            check: "glossary:魔王".to_owned(),
            problem: Problem::Glossary {
                source: "魔王".to_owned(),
                target: "Demon King".to_owned(),
            },
            source: Source::Auto,
            text: "The king!".to_owned(),
            hidden: false,
        }]
    );
    let mut checked = check_page(&renderer(), &project, page.page, &inputs)?;
    checked.retain(|finding| matches!(finding.problem, Problem::Glossary { .. }));
    assert_eq!(checked, slips);

    page.apply(mark_qc_ok(
        &project,
        page.id("slips"),
        "glossary:魔王",
        true,
    )?);
    let hidden = check_glossary(&page.project(), page.page, &inputs)?;
    assert!(hidden.len() == 1 && hidden[0].hidden, "{hidden:?}");

    // Worded again, still leaving the term out, the slip shows again...
    let line = page.id("slips");
    page.session.apply(edit(vec![
        json!({"op": "set_text", "layer": line, "text": "The king, yes!"}),
    ]))?;
    let reworded = check_glossary(&page.project(), page.page, &inputs)?;
    let shown: Vec<(&str, bool)> = reworded
        .iter()
        .map(|finding| (finding.text.as_str(), finding.hidden))
        .collect();
    assert_eq!(shown, [("The king, yes!", false)]);
    // ...undoing that hides it again, and taking the mark back shows it.
    page.session.undo()?;
    let hidden = check_glossary(&page.project(), page.page, &inputs)?;
    assert!(hidden.len() == 1 && hidden[0].hidden, "{hidden:?}");
    page.apply(mark_qc_ok(&page.project(), line, "glossary:魔王", false)?);
    assert_eq!(page.steps().last().map(String::as_str), Some(CLEAR_QC_OK));
    let shown = check_glossary(&page.project(), page.page, &inputs)?;
    assert!(shown.len() == 1 && !shown[0].hidden, "{shown:?}");
    Ok(())
}

#[test]
fn the_run_planner_and_the_check_read_one_sound_effect_rule() -> Result<()> {
    // A page Koharu's Translation stage ran on, with the sound effect `e`
    // read and left untranslated, its clean-up showing.
    let mut page = Fixture::new(
        400,
        400,
        vec![
            translated("a", 0, "Hello there"),
            block("e", 1, "ドドド", Value::Null, c2pa(&["detection", "ocr"])),
        ],
    );
    page.update("e", json!({"role": "sfx"}));
    let skip = LetteringSettings::default();
    let all = LetteringSettings {
        sfx: SfxMode::TranslateAll,
        ..LetteringSettings::default()
    };
    let planned = |page: &Fixture, settings: &LetteringSettings| -> Result<bool> {
        let groups = crate::runs::plan(&page.session, &[page.page], settings)?;
        Ok(groups
            .iter()
            .any(|group| group.stages.contains(&Stage::Translate)))
    };
    let missing = |page: &Fixture, settings: &LetteringSettings| -> Result<bool> {
        let project = page.project();
        let inputs = Inputs::new(
            &project,
            &QcSettings { min_size: 0.01 },
            "en".parse()?,
            settings.clone(),
            None,
        );
        let found = check_page(&renderer(), &project, page.page, &inputs)?;
        Ok(found
            .iter()
            .any(|finding| finding.layer == page.id("e") && finding.check == MISSING_TRANSLATION))
    };
    // Kept as art: translated only to hide its clean-up, and missing nothing.
    assert!(planned(&page, &skip)?);
    assert!(!missing(&page, &skip)?);
    // Once hidden, with the kept mark: nothing planned, nothing found.
    let kept = sfx::kept_original(&page.layer("e"));
    assert!(!kept.is_empty());
    page.apply(Some(Edit {
        label: "test".to_owned(),
        ops: kept,
    }));
    assert!(!planned(&page, &skip)?);
    assert!(!missing(&page, &skip)?);
    // With every sound effect translated, both see its translation missing.
    assert!(planned(&page, &all)?);
    assert!(missing(&page, &all)?);
    Ok(())
}
