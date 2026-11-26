//! Elliptical translation nib parameters and support function.

/// Stroke-wide nib: width, height, and angle of the wide axis.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "pack", derive(pack::Pack))]
pub struct NibShape {
    pub width: f64,
    pub height: f64,
    pub angle: f64,
}

impl Default for NibShape {
    fn default() -> Self {
        Self {
            width: 0.0,
            height: 0.0,
            angle: 0.0,
        }
    }
}

impl NibShape {
    /// A circle of diameter `width` (zero contrast, same as constant-width stroke).
    pub fn circular(width: f64) -> Self {
        Self {
            width,
            height: width,
            angle: 0.0,
        }
    }

    pub fn is_circular(self) -> bool {
        (self.width - self.height).abs() < 1e-9
    }

    /// Eccentricity `1 - height/width` in `(0, 1]`; `0` when circular.
    pub fn eccentricity(self) -> f64 {
        if self.width <= 0.0 {
            return 0.0;
        }
        1.0 - (self.height / self.width).clamp(0.0, 1.0)
    }

    /// Support function in direction `normal_angle` (radians, outward normal).
    pub fn support(self, normal_angle: f64) -> f64 {
        let a = self.width / 2.0;
        let b = self.height / 2.0;
        let d = normal_angle - self.angle;
        ((a * d.cos()).powi(2) + (b * d.sin()).powi(2)).sqrt()
    }

    /// Outward half-width for skeleton tangent angle `tangent_angle`.
    pub fn support_outer(self, tangent_angle: f64) -> f64 {
        self.support(tangent_angle + std::f64::consts::FRAC_PI_2)
    }

    /// Picks perturbation vs Minkowski for open-path offset (see plan checkpoint B).
    pub fn offset_mode(self) -> super::offset_strategy::OffsetMode {
        if self.eccentricity() > 0.7 {
            super::offset_strategy::OffsetMode::Minkowski
        } else {
            super::offset_strategy::OffsetMode::Perturbation
        }
    }
}

/// Optional per-node override (B2); unused fields inherit the stroke default.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "pack", derive(pack::Pack))]
pub struct NibOverride {
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub angle: Option<f64>,
}

/// Interpolates two angles along the shortest arc in `(-π, π]`.
pub fn lerp_angle(a: f64, b: f64, t: f64) -> f64 {
    let mut delta = b - a;
    while delta > std::f64::consts::PI {
        delta -= 2.0 * std::f64::consts::PI;
    }
    while delta <= -std::f64::consts::PI {
        delta += 2.0 * std::f64::consts::PI;
    }
    a + t * delta
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circular_support_is_half_width_everywhere() {
        let nib = NibShape::circular(40.0);
        for angle in [0.0, 0.3, 1.0, 2.5] {
            assert!((nib.support(angle) - 20.0).abs() < 1e-9, "angle {angle}");
        }
    }

    #[test]
    fn lerp_angle_follows_the_short_arc() {
        let a = 0.1;
        let b = -0.1;
        let mid = super::lerp_angle(a, b, 0.5);
        assert!((mid).abs() < 1e-9);
        let far = super::lerp_angle(0.0, std::f64::consts::FRAC_PI_2, 0.5);
        assert!((far - std::f64::consts::FRAC_PI_4).abs() < 1e-9);
    }

    #[test]
    fn support_at_canonical_angles_matches_table() {
        let a = 15.0;
        let b = 1.5;
        let theta = std::f64::consts::FRAC_PI_4;
        let nib = NibShape {
            width: a * 2.0,
            height: b * 2.0,
            angle: theta,
        };

        let vertical_stem = nib.support_outer(std::f64::consts::FRAC_PI_2);
        assert!(
            (vertical_stem - 10.606601717798212).abs() < 0.1,
            "{vertical_stem}"
        );
        let along_nib = nib.support_outer(nib.angle);
        assert!((along_nib - 1.5).abs() < 1e-4, "{along_nib}");
    }

    #[test]
    fn flat_brush_is_thin_along_nib_direction() {
        let nib = NibShape {
            width: 30.0,
            height: 0.0,
            angle: 0.0,
        };
        assert!(nib.support_outer(0.0) < 1e-6);
        assert!((nib.support_outer(std::f64::consts::FRAC_PI_2) - 15.0).abs() < 1e-6);
    }
}
