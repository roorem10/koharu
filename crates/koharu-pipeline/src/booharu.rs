//! Booharu's pipeline hooks: what a stage calls, by one marked line, to apply
//! Booharu's settings and attributes from booharu-core within the stage's
//! own page edit. A run reads Booharu's settings once, as it's queued, into
//! its config's snapshot, and never again mid-stage, so every page of a run
//! uses the same values, whether the Run button, the agent, or MCP queued it.
//!
//! - `queued`, in `Pipeline::run`, takes the snapshot, the run policy of
//!   `[booharu-runs]` included, or refuses the run when a section it reads
//!   can't be read.
//! - `translated`, in the Translation stage's `written`, gives each new
//!   translation the default look and records what the stage wrote, and
//!   `given_back` records a translation it cleared, given back unchanged.
//! - `recognized`, in the Recognition stage, records the wording it read.

use booharu_core::{
    runs::{RunPolicy, RunStage},
    settings::Snapshot,
    styles,
};
use koharu_renderer::RendererConfig;
use koharu_scene::{LayerId, Op, Project};

use crate::{PipelineConfig, Stage};

/// `config`, the one a run was queued with, with Booharu's settings as
/// they're saved now: `[booharu-lettering]`, the first of Koharu's Preferred
/// fonts, and `[booharu-runs]`'s run policy, which the run then follows
/// whoever queued it. A section that can't be read refuses the run, as a bad
/// stored value is an error, never replaced by its default.
pub(crate) fn queued(config: PipelineConfig) -> crate::Result<PipelineConfig> {
    let families = koharu_config::get::<RendererConfig>().map(|renderer| renderer.font_families);
    let booharu = snapshot(Snapshot::read(families), RunPolicy::read())?;
    Ok(PipelineConfig { booharu, ..config })
}

/// A run's snapshot: `read`, with `policy` in it, or the error of the first
/// that couldn't be read, which refuses the run as an invalid request.
fn snapshot(
    read: anyhow::Result<Snapshot>,
    policy: anyhow::Result<RunPolicy>,
) -> crate::Result<Snapshot> {
    let invalid = |error: anyhow::Error| crate::Error::invalid(format!("{error:#}"));
    let read = read.map_err(invalid)?;
    Ok(Snapshot {
        runs: Some(policy.map_err(invalid)?),
        ..read
    })
}

// A preset's stage is a stage of Koharu's, by the same name.
impl From<RunStage> for Stage {
    fn from(stage: RunStage) -> Self {
        match stage {
            RunStage::Detect => Self::Detect,
            RunStage::Recognize => Self::Recognize,
            RunStage::Inpaint => Self::Inpaint,
            RunStage::Translate => Self::Translate,
        }
    }
}

impl From<Stage> for RunStage {
    fn from(stage: Stage) -> Self {
        match stage {
            Stage::Detect => Self::Detect,
            Stage::Recognize => Self::Recognize,
            Stage::Inpaint => Self::Inpaint,
            Stage::Translate => Self::Translate,
        }
    }
}

/// The ops that follow placement's, `placed`, in the Translation stage's
/// edit for `block`'s new translation `text`, both read before the edit, as
/// placement reads the project: the run's default look, when it changes
/// anything, then the hand-edit record, on every translation the stage
/// writes. None when placement failed, which fails the stage.
pub(crate) fn translated(
    project: &Project,
    block: LayerId,
    text: &str,
    placed: Option<&Op>,
    booharu: &Snapshot,
) -> anyhow::Result<Vec<Op>> {
    let Some(placed) = placed else {
        return Ok(Vec::new());
    };
    styles::translated(
        project.layer(block)?,
        text,
        placed,
        &booharu.lettering,
        booharu.font.as_deref(),
    )
}

/// The op that follows the Translation stage's clearing of `block`'s
/// translation, given back unchanged, in its edit: the hand-edit record of
/// no translation, read before the edit.
pub(crate) fn given_back(project: &Project, block: LayerId) -> anyhow::Result<Op> {
    styles::given_back(project.layer(block)?)
}

