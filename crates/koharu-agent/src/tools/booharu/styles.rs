//! Apply, as the agent calls it: the default text look given again to
//! translations already made, on some blocks, some pages, or the whole
//! project, and how many blocks that would change beforehand, each worked
//! out by `booharu::styles` as the Apply dialog's commands are, under the
//! saved `[booharu-lettering]` and the first of Koharu's Preferred fonts as
//! they are when called. The look the agent gives is its choice: each block
//! it changes counts as styled by hand afterwards, so automatic steps, and an
//! Apply that leaves hand-edited blocks alone, keep it.

use booharu::styles::{self, StyleCounts, StyleScope};
use booharu_core::{attrs::Origin, settings::LetteringSettings, styles::first_font};
use koharu_renderer::RendererConfig;
use rig_core::{
    completion::ToolAnnotations,
    tool::{Tool, ToolContext, ToolExecutionError},
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::tools::{Tools, failed, project, schema};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct TextStyleArgs {
    /// The blocks to look at: `{"kind": "blocks", "blocks": [...]}`, every
    /// block on `{"kind": "pages", "pages": [...]}`, or every block of
    /// `{"kind": "project"}`.
    scope: StyleScope,
    /// Whether blocks changed by hand take the look too; when false, the
    /// default, they're kept as they are.
    #[serde(default)]
    include_hand_edited: bool,
}

pub(super) struct ApplyTextStyle;

impl Tool for ApplyTextStyle {
    const NAME: &'static str = "booharu_apply_text_style";
    type Args = TextStyleArgs;
    type Output = StyleCounts;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Give the translations of some blocks, every block on some pages, or every block of the \
         project the default text look again, as one undo step: the default font, the first of \
         Koharu's Preferred fonts, in place of each one's family, and with colour detection off \
         the default colours and outline in place of its own, as `booharu-lettering` in \
         booharu_get_settings holds them; with detection on, no colours change. Booharu leaves \
         blocks changed by hand alone, their wording or their look, unless \
         `include_hand_edited` is true, and always leaves locked blocks alone; blocks without \
         a translation have nothing to restyle. Answers how many blocks it `changed`, kept as \
         `kept_hand_edited` (changed by hand, or locked), and found `untranslated`: tell the \
         user how many were kept, and that including hand-edited blocks would restyle them \
         too. The look it gives counts as chosen by hand, so later automatic steps leave those \
         blocks alone. booharu_count_text_style_targets answers the same numbers beforehand \
         without changing anything."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<TextStyleArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: TextStyleArgs,
    ) -> Result<StyleCounts, Self::Error> {
        let tools = project(context)?;
        // On a blocking thread, as `edit` applies, and under one lock, so the
        // blocks counted are the ones changed.
        tokio::task::spawn_blocking(move || -> anyhow::Result<StyleCounts> {
            let (settings, font) = look(&tools)?;
            let mut session = tools.session.lock();
            let (edit, counts) = styles::apply(
                &session.project(),
                &args.scope,
                args.include_hand_edited,
                &settings,
                font.as_deref(),
                Origin::Agent,
            )?;
            if let Some(edit) = edit {
                session.apply(edit)?;
            }
            Ok(counts)
        })
        .await
        .map_err(failed)?
        .map_err(|error| failed(format!("{error:#}")))
    }
}

pub(super) struct CountTextStyleTargets;

impl Tool for CountTextStyleTargets {
    const NAME: &'static str = "booharu_count_text_style_targets";
    type Args = TextStyleArgs;
    type Output = StyleCounts;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "How many blocks booharu_apply_text_style would change for the same scope and \
         `include_hand_edited` now, changing nothing: `changed`, the translated blocks that \
         would take the default look; `kept_hand_edited`, those it would keep, changed by hand \
         or locked; and `untranslated`, the blocks without a translation. A block whose \
         translation has the look already counts in none."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<TextStyleArgs>()
    }

    fn annotations(&self) -> ToolAnnotations {
        ToolAnnotations::default().with_read_only(true)
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: TextStyleArgs,
    ) -> Result<StyleCounts, Self::Error> {
        let tools = project(context)?;
        tokio::task::spawn_blocking(move || -> anyhow::Result<StyleCounts> {
            let (settings, font) = look(&tools)?;
            let project = tools.session.lock().project();
            styles::count(
                &project,
                &args.scope,
                args.include_hand_edited,
                &settings,
                font.as_deref(),
            )
        })
        .await
        .map_err(failed)?
        .map_err(|error| failed(format!("{error:#}")))
    }
}

