//! Locates inner-offset cusps where `|κ| · h_outer = 1`.

use kurbo::{CubicBez, ParamCurve, ParamCurveDeriv};

use super::nib_shape::NibShape;

pub fn cusp_parameters(cubic: CubicBez, nib: NibShape) -> Vec<f64> {
    const SAMPLES: usize = 16;
    let mut hits = Vec::new();
    for index in 0..=SAMPLES {
        let t = index as f64 / SAMPLES as f64;
        let tangent = cubic.deriv().eval(t).to_vec2();
        let len = tangent.hypot();
        if len < 1e-12 {
            continue;
        }
        let phi = libm::atan2(tangent.y, tangent.x);
        let kappa = curvature(cubic, t);
        let h = nib.support_outer(phi);
        if h > 1e-12 && kappa.abs() * h >= 1.0 - 1e-3 {
            hits.push(t);
        }
    }
    hits
}

fn curvature(cubic: CubicBez, t: f64) -> f64 {
    let d = cubic.deriv().eval(t).to_vec2();
    let dt = 1e-4;
    let t1 = (t + dt).min(1.0);
    let t0 = (t - dt).max(0.0);
    let d0 = cubic.deriv().eval(t0).to_vec2();
    let d1 = cubic.deriv().eval(t1).to_vec2();
    let dd = (d1 - d0) / (t1 - t0).max(1e-12);
    let cross = d.x * dd.y - d.y * dd.x;
    let speed = d.hypot2().powf(1.5);
    if speed < 1e-18 {
        return 0.0;
    }
    cross / speed
}
