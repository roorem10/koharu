//! `[booharu-runs]`: how runs over pages go. Its presets are named sets of
//! the Run menu's stages, which set its switches and start nothing; the rest
//! is the run policy, which `Pipeline::run` reads into each run's snapshot,
//! so the Run button, the agent's run tool and MCP's runs all follow it: how
//! often a failed page stage is tried again, whether the run then goes on
//! with its other pages, and whether its pages go stage by stage or one stage
//! at a time; and whether a run over pages chimes as it ends.

use std::{collections::BTreeSet, time::Duration};

use anyhow::Context as _;
use koharu_config::Section;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

/// A stage of a run, in the order a page's stages run: koharu-pipeline's
/// `Stage`, which this crate can't name, with the same names in the same
/// order, so a preset holds what the Run menu's switches send.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "snake_case")]
pub enum RunStage {
    Detect,
    Recognize,
    Inpaint,
    Translate,
}

/// A named set of stages, which picking in the Run menu switches on, and
/// the others off, starting nothing. A key it doesn't have fails, as no
/// caller's check of the keys it names reaches inside a list.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub struct RunPreset {
    /// Its name, unique among the presets once trimmed, and never blank.
    pub name: String,
    /// Its stages, at least one, in stage order.
    pub stages: BTreeSet<RunStage>,
}

/// What a run does once a page stage has failed every try it had.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum OnPageFailure {
    /// The page fails alone and the run goes on with its other pages,
    /// ending as done with problems, with the failed pages listed.
    #[default]
    Continue,
    /// The failure ends the run, as Koharu's first failure does.
    Stop,
}

/// The order a run's page stages go in.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum RunOrder {
    /// Koharu's own: up to three pages in flight, each going through its
    /// stages while the next starts.
    #[default]
    Pages,
    /// One stage at a time: every page is detected, then every page read,
    /// and so on, each stage's model alone in memory, for a graphics card
    /// with little of it.
    Stages,
}

/// The most presets the Run menu keeps.
pub const MAX_PRESETS: usize = 100;
/// The most times a failed page stage is tried again.
pub const MAX_RETRIES: u8 = 5;

/// `[booharu-runs]`: the Run menu's presets and the run policy.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct RunsSettings {
    /// At most 100, `MAX_PRESETS`, each with a name, unique once trimmed
    /// and never blank, and at least one stage.
    #[serde(deserialize_with = "presets")]
    #[specta(type = Vec<RunPreset>)]
    pub presets: Vec<RunPreset>,
    /// How often a failed page stage is tried again, from 0 to 5,
    /// `MAX_RETRIES`.
    #[serde(deserialize_with = "retries")]
    #[specta(type = u8)]
    pub retries: u8,
    /// What a run does once a page stage has failed every try.
    pub on_page_failure: OnPageFailure,
    pub order: RunOrder,
    /// Whether a run over pages chimes as it ends, whoever started it.
    pub finish_sound: bool,
}

impl Default for RunsSettings {
    fn default() -> Self {
        let preset = |name: &str, stages: &[RunStage]| RunPreset {
            name: name.to_owned(),
            stages: stages.iter().copied().collect(),
        };
        Self {
            presets: vec![
                preset("Detect + OCR", &[RunStage::Detect, RunStage::Recognize]),
                preset(
                    "Translate + clean up",
                    &[RunStage::Inpaint, RunStage::Translate],
                ),
            ],
            retries: 2,
            on_page_failure: OnPageFailure::Continue,
            order: RunOrder::Pages,
            finish_sound: true,
        }
    }
}

impl Section for RunsSettings {
    const NAME: &'static str = "booharu-runs";
}

impl RunsSettings {
    /// The run policy these settings set.
    pub fn policy(&self) -> RunPolicy {
        RunPolicy {
            retries: self.retries,
            on_page_failure: self.on_page_failure,
            order: self.order,
        }
    }
}

/// Presets the Run menu can keep: a bad one fails them all, as a bad stored
/// value is an error, never dropped.
fn presets<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<RunPreset>, D::Error> {
    let presets = Vec::<RunPreset>::deserialize(deserializer)?;
    if presets.len() > MAX_PRESETS {
        return Err(D::Error::custom(format!(
            "{} run presets are more than the {MAX_PRESETS} Booharu keeps",
            presets.len()
        )));
    }
    let mut names = BTreeSet::new();
    for preset in &presets {
        let name = preset.name.trim();
        if name.is_empty() {
            return Err(D::Error::custom("a run preset's name can't be blank"));
        }
        if !names.insert(name) {
            return Err(D::Error::custom(format!(
                "two run presets are named \"{name}\""
            )));
        }
        if preset.stages.is_empty() {
            return Err(D::Error::custom(format!(
                "the run preset \"{name}\" has no stage"
            )));
        }
    }
    Ok(presets)
}

fn retries<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u8, D::Error> {
    let retries = u8::deserialize(deserializer)?;
    if retries > MAX_RETRIES {
        return Err(D::Error::custom(format!(
            "{retries} retries aren't between 0 and {MAX_RETRIES}"
        )));
    }
    Ok(retries)
}

/// How a run goes when one of its page stages fails, and in what order its
/// page stages go, as `[booharu-runs]` sets it when the run is queued. A run
/// without one, as a config only a caller made holds, goes Koharu's way: the
/// first failure ends it, and its pages overlap.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RunPolicy {
    /// How often a failed page stage is tried again, after [`retry_wait`].
    pub retries: u8,
    /// What the run does once a page stage has failed every try.
    pub on_page_failure: OnPageFailure,
    pub order: RunOrder,
}

