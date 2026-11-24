//! Cleaning exact shapes of a page at once, as Koharu's Remove brush cleans
//! a stroke: `booharu_clean_areas` makes rectangles and polygons one area
//! through `booharu::cleanup` and starts the one Inpainting run cleaning
//! it, with the saved pipeline settings, where the app runs Koharu's runs,
//! as Redo boxes starts its own, answering at once for `wait` to follow.
//! Its result goes where the Remove brush's goes, under all the page's
//! lettering.

use anyhow::Context as _;
use booharu::cleanup::{self, CleanAnswer, Point, Rectangle, Shapes};
use koharu_scene::PageId;
use rig_core::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use super::context::blocking;
use crate::tools::schema;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct CleanAreasArgs {
    /// The page.
    page: PageId,
    /// Rectangles to clean, in page pixels, each from its left edge `x` and
    /// its top edge `y`, `width` wide and `height` high.
    rects: Option<Vec<Rectangle>>,
    /// Polygons to clean, in page pixels, each its corners `{"x", "y"}` in
    /// order, at least 3, closed back to the first; one crossing itself
    /// fills every loop.
    polygons: Option<Vec<Vec<Point>>>,
}

pub(super) struct CleanAreas;

impl Tool for CleanAreas {
    const NAME: &'static str = "booharu_clean_areas";
    type Args = CleanAreasArgs;
    type Output = CleanAnswer;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Clean the art under exact shapes of a page at once, as Koharu's Remove brush cleans a \
         stroke: `rects` and `polygons`, in page pixels, make one area, a pixel cleaned when its \
         centre lies inside a shape (a polygon by nonzero winding, so one crossing itself fills \
         every loop), with no growth and clipped to the page, and one Inpainting run cleans it \
         with the inpainter the pipeline's settings clean pages with, going on in the \
         background: follow it with wait. The result is a new layer limited to the shapes, as \
         one undo step, above the scan and the clean-ups made before it, and under all of the \
         page's lettering, so it never covers a translation. It never changes a block's own \
         clean-up, which still shows over it where the block's mask is: to clean the art under \
         a block again, run booharu_redo_boxes with `inpaint` on it instead. Answers at once \
         with the `run`, the `rect` the area covers, the smallest holding every pixel cleaned, \
         and how many `pixels` it cleans. Refused with no shapes, a polygon of fewer than 3 \
         points, a value that isn't a finite number, shapes covering no pixel of the page, or \
         a page with no scan. Example: {\"page\": \"<id>\", \"rects\": [{\"x\": 120, \"y\": 80, \
         \"width\": 60, \"height\": 40}]}."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<CleanAreasArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: CleanAreasArgs,
    ) -> Result<CleanAnswer, Self::Error> {
        let CleanAreasArgs {
            page,
            rects,
            polygons,
        } = args;
        let shapes = Shapes {
            rects: rects.unwrap_or_default(),
            polygons: polygons.unwrap_or_default(),
        };
        blocking(context, move |tools, host| {
            let project = tools.session.lock().project();
            let area = cleanup::area(&project, page, &shapes)?;
            let (rect, pixels) = (area.rect, cleanup::covered(&area));
            let config = host.pipeline_config()?;
            // Queued with the session let go, as the run takes it.
            let run = host
                .start_run(
                    &tools.pipeline,
                    &tools.session,
                    cleanup::request(page, area, config),
                )?
                .context("the project closed")?;
            Ok(CleanAnswer { run, rect, pixels })
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, io::Cursor, sync::Arc};

    use booharu_core::settings::BooharuPreferences;
    use image::{GrayImage, ImageFormat, Luma};
    use koharu_pipeline::{Area, Pipeline, PipelineConfig, Request, RunId, Stage, Target};
    use koharu_renderer::{Renderer, RendererConfig};
    use koharu_scene::{Edit, Hash, Op, Rect, Session};
    use parking_lot::Mutex;
    use rig_core::completion::ToolAnnotations;
    use serde_json::{Map, json};

    use super::*;
    use crate::tools::{Tools, booharu::BooharuHost};

    /// The app as the tool reaches it: the pipeline's saved settings
    /// translating into French, and the runs it was asked to start, which
    /// it never starts, as cleaning would load a model.
    #[derive(Default)]
    struct App {
        started: Mutex<Vec<Request>>,
    }

    impl BooharuHost for App {
        fn preferences(&self) -> anyhow::Result<BooharuPreferences> {
            Ok(BooharuPreferences::default())
        }

        fn save_section(
            &self,
            _: &str,
            _: Map<String, Value>,
        ) -> anyhow::Result<BooharuPreferences> {
            anyhow::bail!("nothing here saves settings")
        }

        fn font_families(&self) -> anyhow::Result<Vec<String>> {
            Ok(Vec::new())
        }

        fn pipeline_config(&self) -> anyhow::Result<PipelineConfig> {
            let mut config = PipelineConfig::default();
            config.translator.target = "fr".parse()?;
            Ok(config)
        }

        fn start_run(
            &self,
            _: &Pipeline,
            _: &Arc<Mutex<Session>>,
            request: Request,
        ) -> anyhow::Result<Option<RunId>> {
            self.started.lock().push(request);
            Ok(Some(RunId(7)))
        }
    }

    /// Tools in [`App`] on a project of a page of 40 by 30 pixels with a
    /// scan and one without; the app, and the two pages.
    fn open() -> (tempfile::TempDir, Tools, Arc<App>, [PageId; 2]) {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        let mut scan = Vec::new();
        GrayImage::from_pixel(40, 30, Luma([200]))
            .write_to(&mut Cursor::new(&mut scan), ImageFormat::Png)
            .expect("a scan");
        let pages = [
            json!({"op": "add_page", "index": 0, "background": Hash::of(&scan)}),
            json!({"op": "add_page", "index": 1, "size": {"width": 40, "height": 30}}),
        ];
        let mut ops = vec![Op::AddFile { bytes: scan }];
        ops.extend(pages.map(|page| serde_json::from_value(page).expect("an op")));
        let change = session
            .apply(Edit {
                label: "test".to_owned(),
                ops,
            })
            .expect("two pages");
        let [scanned, blank] = change.pages.added[..] else {
            panic!("{change:?}");
        };
        let app = Arc::new(App::default());
        let tools = Tools::new(
            Arc::new(Mutex::new(session)),
            Pipeline::new(PipelineConfig::default()),
            Renderer::new(RendererConfig::default()),
        )
        .with_booharu(Arc::clone(&app) as Arc<dyn BooharuHost>);
        (dir, tools, app, [scanned, blank])
    }

    async fn clean(tools: &Tools, args: Value) -> Result<CleanAnswer, String> {
        let args = serde_json::from_value(args).expect("the arguments");
        CleanAreas
            .call(&mut tools.tool_context(), args)
            .await
            .map_err(|error| error.to_string())
    }

    #[tokio::test]
    async fn the_shapes_are_cleaned_as_one_area_at_once() {
        let (_dir, tools, app, [page, _]) = open();
        // A rect and a triangle beside it, which share no pixel.
        let args = json!({
            "page": page,
            "rects": [{"x": 2, "y": 3, "width": 4, "height": 2}],
            "polygons": [[{"x": 6, "y": 3}, {"x": 10, "y": 3}, {"x": 6, "y": 7}]],
        });
        let answer = clean(&tools, args).await.expect("a run");
        let rect = Rect {
            x: 2,
            y: 3,
            width: 7,
            height: 3,
        };
        // The rect's 8 pixels, and the triangle's 3 + 2 + 1, its long side
        // through centres, which it leaves out.
        assert_eq!(
            answer,
            CleanAnswer {
                run: RunId(7),
                rect,
                pixels: 8 + 6,
            }
        );
        let started = app.started.lock().clone();
        let [request] = started.as_slice() else {
            panic!("{started:?}");
        };
        let Target::Area {
            page: cleaned,
            area: Area { rect: at, pixels },
            prompt,
        } = &request.target
        else {
            panic!("{:?}", request.target);
        };
        assert_eq!((*cleaned, *at, prompt), (page, rect, &None));
        #[rustfmt::skip]
        let expected = [
            255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 0,
            0, 0, 0, 0, 255, 0, 0,
        ];
        assert_eq!(pixels.as_slice(), expected.as_slice());
        // Inpainting alone, with the pipeline's saved settings.
        assert_eq!(request.stages, BTreeSet::from([Stage::Inpaint]));
        let config = request.config.as_ref().expect("the saved settings");
        assert_eq!(config.translator.target.to_string(), "fr");
        assert!(!config.booharu.soft_limit);
    }

    #[tokio::test]
    async fn shapes_cleaning_nothing_or_a_page_without_a_scan_are_refused() {
        let (_dir, tools, app, [page, blank]) = open();
        let square = json!([{"x": 0, "y": 0, "width": 4, "height": 4}]);
        for (args, why) in [
            (json!({"page": page}), "there are no shapes to clean"),
            (
                json!({"page": page, "rects": [], "polygons": []}),
                "there are no shapes to clean",
            ),
            (
                json!({"page": page, "polygons": [[{"x": 0, "y": 0}, {"x": 9, "y": 9}]]}),
                "polygon 1 has 2 points; a polygon needs at least 3",
            ),
            (
                json!({"page": page, "rects": [{"x": 50, "y": 0, "width": 4, "height": 4}]}),
                "the shapes cover no pixel of the page, which is 40 by 30 pixels",
            ),
            (
                json!({"page": blank, "rects": square}),
                "this page has no scan",
            ),
        ] {
            let error = clean(&tools, args.clone()).await.expect_err("refused");
            assert!(error.contains(why), "{args}: {error}");
        }
        // A value that isn't a finite number, which JSON can't carry.
        let args = CleanAreasArgs {
            page,
            rects: None,
            polygons: Some(vec![vec![
                Point { x: 0.0, y: 0.0 },
                Point {
                    x: f64::NAN,
                    y: 5.0,
                },
                Point { x: 5.0, y: 5.0 },
            ]]),
        };
        let error = CleanAreas
            .call(&mut tools.tool_context(), args)
            .await
            .expect_err("a point at no number");
        assert!(
            error.to_string().contains("isn't at finite numbers"),
            "{error}"
        );
        assert!(app.started.lock().is_empty(), "nothing was cleaned");
    }

    #[test]
    fn it_cleans_so_it_isnt_read_only_and_its_schema_holds_its_shapes() {
        assert_ne!(CleanAreas.annotations().read_only_hint, Some(true));
        assert_eq!(
            CleanAreas.annotations(),
            ToolAnnotations::default(),
            "a writer's defaults"
        );
        let definitions = Tools::toolset().tool_definitions();
        let offered = definitions
            .iter()
            .find(|tool| tool.name.as_str() == CleanAreas::NAME)
            .expect("the agent and MCP offer it");
        assert_ne!(offered.annotations.read_only_hint, Some(true));
        let schema = CleanAreas.parameters();
        let page = "00000000-0000-0000-0000-000000000001";
        let fits = json!({
            "page": page,
            "rects": [{"x": -3.5, "y": 2, "width": 10, "height": 4.5}],
            "polygons": [[{"x": 0, "y": 0}, {"x": 9, "y": 0}, {"x": 0, "y": 9}]],
        });
        assert_eq!(crate::tools::refusal(&schema, &fits), None);
        for refused in [
            json!({"page": page, "rects": [{"x": 0, "y": 0, "width": -1, "height": 4}]}),
            json!({"page": page, "rects": [{"x": 0, "y": 0, "width": 4}]}),
            json!({"page": page, "polygons": [[{"x": 0, "y": 0, "z": 1}]]}),
            json!({"page": page, "areas": []}),
        ] {
            assert!(
                crate::tools::refusal(&schema, &refused).is_some(),
                "{refused}"
            );
        }
    }
}
