//! Redo boxes (S9-k): reading, translating, or cleaning chosen boxes again
//! on purpose, which the desktop's commands and the agent's tools share.
//!
//! Koharu's stages leave out a box fixed by hand (`protect::left_out`): a
//! source corrected by hand when reading, and a translation written or
//! fixed by hand, or a clean-up hidden by hand on a box with none, when
//! translating. Redo boxes counts the boxes named first ([`count_redo`]),
//! so the interface and the agent can ask before taking the hand-fixed ones
//! too, then runs the stages again on the boxes it may change
//! ([`redo_targets`]): one ordinary run ([`Redo::request`]) whose
//! `booharu.redo` flag lifts the hand-fixed rule alone, answered with the
//! boxes it left alone ([`Redo::answer`]).
//! A lock is never lifted, and neither is the SFX mode keeping a sound
//! effect as art.

use std::collections::BTreeSet;

use anyhow::{Result, ensure};
use booharu_core::{
    protect::{self, Reason},
    runs::RunStage,
    settings::LetteringSettings,
};
use koharu_pipeline::{PipelineConfig, Request, RunId, Target, pipeline_stage};
use koharu_scene::{Content, LayerId, Project};
use serde::{Deserialize, Serialize};

/// The stages Redo boxes runs again.
pub const STAGES: [RunStage; 3] = [RunStage::Recognize, RunStage::Translate, RunStage::Inpaint];

/// How many of the boxes named Redo boxes would run its stages on as they
/// are, how many it would take only on purpose as fixed by hand, and how
/// many it leaves alone as locked.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct RedoCounts {
    /// Boxes the stages work on as they are.
    pub ready: u32,
    /// Boxes fixed by hand for one of the stages, which they leave out
    /// unless the redo takes them on purpose.
    pub hand_fixed: u32,
    /// Boxes that are, or are in, a locked layer, which no stage changes.
    pub locked: u32,
}

/// What Redo boxes did: the run it queued, none when no box was left to
/// run on, with how many boxes it left alone as fixed by hand or locked.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct RedoAnswer {
    pub run: Option<RunId>,
    pub hand_fixed: u32,
    pub locked: u32,
}

/// The boxes a redo runs on, in the order named, with the counts.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Redo {
    pub blocks: Vec<LayerId>,
    pub counts: RedoCounts,
}

impl Redo {
    /// The run Redo boxes queues on these boxes for `stages`, with
    /// `config`, the saved pipeline settings: Koharu's stages by the same
    /// names, its `booharu.redo` set when the redo takes the hand-fixed
    /// boxes too (`include_hand_fixed`), so it reads and translates those
    /// again; none when it takes no box.
    #[must_use]
    pub fn request(
        &self,
        stages: &[RunStage],
        include_hand_fixed: bool,
        mut config: PipelineConfig,
    ) -> Option<Request> {
        if self.blocks.is_empty() {
            return None;
        }
        config.booharu.redo = include_hand_fixed;
        Some(Request {
            target: Target::Blocks(self.blocks.clone()),
            stages: stages
                .iter()
                .filter_map(|&stage| pipeline_stage(stage))
                .collect(),
            config: Some(config),
        })
    }

    /// What Redo boxes answers once it queued `run` on these boxes, none
    /// when there were none: the boxes it left alone, the locked ones, and
    /// the hand-fixed ones unless it took them (`include_hand_fixed`).
    #[must_use]
    pub fn answer(&self, run: Option<RunId>, include_hand_fixed: bool) -> RedoAnswer {
        RedoAnswer {
            run,
            hand_fixed: if include_hand_fixed {
                0
            } else {
                self.counts.hand_fixed
            },
            locked: self.counts.locked,
        }
    }
}

/// How the boxes `blocks` of `project` stand for running `stages` again,
/// under `settings`: a box that is, or is in, a locked layer is locked;
/// otherwise one any of the stages leaves out as fixed by hand
/// (`protect::left_out` giving `Reason::HandFixed`) is hand-fixed; any
/// other is ready. A box named twice counts once.
///
/// It refuses no stages, a stage other than Recognize, Translate, and
/// Inpaint ([`STAGES`]), and a layer that isn't a block.
pub fn count_redo(
    project: &Project,
    blocks: &[LayerId],
    stages: &[RunStage],
    settings: &LetteringSettings,
) -> Result<RedoCounts> {
    Ok(redo_targets(project, blocks, stages, settings, false)?.counts)
}

