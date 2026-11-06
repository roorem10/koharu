#[derive(Clone, Copy)]
pub(super) struct SizeSample {
    pub font_size: f32,
    pub page_height: f32,
}

pub(super) fn fitting_improvement(before: f32, target: f32, after: f32, overflow: bool) -> bool {
    !overflow
        && after.is_finite()
        && after > 0.0
        // A newly configured category ceiling can lie below an older chapter's
        // median. Do not turn a large outlier into a small one (or vice versa).
        && (before <= target || after >= target * 0.8)
        && (before >= target || after <= target * 1.2)
        && (after - target).abs() + 0.01 < (before - target).abs()
}

/// Normalize before taking the median so scan resolution does not skew it.
/// The 20% band avoids chasing ordinary small differences caused by wrapping;
/// outside it, the renderer tries the median and shrinks only as fit requires.
pub(super) fn consistency_targets(samples: &[SizeSample]) -> (Option<f32>, Vec<Option<f32>>) {
    let normalized = samples
        .iter()
        .map(|sample| {
            let ratio = sample.font_size / sample.page_height;
            (sample.font_size.is_finite()
                && sample.font_size > 0.0
                && sample.page_height.is_finite()
                && sample.page_height > 0.0
                && ratio.is_finite()
                && ratio > 0.0)
                .then_some(ratio)
        })
        .collect::<Vec<_>>();
    let mut sizes = normalized.iter().flatten().copied().collect::<Vec<_>>();
    sizes.sort_by(f32::total_cmp);
    let median = if sizes.is_empty() {
        None
    } else {
        let middle = sizes.len() / 2;
        Some(if sizes.len() % 2 == 0 {
            sizes[middle - 1] / 2.0 + sizes[middle] / 2.0
        } else {
            sizes[middle]
        })
    };
    let targets = normalized
        .into_iter()
        .map(|size| {
            let median = median?;
            let size = size?;
            ((size - median).abs() > median * 0.20 + f32::EPSILON).then_some(median)
        })
        .collect();
    (median, targets)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(font_size: f32, page_height: f32) -> SizeSample {
        SizeSample {
            font_size,
            page_height,
        }
    }

    #[test]
    fn median_is_resolution_independent_and_targets_both_outlier_directions() {
        let (median, targets) = consistency_targets(&[
            sample(10.0, 2000.0),
            sample(32.0, 2000.0),
            sample(64.0, 4000.0),
            sample(32.0, 2000.0),
            sample(150.0, 2000.0),
        ]);
        assert_eq!(median, Some(0.016));
        assert_eq!(targets, vec![median, None, None, None, median]);
    }

    #[test]
    fn even_median_tolerance_and_repeating_a_converged_pass() {
        let (median, _) = consistency_targets(&[sample(30.0, 2000.0), sample(34.0, 2000.0)]);
        assert!((median.unwrap() - 0.016).abs() < 1e-7);
        let (_, targets) = consistency_targets(&[
            sample(25.6, 2000.0),
            sample(32.0, 2000.0),
            sample(38.4, 2000.0),
        ]);
        assert_eq!(targets, vec![None; 3]);
        assert_eq!(
            consistency_targets(&[sample(32.0, 2000.0); 5]).1,
            vec![None; 5]
        );
    }

    #[test]
    fn empty_single_and_invalid_measurements_do_not_invent_targets() {
        assert_eq!(consistency_targets(&[]), (None, vec![]));
        assert_eq!(
            consistency_targets(&[sample(32.0, 2000.0)]),
            (Some(0.016), vec![None])
        );
        assert_eq!(
            consistency_targets(&[sample(f32::NAN, 2000.0), sample(32.0, 0.0)]),
            (None, vec![None; 2])
        );
    }

    #[test]
    fn a_candidate_must_fit_and_move_closer_to_the_target() {
        assert!(fitting_improvement(10.0, 32.0, 12.0, false));
        assert!(fitting_improvement(80.0, 32.0, 30.0, false));
        assert!(!fitting_improvement(10.0, 32.0, 12.0, true));
        assert!(!fitting_improvement(10.0, 32.0, 10.0, false));
        assert!(!fitting_improvement(30.0, 32.0, 20.0, false));
        assert!(!fitting_improvement(160.0, 60.0, 32.0, false));
        assert!(!fitting_improvement(10.0, 20.0, 26.0, false));
    }
}
