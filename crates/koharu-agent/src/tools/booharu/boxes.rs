//! The box tools, as the app's own: a text block added over a box the
//! automatic run missed, the balloons whose text is split over several
//! blocks and joining them, and a block split in two, each edit built by
//! `booharu::boxes`, which the app's commands share, as one undo step.

use booharu::boxes::{self, SplitBalloon};
use booharu_core::settings::OcrBoxMode;
use koharu_pipeline::RunId;
use koharu_scene::{Bounds, Change, LayerId, PageId};
use rig_core::{
    completion::ToolAnnotations,
    tool::{Tool, ToolContext, ToolExecutionError},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{apply, read};
use crate::tools::{failed, project, schema};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct AddTextBoxArgs {
    /// The page.
    page: PageId,
    /// The box around the text, in the page's pixels; it's cut to the page.
    rect: Bounds,
    /// What the block goes through: `read` reads it, `read_translate` also
    /// translates it, and `read_translate_clean` also cleans the art under
    /// the whole box.
    mode: OcrBoxMode,
}

/// A block added over a box, and the run reading it.
#[derive(Debug, Serialize)]
pub(super) struct AddedTextBox {
    /// The new block.
    block: LayerId,
    /// The run reading it, which `wait` follows.
    run: RunId,
}

pub(super) struct AddTextBox;

impl Tool for AddTextBox {
    const NAME: &'static str = "booharu_add_text_box";
    type Args = AddTextBoxArgs;
    type Output = AddedTextBox;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Add a text block over a box on a page, for a line of text the automatic run missed, as \
         the O tool adds one, as one undo step, then start the run that reads it, and as the \
         mode says translates it and cleans the art under the whole box; the run goes on in the \
         background, and wait follows it. The block goes in the smallest balloon holding most of \
         the box, else the panel holding its centre, else on the page, read in order among what \
         is there. This doesn't style the text: the translation is lettered as the Translation \
         stage letters one. To read or translate an existing block again, use run with the \
         block as the target's blocks and the stages instead. A box over blank art adds nothing \
         and says there's nothing to read, and a locked balloon or panel refuses it. Answers the \
         new block and the run's id."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<AddTextBoxArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: AddTextBoxArgs,
    ) -> Result<AddedTextBox, Self::Error> {
        let tools = project(context)?;
        let AddTextBoxArgs { page, rect, mode } = args;
        let change = apply(&tools, move |session| {
            boxes::add_box(session, page, rect, mode)
        })
        .await?;
        let block = boxes::added_box(&change).map_err(failed)?;
        let run = read(&tools, vec![block], boxes::stages(mode))?;
        Ok(AddedTextBox { block, run })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct FindSplitBalloonsArgs {
    /// The page to look on; every page, in order, when it's left out.
    #[serde(default)]
    page: Option<PageId>,
}

pub(super) struct FindSplitBalloons;

impl Tool for FindSplitBalloons {
    const NAME: &'static str = "booharu_find_split_balloons";
    type Args = FindSplitBalloonsArgs;
    type Output = Vec<SplitBalloon>;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "The balloons whose text is split over two or more text blocks, on a page or, without \
         one, on every page in order: each with its page and its text blocks in reading order, \
         sound effects and hidden blocks left out. Joining makes a balloon's blocks one: tell \
         the user which balloons are split and ask before joining them, unless they asked for \
         the join."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<FindSplitBalloonsArgs>()
    }

    fn annotations(&self) -> ToolAnnotations {
        ToolAnnotations::default().with_read_only(true)
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: FindSplitBalloonsArgs,
    ) -> Result<Vec<SplitBalloon>, Self::Error> {
        let tools = project(context)?;
        tokio::task::spawn_blocking(move || {
            let project = tools.session.lock().project();
            boxes::find_split_balloons(&project, args.page)
        })
        .await
        .map_err(failed)?
        .map_err(|error| failed(format!("{error:#}")))
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct JoinBalloonBlocksArgs {
    /// A text block of the balloon.
    block: LayerId,
}

pub(super) struct JoinBalloonBlocks;

impl Tool for JoinBalloonBlocks {
    const NAME: &'static str = "booharu_join_balloon_blocks";
    type Args = JoinBalloonBlocksArgs;
    type Output = Change;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Join the text blocks of the balloon holding a block into the first one read, as one \
         undo step. It keeps its place and its id; its source's frame becomes the box around \
         them all, and its wording and its translation every block's that isn't blank, a line \
         each, the translation placed in the balloon as the Translation stage places one; the \
         others' masks and clean-ups merge into its own before they go. Unless the user asked \
         for this join, tell them which balloons are split, as booharu_find_split_balloons finds \
         them, and ask first. A balloon of one text block has nothing to join, and a locked \
         block or balloon refuses it. Answers what changed."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<JoinBalloonBlocksArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: JoinBalloonBlocksArgs,
    ) -> Result<Change, Self::Error> {
        let tools = project(context)?;
        let block = args.block;
        apply(&tools, move |session| boxes::join_balloon(session, block)).await
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct JoinPageBalloonsArgs {
    /// The page.
    page: PageId,
}

pub(super) struct JoinPageBalloons;

impl Tool for JoinPageBalloons {
    const NAME: &'static str = "booharu_join_page_balloons";
    type Args = JoinPageBalloonsArgs;
    type Output = Change;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Join the text blocks of every balloon on a page whose text is split over two or more, \
         each as booharu_join_balloon_blocks joins one, all as one undo step. Unless the user \
         asked for this join, tell them which balloons are split, as \
         booharu_find_split_balloons finds them, and ask first. A page with no split balloon has \
         nothing to join, and a locked block or balloon in any of them refuses them all. \
         Answers what changed."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<JoinPageBalloonsArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: JoinPageBalloonsArgs,
    ) -> Result<Change, Self::Error> {
        let tools = project(context)?;
        let page = args.page;
        apply(&tools, move |session| boxes::join_page(session, page)).await
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct SplitBlockArgs {
    /// The block.
    block: LayerId,
}

/// A block split in two, and the run reading both halves.
#[derive(Debug, Serialize)]
pub(super) struct SplitHalves {
    /// The halves in reading order: the block itself, then its copy.
    halves: Vec<LayerId>,
    /// The run reading and translating both, which `wait` follows.
    run: RunId,
}

pub(super) struct SplitBlock;

impl Tool for SplitBlock {
    const NAME: &'static str = "booharu_split_block";
    type Args = SplitBlockArgs;
    type Output = SplitHalves;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Split a block in two across its lines, as one undo step, then start the run that reads \
         and translates both halves again; the run goes on in the background, and wait follows \
         it. The half read first, the right one of vertical text and the top one of horizontal \
         text, keeps the block's id, and a copy just after it takes the other; each keeps its \
         part of the mask and clean-up and loses its wording and translation until the run \
         writes them again. A block whose halves would be under 4 pixels across its lines, or a \
         locked one, is refused. Answers the halves in reading order and the run's id."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<SplitBlockArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: SplitBlockArgs,
    ) -> Result<SplitHalves, Self::Error> {
        let tools = project(context)?;
        let block = args.block;
        let change = apply(&tools, move |session| boxes::split_block(session, block)).await?;
        let halves = boxes::split_halves(block, &change).map_err(failed)?;
        let run = read(&tools, halves.clone(), boxes::split_stages())?;
        Ok(SplitHalves { halves, run })
    }
}

#[cfg(test)]
mod tests {
    use std::{io::Cursor, sync::Arc, time::Duration};

    use image::{ImageFormat, Rgb, RgbImage};
    use koharu_pipeline::{Pipeline, PipelineConfig};
    use koharu_renderer::{Renderer, RendererConfig};
    use koharu_scene::{Content, Created, Edit, Hash, Op, Session};
    use parking_lot::Mutex;
    use serde_json::json;

    use super::*;
    use crate::Tools;

    /// Tools on a project of one 400 by 400 page, whose scan is white with
    /// lines of black lettering across its top half.
    struct Fixture {
        _dir: tempfile::TempDir,
        tools: Tools,
        page: PageId,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("a folder");
            let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
            let scan = RgbImage::from_fn(400, 400, |_, y| {
                Rgb(if y < 200 && y % 6 < 2 {
                    [0; 3]
                } else {
                    [255; 3]
                })
            });
            let mut png = Vec::new();
            scan.write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
                .expect("a PNG");
            let background = Hash::of(&png);
            let add_page = json!({"op": "add_page", "index": 0, "background": background});
            let change = session
                .apply(Edit {
                    label: "test".to_owned(),
                    ops: vec![
                        Op::AddFile { bytes: png },
                        serde_json::from_value(add_page).expect("an op"),
                    ],
                })
                .expect("a page");
            let page = *change.pages.added.first().expect("the page");
            let tools = Tools::new(
                Arc::new(Mutex::new(session)),
                Pipeline::new(PipelineConfig::default()),
                Renderer::new(RendererConfig::default()),
            );
            Self {
                _dir: dir,
                tools,
                page,
            }
        }

        /// Adds `layer` under `parent`, or on the page, last.
        fn add(&self, parent: Option<LayerId>, layer: Value) -> LayerId {
            let mut session = self.tools.session.lock();
            let project = session.project();
            let index = match parent {
                Some(parent) => project.layer(parent).expect("the parent").children.len(),
                None => project.page(self.page).expect("the page").layers.len(),
            };
            let op = json!({
                "op": "add_layer", "page": self.page, "parent": parent, "index": index,
                "layer": layer,
            });
            let change = session
                .apply(Edit {
                    label: "test".to_owned(),
                    ops: vec![serde_json::from_value(op).expect("an op")],
                })
                .expect("a layer");
            match change.created.first() {
                Some(Created::Layer { id, .. }) => *id,
                _ => panic!("{change:?}"),
            }
        }

        /// A square balloon over the page's top left quarter.
        fn balloon(&self) -> LayerId {
            self.add(
                None,
                json!({"kind": "balloon", "outline": "M0 0 L200 0 L200 200 L0 200 Z"}),
            )
        }

        /// A horizontal text block in `balloon` over `bounds`, reading
        /// `content`.
        fn text(&self, balloon: LayerId, bounds: Value, content: &str) -> LayerId {
            self.add(
                Some(balloon),
                json!({"kind": "block", "source": {
                    "frame": {"box": {"bounds": bounds}},
                    "direction": "horizontal",
                    "content": content,
                    "language": "ja",
                }}),
            )
        }

        fn lock(&self, layer: LayerId) {
            self.tools
                .session
                .lock()
                .apply(Edit {
                    label: "test".to_owned(),
                    ops: vec![Op::UpdateLayer {
                        layer: layer.into(),
                        patch: json!({"locked": true}),
                    }],
                })
                .expect("a lock");
        }

        fn holder(&self, layer: LayerId) -> Option<LayerId> {
            let project = self.tools.session.lock().project();
            let (_, holder) = project.parent(layer).expect("the layer");
            holder.map(|holder| holder.id)
        }

        fn steps(&self) -> Vec<String> {
            self.tools.session.lock().history().steps
        }

        async fn split_balloons(&self) -> Vec<SplitBalloon> {
            FindSplitBalloons
                .call(
                    &mut self.tools.tool_context(),
                    FindSplitBalloonsArgs { page: None },
                )
                .await
                .expect("the split balloons")
        }

        /// How `run` ended, stopped here as soon as it was asked for, so no
        /// model loads.
        async fn ended(&self, run: RunId) -> Value {
            let events = self
                .tools
                .pipeline
                .history(run, Duration::from_secs(10))
                .await
                .expect("the run's events");
            json!(events.last())
        }
    }

    fn stopped(run: RunId) -> Value {
        json!({"kind": "ended", "run": run, "outcome": {"kind": "stopped"}})
    }

    #[tokio::test]
    async fn a_text_box_is_added_by_hand_in_its_balloon_and_read() {
        let fixture = Fixture::new();
        let balloon = fixture.balloon();
        let schema = AddTextBox.parameters().to_string();
        for mode in ["\"read\"", "\"read_translate\"", "\"read_translate_clean\""] {
            assert!(schema.contains(mode), "{mode}");
        }

        let args = json!({
            "page": fixture.page,
            "rect": {"x": 20.0, "y": 30.0, "width": 120.0, "height": 40.0},
            "mode": "read_translate",
        });
        let added = AddTextBox
            .call(
                &mut fixture.tools.tool_context(),
                serde_json::from_value(args).expect("a box"),
            )
            .await
            .expect("the box is added");
        // Before the scheduler can start reading it.
        fixture.tools.pipeline.stop();

        assert_eq!(fixture.holder(added.block), Some(balloon));
        let project = fixture.tools.session.lock().project();
        let layer = project.layer(added.block).expect("the block");
        assert!(matches!(layer.content, Content::Block(_)));
        // No provenance, c2pa's or any other, so it counts as made by hand.
        assert!(layer.attributes.is_empty(), "{:?}", layer.attributes);
        assert_eq!(
            fixture.steps().last().map(String::as_str),
            Some(boxes::ADD_BOX)
        );
        assert_eq!(fixture.ended(added.run).await, stopped(added.run));

        // The page's bottom half is blank: nothing is added.
        let steps = fixture.steps().len();
        let blank = AddTextBox
            .call(
                &mut fixture.tools.tool_context(),
                AddTextBoxArgs {
                    page: fixture.page,
                    rect: Bounds {
                        x: 20.0,
                        y: 250.0,
                        width: 120.0,
                        height: 40.0,
                    },
                    mode: OcrBoxMode::Read,
                },
            )
            .await;
        let error = blank.err().map(|error| error.to_string());
        assert!(
            error
                .as_ref()
                .is_some_and(|error| error.contains("nothing to read")),
            "{error:?}"
        );
        assert_eq!(fixture.steps().len(), steps);
    }

    #[tokio::test]
    async fn a_split_balloon_is_found_joined_and_split_again() {
        let fixture = Fixture::new();
        let balloon = fixture.balloon();
        let bounds = |y: f64| json!({"x": 20.0, "y": y, "width": 160.0, "height": 40.0});
        let first = fixture.text(balloon, bounds(20.0), "あ");
        let second = fixture.text(balloon, bounds(80.0), "う");

        let found = fixture.split_balloons().await;
        assert_eq!(
            found,
            [SplitBalloon {
                page: fixture.page,
                balloon,
                blocks: vec![first, second],
            }]
        );

        JoinBalloonBlocks
            .call(
                &mut fixture.tools.tool_context(),
                JoinBalloonBlocksArgs { block: second },
            )
            .await
            .expect("the balloon is joined");
        assert!(fixture.split_balloons().await.is_empty());
        let project = fixture.tools.session.lock().project();
        assert!(project.layer(second).is_err(), "the second block goes");
        assert_eq!(
            fixture.steps().last().map(String::as_str),
            Some(boxes::JOIN_BALLOON)
        );

        let split = SplitBlock
            .call(
                &mut fixture.tools.tool_context(),
                SplitBlockArgs { block: first },
            )
            .await
            .expect("the block is split");
        // Before the scheduler can start reading the halves.
        fixture.tools.pipeline.stop();
        let [kept, copy] = split.halves[..] else {
            panic!("{:?}", split.halves);
        };
        assert_eq!(kept, first);
        assert_eq!(fixture.holder(copy), Some(balloon));
        assert_eq!(fixture.ended(split.run).await, stopped(split.run));
        let found = fixture.split_balloons().await;
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].blocks, [first, copy]);

        JoinPageBalloons
            .call(
                &mut fixture.tools.tool_context(),
                JoinPageBalloonsArgs { page: fixture.page },
            )
            .await
            .expect("the page is joined");
        assert!(fixture.split_balloons().await.is_empty());
        assert_eq!(
            fixture.steps().last().map(String::as_str),
            Some(boxes::JOIN_PAGE)
        );
    }

    #[tokio::test]
    async fn a_locked_block_is_neither_joined_nor_split() {
        let fixture = Fixture::new();
        let balloon = fixture.balloon();
        let bounds = |y: f64| json!({"x": 20.0, "y": y, "width": 160.0, "height": 40.0});
        let first = fixture.text(balloon, bounds(20.0), "あ");
        let second = fixture.text(balloon, bounds(80.0), "う");
        fixture.lock(second);
        let steps = fixture.steps().len();

        let refused = |result: Result<_, ToolExecutionError>| {
            let error = result.err().map(|error| error.to_string());
            assert!(
                error
                    .as_ref()
                    .is_some_and(|error| error.contains(&format!("layer {second} is locked"))),
                "{error:?}"
            );
        };
        refused(
            JoinBalloonBlocks
                .call(
                    &mut fixture.tools.tool_context(),
                    JoinBalloonBlocksArgs { block: first },
                )
                .await
                .map(drop),
        );
        refused(
            JoinPageBalloons
                .call(
                    &mut fixture.tools.tool_context(),
                    JoinPageBalloonsArgs { page: fixture.page },
                )
                .await
                .map(drop),
        );
        refused(
            SplitBlock
                .call(
                    &mut fixture.tools.tool_context(),
                    SplitBlockArgs { block: second },
                )
                .await
                .map(drop),
        );
        assert_eq!(fixture.steps().len(), steps);
    }
}
