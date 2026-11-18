//! Booharu's tools, one file per feature group, registered at the end of the
//! shared toolset, so the in-app agent and the MCP server offer them both.
//! Each takes the `booharu_` prefix, its schema from `schema::<T>()`, and
//! accurate read-only annotations, which MCP clients go by. Declared inside
//! `tools`, they reach the open project through its private fields and
//! helpers, and the app through the host the app gives
//! `Tools::with_booharu`.
//!
//! | Tool | Does | Only reads |
//! |---|---|---|
//! | `booharu_get_project_overview` | each page's size and blocks, how many are translated, the Booharu attributes on its layers, and which parts of its blocks were changed by hand; the translator's settings and whether its vendor is connected, the typesetting defaults, the font families, the look new text gets, and Booharu's settings | yes |
//! | `booharu_get_settings` | every Booharu settings section, by its table name | yes |
//! | `booharu_set_settings` | changes keys of one section and saves that section alone, all of them or none; an outside app can't change `booharu-mcp` | no |
//! | `booharu_set_translation` | sets a block's translation, placing it first as the Translation stage does when the block has none, as one undo step | no |
//! | `booharu_add_text_box` | adds a text block over a box the automatic run missed, as the O tool does, and starts the run reading it, and as its mode says translating and cleaning it | no |
//! | `booharu_find_split_balloons` | the balloons whose text is split over two or more text blocks, on a page or every page | yes |
//! | `booharu_join_balloon_blocks` | joins the text blocks of a block's balloon into the first one read, as one undo step | no |
//! | `booharu_join_page_balloons` | joins every split balloon of a page, as one undo step | no |
//! | `booharu_split_block` | splits a block in two across its lines, as one undo step, and starts the run reading and translating both halves | no |
//! | `booharu_set_review_note` | sets or clears the reviewer's note on a block or a text, as one undo step | no |
//! | `booharu_apply_text_style` | gives the translations of some blocks, pages, or the whole project the default text look again, each block's category font among it while Pick fonts automatically is on, or the font of a category or balloon shape the agent names, keeping hand-edited blocks unless told otherwise, as one undo step, and answers how many it changed, kept, and found untranslated | no |
//! | `booharu_count_text_style_targets` | how many blocks `booharu_apply_text_style` would change, keep, and find untranslated | yes |
//! | `booharu_run` | carries on with what the pages still need, as the Run button does with every stage on, one group of pages after another, or runs fewer stages on the pages named, and answers the plan with its first run | no |
//! | `booharu_set_lettering_category` | sets blocks' lettering categories as the agent's labels, never over a user's, and while Pick fonts automatically is on their category fonts, never over a hand-changed font, as one undo step | no |
//! | `booharu_get_text_styles` | the default text look, the default font, Pick fonts automatically, and each category's font with where it comes from; for a page or blocks, each block's category with why, its label, its balloon's shape, and whether it was changed by hand | yes |

mod boxes;
mod lettering;
mod notes;
mod project;
mod runs;
mod settings;
mod styles;
mod translation;

pub(crate) use self::runs::WAIT;

use std::{collections::BTreeSet, sync::Arc};

use booharu_core::settings::BooharuPreferences;
use koharu_pipeline::{Request, RunId, Stage, Target};
use koharu_scene::{Change, Edit, LayerId, Session};
use rig_agent::tool::ToolSet;
use rig_core::tool::ToolExecutionError;
use serde_json::{Map, Value};

use super::{Tools, failed};

/// The app, as Booharu's tools reach it beyond the open project's session,
/// pipeline, and renderer. The desktop app gives one; a host blocks, so a
/// tool calls it on a blocking thread.
pub trait BooharuHost: Send + Sync {
    /// Every Booharu settings section, as the file holds it now; a bad stored
    /// value is an error.
    fn preferences(&self) -> anyhow::Result<BooharuPreferences>;

    /// Sets each key `values` names in the Booharu settings section named
    /// `name` and saves that section alone, as
    /// `BooharuPreferences::save_section` does, so whatever another writer
    /// saved meanwhile stays; nothing when a section, key or value is bad.
    /// Answers every section as the file holds them then.
    fn save_section(
        &self,
        name: &str,
        values: Map<String, Value>,
    ) -> anyhow::Result<BooharuPreferences>;

    /// Koharu's Preferred fonts, `[renderer] font_families`, as the app holds
    /// them now; the first is the default font new text gets.
    fn font_families(&self) -> anyhow::Result<Vec<String>>;

    /// What the groups of runs `booharu_run` carries on with reach the app
    /// through, as the Run button's do: the app queues a run only on the
    /// project open and tells the interface each one. `None`, as outside
    /// the app, runs them on the tools' own pipeline and tells no one.
    fn runs(&self) -> Option<Arc<dyn booharu::runs::RunHost>> {
        None
    }
}

impl Tools {
    /// These tools with `host`, which Booharu's tools reach the app through.
    pub fn with_booharu(mut self, host: Arc<dyn BooharuHost>) -> Self {
        self.booharu = Some(host);
        self
    }
}

/// Marks a call an outside app made over MCP: the MCP server puts it in the
/// context of each call it runs, where only Booharu's own code puts anything,
/// so an outside app can neither forge nor drop it. A tool refuses through it
/// what only the user decides, such as how outside apps reach Booharu.
pub(crate) struct OverMcp;

/// Adds Booharu's tools to `tools`.
pub(crate) fn register(tools: &mut ToolSet) {
    tools.add_tool(project::GetProjectOverview);
    tools.add_tool(settings::GetSettings);
    tools.add_tool(settings::SetSettings);
    tools.add_tool(translation::SetTranslation);
    tools.add_tool(boxes::AddTextBox);
    tools.add_tool(boxes::FindSplitBalloons);
    tools.add_tool(boxes::JoinBalloonBlocks);
    tools.add_tool(boxes::JoinPageBalloons);
    tools.add_tool(boxes::SplitBlock);
    tools.add_tool(notes::SetReviewNote);
    tools.add_tool(styles::ApplyTextStyle);
    tools.add_tool(styles::CountTextStyleTargets);
    tools.add_tool(runs::Run);
    tools.add_tool(lettering::SetLetteringCategory);
    tools.add_tool(lettering::GetTextStyles);
}

/// Builds an edit with `build` from the open project and applies it as one
/// undo step, under the lock it was built under, so nothing changes in
/// between, and on a blocking thread, as `edit` applies one: a box action
/// reads pixels. A refusal reaches the model word for word.
async fn apply(
    tools: &Arc<Tools>,
    build: impl FnOnce(&Session) -> anyhow::Result<Edit> + Send + 'static,
) -> Result<Change, ToolExecutionError> {
    let tools = Arc::clone(tools);
    tokio::task::spawn_blocking(move || -> anyhow::Result<Change> {
        let mut session = tools.session.lock();
        let edit = build(&session)?;
        Ok(session.apply(edit)?)
    })
    .await
    .map_err(failed)?
    .map_err(|error| failed(format!("{error:#}")))
}

/// Starts the run of `stages` on `blocks` of the open project, as `run`
/// starts one, for `wait` to follow.
fn read(
    tools: &Tools,
    blocks: Vec<LayerId>,
    stages: BTreeSet<Stage>,
) -> Result<RunId, ToolExecutionError> {
    let request = Request {
        target: Target::Blocks(blocks),
        stages,
        config: None,
    };
    tools
        .pipeline
        .run(Arc::clone(&tools.session), request)
        .map_err(failed)
}
