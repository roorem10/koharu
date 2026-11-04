use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize, Type)]
#[serde(default)]
pub(crate) struct RawPrepOptions {
    pub levels: bool,
    pub clip_percent: f64,
    pub whiten: bool,
    pub threshold: u8,
}

impl Default for RawPrepOptions {
    fn default() -> Self {
        Self {
            levels: true,
            clip_percent: booharu_rawprep::DEFAULT_CLIP_PERCENT,
            whiten: false,
            threshold: 235,
        }
    }
}

impl RawPrepOptions {
    pub(super) fn validate(self) -> Result<()> {
        ensure!(
            self.clip_percent.is_finite() && (0.0..=49.9).contains(&self.clip_percent),
            "clip percentage must be between 0 and 49.9"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Type)]
#[serde(default)]
pub(crate) struct RawPrepSettings {
    pub options: RawPrepOptions,
    pub fix_on_import: bool,
}

pub(super) fn load() -> Result<RawPrepSettings> {
    let settings = koharu_config::load::<RawPrepSettings>("raw_prep")?
        .read()?
        .clone();
    settings.options.validate()?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_nonfinite_and_out_of_range_clipping() {
        for clip_percent in [-0.1, 50.0, f64::NAN, f64::INFINITY] {
            assert!(
                RawPrepOptions {
                    clip_percent,
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        for clip_percent in [0.0, 0.5, 49.9] {
            RawPrepOptions {
                clip_percent,
                ..Default::default()
            }
            .validate()
            .unwrap();
        }
    }
}