/// The op recording `text`, the wording the Recognition stage read for
/// `block`, in its hand-edit record, which keeps its other parts.
pub(crate) fn recognized(project: &Project, block: LayerId, text: &str) -> anyhow::Result<Op> {
    styles::recognized(project.layer(block)?, text)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use anyhow::{Context as _, Result};
    use booharu_core::{
        Edited,
        attrs::Auto,
        edited,
        runs::RunsSettings,
        settings::{DefaultTextStyle, LetteringSettings},
        styles::text_hash,
    };
    use icu_locale_core::langid;
    use koharu_scene::{CharStyle, Content, Created, Edit, Layer, Rgba, Session, Text};
    use serde_json::json;

    use super::*;
    use crate::{TranslatorConfig, stages::translate::written};

    const BLUE: Rgba = Rgba([0.0, 0.0, 1.0, 1.0]);
    const WHITE: Rgba = Rgba([1.0; 4]);
    const BLACK: Rgba = Rgba([0.0, 0.0, 0.0, 1.0]);
    const RED: Rgba = Rgba([1.0, 0.0, 0.0, 1.0]);
    const YELLOW: Rgba = Rgba([1.0, 1.0, 0.0, 1.0]);

    /// The source wording of every block.
    const SOURCE: &str = "どうして";

    /// A project of one 400-pixel page holding a block for each `(label,
    /// role)`, found and read by Koharu's stages: lettered in blue outlined
    /// 3 pixels wide in white at size 32, reading [`SOURCE`], and not yet
    /// translated.
    struct Page {
        _dir: tempfile::TempDir,
        session: Session,
        blocks: BTreeMap<String, LayerId>,
    }

    impl Page {
        fn new(blocks: &[(&str, &str)]) -> Result<Self> {
            let dir = tempfile::tempdir()?;
            let mut session = Session::create(dir.path().join("test.khr"))?;
            let mut ops = vec![
                json!({"op": "add_page", "id": "$p", "index": 0, "size": {"width": 400, "height": 400}}),
            ];
            for (index, (label, role)) in blocks.iter().enumerate() {
                let x = 10.0 + 50.0 * index as f64;
                let block = json!({"kind": "block", "role": role, "source": {
                    "frame": {"box": {"bounds": {"x": x, "y": 20.0, "width": 40.0, "height": 120.0}}},
                    "content": SOURCE,
                    "style": {"chars": {"size": 32.0, "fill": BLUE, "stroke": WHITE, "stroke_width": 3.0}},
                }, "attributes": {"c2pa": {
                    "detection": {"action": "c2pa.created"},
                    "ocr": {"action": "c2pa.created"},
                }}});
                ops.push(json!({"op": "add_layer", "id": format!("${label}"), "page": "$p", "index": index, "layer": block}));
            }
            let change = session.apply(Edit {
                label: "test".to_owned(),
                ops: ops
                    .into_iter()
                    .map(serde_json::from_value)
                    .collect::<Result<_, _>>()?,
            })?;
            let blocks = change
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
            Ok(Self {
                _dir: dir,
                session,
                blocks,
            })
        }

        fn id(&self, label: &str) -> LayerId {
            self.blocks[label]
        }

        fn layer(&self, label: &str) -> Result<Layer> {
            Ok(self.session.project().layer(self.id(label))?.clone())
        }

        fn translation(&self, label: &str) -> Result<Option<Text>> {
            match self.layer(label)?.content {
                Content::Block(block) => Ok(block.translation),
                _ => anyhow::bail!("{label} isn't a block"),
            }
        }

        fn chars(&self, label: &str) -> Result<CharStyle> {
            let translation = self.translation(label)?.context("a translation")?;
            Ok(translation.style.chars)
        }

        fn edit(&mut self, ops: Vec<Op>) -> Result<()> {
            self.session.apply(Edit {
                label: "test".to_owned(),
                ops,
            })?;
            Ok(())
        }

        /// Translates each `(label, text)` as the Translation stage writes
        /// a page's translations, under `booharu`, a run's snapshot: the ops
        /// of `written`, applied as its one edit.
        fn translate(&mut self, texts: &[(&str, &str)], booharu: &Snapshot) -> Result<()> {
            let translated = texts
                .iter()
                .map(|(label, text)| (self.id(label), langid!("ja"), (*text).to_owned()))
                .collect();
            let ops = written(
                &self.session.project(),
                translated,
                &TranslatorConfig::default(),
                booharu,
            )?;
            self.edit(ops)
        }

        /// Reads each `(label, text)` as the Recognition stage writes the
        /// wording it read, with its hook.
        fn recognize(&mut self, texts: &[(&str, &str)]) -> Result<()> {
            let project = self.session.project();
            let mut ops = Vec::new();
            for (label, text) in texts {
                let block = self.id(label);
                ops.push(Op::UpdateLayer {
                    layer: block.into(),
                    patch: json!({"source": {"content": text, "language": "ja"}}),
                });
                ops.push(Op::SetAttribute {
                    layer: block.into(),
                    namespace: "c2pa".to_owned(),
                    key: "ocr".to_owned(),
                    value: Some(json!({"action": "c2pa.created"})),
                });
                ops.push(recognized(&project, block, text)?);
            }
            self.edit(ops)
        }
    }

    fn snapshot(
        detect_colors: bool,
        fill: Option<Rgba>,
        stroke: Option<Rgba>,
        font: Option<&str>,
    ) -> Snapshot {
        Snapshot {
            lettering: LetteringSettings {
                default: DefaultTextStyle {
                    fill,
                    stroke,
                    stroke_width: 2.5,
                },
                detect_colors,
            },
            font: font.map(str::to_owned),
            runs: None,
        }
    }

    #[test]
    fn a_queued_run_takes_the_run_policy_or_is_refused() -> Result<()> {
        let policy = RunsSettings {
            retries: 4,
            ..RunsSettings::default()
        }
        .policy();
        let read = snapshot(true, None, None, Some("CCWildWords"));
        let queued = super::snapshot(Ok(read.clone()), Ok(policy))?;
        assert_eq!(
            queued,
            Snapshot {
                runs: Some(policy),
                ..read.clone()
            }
        );
        // A bad [booharu-runs] refuses the run, naming the section.
        let bad = anyhow::anyhow!("9 retries aren't between 0 and 5")
            .context("Booharu's run settings can't be read from [booharu-runs]");
        let refused = super::snapshot(Ok(read), Err(bad)).expect_err("a bad policy");
        assert!(
            matches!(&refused, crate::Error::Invalid(message)
                if message.contains("[booharu-runs]") && message.contains("9 retries")),
            "{refused}"
        );
        // A config a caller made, as upstream's tests and examples make one,
        // runs Koharu's way.
        assert_eq!(PipelineConfig::default().booharu.runs, None);
        Ok(())
    }

    #[test]
    fn a_presets_stages_are_koharus_by_the_same_names_in_the_same_order() {
        use strum::VariantArray as _;
        for &stage in Stage::VARIANTS {
            let preset = RunStage::from(stage);
            assert_eq!(Stage::from(preset), stage);
            assert_eq!(
                serde_json::to_value(preset).ok(),
                serde_json::to_value(stage).ok()
            );
        }
        let presets: Vec<RunStage> = Stage::VARIANTS.iter().map(|&stage| stage.into()).collect();
        assert!(presets.is_sorted());
    }

    fn colours(chars: &CharStyle) -> (Option<Rgba>, Option<Rgba>, Option<f64>) {
        (chars.fill, chars.stroke, chars.stroke_width)
    }

    #[test]
    fn detection_off_writes_the_default_colours_and_on_writes_none() -> Result<()> {
        let mut page = Page::new(&[("a", "text"), ("b", "text")])?;
        // On: placement's measured colours stay, whatever the default look.
        page.translate(
            &[("a", "Why?")],
            &snapshot(true, Some(RED), Some(BLACK), None),
        )?;
        assert_eq!(
            colours(&page.chars("a")?),
            (Some(BLUE), Some(WHITE), Some(3.0))
        );
        // Off: the default colours replace them, null where it sets none.
        page.translate(&[("b", "Why?")], &snapshot(false, Some(RED), None, None))?;
        let b = page.chars("b")?;
        assert_eq!(colours(&b), (Some(RED), None, None));
        assert_eq!(b.size, Some(32.0), "placement's size stays");
        // Translating again keeps an existing translation's colours.
        page.translate(
            &[("a", "Why!?")],
            &snapshot(false, Some(RED), Some(BLACK), None),
        )?;
        assert_eq!(
            colours(&page.chars("a")?),
            (Some(BLUE), Some(WHITE), Some(3.0))
        );
        Ok(())
    }

    #[test]
    fn a_sound_effect_keeps_a_contrasting_outline_without_a_default_one() -> Result<()> {
        let mut page = Page::new(&[("talk", "text"), ("sfx", "sfx"), ("boom", "sfx")])?;
        // Around the light default fill, a black outline a sixteenth of the
        // page's lettering size; the dialogue gets none.
        let light = snapshot(false, Some(YELLOW), None, None);
        page.translate(&[("talk", "Why?"), ("sfx", "Bam")], &light)?;
        assert_eq!(
            colours(&page.chars("sfx")?),
            (Some(YELLOW), Some(BLACK), Some(2.0))
        );
        assert_eq!(colours(&page.chars("talk")?), (Some(YELLOW), None, None));
        // A default outline is the effect's outline too.
        let outlined = snapshot(false, Some(YELLOW), Some(RED), None);
        page.translate(&[("boom", "Boom")], &outlined)?;
        assert_eq!(
            colours(&page.chars("boom")?),
            (Some(YELLOW), Some(RED), Some(2.5))
        );
        Ok(())
    }

    #[test]
    fn the_default_font_goes_only_into_a_translation_without_a_family() -> Result<()> {
        let mut page = Page::new(&[("a", "text"), ("b", "text"), ("c", "text")])?;
        // b's translation was typed with its own family.
        let b = page.id("b");
        page.edit(vec![
            Op::SetText {
                layer: b.into(),
                text: "Hm".to_owned(),
            },
            Op::UpdateLayer {
                layer: b.into(),
                patch: json!({"translation": {"style": {"chars": {"family": "Comic Neue"}}}}),
            },
        ])?;
        let font = snapshot(true, None, None, Some("CCWildWords"));
        page.translate(&[("a", "Why?"), ("b", "Why?")], &font)?;
        assert_eq!(page.chars("a")?.family.as_deref(), Some("CCWildWords"));
        assert_eq!(page.chars("b")?.family.as_deref(), Some("Comic Neue"));
        // Its family stays the user's choice after the run.
        assert!(edited(&page.layer("b")?).style);
        assert!(!edited(&page.layer("a")?).style);
        // With no Preferred font, none is written: Koharu's chain applies.
        page.translate(&[("c", "Why?")], &snapshot(true, None, None, None))?;
        assert_eq!(page.chars("c")?.family, None);
        Ok(())
    }

    #[test]
    fn every_translated_block_gets_a_record_that_reads_as_automatic() -> Result<()> {
        let mut page = Page::new(&[("a", "text"), ("b", "sfx"), ("c", "text")])?;
        // Styling nothing, as Koharu's own run would: still recorded. c is
        // given back unchanged, which clears its translation, so its record
        // holds no translation.
        let koharu = Snapshot::default();
        page.translate(&[("a", "Why?"), ("b", "Bam"), ("c", SOURCE)], &koharu)?;
        for (label, text) in [("a", "Why?"), ("b", "Bam"), ("c", "")] {
            let layer = page.layer(label)?;
            assert_eq!(Auto::of(&layer).text, Some(text_hash(text)), "{label}");
            assert_eq!(edited(&layer), Edited::default(), "{label}");
        }
        assert_eq!(page.translation("c")?, None);
        // So the same wording typed there afterwards is the user's.
        let c = page.id("c");
        page.edit(vec![Op::SetText {
            layer: c.into(),
            text: SOURCE.to_owned(),
        }])?;
        assert!(edited(&page.layer("c")?).text);

        // Styled: the record holds what the look left.
        let mut page = Page::new(&[("a", "text")])?;
        let styled = snapshot(false, Some(RED), None, Some("CCWildWords"));
        page.translate(&[("a", "Why?")], &styled)?;
        let layer = page.layer("a")?;
        let style = Auto::of(&layer).style.context("a style")?;
        assert_eq!(
            (style.family.as_deref(), style.fill, style.stroke),
            (Some("CCWildWords"), Some(RED), None)
        );
        assert!(!edited(&layer).hand_edited());

        // Retyped by hand, it's the user's.
        let a = page.id("a");
        page.edit(vec![Op::SetText {
            layer: a.into(),
            text: "Why...?".to_owned(),
        }])?;
        let retyped = edited(&page.layer("a")?);
        assert_eq!((retyped.text, retyped.style), (true, false));
        Ok(())
    }

    #[test]
    fn a_corrected_source_reads_as_hand_edited_after_recognition_recorded_it() -> Result<()> {
        let mut page = Page::new(&[("a", "text")])?;
        page.recognize(&[("a", SOURCE)])?;
        assert!(!edited(&page.layer("a")?).source);

        let a = page.id("a");
        let corrected = |text: &str| Op::UpdateLayer {
            layer: a.into(),
            patch: json!({"source": {"content": text}}),
        };
        page.edit(vec![corrected("どうしてよ")])?;
        assert!(edited(&page.layer("a")?).source);

        // Read again, after a translation: the new wording is recorded, and
        // the record keeps what the Translation stage wrote.
        page.translate(&[("a", "Why?")], &Snapshot::default())?;
        page.recognize(&[("a", "どうしてよ")])?;
        let layer = page.layer("a")?;
        assert_eq!(edited(&layer), Edited::default());
        assert_eq!(Auto::of(&layer).text, Some(text_hash("Why?")));
        Ok(())
    }
}
