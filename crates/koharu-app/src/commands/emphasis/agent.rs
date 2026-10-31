use anyhow::{Context as _, Result};
use koharu_scene::{EntityId, FontStyle, Origin, RunStyle};
use koharu_translator::EmphasisSettings;
use schemars::JsonSchema;
use serde::Deserialize;

use super::{Project, category_style, runs, shown_text};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct StyleWords {
    pub layer: String,
    pub text: Option<String>,
    pub start: Option<u32>,
    pub end: Option<u32>,
    pub style: Option<Preset>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub size_percent: Option<f32>,
    pub color: Option<[u8; 4]>,
    pub outline_color: Option<[u8; 4]>,
    pub outline_width: Option<f32>,
    #[serde(default)]
    pub clear: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Preset {
    Emphasis,
}

impl StyleWords {
    fn explicit_style(&self) -> Result<Option<RunStyle>> {
        let explicit = RunStyle {
            font_weight: self.bold.map(|bold| if bold { 700 } else { 400 }),
            font_style: self.italic.map(|italic| {
                if italic {
                    FontStyle::Italic
                } else {
                    FontStyle::Normal
                }
            }),
            size_scale: self.size_percent.map(|size| size / 100.0),
            color: self.color,
            stroke_color: self.outline_color,
            stroke_width: self.outline_width,
            ..Default::default()
        };
        let has_explicit = explicit != RunStyle::default();
        anyhow::ensure!(
            usize::from(self.clear) + usize::from(self.style.is_some()) + usize::from(has_explicit)
                == 1,
            "choose clear, style emphasis, or explicit style fields"
        );
        koharu_translator::emphasis::validate_style(&explicit)?;
        Ok(has_explicit.then_some(explicit))
    }

    fn range(&self, text: &str) -> Result<(u32, u32)> {
        let range = match (&self.text, self.start, self.end) {
            (Some(word), None, None) => runs::find_words(text, std::slice::from_ref(word))
                .into_iter()
                .next()
                .context("text was not found as a whole word or phrase")?,
            (None, Some(start), Some(end)) => (start, end),
            _ => anyhow::bail!("choose text or both start and end (Unicode character offsets)"),
        };
        runs::validate_range(text, range.0, range.1)?;
        Ok(range)
    }
}

impl Project {
    pub(crate) async fn style_words(&mut self, args: StyleWords) -> Result<koharu_scene::Commit> {
        let explicit = args.explicit_style()?;
        let layer: EntityId = serde_json::from_value(serde_json::Value::String(args.layer.clone()))
            .context("invalid layer ID")?;
        let snapshot = self.snapshot();
        let (start, end) = args.range(&shown_text(&snapshot, layer)?)?;
        let style = if args.style.is_some() {
            let settings = EmphasisSettings::load()?.read()?.clone();
            Some(category_style(
                &settings,
                &self.load_context()?,
                &snapshot,
                layer,
            )?)
        } else {
            explicit
        };
        let patch = snapshot.patch(|edit| {
            runs::write(
                edit,
                &snapshot,
                layer,
                start,
                end,
                style.as_ref(),
                Origin::User,
            )
        })?;
        self.commit(patch).await
    }
}
