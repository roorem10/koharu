//! A sound effect's subtitle, as the agent adds one: worked out by
//! `booharu::sfx`, as the app's Add subtitle command works it out, under the
//! saved `[booharu-lettering]`, the first of Koharu's Preferred fonts, and
//! the saved pipeline settings as they are when called. An effect with no
//! words to subtitle it with is translated first, by a Translate run in
//! subtitle mode started as Fix it everywhere's is: through the app's runs,
//! else on the tools' own pipeline.

use anyhow::Context as _;
use booharu::sfx::{self, SfxSubtitleAnswer, Subtitle};
use koharu_scene::LayerId;
use rig_core::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use super::{context::blocking, styles::look};
use crate::tools::schema;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct AddSfxSubtitleArgs {
    /// The sound effect's block.
    block: LayerId,
    /// The subtitle's words, 1 to 500 characters once trimmed; left out,
    /// the effect's own translation, or one made for it first from its
    /// words read when it has none.
    #[serde(default)]
    text: Option<String>,
}

pub(super) struct AddSfxSubtitle;

impl Tool for AddSfxSubtitle {
    const NAME: &'static str = "booharu_add_sfx_subtitle";
    type Args = AddSfxSubtitleArgs;
    type Output = SfxSubtitleAnswer;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Give a sound effect a subtitle, as one undo step: a small line of text saying what it \
         means, set beside the effect (to its left when its text runs down the page, else below \
         it, or on the other side where that would leave the page), right after it in reading \
         order, in the subtitle look of `subtitle` in `booharu-lettering`, while the effect \
         keeps its original art: its own translation goes and its clean-up is hidden, so the \
         art shows. Use it for a sound effect worth explaining that should stay drawn, as the \
         SFX mode (`sfx` in `booharu-lettering`) keeps sound effects by default. `block` is the \
         effect: a block whose lettering category is `sfx`, as booharu_get_text_styles tells \
         it, neither locked nor in a locked layer, whose source has a box to put the subtitle \
         beside. The words are `text` when given, 1 to 500 characters once trimmed, else the \
         effect's own translation; with neither, it starts a Translate run on the effect that \
         writes the translation it gets as the subtitle instead of as the effect's \
         translation, or nothing when that only gives the source back: follow it with wait. \
         An effect given a translation while that run went, typed or by another run, keeps it \
         and gets no subtitle, the run telling `skipped` with it as `hand_fixed`: call again \
         to make the subtitle from that translation. \
         An effect with no words read yet, a blank source, has nothing to translate and is \
         refused without `text`: give `text`, or read it again first with booharu_redo_boxes \
         (`recognize`). Answers `layer`, the subtitle's text layer, or `run`, that Translate \
         run, the other null. An effect with a subtitle keeps it, and the call answers that \
         one and changes nothing: to change its words, set its text with edit. Later \
         translation runs, booharu_redo_boxes included, leave a subtitled effect untranslated \
         whatever the SFX mode."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        schema::<AddSfxSubtitleArgs>()
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: AddSfxSubtitleArgs,
    ) -> Result<SfxSubtitleAnswer, Self::Error> {
        let AddSfxSubtitleArgs { block, text } = args;
        blocking(context, move |tools, host| {
            let config = host.pipeline_config()?;
            let (settings, font) = look(tools)?;
            // Under one lock, so the effect is subtitled as it was read; let
            // go before a run is queued, which reads the project too.
            let layer = {
                let mut session = tools.session.lock();
                let subtitle = sfx::add_sfx_subtitle(
                    &session.project(),
                    block,
                    text,
                    &settings,
                    font.as_deref(),
                    &config.translator.target,
                )?;
                match subtitle {
                    Subtitle::Existing(layer) => Some(layer),
                    Subtitle::Edit(edit) => Some(
                        sfx::created(&session.apply(edit)?)
                            .context("the subtitle added has no layer")?,
                    ),
                    Subtitle::NeedsTranslation => None,
                }
            };
            if let Some(layer) = layer {
                return Ok(SfxSubtitleAnswer {
                    layer: Some(layer),
                    run: None,
                });
            }
            let request = sfx::subtitle_run(block, config);
            let run = host
                .start_run(&tools.pipeline, &tools.session, request)?
                .context("the project closed")?;
            Ok(SfxSubtitleAnswer {
                layer: None,
                run: Some(run),
            })
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, sync::Arc};

    use booharu_core::{
        attrs::{self, Auto, SfxSubtitle},
        settings::BooharuPreferences,
    };
    use koharu_pipeline::{Pipeline, PipelineConfig, Request, RunId, Stage, Target};
    use koharu_renderer::{Renderer, RendererConfig};
    use koharu_scene::{Content, Created, Edit, Layer, Op, Session, Text};
    use parking_lot::Mutex;
    use serde_json::{Map, json};

    use super::*;
    use crate::tools::{Tools, booharu::BooharuHost};

    /// The default font, the first of Koharu's Preferred fonts.
    const FONT: &str = "Anime Ace";

    /// The app as the subtitle tool reaches it: Booharu's settings at their
    /// defaults, [`FONT`] the default font, translating into French, and the
    /// runs it was asked to start, which it never starts, as a translation
    /// would load a model.
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
            Ok(vec![FONT.to_owned()])
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

    /// A page 400 by 400 holding the sound effects `boom`, translated as
    /// "BOOM" in English, `bang`, untranslated, and `frozen`, locked, and
    /// the text block `talk`, each found and read by Koharu's stages; the
    /// tools on it in [`App`], the app, and the blocks' ids.
    fn open() -> (tempfile::TempDir, Tools, Arc<App>, impl Fn(&str) -> LayerId) {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        let block = |name: &str, index: usize, role: &str, locked: bool| {
            let x = 20.0 + 90.0 * index as f64;
            let mut layer = json!({"kind": "block", "role": role, "locked": locked, "source": {
                "content": "ドン",
                "frame": {"box": {"bounds": {"x": x, "y": 100.0, "width": 60.0, "height": 40.0}}},
                "style": {"chars": {"size": 32.0}},
            }, "attributes": {"c2pa": {
                "detection": {"action": "c2pa.created"},
                "ocr": {"action": "c2pa.created"},
            }}});
            if name == "boom" {
                layer["translation"] = json!({"content": "BOOM", "language": "en"});
            }
            json!({"op": "add_layer", "id": format!("${name}"), "page": "$p", "index": index,
                "layer": layer})
        };
        let ops = [
            json!({"op": "add_page", "id": "$p", "index": 0, "size": {"width": 400, "height": 400}}),
            block("boom", 0, "sfx", false),
            block("bang", 1, "sfx", false),
            block("frozen", 2, "sfx", true),
            block("talk", 3, "text", false),
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
        let id = move |name: &str| {
            ids.iter()
                .find_map(|(found, id)| (found == name).then_some(*id))
                .expect("a block")
        };
        let app = Arc::new(App::default());
        let tools = Tools::new(
            Arc::new(Mutex::new(session)),
            Pipeline::new(PipelineConfig::default()),
            Renderer::new(RendererConfig::default()),
        )
        .with_booharu(Arc::clone(&app) as Arc<dyn BooharuHost>);
        (dir, tools, app, id)
    }

    async fn add(tools: &Tools, args: Value) -> Result<SfxSubtitleAnswer, ToolExecutionError> {
        let schema = AddSfxSubtitle.parameters();
        assert_eq!(crate::tools::refusal(&schema, &args), None, "{args}");
        let args = serde_json::from_value(args).expect("the arguments");
        AddSfxSubtitle.call(&mut tools.tool_context(), args).await
    }

    fn layer(tools: &Tools, id: LayerId) -> Layer {
        tools
            .session
            .lock()
            .project()
            .layer(id)
            .expect("the layer")
            .clone()
    }

    fn steps(tools: &Tools) -> Vec<String> {
        tools.session.lock().history().steps
    }

    #[tokio::test]
    async fn a_subtitle_is_made_from_the_effects_translation_as_one_undo_step() {
        let (_dir, tools, app, id) = open();
        let boom = id("boom");
        let before = steps(&tools);
        let answer = add(&tools, json!({"block": boom}))
            .await
            .expect("a subtitle");
        let subtitle = answer.layer.expect("the subtitle's layer");
        assert_eq!(answer.run, None);
        let mut after = before.clone();
        after.push(sfx::LABEL.to_owned());
        assert_eq!(steps(&tools), after);

        // The translation's words, in its language, in the default font.
        let made = layer(&tools, subtitle);
        assert_eq!(
            attrs::get::<SfxSubtitle>(&made),
            Some(SfxSubtitle { block: boom })
        );
        let Content::Text(text) = &made.content else {
            panic!("the subtitle isn't a text: {made:?}");
        };
        let Text {
            content,
            language,
            style,
            ..
        } = &**text;
        assert_eq!(content.text(), "BOOM");
        assert_eq!(language.to_string(), "en");
        assert_eq!(style.chars.family.as_deref(), Some(FONT));
        // The effect keeps its art: no translation, its clean-up hidden,
        // with the kept mark.
        let effect = layer(&tools, boom);
        let Content::Block(block) = &effect.content else {
            panic!("boom isn't a block");
        };
        assert!(block.translation.is_none());
        assert!(!block.cleanup.visible);
        assert!(Auto::of(&effect).kept);

        // Asked again, with words or without, the subtitle it has answers.
        let again = add(&tools, json!({"block": boom, "text": "Crash"}))
            .await
            .expect("the subtitle it has");
        assert_eq!(again.layer, Some(subtitle));
        assert_eq!(steps(&tools), after);
        assert!(app.started.lock().is_empty());
    }

    #[tokio::test]
    async fn an_effect_with_no_words_is_translated_first_in_subtitle_mode() {
        let (_dir, tools, app, id) = open();
        let bang = id("bang");
        let before = steps(&tools);
        let answer = add(&tools, json!({"block": bang})).await.expect("a run");
        assert_eq!(answer.layer, None);
        assert_eq!(answer.run, Some(RunId(7)));
        assert_eq!(steps(&tools), before);
        let started = app.started.lock().clone();
        let [request] = started.as_slice() else {
            panic!("{started:?}");
        };
        assert_eq!(request.target, Target::Blocks(vec![bang]));
        assert_eq!(request.stages, BTreeSet::from([Stage::Translate]));
        let config = request.config.as_ref().expect("the run's own settings");
        assert!(config.booharu.subtitle_run);
        assert!(!config.booharu.redo);

        // Words given need no translation: the subtitle is in the target
        // language, as the effect has none of its own.
        let answer = add(&tools, json!({"block": bang, "text": "  Bang!  "}))
            .await
            .expect("a subtitle");
        let made = layer(&tools, answer.layer.expect("the subtitle's layer"));
        let Content::Text(text) = &made.content else {
            panic!("the subtitle isn't a text: {made:?}");
        };
        assert_eq!(text.content.text(), "Bang!");
        assert_eq!(text.language.to_string(), "fr");
        assert_eq!(app.started.lock().len(), 1);
    }

    #[tokio::test]
    async fn only_an_unlocked_sound_effect_takes_a_subtitle_and_a_refusal_changes_nothing() {
        let (_dir, tools, app, id) = open();
        let before = steps(&tools);
        let talk = add(&tools, json!({"block": id("talk"), "text": "Hey"}))
            .await
            .expect_err("a text block refused");
        assert!(talk.to_string().contains("isn't a sound effect"), "{talk}");
        let frozen = add(&tools, json!({"block": id("frozen"), "text": "Ice"}))
            .await
            .expect_err("a locked effect refused");
        assert!(frozen.to_string().contains("is locked"), "{frozen}");
        let blank = add(&tools, json!({"block": id("bang"), "text": "   "}))
            .await
            .expect_err("blank words refused");
        assert!(blank.to_string().contains("1 to 500"), "{blank}");
        assert_eq!(steps(&tools), before);
        assert!(app.started.lock().is_empty());
        // An effect with no words read has nothing a run could translate.
        let bang = id("bang");
        tools
            .session
            .lock()
            .apply(Edit {
                label: "test".to_owned(),
                ops: vec![Op::UpdateLayer {
                    layer: bang.into(),
                    patch: json!({"source": {"content": ""}}),
                }],
            })
            .expect("a blank source");
        let unread = add(&tools, json!({"block": bang}))
            .await
            .expect_err("an effect with no words read refused");
        assert!(unread.to_string().contains("no words read yet"), "{unread}");
        assert!(app.started.lock().is_empty());
        // What the schema refuses before the call.
        let schema = AddSfxSubtitle.parameters();
        for args in [
            json!({"text": "Boom"}),
            json!({"block": id("bang"), "force": true}),
        ] {
            assert!(crate::tools::refusal(&schema, &args).is_some(), "{args}");
        }
    }

    #[test]
    fn adding_a_subtitle_changes_the_project() {
        assert_ne!(AddSfxSubtitle.annotations().read_only_hint, Some(true));
    }
}
