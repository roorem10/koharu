use super::*;
use koharu_scene::{At, PageDraft, Session};

#[test]
fn missing_or_nonvision_models_are_skipped_before_a_request() {
    use koharu_translator::Provider;

    for (provider, model) in [
        (Provider::OpenAi, None),
        (Provider::Claude, Some("")),
        (Provider::Gemini, Some("  ")),
        (Provider::Local, None),
        (Provider::DeepL, Some("translator")),
        (Provider::Local, Some("lfm2.5-1.2b-instruct")),
    ] {
        let selection = ModelSelection {
            provider,
            model: model.map(str::to_owned),
            vision: true,
            ..Default::default()
        };
        assert!(vision_model(selection).is_none());
    }
    let selection = ModelSelection {
        provider: Provider::Claude,
        model: Some("reader".into()),
        vision: false,
        ..Default::default()
    };
    assert!(vision_model(selection).unwrap().vision);
}

#[test]
fn model_areas_use_page_coordinates_and_reject_invalid_geometry() {
    let area = VisionArea {
        x: 0.25,
        y: 0.1,
        width: 0.5,
        height: 0.2,
    };
    assert_eq!(
        area.pixels(1200.0, 1800.0).unwrap(),
        Bounds {
            x: 300.0,
            y: 180.0,
            width: 600.0,
            height: 360.0,
        }
    );
    for area in [
        VisionArea {
            x: -0.1,
            y: 0.0,
            width: 0.1,
            height: 0.1,
        },
        VisionArea {
            x: 0.9,
            y: 0.0,
            width: 0.2,
            height: 0.1,
        },
        VisionArea {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.1,
        },
        VisionArea {
            x: f64::NAN,
            y: 0.0,
            width: 0.1,
            height: 0.1,
        },
    ] {
        assert!(area.pixels(1000.0, 1000.0).is_err());
    }
    assert!(area.pixels(f64::INFINITY, 1000.0).is_err());
}

#[test]
fn vision_output_fails_closed_instead_of_inventing_a_fix_or_location() {
    for value in [
        json!({"findings":[{"kind":"unknown","detail":"x","area":{"x":0,"y":0,"width":0.1,"height":0.1}}]}),
        json!({"findings":[{"kind":"hard_to_read","detail":"x","area":null}]}),
        json!({"findings":[{"kind":"hard_to_read","detail":"x","area":{"x":0,"y":0,"width":0.1,"height":0.1},"matched_layer":"invented"}]}),
    ] {
        assert!(serde_json::from_value::<VisionReport>(value).is_err());
    }
    let report: VisionReport = serde_json::from_value(json!({"findings":[]})).unwrap();
    assert!(report.findings.is_empty());
}

#[tokio::test]
async fn scopes_keep_project_order_deduplicate_and_reject_missing_pages() {
    let mut session = Session::memory().await.unwrap();
    let mut pages = Vec::new();
    let patch = session
        .snapshot()
        .patch(|edit| {
            for label in ["one", "two", "three"] {
                pages.push(edit.add_page(PageDraft::new(label, 100.0, 100.0), At::End)?);
            }
            Ok(())
        })
        .unwrap();
    session.commit(patch).await.unwrap();
    let snapshot = session.snapshot();
    assert_eq!(selected_pages(&snapshot, &[]).unwrap(), pages);
    assert_eq!(
        selected_pages(&snapshot, &[pages[2], pages[0], pages[2]]).unwrap(),
        vec![pages[0], pages[2]]
    );
    assert!(selected_pages(&snapshot, &[EntityId::new()]).is_err());
}

#[test]
fn sidecar_roundtrip_preserves_review_and_unrelated_data() {
    let directory = tempfile::tempdir().unwrap();
    let page = EntityId::new();
    let report = PageCheckReport {
        revision: Revision::new(7),
        pages: vec![PageCheckPage {
            page,
            page_label: "page".into(),
            revision: Revision::new(7),
            status: PageCheckStatus::Skipped,
            reason: Some(PageCheckSkipReason::NoVisionModel),
            findings: Vec::new(),
        }],
    };
    ProjectContext::update(directory.path(), |context| {
        context
            .other
            .insert("another_work_order".into(), json!({"keep":true}));
        context
            .other
            .insert(SIDECAR_KEY.into(), serde_json::to_value(report).unwrap());
    })
    .unwrap();
    let context = ProjectContext::load(directory.path()).unwrap();
    let saved = stored_report(&context).unwrap();
    assert_eq!(saved.pages[0].page, page);
    assert_eq!(saved.pages[0].revision, Revision::new(7));
    assert_eq!(
        saved.pages[0].reason,
        Some(PageCheckSkipReason::NoVisionModel)
    );
    assert_eq!(context.other["another_work_order"], json!({"keep":true}));
    let mut corrupt = context.clone();
    corrupt.other.insert(SIDECAR_KEY.into(), json!({"pages":"broken"}));
    assert!(stored_report(&corrupt).is_err());
    assert_eq!(observed_context(context), observed_context(corrupt));
}
