//! Booharu's settings: every `[booharu-*]` section, read and changed by its
//! table name through the app, as Booharu's settings commands do. An outside
//! app reads them all, but never changes `[booharu-mcp]`, which decides how
//! outside apps reach Booharu.

use std::sync::Arc;

use booharu_core::settings::{BooharuPreferences, McpSettings};
use koharu_config::Section as _;
use rig_core::{
    completion::ToolAnnotations,
    tool::{Tool, ToolContext, ToolExecutionError},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{BooharuHost, OverMcp};
use crate::tools::{NoArgs, failed, project, schema};

pub(super) struct GetSettings;

impl Tool for GetSettings {
    const NAME: &'static str = "booharu_get_settings";
    type Args = NoArgs;
    type Output = BooharuPreferences;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Booharu's settings, each section under its table name with every key's value: \
         `booharu-mcp` has the `port` the MCP server outside apps reach Booharu through listens \
         on; `booharu-ocr-box` has the `default_mode` a box drawn with the O tool goes through, \
         `read`, `read_translate`, or `read_translate_clean`; `booharu-lettering` has the \
         `default` text look, its `fill` and `stroke` colours as [red, green, blue, alpha] from \
         0 to 1, or null for Koharu's black and no outline, and its `stroke_width` from 0 to \
         50, which new text gets; `detect_colors`, whether a new translation keeps the colours \
         Koharu measured (true) or gets the default look's (false); `auto`, Pick fonts \
         automatically, whether a block's translation is lettered in its lettering category's \
         font as it's written, as its category changes, and by booharu_apply_text_style \
         (true), or in the default font (false); `categories`, the categories given their own \
         font, by id, each with its `family`, null for `normal_dialogue`'s family (in \
         `normal_dialogue`'s own entry, for the default font), `bold`, and `italic`, a \
         category without one using `normal_dialogue`'s, and without either the default \
         font, neither bold nor italic; `small_text`, the share of the page's height, \
         above 0 and at most 1, under which text's glyphs make it small text; and \
         `model_confidence`, from 0 to 1, how sure a model, you included, must be of a \
         delivery category such as `whisper` for its label to win over small text and the \
         balloon's shape (booharu_get_text_styles answers the font each category letters in \
         and each block's category); `booharu-fonts` has \
         the font picker's `favorites`, family names; `booharu-runs` has the Run menu's \
         `presets`, each a `name` and its `stages` (`detect`, `recognize`, `inpaint`, `read`, \
         `translate`; `read`, Read chapter, only with `translate`), which set the menu's stage \
         switches and start nothing, and how runs over \
         pages go, whoever starts them: `retries`, from 0 to 5, how often a failed page stage is \
         tried again, `on_page_failure`, `continue` with the other pages or `stop` the run, \
         `order`, `pages` (each page through its stages) or `stages` (one stage at a time over \
         every page, using less graphics memory), and `finish_sound`, whether a run over pages \
         chimes as it ends; `booharu-reading` has Read chapter's `model`, the model that reads a \
         chapter's pages, which must see images and call tools, as a `vendor/model` reference, \
         or null for the translation model when that one can, `ask`, whether the Context tab's \
         Read chapter asks before it reads, and `pages_per_request`, from 1 to 40, the most \
         pages one request to the model sends (booharu_read_chapter reads with that model, or \
         one it names); Explain this line asks the same `model` when one is set; \
         `booharu-start` has `fresh_start_note_seen`, whether the note on a \
         first start saying where Booharu 0.0.4's series are was dismissed. The default font is the first of Koharu's Preferred fonts, not a \
         Booharu setting. Needs a project open, as every tool here does."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<NoArgs>()
    }

    fn annotations(&self) -> ToolAnnotations {
        ToolAnnotations::default().with_read_only(true)
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        _: NoArgs,
    ) -> Result<BooharuPreferences, Self::Error> {
        let host = host(context)?;
        tokio::task::spawn_blocking(move || host.preferences())
            .await
            .map_err(failed)?
            .map_err(|error| failed(format!("{error:#}")))
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct SetSettingsArgs {
    /// The section's table name, such as `booharu-mcp`.
    section: String,
    /// The keys to change, each to its new value. A table among them, such
    /// as `default`, changes only in the keys it names, so `{"default":
    /// {"fill": [1, 0, 0, 1]}}` keeps the outline; the section's other keys
    /// keep theirs too.
    values: Map<String, Value>,
}

/// A section as it was saved.
#[derive(Serialize)]
pub(super) struct Saved {
    section: String,
    saved: Value,
    /// Why the change doesn't apply yet, when it doesn't.
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<&'static str>,
}

pub(super) struct SetSettings;

impl Tool for SetSettings {
    const NAME: &'static str = "booharu_set_settings";
    type Args = SetSettingsArgs;
    type Output = Saved;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Change keys of one of Booharu's settings sections, named as booharu_get_settings names \
         them, and save that section alone; an unknown section or key, a key inside one of its \
         tables included, or an invalid value, saves nothing. Answers the section as saved, \
         with a note when the change applies only once Booharu restarts. `categories` changes \
         only the categories it names, each only in the keys it names, so `{\"categories\": \
         {\"shout\": {\"bold\": true}}}` keeps Shout's family and every other category's \
         font, and a category set to null, `{\"categories\": {\"shout\": null}}`, loses its \
         own font. A change to `booharu-lettering`, `auto` and `categories` included, applies \
         to text made from then on, never to translations already made: \
         booharu_apply_text_style gives those the new look, their category fonts among it. A \
         change to `booharu-runs` applies to runs started from then on, the next run of a group \
         booharu_run carries on with included; a run already going keeps the settings it \
         started with. A preset's `stages` are among `detect`, `recognize`, `inpaint`, `read` \
         and `translate`, with at least one other than `read`, Read chapter, which goes only \
         with `translate`. A change to `booharu-reading` applies to the next read and the next \
         line explained; a blank `model` is refused, so set it to null for the translation \
         model. A call an outside app makes over MCP can't change `booharu-mcp`. Needs \
         a project open, as every tool here does."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<SetSettingsArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: SetSettingsArgs,
    ) -> Result<Saved, Self::Error> {
        if args.section == McpSettings::NAME && context.scope::<OverMcp>().is_some() {
            return Err(ToolExecutionError::refused(format!(
                "an outside app can't change [{}], which decides how outside apps reach Booharu; \
                 the user changes it in Booharu",
                McpSettings::NAME
            )));
        }
        let host = host(context)?;
        let SetSettingsArgs { section, values } = args;
        tokio::task::spawn_blocking(move || -> anyhow::Result<Saved> {
            let saved = host.save_section(&section, values)?.section(&section)?;
            let note = BooharuPreferences::read_at_launch(&section).then_some(
                "Booharu reads this section as it starts: the change applies once it restarts.",
            );
            Ok(Saved {
                section,
                saved,
                note,
            })
        })
        .await
        .map_err(failed)?
        .map_err(|error| failed(format!("{error:#}")))
    }
}

/// The app, through which the settings are read and saved, as the open
/// project's tools reach it: the MCP server has them only while a project is
/// open.
fn host(context: &ToolContext) -> Result<Arc<dyn BooharuHost>, ToolExecutionError> {
    project(context)?.booharu.clone().ok_or_else(|| {
        ToolExecutionError::other("Booharu's settings aren't available here, outside the app")
    })
}

#[cfg(test)]
mod tests {
    use koharu_pipeline::{Pipeline, PipelineConfig};
    use koharu_renderer::{Renderer, RendererConfig};
    use koharu_scene::Session;
    use parking_lot::Mutex;
    use serde_json::json;

    use super::*;
    use crate::{McpServer, Tools};

    /// Booharu's settings in memory, saved as the file would read them back.
    #[derive(Default)]
    struct Settings(Mutex<BooharuPreferences>);

    impl BooharuHost for Settings {
        fn preferences(&self) -> anyhow::Result<BooharuPreferences> {
            Ok(self.0.lock().clone())
        }

        fn save_section(
            &self,
            name: &str,
            values: Map<String, Value>,
        ) -> anyhow::Result<BooharuPreferences> {
            let mut preferences = self.preferences()?;
            preferences.set_section(name, values)?;
            *self.0.lock() = serde_json::from_value(serde_json::to_value(preferences)?)?;
            self.preferences()
        }

        fn font_families(&self) -> anyhow::Result<Vec<String>> {
            Ok(Vec::new())
        }
    }

    /// Tools on an empty project, in an app whose settings `settings` keeps.
    fn open(settings: &Arc<Settings>) -> (tempfile::TempDir, Tools) {
        let dir = tempfile::tempdir().expect("a folder");
        let session = Session::create(dir.path().join("test.khr")).expect("a project");
        let tools = Tools::new(
            Arc::new(Mutex::new(session)),
            Pipeline::new(PipelineConfig::default()),
            Renderer::new(RendererConfig::default()),
        )
        .with_booharu(Arc::clone(settings) as Arc<dyn BooharuHost>);
        (dir, tools)
    }

    fn port(settings: &Settings) -> u16 {
        settings.0.lock().mcp.port
    }

    #[tokio::test]
    async fn the_settings_are_read_and_saved_through_the_app() {
        let settings = Arc::new(Settings::default());
        let (_dir, tools) = open(&settings);
        let read = GetSettings
            .call(&mut tools.tool_context(), NoArgs {})
            .await
            .expect("the settings");
        assert_eq!(
            json!(read),
            json!({
                "booharu-mcp": {"port": 47821},
                "booharu-ocr-box": {"default_mode": "read_translate"},
                "booharu-lettering": {
                    "default": {"fill": null, "stroke": null, "stroke_width": 1.5},
                    "detect_colors": true,
                    "auto": false,
                    "categories": {},
                    "small_text": 0.012,
                    "model_confidence": 0.6,
                },
                "booharu-fonts": {"favorites": []},
                "booharu-runs": {
                    "presets": [
                        {"name": "Detect + OCR", "stages": ["detect", "recognize"]},
                        {"name": "Translate + clean up", "stages": ["inpaint", "translate"]},
                    ],
                    "retries": 2,
                    "on_page_failure": "continue",
                    "order": "pages",
                    "finish_sound": true,
                },
                "booharu-reading": {"model": null, "ask": true, "pages_per_request": 20},
                "booharu-start": {"fresh_start_note_seen": false},
            })
        );

        let args = |port: u16| SetSettingsArgs {
            section: McpSettings::NAME.to_owned(),
            values: Map::from_iter([("port".to_owned(), json!(port))]),
        };
        let saved = SetSettings
            .call(&mut tools.tool_context(), args(50000))
            .await
            .expect("a new port");
        assert_eq!(json!(saved)["saved"], json!({"port": 50000}));
        assert!(
            saved.note.is_some(),
            "the port applies once Booharu restarts"
        );
        assert_eq!(port(&settings), 50000);

        let refused = SetSettings.call(&mut tools.tool_context(), args(80)).await;
        let error = refused.err().map(|error| error.to_string());
        assert!(
            error
                .as_ref()
                .is_some_and(|error| error.contains("port 80")),
            "{error:?}"
        );
        assert_eq!(port(&settings), 50000);
    }

    #[tokio::test]
    async fn the_default_look_and_the_favourites_are_saved_and_checked() {
        let settings = Arc::new(Settings::default());
        let (_dir, tools) = open(&settings);
        let set = |section: &str, values: Value| SetSettingsArgs {
            section: section.to_owned(),
            values: serde_json::from_value(values).expect("keys"),
        };
        let look = json!({"fill": [1.0, 0.0, 0.0, 1.0], "stroke": null, "stroke_width": 4.0});
        let lettering = set(
            "booharu-lettering",
            json!({"default": look, "detect_colors": false}),
        );
        let saved = SetSettings
            .call(&mut tools.tool_context(), lettering)
            .await
            .expect("a new look");
        assert_eq!(
            json!(saved),
            json!({
                "section": "booharu-lettering",
                "saved": {
                    "default": look,
                    "detect_colors": false,
                    "auto": false,
                    "categories": {},
                    "small_text": 0.012,
                    "model_confidence": 0.6,
                },
            })
        );
        let wide = json!({"default": {"fill": null, "stroke": null, "stroke_width": 60.0}});
        let refused = SetSettings
            .call(&mut tools.tool_context(), set("booharu-lettering", wide))
            .await;
        assert!(refused.is_err());
        assert_eq!(settings.0.lock().lettering.default.stroke_width, 4.0);
        // A table changes only in the keys named.
        let outline = json!({"default": {"stroke": [1.0, 1.0, 1.0, 1.0]}});
        let saved = SetSettings
            .call(&mut tools.tool_context(), set("booharu-lettering", outline))
            .await
            .expect("an outline");
        assert_eq!(
            json!(saved)["saved"]["default"],
            json!({"fill": [1.0, 0.0, 0.0, 1.0], "stroke": [1.0, 1.0, 1.0, 1.0], "stroke_width": 4.0})
        );

        let favourites = set("booharu-fonts", json!({"favorites": ["Anime Ace"]}));
        let saved = SetSettings
            .call(&mut tools.tool_context(), favourites)
            .await
            .expect("new favourites");
        assert_eq!(json!(saved)["saved"], json!({"favorites": ["Anime Ace"]}));
    }

    #[tokio::test]
    async fn category_fonts_change_one_category_and_one_key_at_a_time() {
        let settings = Arc::new(Settings::default());
        let (_dir, tools) = open(&settings);
        let save = async |values: Value| {
            let args = SetSettingsArgs {
                section: "booharu-lettering".to_owned(),
                values: serde_json::from_value(values).expect("keys"),
            };
            let saved = SetSettings.call(&mut tools.tool_context(), args).await?;
            Ok::<_, ToolExecutionError>(json!(saved)["saved"].clone())
        };
        let fonts = json!({"auto": true, "categories": {
            "shout": {"family": "Badaboom BB", "bold": true},
            "whisper": {"italic": true},
        }});
        let saved = save(fonts).await.expect("category fonts");
        assert_eq!(saved["auto"], json!(true));
        let shout = json!({"family": "Badaboom BB", "bold": true, "italic": false});
        let whisper = json!({"family": null, "bold": false, "italic": true});
        assert_eq!(
            saved["categories"],
            json!({"shout": shout, "whisper": whisper})
        );
        // A category changes only in the keys named, and the others keep
        // their fonts.
        let lighter = json!({"categories": {"shout": {"bold": false, "italic": true}}});
        let saved = save(lighter).await.expect("a lighter shout");
        assert_eq!(
            saved["categories"],
            json!({
                "shout": {"family": "Badaboom BB", "bold": false, "italic": true},
                "whisper": whisper,
            })
        );
        // Null takes a category's own font away; an unknown category, or a
        // font that sets nothing, saves nothing.
        let saved = save(json!({"categories": {"whisper": null}}))
            .await
            .expect("whisper's font taken away");
        assert_eq!(saved["categories"].get("whisper"), None, "{saved}");
        for refused in [
            json!({"categories": {"tone": {"bold": true}}}),
            json!({"categories": {"moan": {"family": null}}}),
        ] {
            assert!(save(refused.clone()).await.is_err(), "{refused}");
        }
        let held = settings.0.lock().lettering.clone();
        assert!(held.auto);
        assert_eq!(
            held.categories.keys().collect::<Vec<_>>(),
            [&booharu_core::lettering::LetteringCategory::Shout]
        );
    }

    /// An outside app's call, as it reaches Booharu's MCP server: its own
    /// requests, in the order an MCP client sends them.
    #[tokio::test]
    async fn an_outside_app_cant_change_how_outside_apps_reach_booharu() {
        let settings = Arc::new(Settings::default());
        let (_dir, tools) = open(&settings);
        let server = McpServer::start(0).await.expect("a server");
        server.open(Some(tools));
        let client = reqwest::Client::new();
        let url = format!("http://{}/mcp", server.address());
        let post = |session: Option<&str>, message: Value| {
            let request = client
                .post(&url)
                .header("Accept", "application/json, text/event-stream")
                .json(&message);
            match session {
                Some(session) => request.header("Mcp-Session-Id", session),
                None => request,
            }
            .send()
        };
        let initialize = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1"},
        }});
        let started = post(None, initialize).await.expect("a session");
        let session = started.headers()["mcp-session-id"]
            .to_str()
            .expect("its id")
            .to_owned();
        let initialized = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        post(Some(&session), initialized)
            .await
            .expect("the session starts");
        let call = |id: u32, name: &str, arguments: Value| {
            let call = json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {
                "name": name,
                "arguments": arguments,
            }});
            let answer = post(Some(&session), call);
            async move {
                // The answer comes as a server-sent event.
                let events = answer
                    .await
                    .expect("an answer")
                    .text()
                    .await
                    .expect("events");
                let result = events.lines().find_map(|line| {
                    let message =
                        serde_json::from_str::<Value>(line.strip_prefix("data:")?).ok()?;
                    (message["id"] == json!(id)).then(|| message["result"].clone())
                });
                result.unwrap_or_else(|| panic!("no answer to call {id} in {events}"))
            }
        };

        let change = json!({"section": "booharu-mcp", "values": {"port": 50000}});
        let refused = call(2, "booharu_set_settings", change).await;
        assert_eq!(refused["isError"], json!(true), "{refused}");
        let reason = refused["content"][0]["text"].as_str().unwrap_or_default();
        assert!(reason.contains("an outside app can't"), "{refused}");
        assert_eq!(port(&settings), 47821);
        let read = call(3, "booharu_get_settings", json!({})).await;
        assert_eq!(read["isError"], json!(false), "{read}");
    }
}