impl RunPolicy {
    /// The policy `[booharu-runs]` sets as the file holds it now. A bad
    /// stored value fails, naming the section, never taken for its default.
    pub fn read() -> anyhow::Result<Self> {
        Self::of(koharu_config::get())
    }

    /// The policy of `runs` as it was read, or why it couldn't be.
    fn of(runs: koharu_config::Result<RunsSettings>) -> anyhow::Result<Self> {
        let runs = runs.with_context(|| {
            format!(
                "Booharu's run settings can't be read from [{}]",
                RunsSettings::NAME
            )
        })?;
        Ok(runs.policy())
    }
}

/// How long a failed page stage waits before its try `attempt`, counting its
/// first try as 1, as `Event::Retrying` numbers the try it waits for: 2
/// seconds before the second try, the first retry, 5 before the third, and 10
/// before any later one, as Booharu 0.0.4 waited; the first waits for
/// nothing.
pub fn retry_wait(attempt: u32) -> Duration {
    Duration::from_secs(match attempt {
        0 | 1 => 0,
        2 => 2,
        3 => 5,
        _ => 10,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_table_reads_as_the_defaults() {
        let runs: RunsSettings = toml::from_str("").expect("the defaults");
        assert_eq!(runs, RunsSettings::default());
        let names: Vec<&str> = runs.presets.iter().map(|preset| &*preset.name).collect();
        assert_eq!(names, ["Detect + OCR", "Translate + clean up"]);
        assert_eq!(
            runs.presets[1].stages.iter().copied().collect::<Vec<_>>(),
            [RunStage::Inpaint, RunStage::Translate]
        );
        assert_eq!(
            runs.policy(),
            RunPolicy {
                retries: 2,
                on_page_failure: OnPageFailure::Continue,
                order: RunOrder::Pages,
            }
        );
        assert!(runs.finish_sound);
    }

    #[test]
    fn the_settings_read_back_as_they_were_written() {
        let runs = RunsSettings {
            presets: vec![RunPreset {
                name: "Clean only".to_owned(),
                stages: BTreeSet::from([RunStage::Inpaint]),
            }],
            retries: 5,
            on_page_failure: OnPageFailure::Stop,
            order: RunOrder::Stages,
            finish_sound: false,
        };
        let text = toml::to_string(&runs).expect("a table");
        assert_eq!(toml::from_str::<RunsSettings>(&text).ok(), Some(runs));
        // Stages are written in stage order, whatever order they came in.
        let read: RunsSettings = toml::from_str(
            "[[presets]]\nname = \"Last two\"\nstages = [\"translate\", \"inpaint\"]\n",
        )
        .expect("a preset");
        assert_eq!(
            serde_json::to_value(&read.presets).expect("JSON"),
            serde_json::json!([{"name": "Last two", "stages": ["inpaint", "translate"]}])
        );
        assert_eq!(read.retries, 2, "a key left out keeps its default");
    }

    #[test]
    fn a_bad_preset_retry_count_or_choice_fails_closed() {
        let preset = |name: &str, stages: &str| {
            format!("[[presets]]\nname = \"{name}\"\nstages = [{stages}]\n")
        };
        let too_many = (0..=MAX_PRESETS)
            .map(|at| preset(&format!("Preset {at}"), "\"detect\""))
            .collect::<String>();
        let refused = [
            preset("", "\"detect\""),
            preset("  ", "\"detect\""),
            preset("Read", ""),
            preset("Read", "\"read\""),
            preset("Read", "\"detect\"") + &preset(" Read ", "\"translate\""),
            "[[presets]]\nstages = [\"detect\"]\n".to_owned(),
            preset("Read", "\"recognize\"") + "stage = \"recognize\"\n",
            too_many,
            "retries = 6".to_owned(),
            "retries = -1".to_owned(),
            "on_page_failure = \"skip\"".to_owned(),
            "order = \"overlapped\"".to_owned(),
            "finish_sound = 1".to_owned(),
        ];
        for text in refused {
            assert!(
                toml::from_str::<RunsSettings>(&text).is_err(),
                "took {text}"
            );
        }
        // A key after a `[[presets]]` header would be the last preset's.
        let most = "retries = 5\n".to_owned()
            + &(0..MAX_PRESETS)
                .map(|at| preset(&format!("Preset {at}"), "\"detect\""))
                .collect::<String>();
        let read: RunsSettings = toml::from_str(&most).expect("the most presets and retries");
        assert_eq!((read.presets.len(), read.retries), (MAX_PRESETS, 5));
        let none: RunsSettings = toml::from_str("presets = []\nretries = 0").expect("none");
        assert!(none.presets.is_empty());
        assert_eq!(none.policy().retries, 0);
    }

    #[test]
    fn a_policy_fails_on_a_section_it_cant_read() {
        let stop = RunsSettings {
            on_page_failure: OnPageFailure::Stop,
            ..RunsSettings::default()
        };
        assert_eq!(
            RunPolicy::of(Ok(stop))
                .ok()
                .map(|policy| policy.on_page_failure),
            Some(OnPageFailure::Stop)
        );
        let bad = Err(koharu_config::Error::Invalid {
            path: "config.toml".into(),
            message: "9 retries aren't between 0 and 5".to_owned(),
        });
        let error = RunPolicy::of(bad).expect_err("a bad section");
        let error = format!("{error:#}");
        assert!(
            error.contains("[booharu-runs]") && error.contains("9 retries"),
            "{error}"
        );
    }

    #[test]
    fn a_retry_waits_2_then_5_then_10_seconds() {
        let waits: Vec<u64> = (1..=6)
            .map(|attempt| retry_wait(attempt).as_secs())
            .collect();
        assert_eq!(waits, [0, 2, 5, 10, 10, 10]);
    }
}
