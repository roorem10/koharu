//! Text effects, as the agent reaches them: the skew the FX window sets on
//! whole boxes, as one undo step, and the effects each box of a page
//! carries beyond its lettering default, which its fx tag shows, each worked
//! out by `booharu::effects` as the app's commands are, the effects under
//! the saved `[booharu-lettering]` and the first of Koharu's Preferred fonts
//! as they are when called. A whole box's gradient fill and letter spacing
//! are scene keys Koharu's `edit` sets, a word's `booharu_style_words`; a
//! skew is composed into a text's transform with a record beside it, which
//! only `booharu_set_skew` keeps in step.

use booharu::effects::{self, SkewCounts};
use booharu_core::{effects::TextEffects, skew};
use koharu_scene::{LayerId, PageId};
use rig_core::{
    completion::ToolAnnotations,
    tool::{Tool, ToolContext, ToolExecutionError},
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use super::styles::look;
use crate::tools::{failed, project, schema};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct SetSkewArgs {
    /// The boxes, at least one: blocks, whose translations lean, and text
    /// layers.
    #[schemars(length(min = 1))]
    layers: Vec<LayerId>,
    /// The lean along each box's width, in degrees from -60 to 60: positive
    /// leans its top to the right, as italics do. Left out, each box keeps
    /// its own.
    #[schemars(range(min = -60.0, max = 60.0))]
    x: Option<f64>,
    /// The lean along each box's height, in degrees from -60 to 60:
    /// positive lifts its right side. Left out, each box keeps its own.
    #[schemars(range(min = -60.0, max = 60.0))]
    y: Option<f64>,
}

impl SetSkewArgs {
    /// Refuses a call naming no box or no axis, or an angle out of -60 to
    /// 60 degrees.
    fn check(&self) -> Result<(), ToolExecutionError> {
        if self.layers.is_empty() {
            return Err(ToolExecutionError::invalid_args(
                "name the boxes to skew in `layers`: blocks or text layers",
            ));
        }
        if self.x.is_none() && self.y.is_none() {
            return Err(ToolExecutionError::invalid_args(
                "give `x`, `y`, or both: the skew in degrees along each box's width and height",
            ));
        }
        for (name, angle) in [("x", self.x), ("y", self.y)] {
            if let Some(angle) = angle
                && !skew::in_range(angle)
            {
                return Err(ToolExecutionError::invalid_args(format!(
                    "`{name}` {angle} isn't from -{limit} to {limit} degrees",
                    limit = skew::LIMIT
                )));
            }
        }
        Ok(())
    }
}

pub(super) struct SetSkew;

impl Tool for SetSkew {
    const NAME: &'static str = "booharu_set_skew";
    type Args = SetSkewArgs;
    type Output = SkewCounts;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Skew whole boxes, as the FX window's Skew does, as one undo step: each of `layers`, a \
         block, whose translation leans, or a text layer, leans `x` degrees along its width and \
         `y` along its height, each from -60 to 60, in place of the skew it shows. A positive \
         `x` leans the text's top to the right, as italics do; a positive `y` lifts its right \
         side. An axis left out keeps each box's own skew along it, so one call can change one \
         axis of boxes skewed differently, and 0 on both takes the skew off. The text leans \
         about its box's centre (a point text about its anchor), so its position, rotation, \
         frame and size stay; booharu_get_text_effects tells the skew each box shows. Skew \
         text with this rather than a `transform` sent with `edit`: Booharu records the skew \
         beside the transform, so the FX window shows it and it comes off exactly, after a \
         move, a rotation or a resize too, and a block keeps it when translated again. \
         booharu_reset_lettering takes it off with the rest of a box's look. A locked layer, or \
         one inside a locked layer, is left as it is, and so is a block without a translation, \
         or with only the empty placeholder a style set on it before it was translated leaves, \
         which has no lettering to skew; any other layer is left out, and a box named twice \
         counts once. Answers how many boxes it `changed`, left alone as `locked`, and found \
         `untranslated`: tell the user about any it left alone. A box showing that skew already \
         counts in none, and with nothing to change, it makes no undo step. An empty `layers`, \
         no axis, or an angle out of range is refused. Example: {\"layers\": [\"<id>\"], \"x\": \
         15}."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<SetSkewArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: SetSkewArgs,
    ) -> Result<SkewCounts, Self::Error> {
        args.check()?;
        let tools = project(context)?;
        // On a blocking thread, as `edit` applies, and under one lock, so the
        // boxes counted are the ones skewed.
        tokio::task::spawn_blocking(move || -> anyhow::Result<SkewCounts> {
            let mut session = tools.session.lock();
            let change = effects::set_skew(&session.project(), &args.layers, args.x, args.y)?;
            let counts = change.counts();
            if let Some(edit) = change.edit {
                session.apply(edit)?;
            }
            Ok(counts)
        })
        .await
        .map_err(failed)?
        .map_err(|error| failed(format!("{error:#}")))
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct GetTextEffectsArgs {
    /// The page whose boxes to answer.
    page: PageId,
}

pub(super) struct GetTextEffects;

impl Tool for GetTextEffects {
    const NAME: &'static str = "booharu_get_text_effects";
    type Args = GetTextEffectsArgs;
    type Output = Vec<TextEffects>;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "The text effects each box of a page carries beyond its lettering default, as the fx \
         tags in the app show them, changing nothing: for each translated block and text layer \
         of `page` with any, in reading order, its `layer`, its `effects`, from `border` (an \
         outline unlike its lettering default's, or none where that has one), `second_border` \
         (a second outline drawn outside the first), `outline_method` (outlines drawn letter \
         by letter, `stroke_method` `expansion`), `gradient` (a gradient fill, on the box or on \
         any of its words) and `skew`, and the `skew` it shows, `x` and `y` in degrees, null \
         with none. A box with none is left out. Its lettering default is the look \
         booharu_reset_lettering gives it at its size now, and what Booharu's automatic steps \
         wrote is no effect, so neither an outline Koharu placed nor a translation run's \
         emphasis on stressed words is one; a font, weight, italic, size, flat colour, \
         alignment, line or letter spacing, direction, frame, position or rotation never is. \
         booharu_reset_lettering takes every effect off a box; booharu_set_skew changes its \
         skew, `edit` its gradient and outlines (`fill_gradient`, `stroke`, `stroke_width`, \
         `outer_stroke`, `outer_stroke_width` and `stroke_method` in its style), and \
         booharu_style_words a word's."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<GetTextEffectsArgs>()
    }

    fn annotations(&self) -> ToolAnnotations {
        ToolAnnotations::default().with_read_only(true)
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: GetTextEffectsArgs,
    ) -> Result<Vec<TextEffects>, Self::Error> {
        let tools = project(context)?;
        tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<TextEffects>> {
            let (settings, font) = look(&tools)?;
            let project = tools.session.lock().project();
            effects::page_effects(project.page(args.page)?, &settings, font.as_deref())
        })
        .await
        .map_err(failed)?
        .map_err(|error| failed(format!("{error:#}")))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use booharu_core::{
        attrs::{self, Skew},
        emphasis::Marking,
        settings::{BooharuPreferences, LetteringSettings},
        styles,
    };
    use koharu_pipeline::{Pipeline, PipelineConfig};
    use koharu_renderer::{Renderer, RendererConfig};
    use koharu_scene::{Content, Created, Edit, LanguageIdentifier, Layer, Op, Session};
    use parking_lot::Mutex;
    use rig_core::tool::ToolErrorKind;
    use serde_json::{Map, json};

    use super::*;
    use crate::tools::{
        Tools,
        booharu::{
            BooharuHost,
            styles::{ResetLettering, ResetLetteringArgs},
        },
    };

    /// The default font: the app's first Preferred font.
    const FONT: &str = "CCWildWords";

    /// The app, with Booharu's settings as they start.
    struct App;

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
            Ok(vec![FONT.to_owned()])
        }
    }

    /// A page 400 by 400 holding the blocks `a`, `b` and `locked`, translated
    /// as Koharu's Translation stage translates with Booharu's hook giving
    /// them their lettering default, and `locked` then locked; the
    /// untranslated block `bare`, the text layer `t`, and the raster
    /// `paint`. The tools on it, the page's id, and the layers' ids.
    fn open() -> (tempfile::TempDir, Tools, PageId, impl Fn(&str) -> LayerId) {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        let block = |x: f64| {
            json!({"kind": "block", "source": {
                "frame": {"box": {"bounds": {"x": x, "y": 20.0, "width": 40.0, "height": 120.0}}},
                "content": "どうして",
                "style": {"chars": {"size": 32.0}},
            }, "attributes": {"c2pa": {
                "detection": {"action": "c2pa.created"},
                "ocr": {"action": "c2pa.created"},
            }}})
        };
        let ops = [
            json!({"op": "add_page", "id": "$page", "index": 0, "size": {"width": 400, "height": 400}}),
            json!({"op": "add_layer", "id": "$a", "page": "$page", "index": 0, "layer": block(10.0)}),
            json!({"op": "add_layer", "id": "$b", "page": "$page", "index": 1, "layer": block(60.0)}),
            json!({"op": "add_layer", "id": "$locked", "page": "$page", "index": 2, "layer": block(110.0)}),
            json!({"op": "add_layer", "id": "$bare", "page": "$page", "index": 3, "layer": block(160.0)}),
            json!({"op": "add_layer", "id": "$t", "page": "$page", "index": 4, "layer": {
                "kind": "text",
                "content": "Hi!",
                "style": {"chars": {"family": FONT, "size": 20.0}},
                "frame": {"box": {"bounds": {"x": 200.0, "y": 200.0, "width": 80.0, "height": 40.0}}},
            }}),
            json!({"op": "add_layer", "id": "$paint", "page": "$page", "index": 5,
                "layer": {"kind": "raster"}}),
        ];
        let change = session
            .apply(Edit {
                label: "test".to_owned(),
                ops: ops
                    .into_iter()
                    .map(|op| serde_json::from_value(op).expect("an op"))
                    .collect(),
            })
            .expect("a page");
        let page = change.pages.added[0];
        let ids: Vec<(String, LayerId)> = change
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
        let id = move |label: &str| {
            ids.iter()
                .find_map(|(found, id)| (found == label).then_some(*id))
                .expect("a layer")
        };
        let project = session.project();
        let english: LanguageIdentifier = "en".parse().expect("a language");
        let mut ops = Vec::new();
        for name in ["a", "b", "locked"] {
            let block = id(name);
            let placed = koharu_pipeline::placement(&project, block, &english).expect("a place");
            ops.push(Op::SetText {
                layer: block.into(),
                text: "Why?".to_owned(),
            });
            ops.push(placed.clone());
            ops.extend(
                styles::translated(
                    &project,
                    block,
                    "Why?",
                    &placed,
                    &LetteringSettings::default(),
                    Some(FONT),
                    &Marking::default(),
                )
                .expect("the lettering default"),
            );
            ops.push(Op::SetAttribute {
                layer: block.into(),
                namespace: "c2pa".to_owned(),
                key: "translation".to_owned(),
                value: Some(json!({"action": "c2pa.translated"})),
            });
        }
        ops.push(Op::UpdateLayer {
            layer: id("locked").into(),
            patch: json!({"locked": true}),
        });
        session
            .apply(Edit {
                label: "test".to_owned(),
                ops,
            })
            .expect("the translations");
        let tools = Tools::new(
            Arc::new(Mutex::new(session)),
            Pipeline::new(PipelineConfig::default()),
            Renderer::new(RendererConfig::default()),
        )
        .with_booharu(Arc::new(App));
        (dir, tools, page, id)
    }

    /// Calls `booharu_set_skew` with `args`, as the agent sends them.
    async fn skew(tools: &Tools, args: Value) -> Result<Value, ToolExecutionError> {
        let args = serde_json::from_value(args).expect("arguments the tool takes");
        let counts = SetSkew.call(&mut tools.tool_context(), args).await?;
        Ok(serde_json::to_value(counts).expect("an answer"))
    }

    /// The page's text effects, as `booharu_get_text_effects` answers them.
    async fn listed(tools: &Tools, page: PageId) -> Value {
        let effects = GetTextEffects
            .call(&mut tools.tool_context(), GetTextEffectsArgs { page })
            .await
            .expect("the page's effects");
        serde_json::to_value(effects).expect("an answer")
    }

    /// The undo steps' labels.
    fn steps(tools: &Tools) -> Vec<String> {
        tools.session.lock().history().steps
    }

    fn layer(tools: &Tools, layer: LayerId) -> Layer {
        let project = tools.session.lock().project();
        project.layer(layer).expect("the layer").clone()
    }

    /// The skew `layer` shows, in degrees along its width and its height.
    fn shown(tools: &Tools, id: LayerId) -> (f64, f64) {
        let layer = layer(tools, id);
        let text = match &layer.content {
            Content::Block(block) => block.translation.as_ref().expect("a translation"),
            Content::Text(text) => &**text,
            _ => panic!("{} holds no text", layer.id),
        };
        skew::shown(text, attrs::get::<Skew>(&layer).as_ref())
    }

    /// The counts `booharu_set_skew` answers.
    fn counts(changed: u32, locked: u32, untranslated: u32) -> Value {
        json!({"changed": changed, "locked": locked, "untranslated": untranslated})
    }

    #[tokio::test]
    async fn a_skew_over_two_boxes_is_one_undo_step_leaving_locked_and_untranslated_ones() {
        let (_dir, tools, _page, id) = open();
        let (a, t, locked) = (id("a"), id("t"), id("locked"));
        let kept = layer(&tools, locked);
        let before = steps(&tools).len();
        // The block and the text layer lean, the locked block and the
        // untranslated one are counted and left as they are, the raster is
        // left out, and the block named twice counts once.
        let names = json!({"layers": [a, t, locked, id("bare"), id("paint"), a], "x": 20, "y": -5});
        let answer = skew(&tools, names.clone()).await.expect("skewed");
        assert_eq!(answer, counts(2, 1, 1));
        let labels = steps(&tools);
        assert_eq!(labels.len(), before + 1);
        assert_eq!(labels.last().map(String::as_str), Some(effects::LABEL));
        for skewed in [a, t] {
            assert_eq!(shown(&tools, skewed), (20.0, -5.0), "{skewed}");
            let record = attrs::get::<Skew>(&layer(&tools, skewed)).expect("a record");
            assert_eq!((record.x, record.y), (20.0, -5.0), "{skewed}");
        }
        assert_eq!(layer(&tools, locked), kept);
        // Asked again, nothing changes, with no undo step, and the boxes
        // left alone are counted again.
        let answer = skew(&tools, names).await.expect("skewed again");
        assert_eq!(answer, counts(0, 1, 1));
        assert_eq!(steps(&tools).len(), before + 1);
    }

    #[tokio::test]
    async fn an_axis_left_out_keeps_each_boxs_own() {
        let (_dir, tools, _page, id) = open();
        let (a, b) = (id("a"), id("b"));
        skew(&tools, json!({"layers": [a], "x": 20, "y": 5}))
            .await
            .expect("a skewed");
        skew(&tools, json!({"layers": [b], "x": -10}))
            .await
            .expect("b skewed");
        assert_eq!(shown(&tools, b), (-10.0, 0.0));
        // Only the lean along their height: each keeps its own along its
        // width.
        let answer = skew(&tools, json!({"layers": [a, b], "y": -10}))
            .await
            .expect("both skewed");
        assert_eq!(answer, counts(2, 0, 0));
        assert_eq!(shown(&tools, a), (20.0, -10.0));
        assert_eq!(shown(&tools, b), (-10.0, -10.0));
    }

    #[tokio::test]
    async fn a_call_naming_no_box_no_axis_or_too_steep_a_skew_changes_nothing() {
        let (_dir, tools, _page, id) = open();
        let a = id("a");
        let first = layer(&tools, a);
        let before = steps(&tools).len();
        for (args, reason) in [
            (json!({"layers": [], "x": 10}), "name the boxes to skew"),
            (json!({"layers": [a]}), "give `x`, `y`, or both"),
            (
                json!({"layers": [a], "x": 61}),
                "`x` 61 isn't from -60 to 60 degrees",
            ),
            (
                json!({"layers": [a], "x": 10, "y": -60.5}),
                "`y` -60.5 isn't from -60 to 60 degrees",
            ),
        ] {
            let error = skew(&tools, args.clone()).await.expect_err("refused");
            assert_eq!(error.kind(), ToolErrorKind::InvalidArgs, "{args}: {error}");
            assert!(error.to_string().contains(reason), "{args}: {error}");
        }
        assert_eq!(steps(&tools).len(), before);
        assert_eq!(layer(&tools, a), first);
        // The schema refuses an empty list, too steep a skew, and an unknown
        // argument, and takes the steepest either way.
        let schema = SetSkew.parameters();
        let pivot = json!({"layers": [a], "x": 10, "pivot": [0, 0]});
        assert!(serde_json::from_value::<SetSkewArgs>(pivot.clone()).is_err());
        for refused in [
            json!({"layers": [], "x": 10}),
            json!({"layers": [a], "x": 61}),
            json!({"layers": [a], "y": -61}),
            pivot,
        ] {
            assert!(
                crate::tools::refusal(&schema, &refused).is_some(),
                "{refused}"
            );
        }
        let steepest = json!({"layers": [a], "x": 60, "y": -60});
        assert_eq!(crate::tools::refusal(&schema, &steepest), None);
    }

    #[tokio::test]
    async fn the_effects_list_a_skewed_box_and_one_with_a_gradient_until_reset() {
        let (_dir, tools, page, id) = open();
        let (a, b) = (id("a"), id("b"));
        // As translated, no box carries an effect.
        assert_eq!(listed(&tools, page).await, json!([]));
        // b skewed, and a filled with a gradient as `edit` fills a box.
        skew(&tools, json!({"layers": [b], "x": 12, "y": -3}))
            .await
            .expect("b skewed");
        let gradient = json!({"kind": "linear", "angle": 90.0, "stops": [
            {"offset": 0.0, "color": [1.0, 1.0, 0.0, 1.0]},
            {"offset": 1.0, "color": [1.0, 0.0, 0.0, 1.0]},
        ]});
        tools
            .session
            .lock()
            .apply(Edit {
                label: "test".to_owned(),
                ops: vec![Op::UpdateLayer {
                    layer: a.into(),
                    patch: json!({"translation": {"style": {"chars": {"fill_gradient": gradient}}}}),
                }],
            })
            .expect("a gradient");
        let before = steps(&tools).len();
        assert_eq!(
            listed(&tools, page).await,
            json!([
                {"layer": a, "effects": ["gradient"], "skew": null},
                {"layer": b, "effects": ["skew"], "skew": {"x": 12.0, "y": -3.0}},
            ])
        );
        // Listing changes nothing.
        assert_eq!(steps(&tools).len(), before);
        // Reset to lettering default takes both off.
        let args: ResetLetteringArgs =
            serde_json::from_value(json!({"layers": [a, b]})).expect("arguments the tool takes");
        ResetLettering
            .call(&mut tools.tool_context(), args)
            .await
            .expect("reset");
        assert_eq!(listed(&tools, page).await, json!([]));
        assert_eq!(shown(&tools, b), (0.0, 0.0));
    }

    #[test]
    fn skewing_changes_the_project_and_listing_effects_doesnt() {
        let definitions = Tools::toolset().tool_definitions();
        for (name, annotations, read_only) in [
            (SetSkew::NAME, SetSkew.annotations(), false),
            (GetTextEffects::NAME, GetTextEffects.annotations(), true),
        ] {
            assert_eq!(
                annotations.read_only_hint == Some(true),
                read_only,
                "{name}"
            );
            let tool = definitions
                .iter()
                .find(|tool| tool.name.as_str() == name)
                .unwrap_or_else(|| panic!("{name} is offered"));
            assert!(!tool.parameters.to_string().contains("$ref"), "{name}");
            assert_eq!(
                tool.annotations.read_only_hint == Some(true),
                read_only,
                "{name}"
            );
        }
    }
}
