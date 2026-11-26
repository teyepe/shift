//! Skeleton-based G¹ correction at offset joins (Thottingal, 15° default).

use kurbo::{CubicBez, Vec2};

pub const DEFAULT_G1_THRESHOLD_DEG: f64 = 15.0;

/// If `jump_deg` is below the threshold, blend end handle toward skeleton tangent.
pub fn soften_handle(
    cubic: CubicBez,
    skeleton_tangent: Vec2,
    jump_deg: f64,
    threshold_deg: f64,
) -> CubicBez {
    if jump_deg >= threshold_deg || skeleton_tangent.hypot2() < 1e-18 {
        return cubic;
    }

    let blend = 1.0 - jump_deg / threshold_deg;
    let end = cubic.p3;
    let skeleton_dir = skeleton_tangent.normalize();
    let handle = cubic.p2 - end;
    let aligned = skeleton_dir * handle.dot(skeleton_dir);
    let blended = handle * (1.0 - blend) + aligned * blend;
    CubicBez::new(cubic.p0, cubic.p1, end + blended, end)
}

pub fn tangent_jump_deg(a: Vec2, b: Vec2) -> f64 {
    if a.hypot2() < 1e-18 || b.hypot2() < 1e-18 {
        return 0.0;
    }
    let dot = a.normalize().dot(b.normalize()).clamp(-1.0, 1.0);
    libm::acos(dot).to_degrees()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_tangents_report_zero_jump() {
        let t = Vec2::new(1.0, 0.0);
        assert!(tangent_jump_deg(t, t).abs() < 1e-9);
    }
}
