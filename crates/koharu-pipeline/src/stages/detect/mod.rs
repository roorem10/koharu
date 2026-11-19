//! Detection: the page's panels, balloons, and blocks in reading order, in
//! place of the ones there, whoever made them. A block gets its source's
//! frame, tilt, and direction, and its mask.

mod layers;
mod model;
mod tree;

use koharu_ml::koharu_layout_highres::{Options, Typography};
use koharu_scene::{Content, Layer, Op};
use rayon::prelude::*;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

pub use self::model::Detector;
use super::{Task, key};
use crate::{PipelineConfig, Stage};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(specta::Type)]
pub struct DetectorConfig {
    pub model: Detector,
    /// Each class's lowest confidence a detection keeps, from 0 to 1, or the
    /// model's own.
    #[serde(deserialize_with = "threshold")]
    #[specta(type = Option<f32>)]
    pub text: Option<f32>,
    #[serde(deserialize_with = "threshold")]
    #[specta(type = Option<f32>)]
    pub onomatopoeia: Option<f32>,
    #[serde(deserialize_with = "threshold")]
    #[specta(type = Option<f32>)]
    pub bubble: Option<f32>,
    #[serde(deserialize_with = "threshold")]
    #[specta(type = Option<f32>)]
    pub panel: Option<f32>,
}

fn threshold<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<f32>, D::Error> {
    let threshold = Option::<f32>::deserialize(deserializer)?;
    match threshold {
        Some(value) if !(0.0..=1.0).contains(&value) => Err(D::Error::custom(format!(
            "a detection threshold of {value} isn't between 0 and 1"
        ))),
        _ => Ok(threshold),
    }
}

pub(crate) async fn detect(task: &Task, config: &PipelineConfig) -> anyhow::Result<()> {
    let member = &task.members[0];
    let left = crate::booharu::Left::new(task, config); // Booharu: what Detect again keeps.
    let read = member.clone();
    let scan = tokio::task::spawn_blocking(move || {
        let _span =
            tracing::info_span!("read", stage = %Stage::Detect, page = read.index + 1).entered();
        read.scan()
    })
    .await??;
    let Some((_, scan)) = scan else {
        return Ok(());
    };
    let key = key(Stage::Detect, config).expect("a detector is Koharu's");
    let config = config.detector;
    let options = Options {
        text: config.text,
        onomatopoeia: config.onomatopoeia,
        bubble: config.bubble,
        panel: config.panel,
    };
    let (scan, detected) = task
        .inference(&[member], move |models| {
            let detector = models.get(&key, |device| model::load(config.model, device))?;
            let detected = detector.inference(&scan, &options)?;
            Ok((scan, detected))
        })
        .await?;
    let page = member.page;
    let name = config.model.to_string();
    let additions = tokio::task::spawn_blocking(move || {
        let pixels = scan.to_rgb8();
        let layout = {
            let _span = tracing::info_span!("layout").entered();
            detected.layout(&pixels)
        };
        let _span = tracing::info_span!("typography").entered();
        let typography: Vec<Option<Typography>> = layout
            .lettering
            .par_iter()
            .map(|lettering| lettering.typography(&pixels))
            .collect();
        layers::additions(&layout, &typography, page, &name)
    })
    .await??;
    let detecting = left.writing(member); // Booharu: what Detect again keeps, for the write.
    member
        .apply(Stage::Detect, move |project| {
            let Ok(page) = project.page(page) else {
                return Ok(Vec::new());
            };
            let mut ops = Vec::new();
            removals(page, &page.layers, &mut ops); // Booharu: on the page, for what stays (D1).
            ops.extend(detecting.added(page, additions)); // Booharu: none over what stays (D2).
            Ok(ops)
        })
        .await?;
    left.told(); // Booharu: what Detect again kept, once the edit is in.
    Ok(())
}

/// Removes the page's panels, balloons, and blocks, each with what's inside
/// it, but a locked one, which the user keeps.
// Booharu: crate-wide, for the tests of Booharu's hooks, and given `page`,
// for what Detect again keeps there.
pub(crate) fn removals(page: &koharu_scene::Page, layers: &[Layer], ops: &mut Vec<Op>) {
    for layer in layers.iter().filter(|layer| !layer.locked) {
        match layer.content {
            // Booharu: one Detect again keeps, or one holding a layer kept or
            // locked, stays, and what's inside it is gone through (D1).
            _ if crate::booharu::stays(page, layer) => removals(page, &layer.children, ops),
            Content::Panel(_) | Content::Balloon(_) | Content::Block(_) => {
                ops.push(Op::RemoveLayer {
                    layer: layer.id.into(),
                })
            }
            _ => removals(page, &layer.children, ops), // Booharu: on the page.
        }
    }
}
