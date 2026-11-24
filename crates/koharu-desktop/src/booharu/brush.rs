//! The brush's feel on the desktop (`BrushFeel`): each brush tool's hardness
//! and Draw's opacity, which `booharu_set_brush` takes from the interface as
//! it starts, as it restored the feel, and on every change, remembers, and
//! pushes to the canvas, and the run a Remove stroke asks for, which keeps a
//! soft stroke's edge as its result's limit ([`remove_config`]). No settings
//! file holds the feel: the interface keeps it between runs, as Koharu keeps
//! its brush, so a push of the settings the file holds keeps the feel the
//! window has ([`with_feel`]).

use std::sync::Arc;

use anyhow::ensure;
use koharu_canvas::booharu::{BooharuSettings, BrushFeel};
use koharu_pipeline::PipelineConfig;
use parking_lot::Mutex;
use tauri::State;

use crate::{app::App, commands::Result};

/// The feel the interface sent last; none before it first sends one, which
/// reads as Koharu's hard, opaque brush.
static FEEL: Mutex<Option<BrushFeel>> = Mutex::new(None);

/// Sets how the brush tools' strokes feel: the hardness of Draw's, the
/// Eraser's and Remove's, each from 0, soft from the stroke's centre line
/// out, to 1, Koharu's hard edge, and Draw's opacity, above 0 and at most 1.
/// Strokes started from then on take it, and a soft Remove stroke's result
/// fades out with its edge. Refuses a value that isn't a number in its
/// range, naming it. Changes nothing in the project.
#[tauri::command]
#[specta::specta]
pub fn booharu_set_brush(app: State<'_, Arc<App>>, feel: BrushFeel) -> Result<()> {
    checked(&feel)?;
    *FEEL.lock() = Some(feel);
    app.screen.set_booharu_brush(feel);
    Ok(())
}

/// Fails on a value of `feel` that isn't a number in its range, naming it:
/// a hardness from 0 to 1, Draw's opacity above 0 and at most 1.
fn checked(feel: &BrushFeel) -> anyhow::Result<()> {
    for (name, hardness) in [
        ("draw_hardness", feel.draw_hardness),
        ("eraser_hardness", feel.eraser_hardness),
        ("remove_hardness", feel.remove_hardness),
    ] {
        ensure!(
            (0.0..=1.0).contains(&hardness),
            "the brush's {name} is {hardness}, not from 0 to 1"
        );
    }
    let opacity = feel.draw_opacity;
    ensure!(
        opacity > 0.0 && opacity <= 1.0,
        "the brush's draw_opacity is {opacity}, not above 0 and at most 1"
    );
    Ok(())
}

/// The config a Remove stroke's run takes in the pipeline's place: none, as
/// Koharu runs it, while the Remove tool's hardness is 1, as it is until the
/// interface first sends a feel; below it, the saved pipeline settings, as
/// Generative Fill reads them, with `soft_limit` set, so the result keeps
/// the stroke's soft edge as its limit. A read that fails is logged and
/// gives none, so the stroke still cleans, with a hard edge.
pub(crate) fn remove_config() -> Option<PipelineConfig> {
    let hardness = FEEL.lock().unwrap_or_default().remove_hardness;
    soft(hardness, || Ok(koharu_config::get::<PipelineConfig>()?))
}

/// [`remove_config`] for a Remove stroke of `hardness`, with `read` reading
/// the saved pipeline settings, which only a soft stroke reads.
fn soft(
    hardness: f64,
    read: impl FnOnce() -> anyhow::Result<PipelineConfig>,
) -> Option<PipelineConfig> {
    if hardness >= 1.0 {
        return None;
    }
    match read() {
        Ok(mut config) => {
            config.booharu.soft_limit = true;
            Some(config)
        }
        Err(error) => {
            tracing::warn!("a soft Remove stroke cleans with a hard edge: {error:#}");
            None
        }
    }
}

/// `pushed`, Booharu's settings for the canvas as a push reads them from
/// the settings file, with the brush's feel `held` has, the window's, which
/// the file doesn't hold: saving any setting never resets the feel.
pub(crate) fn with_feel(pushed: BooharuSettings, held: &BooharuSettings) -> BooharuSettings {
    BooharuSettings {
        brush: held.brush,
        ..pushed
    }
}

#[cfg(test)]
mod tests {
    use koharu_canvas::booharu::TextDefaults;
    use koharu_scene::Fit;

    use super::*;

    /// A soft feel: every edge soft in its own way, and Draw at a third.
    const SOFT: BrushFeel = BrushFeel {
        draw_hardness: 0.0,
        eraser_hardness: 0.5,
        remove_hardness: 0.25,
        draw_opacity: 0.33,
    };

    #[test]
    fn a_value_out_of_its_range_is_refused_by_its_name() {
        checked(&BrushFeel::default()).expect("Koharu's brush");
        checked(&SOFT).expect("a soft one");
        checked(&BrushFeel {
            draw_opacity: 0.001,
            ..SOFT
        })
        .expect("an opacity just above 0");
        let refused = |feel: BrushFeel| checked(&feel).expect_err("refused").to_string();
        for wrong in [-0.1, 1.5, f64::NAN, f64::INFINITY] {
            let message = refused(BrushFeel {
                draw_hardness: wrong,
                ..SOFT
            });
            assert!(message.contains("draw_hardness"), "{message}");
            let message = refused(BrushFeel {
                eraser_hardness: wrong,
                ..SOFT
            });
            assert!(message.contains("eraser_hardness"), "{message}");
            let message = refused(BrushFeel {
                remove_hardness: wrong,
                ..SOFT
            });
            assert!(message.contains("remove_hardness"), "{message}");
        }
        for wrong in [0.0, -0.5, 1.01, f64::NAN, f64::NEG_INFINITY] {
            let message = refused(BrushFeel {
                draw_opacity: wrong,
                ..SOFT
            });
            assert!(message.contains("draw_opacity"), "{message}");
        }
    }

    #[test]
    fn only_a_soft_remove_stroke_runs_with_a_soft_limit() {
        // Until the interface sends a feel, a Remove stroke runs as Koharu's.
        assert!(remove_config().is_none());
        // At hardness 1 the settings aren't even read.
        assert!(soft(1.0, || panic!("read at hardness 1")).is_none());
        // Below it, the saved settings as they are, with the flag set.
        let saved = PipelineConfig::default();
        let config = soft(0.25, || Ok(saved.clone())).expect("a config");
        let mut flagged = saved;
        flagged.booharu.soft_limit = true;
        assert_eq!(config, flagged);
        // Settings that don't read leave the stroke its hard edge.
        assert!(soft(0.0, || Err(anyhow::anyhow!("not TOML"))).is_none());
    }

    #[test]
    fn a_settings_push_keeps_the_brushs_feel() {
        let held = BooharuSettings {
            brush: SOFT,
            ..BooharuSettings::default()
        };
        // A push after a save, of the look the file holds now and Koharu's
        // feel, as no file holds one.
        let pushed = BooharuSettings {
            text: TextDefaults {
                family: Some("Anime Ace".to_owned()),
                fit: Fit::Best,
                ..TextDefaults::default()
            },
            ..BooharuSettings::default()
        };
        assert_eq!(
            with_feel(pushed.clone(), &held),
            BooharuSettings {
                brush: SOFT,
                ..pushed
            }
        );
    }
}
