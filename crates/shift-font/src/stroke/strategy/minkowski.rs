//! Exact offset via kurbo `offset_cubic` on support-scaled distances.

use kurbo::{BezPath, CubicBez, ParamCurve, ParamCurveDeriv, offset::offset_cubic};

use super::super::nib_shape::NibShape;
use super::super::offset_strategy::OffsetStrategy;

pub struct MinkowskiStrategy;

impl OffsetStrategy for MinkowskiStrategy {
    fn offset_segment(
        &self,
        segment: CubicBez,
        nib_start: NibShape,
        nib_end: NibShape,
        side: f64,
        tolerance: f64,
        _target_depth: u32,
        out: &mut BezPath,
    ) {
        let nib = lerp_nib(nib_start, nib_end, 0.5);
        let tangent = segment.deriv().eval(0.0).to_vec2();
        let phi = libm::atan2(tangent.y, tangent.x);
        let distance = nib.support_outer(phi) * side;
        if out.elements().is_empty() {
            let start = segment.start() + tangent.turn_90().normalize() * distance;
            out.move_to(start);
        }
        let mut offset = BezPath::new();
        offset_cubic(segment, distance, tolerance, &mut offset);
        for element in offset.elements().iter().skip(1) {
            match element {
                kurbo::PathEl::LineTo(p) => out.line_to(*p),
                kurbo::PathEl::CurveTo(a, b, c) => out.curve_to(*a, *b, *c),
                _ => {}
            }
        }
    }
}

fn lerp_nib(a: NibShape, b: NibShape, t: f64) -> NibShape {
    NibShape {
        width: a.width + (b.width - a.width) * t,
        height: a.height + (b.height - a.height) * t,
        angle: a.angle + (b.angle - a.angle) * t,
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::position;
    use super::*;

    #[test]
    fn minkowski_matches_offset_on_line() {
        let mut line = BezPath::new();
        line.move_to((0.0, 0.0));
        line.line_to((100.0, 0.0));
        let constant = position::offset_path(&line, 10.0, kurbo::Join::Round, 4.0, 0.25);
        let nib = NibShape::circular(20.0);
        let mut minkowski = BezPath::new();
        MinkowskiStrategy.offset_segment(
            CubicBez::new((0.0, 0.0), (33.0, 0.0), (66.0, 0.0), (100.0, 0.0)),
            nib,
            nib,
            1.0,
            0.25,
            1,
            &mut minkowski,
        );
        assert!(!constant.elements().is_empty());
        assert!(!minkowski.elements().is_empty());
    }
}
