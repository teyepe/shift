//! Stage 1: resolve stroke-wide nib defaults with per-node sparse overrides.

use std::collections::BTreeMap;

use kurbo::{CubicBez, PathSeg};

use crate::PointId;
use crate::ir::contour::Contour;
use crate::stroke::layer_edit::contour_skeleton_path;
use crate::stroke::nib_shape::{NibOverride, NibShape};

/// Merges `stroke_default` with optional sparse `patch`.
pub fn resolve_nib(stroke_default: NibShape, patch: NibOverride) -> NibShape {
    NibShape {
        width: patch.width.unwrap_or(stroke_default.width),
        height: patch.height.unwrap_or(stroke_default.height),
        angle: patch.angle.unwrap_or(stroke_default.angle),
    }
}

/// Resolved nib at each on-curve point in contour order.
pub fn resolved_on_curve_nibs(
    contour: &Contour,
    stroke_default: NibShape,
    overrides: &BTreeMap<String, NibOverride>,
) -> Vec<NibShape> {
    contour
        .points()
        .iter()
        .filter(|point| point.is_on_curve())
        .map(|point| {
            let patch = overrides
                .get(&point.id().to_string())
                .copied()
                .unwrap_or_default();
            resolve_nib(stroke_default, patch)
        })
        .collect()
}

/// Cubic segments of the skeleton with nib shapes at each end.
pub fn segment_nib_pairs(
    contour: &Contour,
    stroke_default: NibShape,
    overrides: &BTreeMap<String, NibOverride>,
) -> Vec<(CubicBez, NibShape, NibShape)> {
    let nibs = resolved_on_curve_nibs(contour, stroke_default, overrides);
    if nibs.len() < 2 {
        return Vec::new();
    }

    let path = contour_skeleton_path(contour);
    let mut segments = segment_cubics(&path);
    if segments.is_empty() {
        return Vec::new();
    }

    let closed = contour.is_closed();
    let pair_count = if closed {
        nibs.len()
    } else {
        nibs.len().saturating_sub(1)
    };
    segments.truncate(pair_count);

    segments
        .into_iter()
        .enumerate()
        .map(|(index, cubic)| {
            let start = nibs[index];
            let end = if closed {
                nibs[(index + 1) % nibs.len()]
            } else {
                nibs[index + 1]
            };
            (cubic, start, end)
        })
        .collect()
}

fn segment_cubics(path: &kurbo::BezPath) -> Vec<CubicBez> {
    let mut segments = Vec::new();
    let mut current = kurbo::Point::ZERO;
    let mut start = kurbo::Point::ZERO;
    for el in path.elements() {
        match *el {
            kurbo::PathEl::MoveTo(p) => {
                start = p;
                current = p;
            }
            kurbo::PathEl::LineTo(p) => {
                segments.push(CubicBez::new(current, current, p, p));
                current = p;
            }
            kurbo::PathEl::QuadTo(c, p) => {
                segments.push(PathSeg::Quad(kurbo::QuadBez::new(current, c, p)).to_cubic());
                current = p;
            }
            kurbo::PathEl::CurveTo(a, b, p) => {
                segments.push(CubicBez::new(current, a, b, p));
                current = p;
            }
            kurbo::PathEl::ClosePath => {
                if current != start {
                    segments.push(CubicBez::new(current, current, start, start));
                }
            }
        }
    }
    segments
}

/// Drops override keys that are not on-curve point ids on `contour`.
pub fn live_on_curve_point_ids(contour: &Contour) -> Vec<PointId> {
    contour
        .points()
        .iter()
        .filter(|point| point.is_on_curve())
        .map(|point| point.id().clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::PointType;
    use crate::ir::contour::Contour;

    use super::*;

    #[test]
    fn sparse_patch_inherits_unset_fields() {
        let base = NibShape {
            width: 30.0,
            height: 6.0,
            angle: 0.5,
        };
        let patch = NibOverride {
            width: Some(40.0),
            height: None,
            angle: None,
        };
        let resolved = resolve_nib(base, patch);
        assert!((resolved.width - 40.0).abs() < 1e-9);
        assert!((resolved.height - 6.0).abs() < 1e-9);
        assert!((resolved.angle - 0.5).abs() < 1e-9);
    }

    #[test]
    fn open_contour_segments_pair_endpoints() {
        let mut contour = Contour::new();
        contour.add_point(0.0, 0.0, PointType::OnCurve, false);
        contour.add_point(100.0, 0.0, PointType::OnCurve, false);
        let base = NibShape::circular(20.0);
        let mut overrides = BTreeMap::new();
        let end = contour
            .points()
            .iter()
            .filter(|point| point.is_on_curve())
            .last()
            .unwrap()
            .id()
            .to_string();
        overrides.insert(
            end,
            NibOverride {
                width: Some(40.0),
                height: None,
                angle: None,
            },
        );
        let pairs = segment_nib_pairs(&contour, base, &overrides);
        assert_eq!(pairs.len(), 1);
        assert!((pairs[0].1.width - 20.0).abs() < 1e-9);
        assert!((pairs[0].2.width - 40.0).abs() < 1e-9);
    }
}
