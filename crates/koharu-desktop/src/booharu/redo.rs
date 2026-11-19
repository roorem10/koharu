//! Redo boxes' commands: how the boxes chosen stand for reading,
//! translating, or cleaning them again, so the interface can ask before it
//! takes the ones fixed by hand too, and the one run that does it, each
//! worked out by `booharu::redo`, which the agent's tool shares, under the
//! saved `[booharu-lettering]` as it is when called.

use std::{collections::BTreeSet, sync::Arc};

use booharu::redo::{self, RedoAnswer, RedoCounts};
use booharu_core::{runs::RunStage, settings::LetteringSettings};
use koharu_pipeline::{PipelineConfig, Stage, Target, pipeline_stage};
use koharu_scene::LayerId;
use tauri::State;

use crate::{app::App, commands::Result};

/// How the boxes `blocks` stand for running `stages` again: how many the
/// stages would work on as they are, how many they'd take only on purpose,
/// as fixed by hand, and how many are, or are in, a locked layer, which no
/// redo changes. A box named twice counts once. Refuses no stages, a stage
/// other than `recognize`, `translate`, and `inpaint`, and a layer that
/// isn't a box.
#[tauri::command]
#[specta::specta]
pub async fn booharu_count_redo(
    app: State<'_, Arc<App>>,
    blocks: Vec<LayerId>,
    stages: Vec<RunStage>,
) -> Result<RedoCounts> {
    let project = app.session()?.lock().project();
    Ok(tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let settings = koharu_config::get::<LetteringSettings>()?;
        redo::count_redo(&project, &blocks, &stages, &settings)
    })
    .await??)
}

/// Runs `stages` again on the boxes `blocks`, as `booharu_count_redo`
/// counts them: one ordinary run on the unlocked ones, without those fixed
/// by hand unless `include_hand_fixed`, with the saved pipeline settings,
/// which with `include_hand_fixed` reads and translates the hand-fixed ones
/// again too. A sound effect the SFX mode keeps as art stays art. Answers
/// the run, none when no box is left to run on, with how many boxes it left
/// alone as fixed by hand or locked. Refuses what `booharu_count_redo`
/// refuses.
#[tauri::command]
#[specta::specta]
pub async fn booharu_redo_boxes(
    app: State<'_, Arc<App>>,
    blocks: Vec<LayerId>,
    stages: Vec<RunStage>,
    include_hand_fixed: bool,
) -> Result<RedoAnswer> {
    let project = app.session()?.lock().project();
    let config = koharu_config::get::<PipelineConfig>()?;
    let asked = stages.clone();
    let redo = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let settings = koharu_config::get::<LetteringSettings>()?;
        redo::redo_targets(&project, &blocks, &asked, &settings, include_hand_fixed)
    })
    .await??;
    let queued = match run(redo.blocks.clone(), &stages, include_hand_fixed, config) {
        Some((target, stages, config)) => Some(app.process(target, stages, Some(config))?),
        None => None,
    };
    Ok(redo.answer(queued, include_hand_fixed))
}

/// The run Redo boxes queues on `blocks`, the boxes it takes, for `stages`:
/// Koharu's stages by the same names on those boxes, with `config`, the
/// saved pipeline settings, its `booharu.redo` set when the redo takes the
/// hand-fixed boxes too; none when it takes no box.
fn run(
    blocks: Vec<LayerId>,
    stages: &[RunStage],
    include_hand_fixed: bool,
    mut config: PipelineConfig,
) -> Option<(Target, BTreeSet<Stage>, PipelineConfig)> {
    if blocks.is_empty() {
        return None;
    }
    config.booharu.redo = include_hand_fixed;
    let stages = stages.iter().filter_map(|&stage| pipeline_stage(stage));
    Some((Target::Blocks(blocks), stages.collect(), config))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_redo_runs_once_on_the_boxes_it_takes_lifting_the_hand_fixed_rule_when_asked() {
        let blocks: Vec<LayerId> = ["00000000-0000-0000-0000-000000000001"]
            .iter()
            .map(|id| id.parse().expect("an id"))
            .collect();
        let stages = [RunStage::Translate, RunStage::Recognize];
        assert!(run(Vec::new(), &stages, true, PipelineConfig::default()).is_none());
        for include_hand_fixed in [true, false] {
            let (target, queued, config) = run(
                blocks.clone(),
                &stages,
                include_hand_fixed,
                PipelineConfig::default(),
            )
            .expect("a run");
            assert_eq!(target, Target::Blocks(blocks.clone()));
            assert_eq!(queued, BTreeSet::from([Stage::Recognize, Stage::Translate]));
            assert_eq!(config.booharu.redo, include_hand_fixed);
            assert!(!config.booharu.subtitle_run);
        }
    }
}
