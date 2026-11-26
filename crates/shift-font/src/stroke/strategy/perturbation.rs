//! Linear perturbation offset (Levien / Thottingal).

use kurbo::{BezPath, CubicBez, ParamCurve, ParamCurveDeriv, Point, Vec2};

use super::super::g1_correction::{DEFAULT_G1_THRESHOLD_DEG, soften_handle, tangent_jump_deg};
use super::super::nib_shape::NibShape;
use super::super::offset_strategy::OffsetStrategy;

pub struct PerturbationStrategy;

impl OffsetStrategy for PerturbationStrategy {
    fn offset_segment(
        &self,
        segment: CubicBez,
        nib_start: NibShape,
        nib_end: NibShape,
        side: f64,
        tolerance: f64,
        target_depth: u32,
        out: &mut BezPath,
    ) {
        let _ = tolerance;
        let segments = split_depth(segment, target_depth);
        for cubic in segments {
            append_perturbed_cubic(out, cubic, nib_start, nib_end, side);
        }
    }
}

fn split_depth(cubic: CubicBez, depth: u32) -> Vec<CubicBez> {
    if depth == 0 {
        return vec![cubic];
    }
    let (left, right) = cubic.subdivide();
    let mut out = split_depth(left, depth - 1);
    out.extend(split_depth(right, depth - 1));
    out
}

fn append_perturbed_cubic(
    out: &mut BezPath,
    cubic: CubicBez,
    nib_start: NibShape,
    nib_end: NibShape,
    side: f64,
) {
    let eval = |t: f64| -> (Point, Vec2, f64) {
        let point = cubic.eval(t);
        let tangent = cubic.deriv().eval(t).to_vec2();
        let normal = tangent.turn_90().normalize();
        let phi = libm::atan2(tangent.y, tangent.x);
        let nib = lerp_nib(nib_start, nib_end, t);
        let width = nib.support_outer(phi) * side;
        (point + normal * width, tangent, width)
    };

    let (p0, t0, _) = eval(0.0);
    let (pm, _, _) = eval(0.5);
    let (p3, t3, _) = eval(1.0);

    if out.elements().is_empty() {
        out.move_to(p0);
    }

    let fitted = fit_cubic(p0, pm, p3, t0, t3);
    let jump = tangent_jump_deg(t0, t3);
    let fitted = soften_handle(fitted, t0, jump, DEFAULT_G1_THRESHOLD_DEG);
    out.curve_to(fitted.p1, fitted.p2, fitted.p3);
}

fn lerp_nib(a: NibShape, b: NibShape, t: f64) -> NibShape {
    NibShape {
        width: a.width + (b.width - a.width) * t,
        height: a.height + (b.height - a.height) * t,
        angle: a.angle + (b.angle - a.angle) * t,
    }
}

fn fit_cubic(p0: Point, pm: Point, p3: Point, t0: Vec2, t3: Vec2) -> CubicBez {
    let cross = |a: Vec2, b: Vec2| a.x * b.y - a.y * b.x;
    let mid = Point::new((p0.x + p3.x) / 2.0, (p0.y + p3.y) / 2.0);
    let delta = (pm.to_vec2() - mid.to_vec2()) * (8.0 / 3.0);
    let denom = cross(t0, t3);
    let alpha = if denom.abs() < 1e-18 {
        delta.hypot() / 3.0
    } else {
        cross(delta, t3) / denom
    };
    let beta = if denom.abs() < 1e-18 {
        alpha
    } else {
        cross(delta, t0) / denom
    };
    CubicBez::new(p0, p0 + t0 * alpha, p3 - t3 * beta, p3)
}
