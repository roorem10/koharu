use anyhow::Result;
use koharu_pipeline::Bounds;
use koharu_scene::{EntityId, Geometry, Snapshot, Visibility};

fn valid(area: Bounds) -> bool {
    [area.x, area.y, area.width, area.height]
        .into_iter()
        .all(f64::is_finite)
        && area.width > 0.0
        && area.height > 0.0
        && (area.x + area.width).is_finite()
        && (area.y + area.height).is_finite()
        && (area.width * area.height).is_finite()
}

/// A rough rectangle must describe one concrete area, not just touch or contain it.
/// A second plausible overlap makes the result ambiguous even if one has a better score.
fn unique_match(area: Bounds, candidates: &[(EntityId, Bounds)]) -> Option<EntityId> {
    if !valid(area) {
        return None;
    }
    let mut matched = None;
    let mut plausible = 0;
    for &(id, candidate) in candidates {
        if !valid(candidate) {
            continue;
        }
        let width =
            (area.x + area.width).min(candidate.x + candidate.width) - area.x.max(candidate.x);
        let height =
            (area.y + area.height).min(candidate.y + candidate.height) - area.y.max(candidate.y);
        if width <= 0.0 || height <= 0.0 {
            continue;
        }
        let intersection = width * height;
        let a = area.width * area.height;
        let b = candidate.width * candidate.height;
        if intersection / a.min(b) >= 0.2 {
            plausible += 1;
        }
        if intersection / (a + b - intersection) >= 0.5 {
            matched = Some(id);
        }
    }
    if plausible == 1 {
        matched
    } else {
        None
    }
}

fn bounds(geometry: &Geometry) -> Option<Bounds> {
    let first = geometry.points.first()?;
    let (mut left, mut right, mut top, mut bottom) = (first.x, first.x, first.y, first.y);
    for point in &geometry.points {
        if !point.x.is_finite() || !point.y.is_finite() {
            return None;
        }
        left = left.min(point.x);
        right = right.max(point.x);
        top = top.min(point.y);
        bottom = bottom.max(point.y);
    }
    let area = Bounds {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    };
    valid(area).then_some(area)
}

pub(super) fn matched_layer(
    snapshot: &Snapshot,
    page: EntityId,
    area: Bounds,
) -> Result<Option<EntityId>> {
    let mut candidates = Vec::new();
    for layer in snapshot.page(page)?.text_layers()? {
        let mut ancestor = Some(layer.id());
        let mut visible = true;
        while let Some(id) = ancestor {
            if snapshot
                .component::<Visibility>(id)?
                .is_some_and(|visibility| !visibility.visible || visibility.opacity <= 0.0)
            {
                visible = false;
                break;
            }
            ancestor = snapshot.parent(id)?;
        }
        if !visible {
            continue;
        }
        // Presentation geometry wins: source detection may no longer be where text is drawn.
        let geometry = match layer.frame()? {
            Some(geometry) => Some(geometry),
            None => layer
                .content()?
                .source_region()?
                .map(|region| region.geometry())
                .transpose()?,
        };
        if let Some(area) = geometry.as_ref().and_then(bounds) {
            candidates.push((layer.id(), area));
        }
    }
    Ok(unique_match(area, &candidates))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, width: f64, height: f64) -> Bounds {
        Bounds {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn accepts_a_single_close_area_but_not_a_touch_or_an_entire_panel() {
        let layer = EntityId::new();
        let candidates = [(layer, rect(20., 20., 100., 50.))];
        assert_eq!(
            unique_match(rect(18., 18., 104., 54.), &candidates),
            Some(layer)
        );
        assert_eq!(unique_match(rect(115., 20., 100., 50.), &candidates), None);
        assert_eq!(unique_match(rect(0., 0., 600., 900.), &candidates), None);
        assert_eq!(unique_match(rect(120., 20., 100., 50.), &candidates), None);
    }

    #[test]
    fn refuses_multiple_plausible_areas_independent_of_order() {
        let first = EntityId::new();
        let second = EntityId::new();
        let a = (first, rect(20., 20., 100., 50.));
        let b = (second, rect(90., 20., 100., 50.));
        let area = rect(20., 20., 100., 50.);
        assert_eq!(unique_match(area, &[a, b]), None);
        assert_eq!(unique_match(area, &[b, a]), None);
        assert_eq!(unique_match(area, &[a, (second, a.1)]), None);
    }

    #[test]
    fn invalid_model_geometry_never_matches() {
        let candidates = [(EntityId::new(), rect(0., 0., 100., 100.))];
        for area in [
            rect(f64::NAN, 0., 100., 100.),
            rect(0., f64::INFINITY, 100., 100.),
            rect(0., 0., -100., 100.),
            rect(0., 0., 100., 0.),
            rect(f64::MAX, 0., f64::MAX, 100.),
        ] {
            assert_eq!(unique_match(area, &candidates), None);
        }
        assert_eq!(unique_match(rect(0., 0., 100., 100.), &[]), None);
    }
}
