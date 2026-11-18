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
         50, which new text gets, and `detect_colors`, whether a new translation keeps the \
         colours Koharu measured (true) or gets the default look's (false); `booharu-fonts` has \
         the font picker's `favorites`, family names. New text's font is the first of Koharu's \
         Preferred fonts, not a Booharu setting. Needs a project open, as every tool here does."
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
         tables included, or an invalid value, saves nothing. \
         Answers the section as saved, with a note when the change applies only once Booharu \
         restarts. A change to `booharu-lettering` applies to text made from then on, never to \
         translations already made: booharu_apply_text_style gives those the new look. A call \
         an outside app makes over MCP can't change `booharu-mcp`. Needs a project open, as \
         every tool here does."
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
                },
                "booharu-fonts": {"favorites": []},
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
                "saved": {"default": look, "detect_colors": false},
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
