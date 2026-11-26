//! Strategy selection for open-path nib offset curves.

use kurbo::{BezPath, CubicBez};

use super::nib_shape::NibShape;

/// Which offset construction the open-path solver uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OffsetMode {
    Perturbation,
    Minkowski,
}

/// Builds one offset edge for a skeleton cubic with per-end nib values.
///
/// The `side` argument is a **signed dimensionless scale factor**, not a font-unit
/// distance.  The strategy computes `point + normal * (support_outer(phi) * side)`
/// where `normal` points to the **left** of the direction of travel.
/// `1.0` offsets one nib half-width to the left (the centred-stroke left edge),
/// `-1.0` offsets one half-width to the right (the centred-stroke right edge),
/// and `0.0` stays on the skeleton.  Values outside `[−2, 2]` are valid but
/// unusual (position past the skeleton's opposite side).
pub trait OffsetStrategy {
    fn offset_segment(
        &self,
        segment: CubicBez,
        nib_start: NibShape,
        nib_end: NibShape,
        side: f64,
        tolerance: f64,
        target_depth: u32,
        out: &mut BezPath,
    );
}
