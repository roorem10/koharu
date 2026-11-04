pub const DEFAULT_CLIP_PERCENT: f64 = 0.5;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LevelsStats {
    pub black: u8,
    pub white: u8,
}

pub(crate) fn luminance([red, green, blue]: [u8; 3]) -> u8 {
    // Integer Rec. 709 weights sum to 256, so neutral pixels retain their value.
    ((54 * u32::from(red) + 183 * u32::from(green) + 19 * u32::from(blue) + 128) >> 8) as u8
}

pub(crate) fn histogram_stats(histogram: &[u64; 256], clip_percent: f64) -> LevelsStats {
    let total: u64 = histogram.iter().sum();
    if total == 0 {
        return LevelsStats {
            black: 0,
            white: 255,
        };
    }
    let clip = if clip_percent.is_finite() {
        clip_percent.clamp(0.0, 49.9)
    } else {
        DEFAULT_CLIP_PERCENT
    };
    let discard = ((total as f64 * clip / 100.0).floor() as u64).min((total - 1) / 2);
    let mut cumulative = 0;
    let black = histogram
        .iter()
        .position(|&count| {
            cumulative += count;
            cumulative > discard
        })
        .unwrap_or(0) as u8;
    cumulative = 0;
    let white = histogram
        .iter()
        .rposition(|&count| {
            cumulative += count;
            cumulative > discard
        })
        .unwrap_or(255) as u8;
    LevelsStats { black, white }
}

pub(crate) fn curve(LevelsStats { black, white }: LevelsStats) -> [u8; 256] {
    std::array::from_fn(|value| {
        if white <= black {
            return value as u8;
        }
        let span = u32::from(white - black);
        let above_black = (value as u32).saturating_sub(u32::from(black));
        ((above_black * 255 + span / 2) / span).min(255) as u8
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipping_discards_the_same_count_at_each_end() {
        let mut histogram = [0; 256];
        histogram[0] = 1;
        histogram[40] = 99;
        histogram[200] = 899;
        histogram[255] = 1;
        assert_eq!(
            histogram_stats(&histogram, 0.5),
            LevelsStats {
                black: 40,
                white: 200
            }
        );
        assert_eq!(
            histogram_stats(&histogram, 0.0),
            LevelsStats {
                black: 0,
                white: 255
            }
        );
    }

    #[test]
    fn empty_and_flat_histograms_have_an_identity_curve() {
        assert_eq!(
            curve(histogram_stats(&[0; 256], 0.5)),
            std::array::from_fn(|i| i as u8)
        );
        let mut histogram = [0; 256];
        histogram[190] = 16;
        let stats = histogram_stats(&histogram, 0.5);
        assert_eq!(
            stats,
            LevelsStats {
                black: 190,
                white: 190
            }
        );
        assert_eq!(curve(stats), std::array::from_fn(|i| i as u8));
    }

    #[test]
    fn rounding_and_clamping_are_monotone_and_preserve_colour() {
        let curve = curve(LevelsStats {
            black: 20,
            white: 220,
        });
        assert!(curve.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(
            [curve[0], curve[20], curve[220], curve[255]],
            [0, 0, 255, 255]
        );
        assert_eq!([curve[60], curve[100], curve[140]], [51, 102, 153]);
    }

    #[test]
    fn neutral_luminance_is_exact_at_every_value() {
        for value in 0..=255 {
            assert_eq!(luminance([value; 3]), value);
        }
        assert_eq!(luminance([255, 0, 0]), 54);
    }

    #[test]
    fn tiny_images_and_invalid_clip_values_are_well_defined() {
        let mut histogram = [0; 256];
        histogram[40] = 1;
        histogram[200] = 1;
        for clip in [-1.0, 0.5, 49.9, 50.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                histogram_stats(&histogram, clip),
                LevelsStats {
                    black: 40,
                    white: 200
                }
            );
        }
    }
}