/// The default look as the app holds it now, `[booharu-lettering]`, read as
/// the file holds it outside the app, and the default font, the first of
/// Koharu's Preferred fonts, none when they list none. A section that can't
/// be read is an error, so nothing is restyled by a value never chosen.
pub(super) fn look(tools: &Tools) -> anyhow::Result<(LetteringSettings, Option<String>)> {
    let settings = match &tools.booharu {
        Some(host) => host.preferences()?.lettering,
        None => koharu_config::get::<LetteringSettings>()?,
    };
    let families = koharu_config::get::<RendererConfig>()?.font_families;
    Ok((settings, first_font(&families).map(str::to_owned)))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use booharu_core::{attrs::Auto, edited, settings::BooharuPreferences};
    use koharu_pipeline::{Pipeline, PipelineConfig};
    use koharu_renderer::Renderer;
    use koharu_scene::{CharStyle, Content, Created, Edit, LayerId, Rgba, Session};
    use parking_lot::Mutex;
    use serde_json::json;

    use super::*;
    use crate::tools::booharu::BooharuHost;

    const RED: Rgba = Rgba([1.0, 0.0, 0.0, 1.0]);
    const BLUE: Rgba = Rgba([0.0, 0.0, 1.0, 1.0]);

    /// The app, whose default look is red text with colour detection off.
    struct Red;

    impl BooharuHost for Red {
        fn preferences(&self) -> anyhow::Result<BooharuPreferences> {
            let mut preferences = BooharuPreferences::default();
            preferences.lettering.default.fill = Some(RED);
            preferences.lettering.detect_colors = false;
            Ok(preferences)
        }

        fn save_section(
            &self,
            _: &str,
            _: serde_json::Map<String, Value>,
        ) -> anyhow::Result<BooharuPreferences> {
            anyhow::bail!("nothing here saves settings")
        }
    }

    /// A page of blocks lettered in blue, each found and read by Koharu's
    /// stages: `auto`, translated by them, `typed`, translated by hand,
    /// `locked`, translated by them and locked, and `none`, untranslated;
    /// the tools on it, and the blocks' ids.
    fn open() -> (tempfile::TempDir, Tools, impl Fn(&str) -> LayerId) {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        let found =
            json!({"detection": {"action": "c2pa.created"}, "ocr": {"action": "c2pa.created"}});
        let mut translated = found.clone();
        translated["translation"] = json!({"action": "c2pa.translated"});
        let block = |label: &str, c2pa: &Value, translation: bool, locked: bool| {
            let mut layer = json!({"kind": "block", "locked": locked, "source": {"content": "やあ"}, "attributes": {"c2pa": c2pa}});
            if translation {
                layer["translation"] =
                    json!({"content": "Hi", "style": {"chars": {"size": 32.0, "fill": BLUE}}});
            }
            json!({"op": "add_layer", "id": format!("${label}"), "page": "$page", "index": 0, "layer": layer})
        };
        let ops = [
            json!({"op": "add_page", "id": "$page", "index": 0, "size": {"width": 100, "height": 200}}),
            block("auto", &translated, true, false),
            block("typed", &found, true, false),
            block("locked", &translated, true, true),
            block("none", &found, false, false),
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
                .expect("a block")
        };
        let tools = Tools::new(
            Arc::new(Mutex::new(session)),
            Pipeline::new(PipelineConfig::default()),
            Renderer::new(RendererConfig::default()),
        )
        .with_booharu(Arc::new(Red));
        (dir, tools, id)
    }

    fn args(include_hand_edited: bool) -> TextStyleArgs {
        TextStyleArgs {
            scope: StyleScope::Project,
            include_hand_edited,
        }
    }

    fn counts(changed: u32, kept_hand_edited: u32, untranslated: u32) -> StyleCounts {
        StyleCounts {
            changed,
            kept_hand_edited,
            untranslated,
        }
    }

    async fn count(tools: &Tools, include_hand_edited: bool) -> StyleCounts {
        CountTextStyleTargets
            .call(&mut tools.tool_context(), args(include_hand_edited))
            .await
            .expect("a count")
    }

    #[tokio::test]
    async fn the_agents_look_keeps_hand_edits_and_counts_as_one_afterwards() {
        let (_dir, tools, id) = open();
        // The typed translation and the locked block are kept.
        assert_eq!(count(&tools, false).await, counts(1, 2, 1));
        let applied = ApplyTextStyle
            .call(&mut tools.tool_context(), args(false))
            .await
            .expect("the look applied");
        assert_eq!(applied, counts(1, 2, 1));

        let project = tools.session.lock().project();
        let chars = |label: &str| {
            let layer = project.layer(id(label)).expect("the block");
            match &layer.content {
                Content::Block(block) => block.translation.clone().expect("a translation"),
                _ => panic!("{label} isn't a block"),
            }
            .style
            .chars
        };
        let families = koharu_config::get::<RendererConfig>().expect("the renderer's settings");
        assert_eq!(
            chars("auto"),
            CharStyle {
                family: first_font(&families.font_families).map(str::to_owned),
                size: Some(32.0),
                fill: Some(RED),
                ..CharStyle::default()
            }
        );
        assert_eq!(chars("typed").fill, Some(BLUE));
        assert_eq!(chars("locked").fill, Some(BLUE));
        // The agent chose the look, so it's kept as a hand edit is.
        let auto = project.layer(id("auto")).expect("the block");
        assert_eq!(Auto::of(auto).origin, Origin::Agent);
        assert!(edited(auto).style);
        assert_eq!(count(&tools, false).await, counts(0, 3, 1));
        // Including hand edits restyles the typed one only; the locked block
        // is kept whatever.
        assert_eq!(count(&tools, true).await, counts(1, 1, 1));
    }

    #[test]
    fn an_unknown_argument_is_refused() {
        let args =
            json!({"scope": {"kind": "project"}, "include_hand_edited": true, "force": true});
        assert!(serde_json::from_value::<TextStyleArgs>(args).is_err());
        let args = json!({"scope": {"kind": "pages", "pages": []}});
        let read = serde_json::from_value::<TextStyleArgs>(args).expect("the arguments");
        assert!(!read.include_hand_edited);
        // A scope naming pages it doesn't take would restyle the whole
        // project, so the schema refuses it before the call.
        let schema = ApplyTextStyle.parameters();
        let pages = json!({"scope": {"kind": "project", "pages": []}});
        assert!(crate::tools::refusal(&schema, &pages).is_some());
        let project = json!({"scope": {"kind": "project"}});
        assert_eq!(crate::tools::refusal(&schema, &project), None);
    }
}