/// The boxes of `blocks` a redo of `stages` runs on, under `settings`, with
/// [`count_redo`]'s counts: the unlocked ones, without the hand-fixed ones
/// unless `include_hand_fixed`. Refuses what [`count_redo`] refuses.
pub fn redo_targets(
    project: &Project,
    blocks: &[LayerId],
    stages: &[RunStage],
    settings: &LetteringSettings,
    include_hand_fixed: bool,
) -> Result<Redo> {
    ensure!(!stages.is_empty(), "Redo boxes needs a stage to run again");
    for stage in stages {
        ensure!(
            STAGES.contains(stage),
            "Redo boxes runs Recognize, Translate and Inpaint again, not {stage:?}"
        );
    }
    let mut redo = Redo::default();
    let mut seen = BTreeSet::new();
    for &id in blocks {
        if !seen.insert(id) {
            continue;
        }
        let layer = project.layer(id)?;
        ensure!(
            matches!(layer.content, Content::Block(_)),
            "layer {id} isn't a block"
        );
        let (page, _) = project.parent(id)?;
        if protect::locked(page, id) {
            redo.counts.locked += 1;
            continue;
        }
        let hand_fixed = stages.iter().any(|&stage| {
            protect::left_out(page, layer, stage, settings, false, false) == Some(Reason::HandFixed)
        });
        if hand_fixed {
            redo.counts.hand_fixed += 1;
            if !include_hand_fixed {
                continue;
            }
        } else {
            redo.counts.ready += 1;
        }
        redo.blocks.push(id);
    }
    Ok(redo)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use koharu_pipeline::Stage;
    use koharu_scene::{Created, Edit, Op, Session};
    use serde_json::json;
    use tempfile::TempDir;

    use super::*;

    /// A page holding the blocks `ready`; `read`, whose source was
    /// corrected by hand; `typed`, whose translation was typed by hand;
    /// `hidden`, untranslated with its clean-up hidden by hand; the sound
    /// effect `effect`; the locked block `locked`; and `inside`, in the
    /// locked group `g`.
    struct Fixture {
        _dir: TempDir,
        session: Session,
        layers: BTreeMap<String, LayerId>,
    }

    impl Fixture {
        fn new() -> Result<Self> {
            let dir = tempfile::tempdir()?;
            let mut session = Session::create(dir.path().join("test.khr"))?;
            let block = |x: f64, role: &str, read: bool| {
                let mut c2pa = json!({"detection": {"action": "c2pa.created"}});
                if read {
                    c2pa["ocr"] = json!({"action": "c2pa.created"});
                }
                json!({"kind": "block", "role": role, "source": {
                    "content": "どうして",
                    "frame": {"box": {"bounds": {"x": x, "y": 20.0, "width": 40.0, "height": 40.0}}},
                }, "attributes": {"c2pa": c2pa}})
            };
            let add = |name: &str, index: usize, layer: serde_json::Value| {
                json!({"op": "add_layer", "id": format!("${name}"), "page": "$p", "index": index,
                    "layer": layer})
            };
            let ops = [
                json!({"op": "add_page", "id": "$p", "index": 0, "size": {"width": 400, "height": 400}}),
                add("ready", 0, block(10.0, "text", true)),
                add("read", 1, block(60.0, "text", false)),
                add("typed", 2, block(110.0, "text", true)),
                add("hidden", 3, block(160.0, "text", true)),
                add("effect", 4, block(210.0, "sfx", true)),
                add("locked", 5, block(260.0, "text", true)),
                add("g", 6, json!({"kind": "group"})),
                json!({"op": "add_layer", "id": "$inside", "page": "$p", "parent": "$g", "index": 0,
                    "layer": block(310.0, "text", true)}),
            ];
            let change = session.apply(Edit {
                label: "test".to_owned(),
                ops: ops
                    .into_iter()
                    .map(serde_json::from_value)
                    .collect::<Result<_, _>>()?,
            })?;
            let layers = change
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
            let mut fixture = Self {
                _dir: dir,
                session,
                layers,
            };
            let id = |name: &str| fixture.id(name);
            let ops = vec![
                Op::SetText {
                    layer: id("typed").into(),
                    text: "Why?".to_owned(),
                },
                Op::UpdateLayer {
                    layer: id("hidden").into(),
                    patch: json!({"cleanup": {"visible": false}}),
                },
                Op::UpdateLayer {
                    layer: id("locked").into(),
                    patch: json!({"locked": true}),
                },
                Op::UpdateLayer {
                    layer: id("g").into(),
                    patch: json!({"locked": true}),
                },
            ];
            fixture.session.apply(Edit {
                label: "test".to_owned(),
                ops,
            })?;
            Ok(fixture)
        }

        fn id(&self, name: &str) -> LayerId {
            self.layers[name]
        }

        fn ids(&self, names: &[&str]) -> Vec<LayerId> {
            names.iter().map(|name| self.id(name)).collect()
        }

        fn count(&self, stages: &[RunStage]) -> Result<RedoCounts> {
            let all = [
                "ready", "read", "typed", "hidden", "effect", "locked", "inside", "ready",
            ];
            count_redo(
                &self.session.project(),
                &self.ids(&all),
                stages,
                &LetteringSettings::default(),
            )
        }
    }

    fn counts(ready: u32, hand_fixed: u32, locked: u32) -> RedoCounts {
        RedoCounts {
            ready,
            hand_fixed,
            locked,
        }
    }

    #[test]
    fn each_stage_counts_its_own_hand_fixed_boxes_and_every_stage_the_locked_ones() -> Result<()> {
        let fixture = Fixture::new()?;
        // Reading again: only the corrected source is fixed by hand.
        assert_eq!(fixture.count(&[RunStage::Recognize])?, counts(4, 1, 2));
        // Translating again: the typed translation and the clean-up hidden
        // by hand; the sound effect is left out as art, not as hand-fixed.
        assert_eq!(fixture.count(&[RunStage::Translate])?, counts(3, 2, 2));
        // Cleaning again never asks.
        assert_eq!(fixture.count(&[RunStage::Inpaint])?, counts(5, 0, 2));
        assert_eq!(
            fixture.count(&[RunStage::Recognize, RunStage::Translate, RunStage::Inpaint])?,
            counts(2, 3, 2)
        );
        Ok(())
    }

    #[test]
    fn a_box_styled_before_its_translated_is_ready_to_translate_again() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let ready = fixture.id("ready");
        // A style picked before translating leaves a placeholder, no
        // translation fixed by hand.
        fixture.session.apply(Edit {
            label: "test".to_owned(),
            ops: vec![crate::testing::type_bar_style(ready)],
        })?;
        let redo = redo_targets(
            &fixture.session.project(),
            &[ready],
            &[RunStage::Translate],
            &LetteringSettings::default(),
            false,
        )?;
        assert_eq!(redo.counts, counts(1, 0, 0));
        assert_eq!(redo.blocks, [ready]);
        Ok(())
    }

    #[test]
    fn a_translation_typed_by_hand_and_cleared_is_taken_only_on_purpose() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let ready = fixture.id("ready");
        // A first translation typed in the Story panel, then cleared: a hand
        // edit left blank on purpose, which Redo boxes asks before taking,
        // as a Translate run leaves it alone.
        let english: koharu_scene::LanguageIdentifier = "en".parse()?;
        let typed = crate::translation::edit(
            &fixture.session.project(),
            ready,
            "Why?".to_owned(),
            &english,
            &booharu_core::settings::Snapshot::default(),
        )?;
        fixture.session.apply(typed)?;
        fixture.session.apply(Edit {
            label: "test".to_owned(),
            ops: vec![Op::SetText {
                layer: ready.into(),
                text: String::new(),
            }],
        })?;
        let (stages, settings) = ([RunStage::Translate], LetteringSettings::default());
        let project = fixture.session.project();
        let without = redo_targets(&project, &[ready], &stages, &settings, false)?;
        assert_eq!(without.counts, counts(0, 1, 0));
        assert!(without.blocks.is_empty());
        let with = redo_targets(&project, &[ready], &stages, &settings, true)?;
        assert_eq!(with.blocks, [ready]);
        Ok(())
    }

    #[test]
    fn it_refuses_other_stages_none_and_a_layer_that_isnt_a_block() -> Result<()> {
        let fixture = Fixture::new()?;
        for stages in [&[RunStage::Detect][..], &[RunStage::Read], &[]] {
            assert!(fixture.count(stages).is_err(), "{stages:?}");
        }
        let group = fixture.ids(&["g"]);
        let refused = count_redo(
            &fixture.session.project(),
            &group,
            &[RunStage::Translate],
            &LetteringSettings::default(),
        );
        assert!(refused.is_err());
        Ok(())
    }

    #[test]
    fn redo_takes_the_hand_fixed_boxes_only_when_asked_and_never_the_locked() -> Result<()> {
        let fixture = Fixture::new()?;
        let project = fixture.session.project();
        let settings = LetteringSettings::default();
        let named = fixture.ids(&["ready", "typed", "hidden", "effect", "locked", "inside"]);
        let stages = [RunStage::Translate];

        let without = redo_targets(&project, &named, &stages, &settings, false)?;
        assert_eq!(without.blocks, fixture.ids(&["ready", "effect"]));
        assert_eq!(without.counts, counts(2, 2, 2));
        let with = redo_targets(&project, &named, &stages, &settings, true)?;
        assert_eq!(
            with.blocks,
            fixture.ids(&["ready", "typed", "hidden", "effect"])
        );
        assert_eq!(with.counts, without.counts);
        // The answer tells the boxes left alone: the hand-fixed ones only
        // when the redo didn't take them.
        let run = Some(RunId(9));
        let answer = |hand_fixed| RedoAnswer {
            run,
            hand_fixed,
            locked: 2,
        };
        assert_eq!(without.answer(run, false), answer(2));
        assert_eq!(with.answer(run, true), answer(0));

        // The run's redo flag lifts the hand-fixed rule alone: the sound
        // effect stays art, and a locked box stays out.
        let page = project.pages().next().expect("a page");
        let reason = |name: &str, redo: bool| -> Result<Option<Reason>> {
            let layer = project.layer(fixture.id(name))?;
            Ok(protect::left_out(
                page,
                layer,
                RunStage::Translate,
                &settings,
                redo,
                false,
            ))
        };
        for name in ["typed", "hidden"] {
            assert_eq!(reason(name, false)?, Some(Reason::HandFixed), "{name}");
            assert_eq!(reason(name, true)?, None, "{name}");
        }
        assert_eq!(reason("effect", true)?, Some(Reason::SoundEffect));
        assert_eq!(reason("locked", true)?, Some(Reason::Locked));
        Ok(())
    }

    #[test]
    fn a_redo_is_one_run_lifting_the_hand_fixed_rule_only_when_asked() -> Result<()> {
        let block: LayerId = "00000000-0000-0000-0000-000000000001".parse()?;
        let stages = [RunStage::Translate, RunStage::Recognize];
        let none = Redo::default();
        assert!(
            none.request(&stages, true, PipelineConfig::default())
                .is_none()
        );
        let redo = Redo {
            blocks: vec![block],
            counts: RedoCounts::default(),
        };
        for include_hand_fixed in [true, false] {
            let request = redo
                .request(&stages, include_hand_fixed, PipelineConfig::default())
                .expect("a run");
            assert_eq!(request.target, Target::Blocks(vec![block]));
            assert_eq!(
                request.stages,
                BTreeSet::from([Stage::Recognize, Stage::Translate])
            );
            let config = request.config.expect("the run's own settings");
            assert_eq!(config.booharu.redo, include_hand_fixed);
            assert!(!config.booharu.subtitle_run);
        }
        Ok(())
    }
}
