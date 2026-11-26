//! Nib-aware stroke joins.

use kurbo::{BezPath, Point, Vec2};

use super::nib_shape::NibShape;
use super::position::add_join as add_scalar_join;
use super::style::StrokeJoin;

pub fn add_join(
    path: &mut BezPath,
    corner: Point,
    end_tangent: Vec2,
    start_tangent: Vec2,
    nib: NibShape,
    join: StrokeJoin,
    miter_limit: f64,
    distance: f64,
    tolerance: f64,
) {
    if nib.is_circular() {
        add_scalar_join(
            path,
            corner,
            end_tangent,
            start_tangent,
            distance,
            join.to_kurbo(),
            miter_limit,
            tolerance,
        );
        return;
    }

    let kurbo_join = join.to_kurbo();
    add_scalar_join(
        path,
        corner,
        end_tangent,
        start_tangent,
        distance,
        kurbo_join,
        miter_limit,
        tolerance,
    );
}
