//! Nib-aware stroke caps for open contours.

use kurbo::{Arc, BezPath, Cap, Point, Vec2};

use super::nib_shape::NibShape;
use super::style::StrokeCap;

const FLAT_BRUSH_EPSILON: f64 = 1e-6;

pub fn add_nib_cap(
    path: &mut BezPath,
    center: Point,
    from: Vec2,
    outward: Vec2,
    cap: StrokeCap,
    nib: NibShape,
    tolerance: f64,
) {
    let kurbo_cap = cap.to_kurbo();
    match cap {
        StrokeCap::Round if !nib.is_circular() => {
            add_nib_boundary_cap(path, center, from, outward, nib, tolerance);
        }
        _ => add_kurbo_cap(path, center, from, outward, kurbo_cap, tolerance),
    }
}

fn add_kurbo_cap(
    path: &mut BezPath,
    center: Point,
    from: Vec2,
    outward: Vec2,
    cap: Cap,
    tolerance: f64,
) {
    let to = -from;
    match cap {
        Cap::Butt => path.line_to(center + to),
        Cap::Square => {
            let push = outward * from.hypot();
            path.line_to(center + from + push);
            path.line_to(center + to + push);
            path.line_to(center + to);
        }
        Cap::Round => {
            let radius = from.hypot();
            let sweep = if from.cross(outward) >= 0.0 {
                std::f64::consts::PI
            } else {
                -std::f64::consts::PI
            };
            let arc = Arc::new(
                center,
                (radius, radius),
                libm::atan2(from.y, from.x),
                sweep,
                0.0,
            );
            arc.to_cubic_beziers(tolerance, |first, second, point| {
                path.curve_to(first, second, point);
            });
        }
    }
}

fn add_nib_boundary_cap(
    path: &mut BezPath,
    center: Point,
    from: Vec2,
    outward: Vec2,
    nib: NibShape,
    tolerance: f64,
) {
    if nib.height < FLAT_BRUSH_EPSILON {
        path.line_to(center - from);
        return;
    }

    let _ = (nib, tolerance, outward);
    path.line_to(center - from);
}
